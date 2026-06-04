//! Single-camera abstraction.
//!
//! `Camera` owns a `PtpSession` behind a tokio Mutex (so we can hold it
//! across `.await`). Every USB-touching method:
//!   1. acquires the session lock
//!   2. runs the chdkptp call
//!   3. on error → drops the lock, asks the registry to forget this Camera,
//!      and returns the error to the caller
//!
//! Self-invalidation uses a `Weak<CameraRegistry>` to avoid an Arc cycle.

use crate::camera::lua_scripts;
use crate::camera::registry::CameraRegistry;
use crate::error::{Error, Result};
use chdkptp::chdk::liveview::LV_TFR_VIEWPORT;
use chdkptp::chdk::{ScriptMsg, ScriptValue};
use chdkptp::{DeviceInfo, PtpSession};
use image::{codecs::jpeg::JpegEncoder, ColorType};
use std::sync::Weak;
use tokio::sync::Mutex;

pub struct Camera {
    serial: String,
    session: Mutex<PtpSession>,
    /// Weak ref so the Camera can ask the registry to drop its cached Arc
    /// after a fatal error, without forming a cycle.
    registry: Weak<CameraRegistry>,
}

impl Camera {
    pub(crate) fn new(serial: String, session: PtpSession, registry: Weak<CameraRegistry>) -> Self {
        Self {
            serial,
            session: Mutex::new(session),
            registry,
        }
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Drop ourselves from the registry. Next get_or_open(serial) will
    /// open a fresh session. Called on any session-fatal error.
    fn invalidate_self(&self) {
        if let Some(reg) = self.registry.upgrade() {
            reg.invalidate(&self.serial);
        }
    }

    // ---------- PTP DeviceInfo + CHDK version ----------

    pub async fn device_info(&self) -> Result<DeviceInfo> {
        let mut s = self.session.lock().await;
        match s.get_device_info().await {
            Ok(v) => Ok(v),
            Err(e) => {
                drop(s);
                self.invalidate_self();
                Err(e.into())
            }
        }
    }

    /// `(major, minor)` or `None` if the camera doesn't advertise CHDK.
    pub async fn chdk_version(&self) -> Option<(u32, u32)> {
        let mut s = self.session.lock().await;
        s.chdk_version().await.ok().map(|v| (v.major, v.minor))
    }

    // ---------- Live viewport ----------

    /// One viewport frame, JPEG-encoded at q=80. The CHDK live buffer is an
    /// anamorphic ~720×240 (non-square pixels meant to display at 4:3); we
    /// encode it as-is and let the client correct the aspect.
    pub async fn viewport_jpeg(&self) -> Result<Vec<u8>> {
        let mut s = self.session.lock().await;
        let frame = match s.get_display_data(LV_TFR_VIEWPORT).await {
            Ok(f) => f,
            Err(e) => {
                drop(s);
                self.invalidate_self();
                return Err(Error::new(format!("get_display_data: {e}")));
            }
        };
        // Decode is CPU-bound and doesn't touch USB — release the lock first.
        drop(s);

        let (width, height, rgb) = frame
            .decode_viewport_rgb()
            .map_err(|e| Error::new(e.to_string()))?;

        let mut jpeg = Vec::with_capacity((width * height) as usize);
        let mut enc = JpegEncoder::new_with_quality(&mut jpeg, 80);
        enc.encode(&rgb, width, height, ColorType::Rgb8.into())
            .map_err(|e| Error::new(format!("jpeg encode: {e}")))?;
        Ok(jpeg)
    }

    // ---------- Lua execution ----------

    /// Run a Lua script via ExecuteScript and wait for completion.
    /// Returns every message the camera produced (return value, errors, user prints).
    pub async fn exec_lua(&self, source: &str, timeout_ms: u64) -> Result<Vec<ScriptMsg>> {
        let mut s = self.session.lock().await;
        match s.execute_script_wait(source, timeout_ms).await {
            Ok(v) => Ok(v),
            Err(e) => {
                drop(s);
                self.invalidate_self();
                Err(e.into())
            }
        }
    }

    /// Convenience: run Lua, return just the first `Return { String(...) }`
    /// message. Most internal scripts return one stringified result.
    pub async fn exec_lua_for_string(&self, source: &str, timeout_ms: u64) -> Result<String> {
        let msgs = self.exec_lua(source, timeout_ms).await?;
        Ok(msgs
            .iter()
            .find_map(|m| match m {
                ScriptMsg::Return {
                    value: ScriptValue::String(s),
                    ..
                } => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default())
    }

    // ---------- Mode switching ----------

    /// `target = 1` → record (lens extends, viewfinder pipeline on).
    /// `target = 0` → play (lens retracts, gallery).
    pub async fn switch_mode(&self, target: u32) -> Result<String> {
        let lua = lua_scripts::switch_mode(target);
        let msgs = self.exec_lua(&lua, 15_000).await?;
        for m in &msgs {
            if let ScriptMsg::Error { text, .. } = m {
                return Err(Error::new(format!("script error: {text}")));
            }
        }
        Ok(msgs
            .iter()
            .find_map(|m| match m {
                ScriptMsg::Return {
                    value: ScriptValue::String(s),
                    ..
                } => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_default())
    }

    // ---------- File ops ----------

    /// Lua-driven directory listing — returns the raw pipe-separated
    /// `name:is_dir:size\n…` blob. Routes layer parses it into DirEntry.
    /// Special markers: `ERR_NOLIST`, `ERR_LIST|<reason>`.
    pub async fn list_dir_raw(&self, path: &str) -> Result<String> {
        let lua = lua_scripts::list_dir(path);
        self.exec_lua_for_string(&lua, 15_000).await
    }

    pub async fn download_file(&self, path: &str) -> Result<Vec<u8>> {
        let mut s = self.session.lock().await;
        match s.download_file(path).await {
            Ok(v) => Ok(v),
            Err(e) => {
                drop(s);
                self.invalidate_self();
                Err(Error::new(format!("download_file: {e}")))
            }
        }
    }

    // ---------- Clock-sync shoot helpers (used by PanoArray) ----------

    /// One offset-probe round-trip: the camera's current `get_tick_count()`,
    /// returned as the raw `Integer` value. Used by the clock-sync calibration.
    pub async fn read_tick_count(&self) -> Result<i64> {
        let mut s = self.session.lock().await;
        match s.execute_script_wait(lua_scripts::READ_CLOCK_MS, 2_000).await {
            Ok(msgs) => {
                drop(s);
                msgs.iter()
                    .find_map(|m| match m {
                        ScriptMsg::Return {
                            value: ScriptValue::Integer(v),
                            ..
                        } => Some(*v as i64),
                        _ => None,
                    })
                    .ok_or_else(|| Error::new("camera did not return an integer tick"))
            }
            Err(e) => {
                drop(s);
                self.invalidate_self();
                Err(e.into())
            }
        }
    }

    /// Run the combined warmup→busy-wait→fire script for a clock-synced shoot.
    /// Returns `(return_string, error_strings)` — the 9-value diagnostic string
    /// from `clocksync_combined` plus any script errors, parsed by the caller.
    pub async fn clocksync_shoot_raw(
        &self,
        target_tick: i64,
        flash: bool,
    ) -> Result<(Option<String>, Vec<String>)> {
        let lua = lua_scripts::clocksync_combined(target_tick, flash);
        let msgs = self.exec_lua(&lua, 25_000).await?;
        let mut errors = Vec::new();
        let mut ret = None;
        for m in &msgs {
            match m {
                ScriptMsg::Error { text, category, .. } => {
                    errors.push(format!("[{category:?}] {text}"))
                }
                ScriptMsg::Return {
                    value: ScriptValue::String(s),
                    ..
                } => ret = Some(s.clone()),
                _ => {}
            }
        }
        Ok((ret, errors))
    }
}
