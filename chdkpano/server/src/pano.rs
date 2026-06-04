//! `PanoArray` — fixed-size array of four camera slots that compose into
//! a panorama rig. Real implementation of the multi-camera operations,
//! built on top of `Camera` so it doesn't duplicate any session-management.
//!
//! Slot model: a 4-element array of `Option<String>` (serial). `None` =
//! empty slot. Slots are reassignable at runtime (PUT /api/pano/slot/N).
//!
//! Operations run concurrently across cameras using `futures::join!` —
//! each camera has its own session mutex, so per-camera parallelism is
//! real (USB transfers, JPEG encoding, etc. all overlap).

use crate::camera::{Camera, CameraRegistry};
use crate::error::{Error, Result};
use futures::future::join_all;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;

pub const SLOT_COUNT: usize = 4;
pub type Slots = [Option<String>; SLOT_COUNT];

/// Fixed rig camera serials by physical slot (cameras 1–4). MUST stay in sync
/// with the client's `RIG_SERIALS` so the API's slot indices line up with the
/// grid the user sees. Dummy entries are placeholders for cameras that aren't
/// wired yet — they simply fail `get_or_open` and report as errored slots.
pub const RIG_SERIALS: [&str; SLOT_COUNT] = [
    "DUMMY_SERIAL_CAM1",
    "FA934BBFD3514EF19CA0B81E72A213F7", // camera 2
    "D8359439FEB74E79899654E98FD41CA1", // camera 3
    "524EE2E7D9E34C6194BB238558A9EF91", // camera 4
];

/// Default NTP-style offset samples per camera; the best-RTT sample wins.
pub const CLOCKSYNC_OFFSET_SAMPLES: usize = 20;
/// Default lead before the synchronized shot fires. Must exceed the slowest
/// camera's warmup (~700 ms from record mode) plus margin — see the
/// `shoot_all_clocksync` example in chdkptp_rs for the reasoning.
pub const CLOCKSYNC_LEAD_MS: f64 = 2500.0;

/// Per-camera result of a clock-synced shoot — the same diagnostics the
/// chdkptp_rs `shoot_all_clocksync` example prints, surfaced over the API.
#[derive(Debug, Clone, Default)]
pub struct ClockSyncSlot {
    pub idx: usize,
    pub serial: Option<String>,
    /// "empty" | "fired" | "missed" | "err"
    pub status: &'static str,
    pub offset_ms: Option<f64>,
    pub offset_rtt_ms: Option<f64>,
    pub target_tick: Option<i64>,
    /// Camera-side time spent in the busy-wait spin loop (warmup → target).
    pub busy_wait_ms: Option<i64>,
    /// Busy-wait exit, converted back to host wall-clock via the offset.
    pub actual_exit_host_ms: Option<f64>,
    /// `actual_exit_host_ms - target_host_ms` (positive = fired late).
    pub overshoot_ms: Option<f64>,
    /// Whether `exp_count` increased — i.e. the shutter actually actuated.
    pub fired: Option<bool>,
    /// Camera path of the file this shot wrote, derived natively from the
    /// shoot's `get_image_dir()` + `exp_count` (no SD-card scan).
    pub image_path: Option<String>,
    pub error: Option<String>,
}

/// Full report from a clock-synced shoot.
#[derive(Debug, Clone)]
pub struct ClockSyncReport {
    pub slots: Vec<ClockSyncSlot>,
    /// Spread of busy-wait exits across cameras that exited — the headline
    /// "how synchronized was it" number, in host milliseconds.
    pub inter_camera_skew_ms: Option<f64>,
    pub target_host_ms: f64,
    pub lead_ms: f64,
    pub samples: usize,
}

/// Intermediate per-slot state between offset calibration and shot dispatch.
enum Calibrated {
    Empty,
    Err(Error),
    Ok {
        cam: Arc<Camera>,
        offset_ms: f64,
        rtt_ms: f64,
    },
}

pub struct PanoArray {
    slots: Mutex<Slots>,
    registry: Arc<CameraRegistry>,
}

/// Per-slot result from a fan-out operation. `Empty` = the slot was None;
/// `Ok(_)` / `Err(_)` = the camera in that slot succeeded / failed.
#[derive(Debug)]
pub enum SlotOutcome<T> {
    Empty,
    Ok(T),
    Err(Error),
}

