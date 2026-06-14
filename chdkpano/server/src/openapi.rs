//! OpenAPI document collector.
//!
//! Every handler annotated with `#[utoipa::path(...)]` is listed under
//! `paths(...)` below. Every `ToSchema`-derived DTO is listed under
//! `components(schemas(...))`. Missing one is a compile error — that's
//! the whole point of using utoipa instead of hand-rolling a doc page.

use utoipa::OpenApi;

use crate::routes::{cameras, capture, exec, files, live_state, pano, stitch, viewport, wifi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "chdkpano",
        version = "0.1.0",
        description = "HTTP API for chdkpano — Canon CHDK panorama rig.",
    ),
    tags(
        (name = "cameras", description = "Single-camera enumeration, info, mode-switching"),
        (name = "viewport", description = "Live-view JPEG frames"),
        (name = "lua",      description = "Arbitrary on-camera Lua execution"),
        (name = "files",    description = "SD-card file browser and downloads"),
        (name = "pano",     description = "Four-camera panorama rig: slots, sync shoot, viewport grid"),
        (name = "wifi",     description = "Radio status (AP + client) and client-network reconfigure"),
    ),
    paths(
        cameras::list_cameras,
        cameras::camera_info,
        cameras::mode_record,
        cameras::mode_play,
        viewport::viewport_jpeg,
        live_state::live_state,
        exec::exec_lua,
        files::list_files,
        files::get_file,
        pano::get_state,
        pano::assign_slot,
        pano::autofill,
        pano::shoot_clocksync,
        pano::viewport_slot,
        capture::capture_stitch,
        capture::get_capture_file,
        stitch::stitch,
        wifi::wifi_status,
        wifi::set_client,
    ),
    components(schemas(
        cameras::CameraDto,
        cameras::InfoDto,
        cameras::ModeResponse,
        live_state::LiveStateDto,
        exec::ExecRequest,
        exec::ExecResponse,
        exec::MessageDto,
        exec::ValueDto,
        files::DirEntry,
        files::ListDirResponse,
        pano::SlotDto,
        pano::StateDto,
        pano::AssignBody,
        pano::ClockSyncSlotDto,
        pano::ClockSyncReportDto,
        capture::CameraCaptureDto,
        capture::StitchResultDto,
        capture::CaptureManifestDto,
        wifi::ApInfoDto,
        wifi::ClientInfoDto,
        wifi::WifiStatusDto,
        wifi::SetClientBody,
        wifi::SetClientResultDto,
    )),
)]
pub struct ApiDoc;
