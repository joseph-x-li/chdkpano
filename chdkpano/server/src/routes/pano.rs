//! `/api/pano/*` — the panorama rig (4-camera array).
//!
//! Routes:
//!   GET    /api/pano/state              — current slot assignments + camera presence
//!   POST   /api/pano/autofill           — auto-assign every attached camera
//!   PUT    /api/pano/slot/:idx          — { "serial": "..." } or { "serial": null }
//!   POST   /api/pano/shoot              — naive parallel shoot
//!   POST   /api/pano/shoot_synced       — clock-synced shoot
//!   GET    /api/pano/viewport/:idx      — viewport JPEG from one slot

use crate::pano::{
    ClockSyncReport, ClockSyncSlot, PanoArray, SlotOutcome, CLOCKSYNC_LEAD_MS,
    CLOCKSYNC_OFFSET_SAMPLES, SLOT_COUNT,
};
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

// ─── Clock-synced shoot (the sophisticated one) ────────────────────────

#[derive(Serialize, ToSchema)]
pub struct ClockSyncSlotDto {
    pub idx: usize,
    pub serial: Option<String>,
    /// "empty" | "fired" | "missed" | "err"
    pub status: String,
    pub offset_ms: Option<f64>,
    pub offset_rtt_ms: Option<f64>,
    pub target_tick: Option<i64>,
    pub busy_wait_ms: Option<i64>,
    pub actual_exit_host_ms: Option<f64>,
    pub overshoot_ms: Option<f64>,
    pub fired: Option<bool>,
    /// Camera path of the file this shot wrote (e.g. `A/DCIM/100CANON/IMG_0123.JPG`).
    pub image_path: Option<String>,
    pub error: Option<String>,
}

impl From<ClockSyncSlot> for ClockSyncSlotDto {
    fn from(s: ClockSyncSlot) -> Self {
        Self {
            idx: s.idx,
            serial: s.serial,
            status: s.status.to_string(),
            offset_ms: s.offset_ms,
            offset_rtt_ms: s.offset_rtt_ms,
            target_tick: s.target_tick,
            busy_wait_ms: s.busy_wait_ms,
            actual_exit_host_ms: s.actual_exit_host_ms,
            overshoot_ms: s.overshoot_ms,
            fired: s.fired,
            image_path: s.image_path,
            error: s.error,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ClockSyncReportDto {
    pub slots: Vec<ClockSyncSlotDto>,
    /// Spread of busy-wait exits across cameras (host ms) — the headline
    /// "how synchronized was it" number.
    pub inter_camera_skew_ms: Option<f64>,
    pub target_host_ms: f64,
    pub lead_ms: f64,
    pub samples: usize,
    pub elapsed_ms: u64,
}

#[derive(Deserialize, Default, IntoParams)]
pub struct ClockSyncQuery {
    /// Lead before the synchronized shot fires (ms). Default 2500 — must
    /// exceed the slowest camera's warmup.
    pub lead_ms: Option<f64>,
    /// NTP-style offset probes per camera (best RTT wins). Default 20.
    pub samples: Option<usize>,
    /// Force the flash on for the shot. Default false (flash off).
    pub flash: Option<bool>,
}

#[utoipa::path(
    post,
    path = "/api/pano/shoot_clocksync",
    tag = "pano",
    params(ClockSyncQuery),
    responses((status = 200, description = "Full clock-synced shoot: combined warmup→busy-wait→fire script per camera, with per-camera skew diagnostics.", body = ClockSyncReportDto)),
)]
pub async fn shoot_clocksync(
    State(pano): State<Arc<PanoArray>>,
    axum::extract::Query(q): axum::extract::Query<ClockSyncQuery>,
) -> Json<ClockSyncReportDto> {
    let t = std::time::Instant::now();
    let lead = q.lead_ms.unwrap_or(CLOCKSYNC_LEAD_MS);
    let samples = q.samples.unwrap_or(CLOCKSYNC_OFFSET_SAMPLES);
    let flash = q.flash.unwrap_or(false);
    let report: ClockSyncReport = pano.shoot_all_clocksync(lead, samples, flash).await;
    Json(ClockSyncReportDto {
        slots: report.slots.into_iter().map(Into::into).collect(),
        inter_camera_skew_ms: report.inter_camera_skew_ms,
        target_host_ms: report.target_host_ms,
        lead_ms: report.lead_ms,
        samples: report.samples,
        elapsed_ms: t.elapsed().as_millis() as u64,
    })
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
