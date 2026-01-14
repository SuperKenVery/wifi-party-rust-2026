//! Hardware and network I/O.
//!
//! This module provides concrete [`Sink`](crate::pipeline::Sink)/[`Source`](crate::pipeline::Source)
//! implementations that interface with the outside world:
//!
//! - [`AudioInput`] / [`AudioOutput`] - Microphone capture and speaker playback via cpal
//! - [`NetworkSender`] / [`NetworkReceiver`] - UDP multicast for audio frame transport

pub mod audio;
pub mod network;

pub use audio::{AudioInput, AudioOutput};
pub use network::{get_local_ip, NetworkReceiver, NetworkSender, MULTICAST_ADDR, MULTICAST_PORT, TTL};
