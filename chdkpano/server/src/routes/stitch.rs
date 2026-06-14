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

pub(crate) fn stitch_base_url() -> String {
    std::env::var("CHDKPANO_STITCH_URL").unwrap_or_else(|_| "http://localhost:3040".into())
}

/// Forward a set of JPEG images to the panostitch service and return the
/// stitched panorama bytes. Shared by `/api/stitch` (viewport frames) and
/// `/api/pano/capture_stitch` (full-res frames). Err is a human message.
pub(crate) async fn forward_to_stitch(images: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut form = reqwest::multipart::Form::new();
    for (i, jpeg) in images.into_iter().enumerate() {
        let part = reqwest::multipart::Part::bytes(jpeg)
            .file_name(format!("img{i}.jpg"))
            .mime_str("image/jpeg")
            .map_err(|e| format!("mime: {e}"))?;
        form = form.part(format!("img{i}"), part);
    }

    let url = format!("{}/stitch", stitch_base_url().trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300)) // emulated stitching can be slow
        .build()
        .map_err(|e| format!("http client: {e}"))?;

    let resp = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("stitch service unreachable at {url}: {e}"))?;

    let status = resp.status();
    let body = resp
        .bytes()
        .await
        .map_err(|e| format!("reading stitch response failed: {e}"))?;

    if !status.is_success() {
        return Err(format!(
            "stitch service returned {}: {}",
            status,
            String::from_utf8_lossy(&body)
        ));
    }
    Ok(body.to_vec())
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

    // 2. Forward to the stitch service and relay the panorama back.
    match forward_to_stitch(frames).await {
        Ok(body) => {
            tracing::info!("stitched {n} frames -> {} bytes", body.len());
            (
                [
                    (header::CONTENT_TYPE, "image/jpeg"),
                    (header::CACHE_CONTROL, "no-store"),
                ],
                body,
            )
                .into_response()
        }
        Err(e) => (StatusCode::BAD_GATEWAY, e).into_response(),
    }
}
