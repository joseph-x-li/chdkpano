//! Application-wide error type.
//!
//! Everything that crosses an HTTP boundary returns `Result<_, Error>`.
//! `Error` is intentionally just a string under the hood — we don't need
//! taxonomic precision at the API layer, just a clean message to ship
//! back to the client and log.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, Clone)]
pub struct Error(pub String);

impl Error {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<chdkptp::Error> for Error {
    fn from(e: chdkptp::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0 })),
        )
            .into_response()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
