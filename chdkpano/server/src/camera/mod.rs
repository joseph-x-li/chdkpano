//! Camera abstraction layer.
//!
//! - `camera::Camera` — one PTP session, owns its retry/invalidate policy
//! - `camera::CameraRegistry` — caches open Cameras by serial
//! - `camera::lua_scripts` — Lua templates that run on the camera

#[allow(clippy::module_inception)]
pub mod camera;
pub mod lua_scripts;
pub mod registry;

pub use camera::Camera;
pub use registry::CameraRegistry;
