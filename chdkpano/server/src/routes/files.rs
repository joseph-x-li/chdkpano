//! `/api/files/:serial` (list) + `/api/file/:serial` (download).

use crate::camera::CameraRegistry;
use crate::error::{Error, Result};
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

#[derive(Serialize, Clone, ToSchema)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Serialize, ToSchema)]
pub struct ListDirResponse {
    pub path: String,
    pub entries: Vec<DirEntry>,
    /// Set when the camera build can't list the directory (e.g. missing
    /// `os.listdir`). Falls back to well-known SD-root entries when `path=A`.
    pub note: Option<String>,
}

#[derive(Deserialize, IntoParams)]
pub struct PathQuery {
    /// Camera-side path. Restricted to `[A-Za-z0-9/._\-+ ]`, max 255 chars.
    /// Defaults to `A` (SD root) for /api/files; required for /api/file.
    pub path: Option<String>,
}

/// Only allow ASCII alphanumerics + a handful of path-safe punctuation —
/// keeps Lua string interpolation safe from injection and prevents
/// directory-traversal mischief.
fn is_safe_camera_path(p: &str) -> bool {
    !p.is_empty()
        && p.len() < 256
        && p.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '+' | ' ')
        })
}

fn well_known_sd_root_entries() -> Vec<DirEntry> {
    ["DCIM", "CHDK", "MISC", "CANONMSC"]
        .into_iter()
        .map(|n| DirEntry {
            name: n.to_string(),
            is_dir: true,
            size: 0,
        })
        .collect()
}

#[utoipa::path(
    get,
    path = "/api/files/{serial}",
    tag = "files",
    params(
        ("serial" = String, description = "Camera USB serial"),
        PathQuery,
    ),
    responses(
        (status = 200, description = "Directory listing via on-camera Lua (os.listdir + os.stat).", body = ListDirResponse)
    ),
)]
pub async fn list_files(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
    Query(q): Query<PathQuery>,
) -> Result<Json<ListDirResponse>> {
    let path = q.path.unwrap_or_else(|| "A".to_string());
    if !is_safe_camera_path(&path) {
        return Err(Error::new(format!("unsafe path: {path:?}")));
    }

    let cam = reg.get_or_open(&serial).await?;
    let raw = cam.list_dir_raw(&path).await?;

    if raw == "ERR_NOLIST" {
        return Ok(Json(ListDirResponse {
            path,
            entries: vec![],
            note: Some("camera build lacks os.listdir — directory browsing not available".into()),
        }));
    }
    if let Some(err) = raw.strip_prefix("ERR_LIST|") {
        // SD-root listing is unreliable on some CHDK builds — fall back to
        // the well-known Canon top-level directories so the user can still
        // navigate into them. Subdir listings work normally.
        if path == "A" || path == "A/" {
            return Ok(Json(ListDirResponse {
                path,
                entries: well_known_sd_root_entries(),
                note: None,
            }));
        }
        return Ok(Json(ListDirResponse {
            path,
            entries: vec![],
            note: Some(format!("listdir failed: {err}")),
        }));
    }

    let mut entries: Vec<DirEntry> = raw
        .split('\n')
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            // name:is_dir:size — name may contain ':' in pathological cases,
            // so rsplit twice keeps the name intact.
            let (head, size) = line.rsplit_once(':')?;
            let (name, is_dir) = head.rsplit_once(':')?;
            Some(DirEntry {
                name: name.to_string(),
                is_dir: is_dir == "1",
                size: size.parse().unwrap_or(0),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        // Directories first, then alpha (case-insensitive).
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(Json(ListDirResponse {
        path,
        entries,
        note: None,
    }))
}

#[utoipa::path(
    get,
    path = "/api/file/{serial}",
    tag = "files",
    params(
        ("serial" = String, description = "Camera USB serial"),
        PathQuery,
    ),
    responses(
        (status = 200, description = "File bytes with content-type guessed from extension (image/jpeg for .jpg, etc.).",
            content_type = "application/octet-stream", body = Vec<u8>)
    ),
)]
pub async fn get_file(
    State(reg): State<Arc<CameraRegistry>>,
    Path(serial): Path<String>,
    Query(q): Query<PathQuery>,
) -> Response {
    let path = match q.path {
        Some(p) if is_safe_camera_path(&p) => p,
        Some(p) => return Error::new(format!("unsafe path: {p:?}")).into_response(),
        None => return Error::new("missing ?path=").into_response(),
    };
    let cam = match reg.get_or_open(&serial).await {
        Ok(c) => c,
        Err(e) => return e.into_response(),
    };
    let data = match cam.download_file(&path).await {
        Ok(d) => d,
        Err(e) => return e.into_response(),
    };

    let ct = guess_content_type(&path);
    (
        [
            (header::CONTENT_TYPE, ct),
            (header::CACHE_CONTROL, "private, max-age=60"),
        ],
        data,
    )
        .into_response()
}

fn guess_content_type(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".cr2") || lower.ends_with(".crw") || lower.ends_with(".dng") {
        "image/x-canon-raw"
    } else if lower.ends_with(".mov") || lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".txt") || lower.ends_with(".log") || lower.ends_with(".lua") {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}
