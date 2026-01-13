//! Network node orchestration.
//!
//! This module provides [`NetworkNode`], which coordinates the network layer for
//! audio transport. It manages both sending (via [`NetworkSender`]) and receiving
//! (via [`NetworkReceiver`]) of audio frames over UDP multicast.
//!
//! # Architecture
//!
//! ```text
//! Local Audio Input
//!       │
//!       ▼
//! ┌─────────────┐
//! │NetworkSender│ ──── UDP Multicast ────► Other Peers
//! └─────────────┘
//!
//!                                          Other Peers
//!                                               │
//!                                          UDP Multicast
//!                                               │
//!                                               ▼
//!                                     ┌───────────────────┐
//!                                     │NetworkReceiver    │
//!                                     │(background thread)│
//!                                     └────────┬──────────┘
//!                                              │
//!                                              ▼
//!                                   ┌──────────────────────┐
//!                                   │HostPipelineManager   │
//!                                   │(per-host jitter bufs)│
//!                                   └──────────┬───────────┘
//!                                              │
//!                                              ▼
//!                                     ┌──────────────┐
//!                                     │NetworkSource │
//!                                     │(mixed output)│
//!                                     └──────────────┘
//!                                              │
//!                                              ▼
//!                                       Local Speaker
//! ```
//!
//! # Usage
//!
//! Call [`NetworkNode::start`] to initialize network transport. It returns:
//! - A [`Sink`] for sending local audio frames to the network
//! - A [`Source`] that provides **mixed audio from all connected peers**
//!
//! The returned source automatically handles:
//! - Per-host jitter buffering for network delay compensation
//! - Mixing audio from multiple peers into a single stream

use std::marker::PhantomData;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use tracing::error;

use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crate::io::{NetworkReceiver, NetworkSender};
use crate::pipeline::{Sink, Source};
use crate::state::AppState;

use super::host::{HostPipelineManager, NetworkSource};

/// Orchestrates network audio transport.
///
/// `NetworkNode` manages the lifecycle of network sender and receiver components,
/// providing a simple interface for the audio pipeline to send and receive frames.
///
/// # Thread Model
///
/// - The sender operates synchronously when frames are pushed
/// - The receiver runs in a dedicated background thread, continuously listening
///   for incoming packets and dispatching them to per-host pipelines
pub struct NetworkNode<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _receiver_handle: Option<thread::JoinHandle<()>>,
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            _receiver_handle: None,
            _marker: PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <AudioFrame<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived: rkyv::Deserialize<
            AudioFrame<Sample, CHANNELS, SAMPLE_RATE>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        >,
{
    /// Starts the network transport layer.
    ///
    /// This initializes the UDP multicast sender and spawns a background thread
    /// for the receiver.
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `Sink` - Push local audio frames here to broadcast to other peers
    /// - `Source` - Pull from here to get **mixed audio from all connected peers**.
    ///   Each pull returns a single frame that combines audio from all hosts,
    ///   with per-host jitter buffering already applied.
    pub fn start(
        &mut self,
        pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
        state: Arc<AppState>,
    ) -> Result<(
        NetworkSender<Sample, CHANNELS, SAMPLE_RATE>,
        NetworkSource<Sample, CHANNELS, SAMPLE_RATE>,
    )> {
        let sender = NetworkSender::<Sample, CHANNELS, SAMPLE_RATE>::new()?;

        let pipeline_manager_clone = pipeline_manager.clone();
        let state_clone = state.clone();
        let receiver_handle = thread::spawn(move || {
            match NetworkReceiver::<Sample, CHANNELS, SAMPLE_RATE>::new(
                state_clone,
                pipeline_manager_clone,
            ) {
                Ok(receiver) => receiver.run(),
                Err(e) => error!("Failed to start network receiver: {}", e),
            }
        });

        self._receiver_handle = Some(receiver_handle);

        let source = NetworkSource::new(pipeline_manager);

        Ok((sender, source))
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}
