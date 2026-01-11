//! Hardware I/O implementations.
//!
//! This module contains implementations that interact with hardware and OS resources:
//! - Audio devices (microphone capture, speaker playback) via cpal
//! - Network sockets (UDP multicast send/receive)
//!
//! These are concrete `Sink`/`Source` implementations that bridge the pipeline
//! framework with real-world I/O.

pub mod audio;
pub mod network;

pub use audio::{AudioInput, AudioOutput};
pub use network::{NetworkReceiver, NetworkSender, MULTICAST_ADDR, MULTICAST_PORT, TTL};
