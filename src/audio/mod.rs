//! Audio data types and sample processing.
//!
//! This module defines the core audio data structures:
//!
//! - [`AudioSample`] - Trait for audio sample types (i16, f32, etc.)
//! - [`AudioBuffer`] - A buffer of audio samples (raw PCM data)
//! - [`AudioFrame`] - An [`AudioBuffer`] with sequence number and timestamp for network transport

pub mod frame;
pub mod sample;

pub use sample::AudioSample;
