//! chdkpano-server: axum backend that wraps the chdkptp library and serves
//! the WASM frontend.

use axum::{
    extract::{Path, Query, State},
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
use std::sync::{Arc, OnceLock};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
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
static STATIC_DIR: OnceLock<String> = OnceLock::new();

/// Static-file server with SPA fallback. Serves real files when they exist
/// (so trunk-emitted JS/WASM/CSS at absolute paths work) and otherwise hands
/// back `index.html` with 200 so the Leptos router can take over.
async fn static_or_spa(uri: axum::http::Uri) -> Response {
    let dir = STATIC_DIR.get().map(String::as_str).unwrap_or("client/dist");
    let req_path = uri.path().trim_start_matches('/');
    let candidate = format!("{dir}/{req_path}");

    // Tiny path-traversal guard — file lookups are inside `dir` only.
    if req_path.contains("..") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }

    if !req_path.is_empty() {
        if let Ok(bytes) = tokio::fs::read(&candidate).await {
            return (
                [(header::CONTENT_TYPE, static_content_type(req_path))],
                bytes,
            )
                .into_response();
        }
    }

    match tokio::fs::read(format!("{dir}/index.html")).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], bytes).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("index.html missing in {dir}"),
        )
            .into_response(),
    }
}

fn static_content_type(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if lower.ends_with(".js") || lower.ends_with(".mjs") {
        "application/javascript"
    } else if lower.ends_with(".wasm") {
        "application/wasm"
    } else if lower.ends_with(".css") {
        "text/css"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else if lower.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}

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

// ---------- Camera info ----------

#[derive(Serialize)]
struct InfoDto {
    // USB
    serial: String,
    vendor_id: u16,
    product_id: u16,
    bus_number: u8,
    device_address: u8,
    usb_manufacturer: Option<String>,
    usb_product: Option<String>,
    // PTP DeviceInfo
    ptp_standard_version: u16,
    vendor_extension_id: u32,
    vendor_extension_version: u16,
    vendor_extension_desc: String,
    functional_mode: u16,
    ptp_manufacturer: String,
    ptp_model: String,
    device_version: String,
    serial_number: String,
    operations_supported: Vec<u16>,
    events_supported: Vec<u16>,
    device_properties_supported: Vec<u16>,
    capture_formats: Vec<u16>,
    image_formats: Vec<u16>,
    chdk_advertised: bool,
    // CHDK
    chdk_version_major: Option<u32>,
    chdk_version_minor: Option<u32>,
}

async fn camera_info(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
) -> Result<Json<InfoDto>, ApiError> {
    let cams = chdkptp::list_cameras()?;
    let cam = cams
        .iter()
        .find(|c| c.serial() == Some(&serial))
        .ok_or_else(|| ApiError(format!("camera with serial {serial} not found")))?
        .clone();

    let cam_session = pool.get_or_open(&serial).await?;
    let mut session = cam_session.session.lock().await;

    let info = session.get_device_info().await.map_err(|e| {
        pool.invalidate(&serial);
        ApiError::from(e)
    })?;
    let chdk_advertised = info.has_chdk();

    let (cmaj, cmin) = if chdk_advertised {
        match session.chdk_version().await {
            Ok(v) => (Some(v.major), Some(v.minor)),
            Err(_) => (None, None),
        }
    } else {
        (None, None)
    };

    Ok(Json(InfoDto {
        serial: serial.clone(),
        vendor_id: cam.vendor_id(),
        product_id: cam.product_id(),
        bus_number: cam.bus_number(),
        device_address: cam.device_address(),
        usb_manufacturer: cam.manufacturer().map(String::from),
        usb_product: cam.product().map(String::from),
        ptp_standard_version: info.standard_version,
        vendor_extension_id: info.vendor_extension_id,
        vendor_extension_version: info.vendor_extension_version,
        vendor_extension_desc: info.vendor_extension_desc,
        functional_mode: info.functional_mode,
        ptp_manufacturer: info.manufacturer,
        ptp_model: info.model,
        device_version: info.device_version,
        serial_number: info.serial_number,
        operations_supported: info.operations_supported,
        events_supported: info.events_supported,
        device_properties_supported: info.device_properties_supported,
        capture_formats: info.capture_formats,
        image_formats: info.image_formats,
        chdk_advertised,
        chdk_version_major: cmaj,
        chdk_version_minor: cmin,
    }))
}

// ---------- Live runtime state via Lua ----------

#[derive(Serialize)]
struct LiveStateDto {
    in_record: Option<bool>,
    is_movie: Option<bool>,
    mode_code: Option<i32>,
    zoom: Option<i32>,
    exp_count: Option<i32>,
    vbatt_mv: Option<i32>,
    image_dir: Option<String>,
    free_kb: Option<i32>,
    iso_mode: Option<i32>,
    sv96: Option<i32>,
    tv96: Option<i32>,
    av96: Option<i32>,
    focus: Option<i32>,
    propset: Option<i32>,
    flash_mode: Option<i32>,
    flash_ready: Option<bool>,
    is_shooting: Option<bool>,
    raw: String,
}

const LIVE_STATE_LUA: &str = "\
    local function s(f) local ok, v = pcall(f); if ok then return tostring(v) end return '?' end \
    local m1, m2, m3 = get_mode() \
    return tostring(m1)..'|'..tostring(m2)..'|'..tostring(m3) \
        ..'|'..s(function() return get_zoom() end) \
        ..'|'..s(function() return get_exp_count() end) \
        ..'|'..s(function() return get_vbatt() end) \
        ..'|'..s(function() return get_image_dir() end) \
        ..'|'..s(function() return get_free_disk_space() end) \
        ..'|'..s(function() return get_iso_mode() end) \
        ..'|'..s(function() return get_sv96() end) \
        ..'|'..s(function() return get_tv96() end) \
        ..'|'..s(function() return get_av96() end) \
        ..'|'..s(function() return get_focus() end) \
        ..'|'..s(function() return get_propset() end) \
        ..'|'..s(function() return get_flash_mode() end) \
        ..'|'..s(function() return get_flash_ready() end) \
        ..'|'..s(function() return get_shooting() end)";

async fn live_state(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
) -> Result<Json<LiveStateDto>, ApiError> {
    let cam_session = pool.get_or_open(&serial).await?;
    let mut session = cam_session.session.lock().await;
    let msgs = session
        .execute_script_wait(LIVE_STATE_LUA, 8_000)
        .await
        .map_err(|e| {
            pool.invalidate(&serial);
            ApiError::from(e)
        })?;

    let raw = msgs
        .iter()
        .find_map(|m| match m {
            chdkptp::chdk::ScriptMsg::Return {
                value: chdkptp::chdk::ScriptValue::String(s),
                ..
            } => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let parts: Vec<&str> = raw.split('|').collect();
    let pi = |i: usize| -> Option<i32> { parts.get(i)?.parse().ok() };
    let pb = |i: usize| -> Option<bool> {
        let s = *parts.get(i)?;
        if s == "true" { Some(true) } else if s == "false" { Some(false) } else { None }
    };
    let ps = |i: usize| -> Option<String> {
        let s = *parts.get(i)?;
        if s == "?" || s == "nil" { None } else { Some(s.to_string()) }
    };

    Ok(Json(LiveStateDto {
        in_record: pb(0),
        is_movie: pb(1),
        mode_code: pi(2),
        zoom: pi(3),
        exp_count: pi(4),
        vbatt_mv: pi(5),
        image_dir: ps(6),
        free_kb: pi(7).map(|kb| kb / 1024),
        iso_mode: pi(8),
        sv96: pi(9),
        tv96: pi(10),
        av96: pi(11),
        focus: pi(12),
        propset: pi(13),
        flash_mode: pi(14),
        flash_ready: pb(15),
        is_shooting: pb(16),
        raw,
    }))
}

// ---------- File browser ----------

#[derive(Serialize)]
struct DirEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

#[derive(Serialize)]
struct ListDirResponse {
    path: String,
    entries: Vec<DirEntry>,
    note: Option<String>,
}

#[derive(serde::Deserialize)]
struct PathQuery {
    path: Option<String>,
}

/// Only allow ASCII alphanumerics + a handful of path-safe punctuation —
/// keeps Lua string interpolation safe from injection and prevents directory-
/// traversal mischief from leaking server file paths into the camera.
fn is_safe_camera_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() < 256
        && p.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ' ')
        })
}

