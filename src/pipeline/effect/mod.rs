//! Audio effect nodes.
//!
//! This module provides audio processing effects that implement [`Node`](super::Node).
//! Effects transform audio buffers in-place.

pub mod gain;
pub mod mute;
pub mod noise_gate;

pub use gain::Gain;
pub use mute::Mute;
pub use noise_gate::NoiseGate;
