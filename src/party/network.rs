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
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::thread;

use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crate::io::{NetworkReceiver, NetworkSender, MULTICAST_ADDR, MULTICAST_PORT, TTL};
use crate::pipeline::{Sink, Source};
use crate::state::AppState;
use crate::io::get_local_ip;

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
        pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>,
        state: Arc<AppState>,
    ) -> Result<(
        impl Sink<Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
        impl Source<Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
    )> {
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;
        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), MULTICAST_PORT);

        // Create shared socket for both sending and receiving
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;
        socket
            .set_reuse_address(true)
            .context("Failed to set reuse address")?;
        socket
            .set_nonblocking(true)
            .context("Failed to set non-blocking")?;
        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast TTL")?;
        socket
            .set_multicast_loop_v4(false)
            .context("Failed to disable multicast loop")?;

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context("Failed to bind socket")?;
        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .context("Failed to join multicast group")?;

        let socket: UdpSocket = socket.into();

        info!(
            "Network socket initialized for multicast group {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        // Clone socket for sender (keeps non-blocking for audio thread)
        let send_socket = socket
            .try_clone()
            .context("Failed to clone socket for sender")?;

        let sender = NetworkSender::<Sample, CHANNELS, SAMPLE_RATE>::new(send_socket, multicast_addr);

        // Receiver socket should be blocking (it runs in its own thread)
        socket
            .set_nonblocking(false)
            .context("Failed to set receiver socket to blocking")?;

        // Get local IPs for filtering out our own packets
        let local_ips = match get_local_ip() {
            Ok(host_id) => vec![host_id.as_socket_addr().ip()],
            Err(e) => {
                info!("Could not determine local IP for self-filtering: {}", e);
                vec![]
            }
        };

        let pipeline_manager_clone = pipeline_manager.clone();
        let state_clone = state.clone();
        let receiver_handle = thread::spawn(move || {
            let receiver = NetworkReceiver::<Sample, CHANNELS, SAMPLE_RATE>::new(
                socket,
                state_clone,
                pipeline_manager_clone,
                local_ips,
            );
            receiver.run();
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