/// CHDK Lua filesystem APIs vary by build. `os.listdir` is usually present
/// but the SD root (`A`) is finicky: some builds return nil unless the path
/// has a trailing slash or `.`, and `os.stat("A")` similarly returns nil.
/// We probe a few variants before giving up, and skip stat for the root so
/// we don't reject directories whose parent we can't stat.
fn list_dir_lua(path: &str) -> String {
    format!(
        "local path = '{path}' \
         if not os.listdir then return 'ERR_NOLIST' end \
         local function try_list(p) \
           local ok, t = pcall(os.listdir, p) \
           if ok and type(t) == 'table' then return t end \
           return nil \
         end \
         local function join(p, name) \
           if p == '' or p == '/' then return name end \
           if string.sub(p, -1) == '/' then return p .. name end \
           return p .. '/' .. name \
         end \
         local t = try_list(path) \
         if not t then t = try_list(path .. '/') end \
         if not t and path == 'A' then t = try_list('A/.') end \
         if not t then return 'ERR_LIST|nil' end \
         local out = {{}} \
         for _, e in ipairs(t) do \
           if e ~= '.' and e ~= '..' then \
             local full = join(path, e) \
             local sok, st = pcall(os.stat, full) \
             local is_dir = (sok and st and st.is_dir) and '1' or '0' \
             local size = (sok and st and st.size) or 0 \
             table.insert(out, e..':'..is_dir..':'..size) \
           end \
         end \
         return table.concat(out, '\\n')"
    )
}

