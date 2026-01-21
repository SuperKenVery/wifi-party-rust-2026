//! Audio effect nodes.
//!
//! This module provides audio processing effects that implement [`Node`](super::Node).
//! Effects transform audio buffers in-place.

pub mod gain;
pub mod level_meter;
pub mod mute;
pub mod noise_gate;

pub use level_meter::{LevelMeter, calculate_rms_level};
