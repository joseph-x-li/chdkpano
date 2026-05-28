//! Camera-level endpoints: list, info, mode switch.

use crate::camera::CameraRegistry;
use crate::error::{Error, Result};
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

// ---------- /api/cameras ----------

#[derive(Serialize, ToSchema)]
pub struct CameraDto {
    pub serial: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_number: u8,
    pub device_address: u8,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
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

#[utoipa::path(
    get,
    path = "/api/cameras",
    tag = "cameras",
    responses(
        (status = 200, description = "All Canon devices currently enumerated over USB.", body = [CameraDto])
    ),
)]
pub async fn list_cameras(State(reg): State<Arc<CameraRegistry>>) -> Result<Json<Vec<CameraDto>>> {
    let cams = reg.list_attached()?;
    Ok(Json(cams.iter().map(CameraDto::from).collect()))
}

// ---------- /api/info/:serial ----------

#[derive(Serialize, ToSchema)]
pub struct InfoDto {
    // USB
    pub serial: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bus_number: u8,
    pub device_address: u8,
    pub usb_manufacturer: Option<String>,
    pub usb_product: Option<String>,
    // PTP DeviceInfo
    pub ptp_standard_version: u16,
    pub vendor_extension_id: u32,
    pub vendor_extension_version: u16,
    pub vendor_extension_desc: String,
    pub functional_mode: u16,
    pub ptp_manufacturer: String,
    pub ptp_model: String,
    pub device_version: String,
    pub serial_number: String,
    pub operations_supported: Vec<u16>,
    pub events_supported: Vec<u16>,
    pub device_properties_supported: Vec<u16>,
    pub capture_formats: Vec<u16>,
    pub image_formats: Vec<u16>,
    pub chdk_advertised: bool,
    pub chdk_version_major: Option<u32>,
    pub chdk_version_minor: Option<u32>,
}

#[utoipa::path(
    get,
    path = "/api/info/{serial}",
    tag = "cameras",
    params(
        ("serial" = String, Path, description = "Camera USB serial (from /api/cameras)")
    ),
    responses(
        (status = 200, description = "Full PTP DeviceInfo + CHDK version.", body = InfoDto),
        (status = 500, description = "Camera not found, USB claim race, or PTP failure.")
    ),
)]
pub async fn camera_info(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
) -> Result<Json<InfoDto>> {
    // USB-level metadata comes from the enumeration, not the PTP session.
    let cams = reg.list_attached()?;
    let usb = cams
        .iter()
        .find(|c| c.serial() == Some(&serial))
        .ok_or_else(|| Error::new(format!("camera with serial {serial} not found")))?
        .clone();

    let cam = reg.get_or_open(&serial).await?;
    let info = cam.device_info().await?;
    let chdk_advertised = info.has_chdk();
    let (cmaj, cmin) = if chdk_advertised {
        cam.chdk_version()
            .await
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(Json(InfoDto {
        serial: serial.clone(),
        vendor_id: usb.vendor_id(),
        product_id: usb.product_id(),
        bus_number: usb.bus_number(),
        device_address: usb.device_address(),
        usb_manufacturer: usb.manufacturer().map(String::from),
        usb_product: usb.product().map(String::from),
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

// ---------- /api/mode/{record,play}/:serial ----------

#[derive(Serialize, ToSchema)]
pub struct ModeResponse {
    /// "record" or "play" — the camera's mode after the switch.
    pub mode: String,
}

#[utoipa::path(
    post,
    path = "/api/mode/record/{serial}",
    tag = "cameras",
    params(("serial" = String, description = "Camera USB serial")),
    responses((status = 200, body = ModeResponse)),
)]
pub async fn mode_record(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
) -> Result<Json<ModeResponse>> {
    let cam = reg.get_or_open(&serial).await?;
    let mode = cam.switch_mode(1).await?;
    Ok(Json(ModeResponse { mode }))
}

#[utoipa::path(
    post,
    path = "/api/mode/play/{serial}",
    tag = "cameras",
    params(("serial" = String, description = "Camera USB serial")),
    responses((status = 200, body = ModeResponse)),
)]
pub async fn mode_play(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
) -> Result<Json<ModeResponse>> {
    let cam = reg.get_or_open(&serial).await?;
    let mode = cam.switch_mode(0).await?;
    Ok(Json(ModeResponse { mode }))
}