fn well_known_sd_root_entries() -> Vec<DirEntry> {
    ["DCIM", "CHDK", "MISC", "CANONMSC"]
        .into_iter()
        .map(|n| DirEntry {
            name: n.to_string(),
            is_dir: true,
            size: 0,
        })
        .collect()
}

async fn list_files(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
    Query(q): Query<PathQuery>,
) -> Result<Json<ListDirResponse>, ApiError> {
    let path = q.path.unwrap_or_else(|| "A".to_string());
    if !is_safe_camera_path(&path) {
        return Err(ApiError(format!("unsafe path: {path:?}")));
    }
    let cam_session = pool.get_or_open(&serial).await?;
    let mut session = cam_session.session.lock().await;
    let msgs = session
        .execute_script_wait(&list_dir_lua(&path), 15_000)
        .await
        .map_err(|e| {
            pool.invalidate(&serial);
            ApiError::from(e)
        })?;
    let raw = msgs
        .iter()
        .find_map(|m| match m {
            chdkptp::chdk::ScriptMsg::Return {
                value: chdkptp::chdk::ScriptValue::String(s),
                ..
            } => Some(s.clone()),
            _ => None,
        })
        .unwrap_or_default();

    if raw == "ERR_NOLIST" {
        return Ok(Json(ListDirResponse {
            path,
            entries: vec![],
            note: Some("camera build lacks os.listdir — directory browsing not available".into()),
        }));
    }
    if let Some(err) = raw.strip_prefix("ERR_LIST|") {
        // SD-root listing is unreliable on some CHDK builds (returns nil) —
        // fall back to the well-known Canon top-level directories so the user
        // can still navigate into them. Subdir listings work normally.
        if path == "A" || path == "A/" {
            return Ok(Json(ListDirResponse {
                path,
                entries: well_known_sd_root_entries(),
                note: None,
            }));
        }
        return Ok(Json(ListDirResponse {
            path,
            entries: vec![],
            note: Some(format!("listdir failed: {err}")),
        }));
    }

    let mut entries: Vec<DirEntry> = raw
        .split('\n')
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            // name:is_dir:size — name may contain ':' in pathological cases,
            // so rsplit twice keeps the name intact.
            let (head, size) = line.rsplit_once(':')?;
            let (name, is_dir) = head.rsplit_once(':')?;
            Some(DirEntry {
                name: name.to_string(),
                is_dir: is_dir == "1",
                size: size.parse().unwrap_or(0),
            })
        })
        .collect();
    // Directories first, then alpha
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Json(ListDirResponse {
        path,
        entries,
        note: None,
    }))
}

/// Download a single file from the camera. Sets a guessed content-type so the
/// browser renders JPEG/PNG inline (`<img src=...>` works) and downloads
/// everything else as application/octet-stream.
async fn get_file(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
    Query(q): Query<PathQuery>,
) -> Response {
    let path = match q.path {
        Some(p) if is_safe_camera_path(&p) => p,
        Some(p) => return ApiError(format!("unsafe path: {p:?}")).into_response(),
        None => return ApiError("missing ?path=".into()).into_response(),
    };
    let cam_session = match pool.get_or_open(&serial).await {
        Ok(s) => s,
        Err(e) => return e.into_response(),
    };
    let mut session = cam_session.session.lock().await;
    let data = match session.download_file(&path).await {
        Ok(d) => d,
        Err(e) => {
            pool.invalidate(&serial);
            return ApiError(format!("download_file: {e}")).into_response();
        }
    };

    let ct = guess_content_type(&path);
    (
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, "private, max-age=60"),
        ],
        data,
    )
        .into_response()
}

