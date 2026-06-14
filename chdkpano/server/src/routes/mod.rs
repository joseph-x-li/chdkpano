//! HTTP route handlers. Each module owns the handlers + DTOs for one
//! feature area. Wiring happens in `crate::app`.

pub mod cameras;
pub mod exec;
pub mod files;
pub mod live_state;
pub mod pano;
pub mod stitch;
pub mod viewport;
pub mod wifi;
