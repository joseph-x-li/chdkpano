//! `/api/viewport/:serial` — one live-view frame as JPEG.
//!
//! Returns a friendly SVG placeholder on failure so the client's `<img>`
//! renders the reason inline (no devtools required to see what went wrong).

use crate::camera::CameraRegistry;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use std::sync::Arc;

#[utoipa::path(
    get,
    path = "/api/viewport/{serial}",
    tag = "viewport",
    params(("serial" = String, description = "Camera USB serial")),
    responses(
        (status = 200, description = "One live-view frame, JPEG @ q=80, ~30–80 KB at 640×480.",
            content_type = "image/jpeg", body = Vec<u8>),
        (status = 200, description = "If viewport unavailable, returns an SVG placeholder explaining why.",
            content_type = "image/svg+xml", body = String),
    ),
)]
pub async fn viewport_jpeg(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
) -> Response {
    let cam = match reg.get_or_open(&serial).await {
        Ok(c) => c,
        Err(e) => return placeholder_svg(&serial, &format!("open camera: {}", e.message())).into_response(),
    };
    match cam.viewport_jpeg().await {
        Ok(jpeg) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
            ],
            jpeg,
        )
            .into_response(),
        Err(e) => placeholder_svg(&serial, e.message()).into_response(),
    }
}

pub fn placeholder_svg(serial: &str, reason: &str) -> impl IntoResponse {
    let short = serial.chars().take(12).collect::<String>();
    // Wrap the diagnostic text across multiple lines so the SVG actually
    // shows it instead of overflowing the viewBox.
    let lines: Vec<String> = reason.split(" | ").map(|s| s.to_string()).collect();
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
