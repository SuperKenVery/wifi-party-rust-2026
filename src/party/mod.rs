//! Audio sharing orchestration.
//!
//! This module coordinates the complete audio sharing pipeline, connecting
//! audio inputs, network transport, and speaker output.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌───────────────────┐     ┌─────────────┐
//! │  Microphone │ ──► │RealtimeFramePacker│ ──► │NetworkSender│ ──► Network
//! └─────────────┘     │  (stream_id=Mic)  │     └─────────────┘
//!                     └───────────────────┘
//!
//! ┌─────────────┐     ┌───────────────────┐     ┌─────────────┐
//! │System Audio │ ──► │RealtimeFramePacker│ ──► │NetworkSender│ ──► Network
//! └─────────────┘     │ (stream_id=System)│     └─────────────┘
//!                     └───────────────────┘
//!
//! Network ──► ┌───────────────────┐     ┌─────────────┐
//!             │RealtimeAudioStream│ ──► │   Speaker   │
//!             │ (per-host/stream  │     └─────────────┘
//!             │  jitter buffers)  │
//!             └───────────────────┘
//! ```
//!
//! # Submodules
//!
//! - [`party`] - Main [`Party`] orchestrator that wires everything together
//! - [`stream`] - Audio stream abstraction ([`NetworkPacket`], [`RealtimeAudioStream`])
//! - [`network`] - [`NetworkNode`] for managing network send/receive
//! - [`codec`] - Legacy frame packing/unpacking (deprecated)
//! - [`combinator`] - Pipeline routing utilities (tee, switch, mix)

pub mod codec;
pub mod combinator;
pub mod host;
pub mod network;
pub mod party;
pub mod stream;

pub use party::Party;
