//! panostitch — a tiny standalone web service that stitches a set of images
//! into a single panorama with a from-scratch (pure-Rust) pipeline.
//!
//! Intended to run on a beefier machine than the capture Pi (e.g. the ThinkPad):
//! the Pi shoots, POSTs its frames here, gets a panorama back.
//!
//! No settings are baked in yet — POST N images (>= 2) and it stitches them in
//! the order received, matching each onto the running panorama.

mod features;
mod homography;
mod stitch;
mod warp;

use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "panostitch=info,tower_http=info".into()),
        )
        .init();

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/stitch", post(stitch_handler))
        // Photos are big; allow generous uploads (default is 2 MB).
        .layer(DefaultBodyLimit::max(256 * 1024 * 1024))
        .layer(TraceLayer::new_for_http());

    let addr = std::env::var("PANOSTITCH_ADDR").unwrap_or_else(|_| "0.0.0.0:3040".into());
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    tracing::info!("panostitch listening on http://{addr}");
    axum::serve(listener, app).await.expect("serve");
}

/// `POST /stitch` — multipart form, one image per field (any field names).
/// Returns the stitched panorama as JPEG.
async fn stitch_handler(mut multipart: Multipart) -> Result<impl IntoResponse, ApiError> {
    let mut images = Vec::new();
    while let Some(field) = multipart.next_field().await? {
        let data = field.bytes().await?;
        let img = image::load_from_memory(&data)
            .map_err(|e| ApiError::bad(format!("could not decode an uploaded image: {e}")))?;
        images.push(img);
    }

    if images.len() < 2 {
        return Err(ApiError::bad(format!(
            "need at least 2 images to stitch, got {}",
            images.len()
        )));
    }
    let count = images.len();
    tracing::info!("stitching {count} images");

    // Stitching is CPU-bound; keep it off the async runtime threads.
    let pano = tokio::task::spawn_blocking(move || stitch::stitch(images))
        .await
        .map_err(|e| ApiError::internal(format!("stitch task panicked: {e}")))?
        .map_err(|e| ApiError::unprocessable(format!("stitch failed: {e}")))?;

    let mut buf = std::io::Cursor::new(Vec::new());
    pano.write_to(&mut buf, image::ImageOutputFormat::Jpeg(90))
        .map_err(|e| ApiError::internal(format!("encoding panorama failed: {e}")))?;

    tracing::info!("stitched {count} images -> {} bytes", buf.get_ref().len());
    Ok(([(header::CONTENT_TYPE, "image/jpeg")], buf.into_inner()))
}

/// Minimal error type that turns into an HTTP response.
struct ApiError {
    code: StatusCode,
    msg: String,
}

impl ApiError {
    fn bad(msg: String) -> Self {
        Self { code: StatusCode::BAD_REQUEST, msg }
    }
    fn unprocessable(msg: String) -> Self {
        Self { code: StatusCode::UNPROCESSABLE_ENTITY, msg }
    }
    fn internal(msg: String) -> Self {
        Self { code: StatusCode::INTERNAL_SERVER_ERROR, msg }
    }
}

impl From<axum::extract::multipart::MultipartError> for ApiError {
    fn from(e: axum::extract::multipart::MultipartError) -> Self {
        Self::bad(format!("multipart error: {e}"))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.code, self.msg).into_response()
    }
}
