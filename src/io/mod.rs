//! Hardware and network I/O.
//!
//! This module provides concrete [`Sink`](crate::pipeline::Sink)/[`Source`](crate::pipeline::Source)
//! implementations that interface with the outside world:
//!
//! - [`AudioInput`] / [`AudioOutput`] - Microphone capture and speaker playback via cpal
//! - [`LoopbackInput`] - System audio capture (loopback recording) via cpal
//! - [`NetworkSender`] / [`NetworkReceiver`] - UDP multicast for audio packet transport
//! - [`MulticastLock`] - Android multicast lock (no-op on other platforms)
//! - [`file_picker`] - Native file picker for Android (JNI-based)

pub mod audio;
pub mod file_picker;
pub mod multicast_lock;
pub mod network;

pub use audio::{AudioInput, AudioOutput, LoopbackInput};
pub use file_picker::{pick_audio_file, FilePickerResult};
pub use multicast_lock::MulticastLock;
pub use network::{
    MULTICAST_ADDR_V4, MULTICAST_ADDR_V6, MULTICAST_PORT, NetworkReceiver, NetworkSender, TTL,
};
