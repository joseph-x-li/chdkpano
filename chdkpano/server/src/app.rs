//! AppState + Router builder.
//!
//! `AppState` holds the two long-lived Arcs (registry + pano). axum's
//! `FromRef` lets each route declare which one(s) it needs via `State<_>`,
//! so handlers don't all take the whole AppState.

use crate::camera::CameraRegistry;
use crate::openapi::ApiDoc;
use crate::pano::PanoArray;
use crate::routes;
use crate::static_files::static_or_spa;
use axum::extract::FromRef;
use axum::routing::{get, post, put};
use axum::Router;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Clone)]
pub struct AppState {
    pub registry: Arc<CameraRegistry>,
    pub pano: Arc<PanoArray>,
}

impl AppState {
    pub fn new() -> Self {
        let registry = CameraRegistry::new();
        let pano = PanoArray::new(registry.clone());
        Self { registry, pano }
    }
}

impl FromRef<AppState> for Arc<CameraRegistry> {
    fn from_ref(s: &AppState) -> Self {
        s.registry.clone()
    }
}

impl FromRef<AppState> for Arc<PanoArray> {
    fn from_ref(s: &AppState) -> Self {
        s.pano.clone()
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        // ---- OpenAPI doc + Swagger UI ----
        // The merge() consumes a Router (already stateless), so we attach
        // it before adding our stateful routes.
        .merge(
            SwaggerUi::new("/swagger-ui")
                .url("/api/openapi.json", ApiDoc::openapi()),
        )
        // ---- single-camera endpoints ----
        .route("/api/cameras", get(routes::cameras::list_cameras))
        .route("/api/info/:serial", get(routes::cameras::camera_info))
        .route("/api/mode/record/:serial", post(routes::cameras::mode_record))
        .route("/api/mode/play/:serial", post(routes::cameras::mode_play))
        .route("/api/viewport/:serial", get(routes::viewport::viewport_jpeg))
        .route("/api/live_state/:serial", get(routes::live_state::live_state))
        .route("/api/exec/:serial", post(routes::exec::exec_lua))
        .route("/api/files/:serial", get(routes::files::list_files))
        .route("/api/file/:serial", get(routes::files::get_file))
        // ---- pano rig endpoints ----
        .route("/api/pano/state", get(routes::pano::get_state))
        .route("/api/pano/autofill", post(routes::pano::autofill))
        .route("/api/pano/slot/:idx", put(routes::pano::assign_slot))
        .route("/api/pano/shoot_clocksync", post(routes::pano::shoot_clocksync))
        .route("/api/pano/viewport/:idx", get(routes::pano::viewport_slot))
        // grab a frame per slot, hand off to the panostitch service, return the pano
        .route("/api/stitch", post(routes::stitch::stitch))
        // ---- wifi (radio status + client reconfigure) ----
        .route("/api/wifi", get(routes::wifi::wifi_status))
        .route("/api/wifi/client", post(routes::wifi::set_client))
        // ---- wiring ----
        .with_state(state)
        // SPA fallback: real files where they exist, else index.html + 200
        // so the WASM router takes over on deep-route reloads.
        .fallback(static_or_spa)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
}
