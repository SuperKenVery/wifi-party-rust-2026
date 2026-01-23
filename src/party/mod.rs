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
//! - [`stream`] - Realtime audio stream abstraction ([`NetworkPacket`], [`RealtimeAudioStream`])
//! - [`sync_stream`] - Synchronized audio stream for music playback
//! - [`network`] - [`NetworkNode`] for managing network send/receive
//! - [`codec`] - Legacy frame packing/unpacking (deprecated)
//! - [`combinator`] - Pipeline routing utilities (tee, switch, mix)

pub mod combinator;
pub mod config;
pub mod music;
pub mod network;
pub mod ntp;
pub mod party;
pub mod stream;
pub mod sync_stream;

pub use config::PartyConfig;
pub use music::{MusicStream, MusicStreamInfo};
pub use ntp::{NtpDebugInfo, NtpPacket, NtpService};
pub use party::Party;
pub use stream::{
    NetworkPacket, RealtimeAudioStream, RealtimeFrame, RealtimeStreamId, StreamSnapshot,
};
pub use sync_stream::{
    SyncedAudioStream, SyncedFrame, SyncedStreamId, SyncedStreamInfo, SyncedStreamMeta,
    new_stream_id,
};
