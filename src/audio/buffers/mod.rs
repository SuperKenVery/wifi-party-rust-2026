//! Audio buffer implementations.
//!
//! This module provides buffer types for audio processing pipelines:
//!
//! - [`SimpleBuffer`] - A simple FIFO buffer for audio samples
//! - [`AudioBatcher`] - Batches audio samples to reduce packet frequency
//! - [`JitterBuffer`] - Reorders out-of-order frames with adaptive latency control

pub mod audio_batcher;
pub mod jitter_buffer;
pub mod simple_buffer;

pub use audio_batcher::AudioBatcher;
pub use jitter_buffer::{JitterBuffer, PullSnapshot};
pub use simple_buffer::SimpleBuffer;
