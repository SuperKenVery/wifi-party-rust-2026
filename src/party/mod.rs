//! Party orchestration and domain logic.
//!
//! This module coordinates the audio sharing functionality:
//! - `host` - Per-host pipeline management and mixing
//! - `codec` - Audio frame encoding/decoding
//! - `combinator` - Pipeline routing utilities (tee, switch, mix)
//! - `network_node` - Network transport orchestration
//! - `party` - Main orchestrator

pub mod codec;
pub mod combinator;
pub mod host;
pub mod network_node;
pub mod party;

pub use codec::{Decoder, Encoder};
pub use combinator::{LoopbackSwitch, MixingSource, Tee};
pub use host::{HostPipelineManager, NetworkSource};
pub use network_node::NetworkNode;
pub use party::Party;
