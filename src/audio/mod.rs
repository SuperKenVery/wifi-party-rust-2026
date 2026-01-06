//! Audio processing, capture, and playback.
//!
//! This module handles everything related to audio I/O and processing,
//! including microphone capture, mixing, and speaker playback.

pub mod frame;
pub mod jitter;
pub mod sample;

pub use frame::AudioFrame;
pub use sample::AudioSample;
