//! Audio data types, processing nodes, and effects.
//!
//! This module defines the core audio data structures and processing components:
//!
//! # Data Types
//! - [`AudioSample`] - Trait for audio sample types (i16, f32, etc.)
//! - [`frame::AudioBuffer`] - A buffer of audio samples (raw PCM data)
//! - [`frame::AudioFrame`] - An [`frame::AudioBuffer`] with sequence number for network transport
//!
//! # Codec
//! - [`opus`] - Opus codec with FEC for network transmission
//!
//! # Sources
//! - [`file`] - Audio file decoding with symphonia
//!
//! # Buffers
//! - [`buffers::SimpleBuffer`] - Simple FIFO buffer
//! - [`buffers::AudioBatcher`] - Batches samples to reduce packet frequency
//! - [`buffers::JitterBuffer`] - Reorders out-of-order frames with adaptive latency
//!
//! # Effects
//! - [`effects::gain`] - Volume control
//! - [`effects::mute`] - Silence output
//! - [`effects::noise_gate`] - RMS-based noise gate
//! - [`effects::level_meter`] - Audio level metering

pub mod buffers;
pub mod effects;
pub mod file;
pub mod frame;
pub mod opus;
pub mod sample;

pub use buffers::{AudioBatcher, JitterBuffer, PullSnapshot, SimpleBuffer};
pub use effects::{LevelMeter, calculate_rms_level};
pub use file::{AudioFileInfo, AudioFileReader};
pub use opus::{OpusDecoder, OpusEncoder, OpusPacket};
pub use sample::AudioSample;
