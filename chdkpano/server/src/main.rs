//! chdkpano-server: axum backend that wraps the chdkptp library and serves
//! the WASM frontend.

use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chdkptp::chdk::liveview::LV_TFR_VIEWPORT;
use chdkptp::PtpSession;
use image::{codecs::jpeg::JpegEncoder, ColorType};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Serialize)]
struct CameraDto {
    serial: String,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    manufacturer: Option<String>,
    product: Option<String>,
}

impl From<&chdkptp::CameraInfo> for CameraDto {
    fn from(c: &chdkptp::CameraInfo) -> Self {
        Self {
            serial: c.serial().unwrap_or("?").to_string(),
            vendor_id: c.vendor_id(),
            product_id: c.product_id(),
            bus_number: c.bus_number(),
            device_address: c.device_address(),
            manufacturer: c.manufacturer().map(String::from),
            product: c.product().map(String::from),
        }
    }
}

/// Per-camera session cache. The cache holds open `PtpSession`s by serial so
/// repeat viewport polls don't pay the ~50–100 ms session-open cost each call.
///
/// `tokio::sync::Mutex` so the lock can be held across an `.await` in the
/// async handler. Sessions are never shared across requests concurrently —
/// `lock().await` serializes per-camera.
struct CameraSession {
    session: Arc<tokio::sync::Mutex<PtpSession>>,
    /// When the currently-running half-press arm script will release.
    /// `None` means no arm is active; we need to send a fresh one.
    armed_until: parking_lot::Mutex<Option<std::time::Instant>>,
}

#[derive(Default)]
struct CameraPool {
    sessions: Mutex<HashMap<String, Arc<CameraSession>>>,
}

impl CameraPool {
    /// Get or open a session for the given camera serial. Retries with
    /// backoff if the interface claim fails — typical when macOS's
    /// `ptpcamerad` is racing us to the device.
    async fn get_or_open(&self, serial: &str) -> Result<Arc<CameraSession>, ApiError> {
        if let Some(s) = self.sessions.lock().get(serial).cloned() {
            return Ok(s);
        }
        let cams = chdkptp::list_cameras()?;
        let cam = cams
            .into_iter()
            .find(|c| c.serial() == Some(serial))
            .ok_or_else(|| ApiError(format!("camera with serial {serial} not found")))?;

        let mut last_err: Option<ApiError> = None;
        for attempt in 0..5 {
            match cam.open_ptp().await {
                Ok(session) => {
                    let arc = Arc::new(CameraSession {
                        session: Arc::new(tokio::sync::Mutex::new(session)),
                        armed_until: parking_lot::Mutex::new(None),
                    });
                    self.sessions.lock().insert(serial.to_string(), arc.clone());
                    return Ok(arc);
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_err = Some(ApiError(format!("attempt {}: {msg}", attempt + 1)));
                    if !msg.contains("exclusive access") {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(
                        200 * (attempt as u64 + 1),
                    )).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| ApiError("open failed".into())))
    }

    /// Drop the cached session for this serial (e.g. after a USB error).
    fn invalidate(&self, serial: &str) {
        self.sessions.lock().remove(serial);
    }
}

static POOL: Lazy<Arc<CameraPool>> = Lazy::new(|| Arc::new(CameraPool::default()));

async fn list_cameras() -> Result<Json<Vec<CameraDto>>, ApiError> {
    let cams = chdkptp::list_cameras().map_err(ApiError::from)?;
    Ok(Json(cams.iter().map(CameraDto::from).collect()))
}

/// Real viewport endpoint: pulls one live view frame from the camera, decodes
/// YUV → RGB, encodes JPEG, returns. The client polls this URL.
///
/// If the camera isn't producing a viewport (fb_type=0 — typical when in
/// playback mode), returns a friendly SVG with the reason instead of a 500.
/// The client's `<img>` element renders the SVG inline so the user sees the
/// problem without having to open devtools.
async fn viewport_jpeg(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
) -> Response {
    match try_viewport(&pool, &serial).await {
        Ok(jpeg) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
            ],
            jpeg,
        )
            .into_response(),
        Err(reason) => placeholder_svg(&serial, &reason).into_response(),
    }
}

async fn try_viewport(pool: &CameraPool, serial: &str) -> Result<Vec<u8>, String> {
    let cam_session = pool
        .get_or_open(serial)
        .await
        .map_err(|e| format!("open camera: {}", e.0))?;
    let mut session = cam_session.session.lock().await;

    // No half-press, no scripting. If the LCD is on, the viewfinder pipeline
    // is already running and writing the YUV buffer. Just read it.
    let frame = session
        .get_display_data(LV_TFR_VIEWPORT)
        .await
        .map_err(|e| {
            pool.invalidate(serial);
            format!("get_display_data: {e}")
        })?;

    // Canonical CHDK viewport format: Y411 / UYVYYY (12 bpp, 6 bytes per 4 pixels).
    let (width, height, rgb) = frame.decode_viewport_rgb().map_err(|e| e.to_string())?;

    let mut jpeg = Vec::with_capacity((width * height) as usize);
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 80);
    enc.encode(&rgb, width, height, ColorType::Rgb8.into())
        .map_err(|e| format!("jpeg encode: {e}"))?;
    Ok(jpeg)
}

