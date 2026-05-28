//! Static-file server with SPA fallback.
//!
//! Serves real trunk-emitted files (JS/WASM/CSS at absolute paths) when
//! they exist, and otherwise returns `index.html` with 200 OK so the
//! Leptos client-side router can take over on deep-route reloads
//! (`/api-docs`, `/camera/:serial`, …). Avoids ServeDir's "404 + index
//! body" quirk that confused browsers in an earlier iteration.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::OnceLock;

static STATIC_DIR: OnceLock<String> = OnceLock::new();

/// Call once at startup to record where trunk's `dist/` lives.
pub fn set_static_dir(dir: String) {
    STATIC_DIR.set(dir).expect("STATIC_DIR set twice");
}

pub async fn static_or_spa(uri: axum::http::Uri) -> Response {
    let dir = STATIC_DIR
        .get()
        .map(String::as_str)
        .unwrap_or("client/dist");
    let req_path = uri.path().trim_start_matches('/');

    // Tiny path-traversal guard — file lookups stay inside `dir`.
    if req_path.contains("..") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }

    if !req_path.is_empty() {
        let candidate = format!("{dir}/{req_path}");
        if let Ok(bytes) = tokio::fs::read(&candidate).await {
            return (
                [(header::CONTENT_TYPE, static_content_type(req_path))],
                bytes,
            )
                .into_response();
        }
    }

    match tokio::fs::read(format!("{dir}/index.html")).await {
        Ok(bytes) => (
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            bytes,
        )
            .into_response(),
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