impl<T> SlotOutcome<T> {
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }
    pub fn ok(self) -> Option<T> {
        if let Self::Ok(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> SlotOutcome<U> {
        match self {
            Self::Empty => SlotOutcome::Empty,
            Self::Ok(v) => SlotOutcome::Ok(f(v)),
            Self::Err(e) => SlotOutcome::Err(e),
        }
    }
}

impl PanoArray {
    pub fn new(registry: Arc<CameraRegistry>) -> Arc<Self> {
        // Pre-wire the fixed rig serials so the shoot endpoint targets the
        // four cameras out of the box. Slots stay reassignable at runtime.
        let slots: Slots = RIG_SERIALS.map(|s| Some(s.to_string()));
        Arc::new(Self {
            slots: Mutex::new(slots),
            registry,
        })
    }

    pub fn snapshot(&self) -> Slots {
        self.slots.lock().clone()
    }

    /// Assign (or clear) the camera in a slot. `idx` is 0..SLOT_COUNT.
    pub fn assign(&self, idx: usize, serial: Option<String>) -> Result<()> {
        if idx >= SLOT_COUNT {
            return Err(Error::new(format!(
                "slot {idx} out of range (max {})",
                SLOT_COUNT - 1
            )));
        }
        self.slots.lock()[idx] = serial;
        Ok(())
    }

    /// Auto-fill: assign every currently-attached camera into the first
    /// available slot, in serial-sort order. Useful for "I plugged in
    /// four cameras, just wire them up."
    pub fn autofill(&self) -> Result<Slots> {
        let mut attached: Vec<String> = self
            .registry
            .list_attached()?
            .iter()
            .filter_map(|c| c.serial().map(|s| s.to_string()))
            .collect();
        attached.sort();

        let mut slots = self.slots.lock();
        for (i, slot) in slots.iter_mut().enumerate() {
            *slot = attached.get(i).cloned();
        }
        Ok(slots.clone())
    }

    /// Resolve every slot to an `Arc<Camera>` (or `None` / `Err`). Opens
    /// sessions in parallel for any slot whose camera isn't cached yet.
    pub async fn cameras(&self) -> [SlotOutcome<Arc<Camera>>; SLOT_COUNT] {
        let snap = self.snapshot();
        let registry = self.registry.clone();

        let futs = snap.into_iter().map(|maybe_serial| {
            let registry = registry.clone();
            async move {
                match maybe_serial {
                    None => SlotOutcome::Empty,
                    Some(serial) => match registry.get_or_open(&serial).await {
                        Ok(c) => SlotOutcome::Ok(c),
                        Err(e) => SlotOutcome::Err(e),
                    },
                }
            }
        });
        let results: Vec<SlotOutcome<Arc<Camera>>> = join_all(futs).await;
        results.try_into().unwrap_or_else(|_| unreachable!("len = SLOT_COUNT"))
    }

    /// Fetch a viewport JPEG from every populated slot in parallel.
    /// Used by the upcoming 2×2 grid view in the web UI.
    pub async fn viewport_grid(&self) -> [SlotOutcome<Vec<u8>>; SLOT_COUNT] {
        let cams = self.cameras().await;
        let futs = cams.into_iter().map(|outcome| async move {
            match outcome {
                SlotOutcome::Empty => SlotOutcome::Empty,
                SlotOutcome::Err(e) => SlotOutcome::Err(e),
                SlotOutcome::Ok(cam) => match cam.viewport_jpeg().await {
                    Ok(bytes) => SlotOutcome::Ok(bytes),
                    Err(e) => SlotOutcome::Err(e),
                },
            }
        });
        let v: Vec<SlotOutcome<Vec<u8>>> = join_all(futs).await;
        v.try_into().unwrap_or_else(|_| unreachable!("len = SLOT_COUNT"))
    }

    /// The "real" synchronized shoot — a port of chdkptp_rs's
    /// `shoot_all_clocksync` example into async/axum land.
    ///
    /// Two phases with an implicit barrier (the first `join_all` completes
    /// before the second starts — the async equivalent of the example's
    /// thread `Barrier`):
    ///   1. **Calibrate** every camera in parallel: take `samples` NTP-style
    ///      `get_tick_count()` probes, keep the offset from the lowest-RTT one.
    ///   2. Pick a single host deadline `lead_ms` in the future, translate it
    ///      to each camera's tick, and dispatch the combined warmup→busy-wait→
    ///      fire script (one `lua_State`, half-press held throughout).
    ///
    /// Cameras should already be in record mode (the viewfinder ensures this);
    /// otherwise a cold mode-switch can overrun the default lead.
    pub async fn shoot_all_clocksync(
        &self,
        lead_ms: f64,
        samples: usize,
        flash: bool,
    ) -> ClockSyncReport {
        let snap = self.snapshot();
        let cams = self.cameras().await;
        let t0 = Instant::now();
        let samples = samples.max(1);

        // ── Phase 1: per-camera tick-offset calibration (parallel) ──────────
        let cal_futs = cams.into_iter().map(|outcome| async move {
            match outcome {
                SlotOutcome::Empty => Calibrated::Empty,
                SlotOutcome::Err(e) => Calibrated::Err(e),
                SlotOutcome::Ok(cam) => {
                    let mut best: Option<(f64, f64)> = None; // (offset, rtt)
                    for _ in 0..samples {
                        let h1 = host_ms(t0);
                        match cam.read_tick_count().await {
                            Ok(tick) => {
                                let h2 = host_ms(t0);
                                let rtt = h2 - h1;
                                // Assume symmetric transit: the camera read
                                // happened ~midway between our two host samples.
                                let offset = tick as f64 - (h1 + h2) / 2.0;
                                if best.map_or(true, |(_, br)| rtt < br) {
                                    best = Some((offset, rtt));
                                }
                            }
                            Err(e) => return Calibrated::Err(e),
                        }
                    }
                    match best {
                        Some((offset_ms, rtt_ms)) => Calibrated::Ok {
                            cam,
                            offset_ms,
                            rtt_ms,
                        },
                        None => Calibrated::Err(Error::new("no offset samples")),
                    }
                }
            }
        });
        let calibrated: Vec<Calibrated> = join_all(cal_futs).await;

        // Barrier passed: all offsets known. Pick the shared host deadline.
        let target_host_ms = host_ms(t0) + lead_ms;

        // ── Phase 2: dispatch the combined shoot script (parallel) ──────────
        let shoot_futs =
            calibrated
                .into_iter()
                .enumerate()
                .map(|(idx, cal)| {
                    let serial = snap[idx].clone();
                    async move {
                        match cal {
                            Calibrated::Empty => ClockSyncSlot {
                                idx,
                                status: "empty",
                                ..Default::default()
                            },
                            Calibrated::Err(e) => ClockSyncSlot {
                                idx,
                                serial,
                                status: "err",
                                error: Some(e.message().to_string()),
                                ..Default::default()
                            },
                            Calibrated::Ok {
                                cam,
                                offset_ms,
                                rtt_ms,
                            } => {
                                let target_tick = (target_host_ms + offset_ms).round() as i64;
                                let mut slot = ClockSyncSlot {
                                    idx,
                                    serial,
                                    status: "err",
                                    offset_ms: Some(offset_ms),
                                    offset_rtt_ms: Some(rtt_ms),
                                    target_tick: Some(target_tick),
                                    ..Default::default()
                                };
                                match cam.clocksync_shoot_raw(target_tick, flash).await {
                                    Ok((ret, errors)) => {
                                        if let Some((p, image_dir)) =
                                            ret.as_deref().and_then(parse_combined_return)
                                        {
                                            let (warmup_done, t_exit) = (p[1], p[2]);
                                            let (exp_at_start, exp_after) = (p[4], p[8]);
                                            let actual_exit = t_exit as f64 - offset_ms;
                                            let fired = exp_after > exp_at_start;
                                            slot.busy_wait_ms = Some(t_exit - warmup_done);
                                            slot.actual_exit_host_ms = Some(actual_exit);
                                            slot.overshoot_ms = Some(actual_exit - target_host_ms);
                                            slot.fired = Some(fired);
                                            slot.status = if fired { "fired" } else { "missed" };
                                            // Name the file this shot wrote, natively.
                                            if fired && !image_dir.is_empty() {
                                                slot.image_path = Some(format!(
                                                    "{image_dir}/IMG_{exp_after:04}.JPG"
                                                ));
                                            }
                                        }
                                        if !errors.is_empty() {
                                            slot.error = Some(errors.join("; "));
                                        }
                                    }
                                    Err(e) => slot.error = Some(e.message().to_string()),
                                }
                                slot
                            }
                        }
                    }
                });
        let slots: Vec<ClockSyncSlot> = join_all(shoot_futs).await;

        // Headline: spread of busy-wait exits across cameras that exited.
        let exits: Vec<f64> = slots.iter().filter_map(|s| s.actual_exit_host_ms).collect();
        let inter_camera_skew_ms = if exits.len() >= 2 {
            let min = exits.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = exits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            Some(max - min)
        } else {
            None
        };

        ClockSyncReport {
            slots,
            inter_camera_skew_ms,
            target_host_ms,
            lead_ms,
            samples,
        }
    }
}

fn host_ms(t0: Instant) -> f64 {
    t0.elapsed().as_secs_f64() * 1000.0
}

/// Parse the return from `lua_scripts::clocksync_combined`: nine numeric
/// fields followed by the image directory string. Returns `(nums, image_dir)`.
fn parse_combined_return(s: &str) -> Option<([i64; 9], String)> {
    // splitn(10) keeps the dir intact even in the (impossible) case it has a
    // comma; a too-short string fails the `?` on a missing numeric field.
    let mut it = s.splitn(10, ',');
    let mut nums = [0i64; 9];
    for slot in nums.iter_mut() {
        *slot = it.next()?.trim().parse().ok()?;
    }
    let image_dir = it.next().unwrap_or("").trim().to_string();
    Some((nums, image_dir))
}
