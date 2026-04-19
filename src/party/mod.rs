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
//! - [`packet_dispatcher`] - Network packet receiving and dispatching
//! - [`combinator`] - Pipeline routing utilities (tee, switch, mix)

pub mod combinator;
pub mod config;
pub mod music;
pub mod network_stream;
pub mod ntp;
pub mod packet_dispatcher;
pub mod party;
pub mod realtime_stream;
pub mod sync_stream;
pub mod tagged_packet;

mod tests;

pub use config::PartyConfig;

pub use ntp::NtpDebugInfo;
pub use party::Party;
pub use realtime_stream::StreamSnapshot;
pub use sync_stream::{SyncedStreamId, SyncedStreamState};
