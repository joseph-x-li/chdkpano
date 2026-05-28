//! chdkpano-server: axum backend that wraps the chdkptp library and serves
//! the WASM frontend. See `app.rs` for routing, `camera/` for the Camera
//! abstraction, `pano.rs` for the multi-camera rig, and `routes/` for the
//! HTTP handlers.

mod app;
mod camera;
mod error;
mod openapi;
mod pano;
mod routes;
mod static_files;

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "chdkpano_server=info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let static_dir = std::env::var("CHDKPANO_STATIC_DIR").unwrap_or_else(|_| {
        for candidate in ["client/dist", "../client/dist", "../../client/dist"] {
            if std::path::Path::new(candidate).join("index.html").is_file() {
                return candidate.to_string();
            }
        }
        eprintln!(
            "WARNING: could not find client/dist/index.html. \
             Did you run `trunk build` in the client crate? \
             Set CHDKPANO_STATIC_DIR to override."
        );
        "client/dist".into()
    });
    static_files::set_static_dir(static_dir.clone());

    let state = app::AppState::new();
    let app = app::router(state);

    let addr = std::env::var("CHDKPANO_ADDR").unwrap_or_else(|_| "0.0.0.0:3030".into());
    tracing::info!("chdkpano-server listening on http://{addr}  (static: {static_dir})");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
