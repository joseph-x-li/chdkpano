//! `/api/exec/:serial` — POST arbitrary Lua to a camera; returns every
//! message produced (Return / Error / User), with timing.

use crate::camera::CameraRegistry;
use crate::error::Result;
use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ExecRequest {
    /// Lua source — runs on the camera via ExecuteScript with a 20s timeout.
    pub source: String,
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessageDto {
    Return { value: ValueDto },
    Error { category: String, text: String },
    User { value: ValueDto },
}

#[derive(Serialize, ToSchema)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ValueDto {
    Nil,
    Boolean(bool),
    Integer(i32),
    String(String),
    Table(String),
    Unsupported,
}

impl From<&chdkptp::chdk::ScriptValue> for ValueDto {
    fn from(v: &chdkptp::chdk::ScriptValue) -> Self {
        use chdkptp::chdk::ScriptValue;
        match v {
            ScriptValue::Nil => ValueDto::Nil,
            ScriptValue::Boolean(b) => ValueDto::Boolean(*b),
            ScriptValue::Integer(i) => ValueDto::Integer(*i),
            ScriptValue::String(s) => ValueDto::String(s.clone()),
            ScriptValue::Table(s) => ValueDto::Table(s.clone()),
            ScriptValue::Unsupported => ValueDto::Unsupported,
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct ExecResponse {
    pub messages: Vec<MessageDto>,
    pub elapsed_ms: u64,
}

#[utoipa::path(
    post,
    path = "/api/exec/{serial}",
    tag = "lua",
    params(("serial" = String, description = "Camera USB serial")),
    request_body = ExecRequest,
    responses(
        (status = 200, description = "All messages produced by the script (return, errors, user prints) + elapsed time.", body = ExecResponse)
    ),
)]
pub async fn exec_lua(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
    Json(req): Json<ExecRequest>,
) -> Result<Json<ExecResponse>> {
    let cam = reg.get_or_open(&serial).await?;
    let t = std::time::Instant::now();
    let msgs = cam.exec_lua(&req.source, 20_000).await?;
    let elapsed_ms = t.elapsed().as_millis() as u64;

    let messages: Vec<MessageDto> = msgs
        .iter()
        .filter_map(|m| match m {
            chdkptp::chdk::ScriptMsg::None => None,
            chdkptp::chdk::ScriptMsg::Return { value, .. } => Some(MessageDto::Return {
                value: value.into(),
            }),
            chdkptp::chdk::ScriptMsg::Error { category, text, .. } => Some(MessageDto::Error {
                category: format!("{category:?}"),
                text: text.clone(),
            }),
            chdkptp::chdk::ScriptMsg::User { value, .. } => Some(MessageDto::User {
                value: value.into(),
            }),
        })
        .collect();

    Ok(Json(ExecResponse {
        messages,
        elapsed_ms,
    }))
}
