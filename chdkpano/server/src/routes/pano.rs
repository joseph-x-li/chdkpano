//! `/api/pano/*` — the panorama rig (4-camera array).
//!
//! Routes:
//!   GET    /api/pano/state              — current slot assignments + camera presence
//!   POST   /api/pano/autofill           — auto-assign every attached camera
//!   PUT    /api/pano/slot/:idx          — { "serial": "..." } or { "serial": null }
//!   POST   /api/pano/shoot              — naive parallel shoot
//!   POST   /api/pano/shoot_synced       — clock-synced shoot
//!   GET    /api/pano/viewport/:idx      — viewport JPEG from one slot

use crate::pano::{PanoArray, SlotOutcome, SLOT_COUNT};
use crate::routes::viewport::placeholder_svg;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, ToSchema)]
pub struct SlotDto {
    pub idx: usize,
    pub serial: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct StateDto {
    pub slots: Vec<SlotDto>,
}

#[utoipa::path(
    get,
    path = "/api/pano/state",
    tag = "pano",
    responses((status = 200, body = StateDto)),
)]
pub async fn get_state(State(pano): State<Arc<PanoArray>>) -> Json<StateDto> {
    let snap = pano.snapshot();
    Json(StateDto {
        slots: snap
            .into_iter()
            .enumerate()
            .map(|(idx, serial)| SlotDto { idx, serial })
            .collect(),
    })
}

#[derive(Deserialize, ToSchema)]
pub struct AssignBody {
    /// Camera serial to put in this slot, or `null` to clear.
    pub serial: Option<String>,
}

#[utoipa::path(
    put,
    path = "/api/pano/slot/{idx}",
    tag = "pano",
    params(("idx" = usize, description = "Slot index 0..4")),
    request_body = AssignBody,
    responses((status = 200, body = StateDto)),
)]
pub async fn assign_slot(
    State(pano): State<Arc<PanoArray>>,
    Path(idx): Path<usize>,
    Json(body): Json<AssignBody>,
) -> Result<Json<StateDto>, crate::error::Error> {
    pano.assign(idx, body.serial)?;
    Ok(get_state(State(pano)).await)
}

#[utoipa::path(
    post,
    path = "/api/pano/autofill",
    tag = "pano",
    responses((status = 200, description = "Assigns every attached camera into the first free slot.", body = StateDto)),
)]
pub async fn autofill(
    State(pano): State<Arc<PanoArray>>,
) -> Result<Json<StateDto>, crate::error::Error> {
    pano.autofill()?;
    Ok(get_state(State(pano)).await)
}

#[derive(Serialize, ToSchema)]
pub struct ShootResultDto {
    pub idx: usize,
    pub status: &'static str, // "ok" | "err" | "empty"
    pub error: Option<String>,
}

fn results_to_dto(results: [SlotOutcome<()>; SLOT_COUNT]) -> Vec<ShootResultDto> {
    results
        .into_iter()
        .enumerate()
        .map(|(idx, o)| match o {
            SlotOutcome::Empty => ShootResultDto {
                idx,
                status: "empty",
                error: None,
            },
            SlotOutcome::Ok(()) => ShootResultDto {
                idx,
                status: "ok",
                error: None,
            },
            SlotOutcome::Err(e) => ShootResultDto {
                idx,
                status: "err",
                error: Some(e.message().to_string()),
            },
        })
        .collect()
}

#[derive(Serialize, ToSchema)]
pub struct ShootResponse {
    pub results: Vec<ShootResultDto>,
    pub elapsed_ms: u64,
    pub mode: String,
}

#[utoipa::path(
    post,
    path = "/api/pano/shoot",
    tag = "pano",
    responses((status = 200, description = "Naive parallel shoot — each camera receives shoot() concurrently. Skew ~50–200 ms.", body = ShootResponse)),
)]
pub async fn shoot(State(pano): State<Arc<PanoArray>>) -> Json<ShootResponse> {
    let t = std::time::Instant::now();
    let results = pano.shoot_all().await;
    Json(ShootResponse {
        results: results_to_dto(results),
        elapsed_ms: t.elapsed().as_millis() as u64,
        mode: "parallel".into(),
    })
}

#[derive(Deserialize, Default, IntoParams)]
pub struct ShootSyncedQuery {
    /// Milliseconds ahead the target deadline is scheduled. 500 is enough
    /// to overcome PTP round-trip jitter on USB 2.0 hubs.
    pub lead_ms: Option<i64>,
}

#[utoipa::path(
    post,
    path = "/api/pano/shoot_synced",
    tag = "pano",
    params(ShootSyncedQuery),
    responses((status = 200, description = "Clock-calibrated synchronized shoot. Per-camera skew ~5–20 ms.", body = ShootResponse)),
)]
pub async fn shoot_synced(
    State(pano): State<Arc<PanoArray>>,
    axum::extract::Query(q): axum::extract::Query<ShootSyncedQuery>,
) -> Result<Json<ShootResponse>, crate::error::Error> {
    let t = std::time::Instant::now();
    let lead = q.lead_ms.unwrap_or(500);
    let results = pano.shoot_all_synced(lead).await?;
    Ok(Json(ShootResponse {
        results: results_to_dto(results),
        elapsed_ms: t.elapsed().as_millis() as u64,
        mode: "clock-synced".into(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/pano/viewport/{idx}",
    tag = "pano",
    params(("idx" = usize, description = "Slot index 0..4")),
    responses(
        (status = 200, content_type = "image/jpeg", body = Vec<u8>),
        (status = 200, content_type = "image/svg+xml", body = String, description = "Placeholder when slot is empty or camera errored"),
    ),
)]
pub async fn viewport_slot(
    State(pano): State<Arc<PanoArray>>,
    Path(idx): Path<usize>,
) -> Response {
    if idx >= SLOT_COUNT {
        return placeholder_svg(&format!("slot {idx}"), "out of range").into_response();
    }
    let grid = pano.viewport_grid().await;
    let outcome = grid.into_iter().nth(idx).unwrap();
    match outcome {
        SlotOutcome::Empty => placeholder_svg(&format!("slot {idx}"), "empty slot").into_response(),
        SlotOutcome::Err(e) => placeholder_svg(&format!("slot {idx}"), e.message()).into_response(),
        SlotOutcome::Ok(jpeg) => (
            [
                (header::CONTENT_TYPE, "image/jpeg"),
                (header::CACHE_CONTROL, "no-store, no-cache, must-revalidate"),
            ],
            jpeg,
        )
            .into_response(),
    }
}
