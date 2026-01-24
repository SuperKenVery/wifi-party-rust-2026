//! Audio effect nodes.
//!
//! This module provides audio processing effects that implement [`Node`](crate::pipeline::Node).
//! Effects transform audio buffers in-place.
#![allow(dead_code)]

pub mod gain;
pub mod level_meter;
pub mod noise_gate;
pub mod switch;

pub use gain::Gain;
pub use level_meter::{LevelMeter, calculate_rms_level};
pub use switch::Switch;
