//! `CameraRegistry` — caches open `Camera`s by serial so repeat requests
//! don't pay the ~50–100 ms session-open cost each time, and so the per-
//! camera mutex actually serializes correctly across requests.
//!
//! `Arc<Camera>` is the unit of sharing. The registry hands out clones;
//! when a Camera method hits a fatal error it asks the registry to drop
//! its cached Arc (via a `Weak<CameraRegistry>` stored in the Camera).

use crate::camera::camera::Camera;
use crate::error::{Error, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub struct CameraRegistry {
    cameras: Mutex<HashMap<String, Arc<Camera>>>,
}

impl CameraRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Look up the cached Camera for this serial, opening a new PTP session
    /// if none exists. Retries with backoff on "exclusive access" failures
    /// (macOS's `ptpcamerad` races us for the device; the Pi's `gvfs-gphoto2`
    /// can do the same).
    pub async fn get_or_open(self: &Arc<Self>, serial: &str) -> Result<Arc<Camera>> {
        if let Some(c) = self.cameras.lock().get(serial).cloned() {
            return Ok(c);
        }

        let cams = chdkptp::list_cameras().map_err(Error::from)?;
        let cam = cams
            .into_iter()
            .find(|c| c.serial() == Some(serial))
            .ok_or_else(|| Error::new(format!("camera with serial {serial} not found")))?;

        let weak = Arc::downgrade(self);
        let mut last_err: Option<Error> = None;
        for attempt in 0..5 {
            match cam.open_ptp().await {
                Ok(session) => {
                    let camera = Arc::new(Camera::new(serial.to_string(), session, weak.clone()));
                    self.cameras.lock().insert(serial.to_string(), camera.clone());
                    return Ok(camera);
                }
                Err(e) => {
                    let msg = e.to_string();
                    last_err = Some(Error::new(format!("attempt {}: {msg}", attempt + 1)));
                    if !msg.contains("exclusive access") {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(200 * (attempt as u64 + 1))).await;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| Error::new("open failed")))
    }

    /// Drop the cached Camera for this serial. Next `get_or_open` opens
    /// a fresh session.
    pub fn invalidate(&self, serial: &str) {
        self.cameras.lock().remove(serial);
    }

    /// All currently-attached cameras (this enumerates USB; the cache is
    /// for open sessions, not for the listing itself).
    pub fn list_attached(&self) -> Result<Vec<chdkptp::CameraInfo>> {
        chdkptp::list_cameras().map_err(Error::from)
    }
}
