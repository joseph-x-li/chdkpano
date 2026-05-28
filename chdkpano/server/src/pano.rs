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
        Arc::new(Self {
            slots: Mutex::new(Default::default()),
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

    /// Naive parallel shoot. Each camera receives `shoot()` independently,
    /// kicked off concurrently. Worst-case skew across cameras is whatever
    /// Tokio's task scheduling + USB-bus contention give you — typically
    /// 50–200 ms apart on chdkpano's setup. For tighter sync see `shoot_all_synced`.
    pub async fn shoot_all(&self) -> [SlotOutcome<()>; SLOT_COUNT] {
        let cams = self.cameras().await;
        let futs = cams.into_iter().map(|outcome| async move {
            match outcome {
                SlotOutcome::Empty => SlotOutcome::Empty,
                SlotOutcome::Err(e) => SlotOutcome::Err(e),
                SlotOutcome::Ok(cam) => match cam.shoot_now().await {
                    Ok(()) => SlotOutcome::Ok(()),
                    Err(e) => SlotOutcome::Err(e),
                },
            }
        });
        let v: Vec<SlotOutcome<()>> = join_all(futs).await;
        v.try_into().unwrap_or_else(|_| unreachable!("len = SLOT_COUNT"))
    }

    /// Clock-synced shoot. Replicates the original chdkptp Lua harness
    /// trick:
    ///   1. Read each camera's monotonic tick AND measure host time around
    ///      the read to estimate the camera-to-host offset.
    ///   2. Compute a host-time deadline a bit in the future
    ///      (`lead_ms` parameter; default 500 ms is enough to overcome
    ///      PTP round-trip jitter at modest serial USB speeds).
    ///   3. Translate the host deadline to each camera's local clock and
    ///      send `shoot_at(deadline)`. Each camera busy-waits to its own
    ///      target tick, then fires.
    ///
    /// Per-camera sync is usually within ~5–20 ms of each other this way,
    /// vs ~100 ms+ with `shoot_all`.
    pub async fn shoot_all_synced(&self, lead_ms: i64) -> Result<[SlotOutcome<()>; SLOT_COUNT]> {
        let cams = self.cameras().await;

        // Step 1: calibrate each camera. Per-camera offset = camera_ms - host_ms.
        let host_clock = Instant::now();
        let offset_futs = cams.iter().map(|outcome| async move {
            match outcome {
                SlotOutcome::Empty => SlotOutcome::Empty,
                SlotOutcome::Err(e) => SlotOutcome::Err(e.clone()),
                SlotOutcome::Ok(cam) => {
                    let host_before = host_clock.elapsed().as_millis() as i64;
                    let cam_clock = cam.read_clock_ms().await;
                    let host_after = host_clock.elapsed().as_millis() as i64;
                    match cam_clock {
                        Ok(cam_ms) => {
                            // Best estimate of the camera-vs-host offset assumes
                            // PTP transit was symmetric: the camera's read happened
                            // ~midway between our two host samples.
                            let host_midpoint = (host_before + host_after) / 2;
                            SlotOutcome::Ok(cam_ms - host_midpoint)
                        }
                        Err(e) => SlotOutcome::Err(e),
                    }
                }
            }
        });
        let offsets: Vec<SlotOutcome<i64>> = join_all(offset_futs).await;

        // Step 2: pick a host deadline lead_ms in the future.
        let host_deadline = host_clock.elapsed().as_millis() as i64 + lead_ms;

        // Step 3: each camera shoots at host_deadline + its own offset.
        let shoot_futs = cams
            .into_iter()
            .zip(offsets.into_iter())
            .map(|(cam_out, off_out)| async move {
                match (cam_out, off_out) {
                    (SlotOutcome::Empty, _) | (_, SlotOutcome::Empty) => SlotOutcome::Empty,
                    (SlotOutcome::Err(e), _) | (_, SlotOutcome::Err(e)) => SlotOutcome::Err(e),
                    (SlotOutcome::Ok(cam), SlotOutcome::Ok(offset)) => {
                        let cam_target = host_deadline + offset;
                        match cam.shoot_at(cam_target).await {
                            Ok(()) => SlotOutcome::Ok(()),
                            Err(e) => SlotOutcome::Err(e),
                        }
                    }
                }
            });
        let results: Vec<SlotOutcome<()>> = join_all(shoot_futs).await;
        Ok(results
            .try_into()
            .unwrap_or_else(|_| unreachable!("len = SLOT_COUNT")))
    }
}
