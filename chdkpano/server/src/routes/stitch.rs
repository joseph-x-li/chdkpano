//! Pi → stitch-service handoff.
//!
//! The Pi is a capture/control node; panorama stitching is heavy (CPU + RAM)
//! and runs on a beefier box — the standalone `panostitch` service, e.g. on the
//! ThinkPad. This route grabs a viewport frame from each rig slot, POSTs them to
//! that service as multipart, and streams the resulting panorama JPEG back.
//!
//! The target comes from `CHDKPANO_STITCH_URL` (default `http://localhost:3040`)
//! so nothing host-specific is baked in; the NixOS module points it at the
//! stitch host.

use crate::pano::PanoArray;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use std::sync::Arc;
use std::time::Duration;

fn stitch_base_url() -> String {
    std::env::var("CHDKPANO_STITCH_URL").unwrap_or_else(|_| "http://localhost:3040".into())
}

#[utoipa::path(
    post,
    path = "/api/stitch",
    tag = "pano",
    responses(
        (status = 200, content_type = "image/jpeg", body = Vec<u8>, description = "Stitched panorama"),
        (status = 422, description = "Fewer than two slots produced a usable frame"),
        (status = 502, description = "Stitch service unreachable or returned an error"),
    ),
)]
pub async fn stitch(State(pano): State<Arc<PanoArray>>) -> Response {
    // 1. Grab a frame from every slot; keep only the ones that produced a JPEG
    //    (empty / errored slots are skipped).
    let frames: Vec<Vec<u8>> = pano
        .viewport_grid()
        .await
        .into_iter()
        .filter_map(|o| o.ok())
        .collect();

    if frames.len() < 2 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("need >= 2 usable camera frames to stitch, got {}", frames.len()),
        )
            .into_response();
    }
    let n = frames.len();

    // 2. Build a multipart body — one image part per frame.
    let mut form = reqwest::multipart::Form::new();
    for (i, jpeg) in frames.into_iter().enumerate() {
        let part = reqwest::multipart::Part::bytes(jpeg)
            .file_name(format!("slot{i}.jpg"))
            .mime_str("image/jpeg")
            .expect("image/jpeg is a valid mime type");
        form = form.part(format!("slot{i}"), part);
    }

    // 3. Forward to the stitch service and relay the response.
    let url = format!("{}/stitch", stitch_base_url().trim_end_matches('/'));
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(300)) // emulated stitching can be slow
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("http client: {e}")).into_response()
        }
    };

    let resp = match client.post(&url).multipart(form).send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("stitch service unreachable at {url}: {e}"),
            )
                .into_response()
        }
    };

    let status = resp.status();
    let body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("reading stitch response failed: {e}"),
            )
                .into_response()
        }
    };

    if !status.is_success() {
        return (
            StatusCode::BAD_GATEWAY,
            format!(
                "stitch service returned {}: {}",
                status,
                String::from_utf8_lossy(&body)
            ),
        )
            .into_response();
    }

    tracing::info!("stitched {n} frames via {url} -> {} bytes", body.len());
    (
        [
            (header::CONTENT_TYPE, "image/jpeg"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body.to_vec(),
    )
        .into_response()
}
