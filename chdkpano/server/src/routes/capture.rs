//! Capture & Stitch.
//!
//! `POST /api/pano/capture_stitch` fires the whole rig synchronously, pulls the
//! **full-resolution** frame from each camera that actually fired, forwards them
//! to the stitch service, and persists a timestamped capture folder:
//!
//!   <CHDKPANO_CAPTURE_DIR>/<id>/
//!     ├── slot0.jpg … slot3.jpg   (full-res, only for cameras that fired)
//!     ├── panorama.jpg            (if >= 2 frames stitched)
//!     └── manifest.json           (what follows)
//!
//! `CHDKPANO_CAPTURE_DIR` defaults to a `chdkpano-captures` folder in the system
//! temp dir. Nothing is auto-pruned. Result files are served back by
//! `GET /api/captures/{id}/{file}`.

use crate::pano::{PanoArray, SlotOutcome, CLOCKSYNC_LEAD_MS, CLOCKSYNC_OFFSET_SAMPLES};
use crate::routes::stitch::forward_to_stitch;
use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use utoipa::ToSchema;

fn capture_root() -> PathBuf {
    std::env::var("CHDKPANO_CAPTURE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("chdkpano-captures"))
}

/// Per-camera outcome of a capture.
#[derive(Serialize, ToSchema)]
pub struct CameraCaptureDto {
    pub slot: usize,
    pub serial: Option<String>,
    /// Whether the shutter actually actuated.
    pub fired: bool,
    /// Path of the file on the camera's SD card (if it fired).
    pub camera_path: Option<String>,
    /// Local filename inside the capture folder (e.g. "slot0.jpg"), if pulled.
    pub file: Option<String>,
    pub bytes: Option<usize>,
    pub error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct StitchResultDto {
    pub ok: bool,
    /// How many full-res frames were sent to the stitcher.
    pub inputs: usize,
    /// Local filename of the panorama (e.g. "panorama.jpg"), if stitched.
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct CaptureManifestDto {
    /// Capture id (also the folder name).
    pub id: String,
    /// Absolute path of the capture folder on the server.
    pub dir: String,
    pub cameras: Vec<CameraCaptureDto>,
    pub stitch: StitchResultDto,
}

#[utoipa::path(
    post,
    path = "/api/pano/capture_stitch",
    tag = "pano",
    responses(
        (status = 200, description = "Capture manifest: per-camera files + stitch result", body = CaptureManifestDto),
        (status = 500, description = "Could not create the capture folder"),
    ),
)]
pub async fn capture_stitch(State(pano): State<Arc<PanoArray>>) -> Response {
    let id = now_millis().to_string();
    let dir = capture_root().join(&id);
    if let Err(e) = tokio::fs::create_dir_all(&dir).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("could not create capture dir {}: {e}", dir.display()),
        )
            .into_response();
    }

    // 1. Fire the whole rig synchronously (per-slot report carries image_path).
    let report = pano
        .shoot_all_clocksync(CLOCKSYNC_LEAD_MS, CLOCKSYNC_OFFSET_SAMPLES, false)
        .await;
    // 2. Camera handles to download full-res from.
    let cams = pano.cameras().await;

    // 3. For each slot that fired, download its full-res frame to the folder.
    let mut cameras: Vec<CameraCaptureDto> = Vec::new();
    let mut images: Vec<Vec<u8>> = Vec::new();

    for slot in &report.slots {
        let idx = slot.idx;
        let fired = slot.fired.unwrap_or(false);
        let camera_path = slot.image_path.clone();

        let mut rec = CameraCaptureDto {
            slot: idx,
            serial: slot.serial.clone(),
            fired,
            camera_path: camera_path.clone(),
            file: None,
            bytes: None,
            error: None,
        };

        match (fired, &camera_path, cams.get(idx)) {
            (true, Some(path), Some(SlotOutcome::Ok(cam))) => match cam.download_file(path).await {
                Ok(bytes) => {
                    let fname = format!("slot{idx}.jpg");
                    match tokio::fs::write(dir.join(&fname), &bytes).await {
                        Ok(()) => {
                            rec.bytes = Some(bytes.len());
                            rec.file = Some(fname);
                            images.push(bytes);
                        }
                        Err(e) => rec.error = Some(format!("write failed: {e}")),
                    }
                }
                Err(e) => rec.error = Some(e.message().to_string()),
            },
            _ => {
                // Didn't fire / no file path / no live handle.
                rec.error = slot.error.clone().or_else(|| Some(slot.status.to_string()));
            }
        }
        cameras.push(rec);
    }

    // 4. Stitch the full-res frames (need >= 2).
    let inputs = images.len();
    let stitch = if inputs >= 2 {
        match forward_to_stitch(images).await {
            Ok(pano_bytes) => match tokio::fs::write(dir.join("panorama.jpg"), &pano_bytes).await {
                Ok(()) => StitchResultDto {
                    ok: true,
                    inputs,
                    result: Some("panorama.jpg".into()),
                    error: None,
                },
                Err(e) => StitchResultDto {
                    ok: false,
                    inputs,
                    result: None,
                    error: Some(format!("write panorama failed: {e}")),
                },
            },
            Err(e) => StitchResultDto { ok: false, inputs, result: None, error: Some(e) },
        }
    } else {
        StitchResultDto {
            ok: false,
            inputs,
            result: None,
            error: Some(format!(
                "only {inputs} camera frame(s) available; need >= 2 to stitch"
            )),
        }
    };

    let manifest = CaptureManifestDto {
        id,
        dir: dir.to_string_lossy().into_owned(),
        cameras,
        stitch,
    };

    // 5. Persist manifest.json alongside the images.
    if let Ok(json) = serde_json::to_vec_pretty(&manifest) {
        let _ = tokio::fs::write(dir.join("manifest.json"), json).await;
    }

    tracing::info!(
        "capture {} -> {} frames, stitch ok={}",
        manifest.id,
        manifest.stitch.inputs,
        manifest.stitch.ok
    );
    Json(manifest).into_response()
}

#[utoipa::path(
    get,
    path = "/api/captures/{id}/{file}",
    tag = "pano",
    params(
        ("id" = String, description = "Capture id"),
        ("file" = String, description = "File within the capture folder (panorama.jpg, slot0.jpg, manifest.json, …)"),
    ),
    responses((status = 200, description = "The requested capture file")),
)]
pub async fn get_capture_file(Path((id, file)): Path<(String, String)>) -> Response {
    if !safe_component(&id) || !safe_component(&file) {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }
    let path = capture_root().join(&id).join(&file);
    match tokio::fs::read(&path).await {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, content_type(&file)),
                (header::CACHE_CONTROL, "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Reject anything that could escape the capture folder (path traversal).
fn safe_component(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains('\0')
}

fn content_type(file: &str) -> &'static str {
    let lower = file.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    }
}