fn placeholder_svg(serial: &str, reason: &str) -> impl IntoResponse {
    let short = serial.chars().take(12).collect::<String>();
    // Wrap the diagnostic text across multiple lines so the SVG actually
    // shows it instead of overflowing the viewBox.
    let lines: Vec<String> = reason
        .split(" | ")
        .map(|s| s.to_string())
        .collect();
    let mut text_lines = String::new();
    let start_y = 240 - (lines.len() as i32 * 9);
    for (i, line) in lines.iter().enumerate() {
        let y = start_y + (i as i32) * 18;
        let escaped = line.replace('<', "&lt;").replace('>', "&gt;");
        text_lines.push_str(&format!(
            r##"<text x="320" y="{y}" font-family="ui-monospace, monospace" font-size="12" fill="#aaa" text-anchor="middle">{escaped}</text>"##,
        ));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="480" viewBox="0 0 640 480">
  <rect width="640" height="480" fill="#1a1a1a"/>
  <text x="320" y="40" font-family="-apple-system, sans-serif" font-size="18" fill="#888" text-anchor="middle">viewport unavailable</text>
  {text_lines}
  <text x="320" y="440" font-family="ui-monospace, monospace" font-size="10" fill="#555" text-anchor="middle">serial: {short}…</text>
</svg>"##
    );
    (
        [
            (header::CONTENT_TYPE, "image/svg+xml"),
            (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
        ],
        svg,
    )
}

/// Switch the camera into record mode (extends lens, enables capture).
async fn mode_record(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    switch_mode(&pool, &serial, 1).await
}

/// Switch the camera into playback mode (retracts lens, shows gallery).
async fn mode_play(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    switch_mode(&pool, &serial, 0).await
}

async fn switch_mode(
    pool: &CameraPool,
    serial: &str,
    target: u32,
) -> Result<Json<serde_json::Value>, ApiError> {
    let cam_session = pool.get_or_open(serial).await?;
    let mut session = cam_session.session.lock().await;
    let want_record = target == 1;
    // After switching to record, also force the LCD on so the viewfinder
    // task runs and get_display_data() can return real frames. Guarded by
    // type() checks because not every CHDK build exposes these functions.
    let lua = format!(
        "local in_record = get_mode() \
         if {target_truthy} ~= (in_record and true or false) then \
           switch_mode_usb({target}) \
           sleep(2000) \
         end \
         if {want_record_truthy} then \
           if type(set_lcd_display) == 'function' then set_lcd_display(1) end \
           if type(set_backlight)   == 'function' then set_backlight(1)   end \
           if type(request_live_view) == 'function' then request_live_view(15) end \
         end \
         return get_mode() and 'record' or 'play'",
        target_truthy = if want_record { "true" } else { "false" },
        want_record_truthy = if want_record { "true" } else { "false" },
        target = target,
    );
    let msgs = session
        .execute_script_wait(&lua, 15_000)
        .await
        .map_err(|e| {
            pool.invalidate(serial);
            ApiError::from(e)
        })?;
    for m in &msgs {
        if let chdkptp::chdk::ScriptMsg::Error { text, .. } = m {
            return Err(ApiError(format!("script error: {text}")));
        }
    }
    let mode = msgs
        .iter()
        .find_map(|m| match m {
            chdkptp::chdk::ScriptMsg::Return {
                value: chdkptp::chdk::ScriptValue::String(s),
                ..
            } => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();
    Ok(Json(serde_json::json!({ "mode": mode })))
}

#[derive(Debug)]
struct ApiError(String);
impl From<chdkptp::Error> for ApiError {
    fn from(e: chdkptp::Error) -> Self {
        ApiError(e.to_string())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "chdkpano_server=info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let static_dir = std::env::var("CHDKPANO_STATIC_DIR").unwrap_or_else(|_| {
        for candidate in ["client/dist", "../client/dist", "../../client/dist"] {
            if std::path::Path::new(candidate).join("index.html").is_file() {
                return candidate.to_string();
            }
        }
        eprintln!(
            "WARNING: could not find client/dist/index.html. \
             Did you run `trunk build` in the client crate? \
             Set CHDKPANO_STATIC_DIR to override."
        );
        "client/dist".into()
    });

    let pool = POOL.clone();
    let app = Router::new()
        .route("/api/cameras", get(list_cameras))
        .route("/api/viewport/:serial", get(viewport_jpeg))
        .route("/api/mode/record/:serial", post(mode_record))
        .route("/api/mode/play/:serial", post(mode_play))
        .with_state(pool)
        .fallback_service(ServeDir::new(&static_dir))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = std::env::var("CHDKPANO_ADDR").unwrap_or_else(|_| "0.0.0.0:3030".into());
    tracing::info!("chdkpano-server listening on http://{addr}  (static: {static_dir})");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
