//! `/api/live_state/:serial` — runtime camera state via one Lua script
//! (mode, zoom, exposure counts, battery, image dir, free disk, focus,
//! flash, etc.). One PTP round-trip per call.

use crate::camera::{lua_scripts, CameraRegistry};
use crate::error::Result;
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Serialize, ToSchema)]
pub struct LiveStateDto {
    pub in_record: Option<bool>,
    pub is_movie: Option<bool>,
    pub mode_code: Option<i32>,
    pub zoom: Option<i32>,
    pub exp_count: Option<i32>,
    pub vbatt_mv: Option<i32>,
    pub image_dir: Option<String>,
    pub free_kb: Option<i32>,
    pub iso_mode: Option<i32>,
    pub sv96: Option<i32>,
    pub tv96: Option<i32>,
    pub av96: Option<i32>,
    pub focus: Option<i32>,
    pub propset: Option<i32>,
    pub flash_mode: Option<i32>,
    pub flash_ready: Option<bool>,
    pub is_shooting: Option<bool>,
    pub raw: String,
}

#[utoipa::path(
    get,
    path = "/api/live_state/{serial}",
    tag = "cameras",
    params(("serial" = String, description = "Camera USB serial")),
    responses(
        (status = 200, description = "Runtime camera state via a single on-camera Lua script.", body = LiveStateDto)
    ),
)]
pub async fn live_state(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
) -> Result<Json<LiveStateDto>> {
    let cam = reg.get_or_open(&serial).await?;
    let raw = cam.exec_lua_for_string(lua_scripts::LIVE_STATE, 8_000).await?;
    Ok(Json(parse_live_state(raw)))
}

fn parse_live_state(raw: String) -> LiveStateDto {
    let parts: Vec<&str> = raw.split('|').collect();
    let pi = |i: usize| -> Option<i32> { parts.get(i)?.parse().ok() };
    let pb = |i: usize| -> Option<bool> {
        let s = *parts.get(i)?;
        if s == "true" {
            Some(true)
        } else if s == "false" {
            Some(false)
        } else {
            None
        }
    };
    let ps = |i: usize| -> Option<String> {
        let s = *parts.get(i)?;
        if s == "?" || s == "nil" {
            None
        } else {
            Some(s.to_string())
        }
    };

    LiveStateDto {
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
    }
}
