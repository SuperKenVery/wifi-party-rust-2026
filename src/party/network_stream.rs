//! Self-contained stream abstraction for network packet dispatch.
//!
//! Each stream type (realtime audio, synced music, NTP) implements
//! [`NetworkStream`] to own its packet handling logic. [`StreamRegistry`]
//! maps incoming [`PacketTag`]s to the right stream without the caller
//! knowing the stream types.
//!
//! # Adding a new stream
//!
//! 1. Define tag constant(s) in [`tagged_packet`].
//! 2. Implement `NetworkStream` on the new stream struct.
//! 3. In `Party::run`: construct the stream, call `registry.register(arc)`.
//! 4. Put any runtime startup in [`NetworkStream::start`].

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use tracing::warn;

use crate::audio::AudioSample;
use crate::io::NetworkSender;
use crate::party::tagged_packet::{PacketTag, TaggedPacket};
use crate::state::PartyViewState;

#[derive(Clone)]
pub struct NetworkStreamContext {
    pub view_state: Arc<PartyViewState>,
    pub sender: NetworkSender,
}

/// A self-contained handler for one or more [`PacketTag`] values.
///
/// Implement this on any struct that receives network packets and processes them.
/// The implementation deserializes its own payload bytes and dispatches internally.
pub trait NetworkStream<S: AudioSample, const C: usize, const SR: u32>: Send + Sync {
    /// The packet tags this stream handles. Must be unique across all registered streams.
    fn tags(&self) -> &'static [PacketTag];

    /// Handle an inbound packet. `bytes` is the raw rkyv payload after the tag header.
    fn handle(&self, source: SocketAddr, tag: PacketTag, bytes: &[u8]) -> anyhow::Result<()>;

    /// Start stream-owned background tasks inside the network runtime.
    fn start(self: Arc<Self>, _ctx: NetworkStreamContext) {}
}

/// Routes incoming [`TaggedPacket`]s to the correct [`NetworkStream`].
pub struct StreamRegistry<S: AudioSample, const C: usize, const SR: u32> {
    streams: Vec<Arc<dyn NetworkStream<S, C, SR>>>,
    by_tag: HashMap<PacketTag, Arc<dyn NetworkStream<S, C, SR>>>,
}

impl<S: AudioSample, const C: usize, const SR: u32> StreamRegistry<S, C, SR> {
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            by_tag: HashMap::new(),
        }
    }

    pub fn from_streams(streams: Vec<Arc<dyn NetworkStream<S, C, SR>>>) -> Self {
        let mut registry = Self::new();
        for stream in streams {
            registry.register(stream);
        }
        registry
    }

    pub fn register(&mut self, stream: Arc<dyn NetworkStream<S, C, SR>>) {
        for &tag in stream.tags() {
            let prev = self.by_tag.insert(tag, stream.clone());
            assert!(prev.is_none(), "duplicate tag {tag} registered");
        }
        self.streams.push(stream);
    }

    pub fn start_all(&self, ctx: NetworkStreamContext) {
        for stream in &self.streams {
            stream.clone().start(ctx.clone());
        }
    }

    /// Deserialize the envelope and dispatch to the matching stream.
    pub fn dispatch(&self, source: SocketAddr, data: &[u8]) -> anyhow::Result<()> {
        let envelope = rkyv::from_bytes::<TaggedPacket, rkyv::rancor::Error>(data)
            .map_err(|e| anyhow::anyhow!("envelope deserialize: {:?}", e))?;

        match self.by_tag.get(&envelope.tag) {
            Some(stream) => stream.handle(source, envelope.tag, &envelope.payload),
            None => {
                warn!("no handler for tag {}", envelope.tag);
                Ok(())
            }
        }
    }
}
