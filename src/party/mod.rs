//! Audio sharing orchestration.
//!
//! This module coordinates the complete audio sharing pipeline, connecting
//! microphone input, network transport, and speaker output.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │  Microphone │ ──► │ FramePacker │ ──► │NetworkSender│ ──► Network
//! └─────────────┘     └─────────────┘     └─────────────┘
//!        │
//!        └──► Loopback (optional) ──┐
//!                                   │
//! Network ──► ┌─────────────────┐   │   ┌─────────────┐
//!             │HostPipelineManager│ ──┼─► │   Speaker   │
//!             │  (per-host jitter │   │   └─────────────┘
//!             │   bufs + mixing)  │ ◄─┘
//!             └─────────────────┘
//! ```
//!
//! # Submodules
//!
//! - [`party`] - Main [`Party`] orchestrator that wires everything together
//! - [`network`] - [`NetworkNode`] for managing network send/receive
//! - [`host`] - [`HostPipelineManager`] for per-host jitter buffering and mixing
//! - [`codec`] - Frame packing/unpacking between [`AudioBuffer`](crate::audio::AudioBuffer) and [`AudioFrame`](crate::audio::AudioFrame)
//! - [`combinator`] - Pipeline routing utilities (tee, switch, mix)

pub mod codec;
pub mod combinator;
pub mod host;
pub mod network;
pub mod party;

pub use codec::{FramePacker, FrameUnpacker};
pub use combinator::{LoopbackSwitch, MixingSource, Tee};
pub use host::{HostPipelineManager, NetworkSource};
pub use network::NetworkNode;
pub use party::Party;
