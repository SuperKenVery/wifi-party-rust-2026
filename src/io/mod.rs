//! Hardware and network I/O.
//!
//! This module provides concrete [`Sink`](crate::pipeline::Sink)/[`Source`](crate::pipeline::Source)
//! implementations that interface with the outside world:
//!
//! - [`AudioInput`] / [`AudioOutput`] - Microphone capture and speaker playback via cpal
//! - [`LoopbackInput`] - System audio capture (loopback recording) via cpal
//! - [`NetworkSender`] / [`NetworkReceiver`] - UDP multicast for audio packet transport

pub mod audio;
pub mod network;

pub use audio::{AudioInput, AudioOutput, LoopbackInput};
pub use network::{
    MULTICAST_ADDR_V4, MULTICAST_ADDR_V6, MULTICAST_PORT, NetworkReceiver, NetworkSender, TTL,
    get_local_ip,
};