fn guess_content_type(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".cr2") || lower.ends_with(".crw") || lower.ends_with(".dng") {
        "image/x-canon-raw"
    } else if lower.ends_with(".mov") || lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".txt") || lower.ends_with(".log") || lower.ends_with(".lua") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

// ---------- Lua REPL endpoint ----------

#[derive(serde::Deserialize)]
struct ExecRequest {
    source: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum MessageDto {
    Return { value: ValueDto },
    Error { category: String, text: String },
    User { value: ValueDto },
}

#[derive(Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
enum ValueDto {
    Nil,
    Boolean(bool),
    Integer(i32),
    String(String),
    Table(String),
    Unsupported,
}

impl From<&chdkptp::chdk::ScriptValue> for ValueDto {
    fn from(v: &chdkptp::chdk::ScriptValue) -> Self {
        use chdkptp::chdk::ScriptValue;
        match v {
            ScriptValue::Nil => ValueDto::Nil,
            ScriptValue::Boolean(b) => ValueDto::Boolean(*b),
            ScriptValue::Integer(i) => ValueDto::Integer(*i),
            ScriptValue::String(s) => ValueDto::String(s.clone()),
            ScriptValue::Table(s) => ValueDto::Table(s.clone()),
            ScriptValue::Unsupported => ValueDto::Unsupported,
        }
    }
}

#[derive(Serialize)]
struct ExecResponse {
    messages: Vec<MessageDto>,
    elapsed_ms: u64,
}

/// Run arbitrary Lua on the camera via ExecuteScript; return all messages.
async fn exec_lua(
    State(pool): State<Arc<CameraPool>>,
    Path(serial): Path<String>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, ApiError> {
    let cam_session = pool.get_or_open(&serial).await?;
    let mut session = cam_session.session.lock().await;
    let t = std::time::Instant::now();
    let msgs = session
        .execute_script_wait(&req.source, 20_000)
        .await
        .map_err(|e| {
            pool.invalidate(&serial);
            ApiError::from(e)
        })?;
    let elapsed_ms = t.elapsed().as_millis() as u64;

    let messages: Vec<MessageDto> = msgs
        .iter()
        .filter_map(|m| match m {
            chdkptp::chdk::ScriptMsg::None => None,
            chdkptp::chdk::ScriptMsg::Return { value, .. } => Some(MessageDto::Return {
                value: value.into(),
            }),
            chdkptp::chdk::ScriptMsg::Error { category, text, .. } => Some(MessageDto::Error {
                category: format!("{category:?}"),
                text: text.clone(),
            }),
            chdkptp::chdk::ScriptMsg::User { value, .. } => Some(MessageDto::User {
                value: value.into(),
            }),
        })
        .collect();

    Ok(Json(ExecResponse { messages, elapsed_ms }))
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
    STATIC_DIR
        .set(static_dir.clone())
        .expect("STATIC_DIR set twice");

    let pool = POOL.clone();
    let app = Router::new()
        .route("/api/cameras", get(list_cameras))
        .route("/api/viewport/:serial", get(viewport_jpeg))
        .route("/api/mode/record/:serial", post(mode_record))
        .route("/api/mode/play/:serial", post(mode_play))
        .route("/api/exec/:serial", post(exec_lua))
        .route("/api/info/:serial", get(camera_info))
        .route("/api/live_state/:serial", get(live_state))
        .route("/api/files/:serial", get(list_files))
        .route("/api/file/:serial", get(get_file))
        .with_state(pool)
        // Single handler that serves static files when they exist and falls
        // back to index.html with 200 OK so the WASM router takes over on
        // deep-route reloads (/api-docs, /camera/:serial, etc.). Avoids
        // ServeDir's "404 + index body" quirk that confuses browsers.
        .fallback(static_or_spa)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = std::env::var("CHDKPANO_ADDR").unwrap_or_else(|_| "0.0.0.0:3030".into());
    tracing::info!("chdkpano-server listening on http://{addr}  (static: {static_dir})");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
