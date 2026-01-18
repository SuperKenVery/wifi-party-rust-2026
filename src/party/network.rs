//! Network node orchestration.
//!
//! This module provides [`NetworkNode`], which coordinates the network layer for
//! audio transport. It manages both sending (via [`NetworkSender`]) and receiving
//! (via [`NetworkReceiver`]) of audio packets over UDP multicast.
//!
//! # Architecture
//!
//! ```text
//! Local Audio Input
//!       │
//!       ▼
//! ┌───────────────────┐
//! │RealtimeFramePacker│
//! │  (stream_id=Mic)  │
//! └─────────┬─────────┘
//!           │
//!           ▼
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
//!                                   │RealtimeAudioStream   │
//!                                   │(per-host/stream      │
//!                                   │ jitter buffers)      │
//!                                   └──────────┬───────────┘
//!                                              │
//!                                              ▼
//!                                       Local Speaker
//! ```
//!
//! # Usage
//!
//! Call [`NetworkNode::start`] to initialize network transport. It returns:
//! - A [`Sink`] for sending [`NetworkPacket`]s to the network
//! - A reference to [`RealtimeAudioStream`] that provides mixed audio from all peers

use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use socket2::{Domain, Protocol, Socket, Type};
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::io::get_local_ip;
use crate::io::{MULTICAST_ADDR, MULTICAST_PORT, NetworkReceiver, NetworkSender, TTL};
use crate::party::stream::{NetworkPacket, RealtimeAudioStream};
use crate::pipeline::Sink;
use crate::state::AppState;

/// Orchestrates network audio transport.
///
/// `NetworkNode` manages the lifecycle of network sender and receiver components,
/// providing a simple interface for the audio pipeline to send and receive packets.
///
/// # Thread Model
///
/// - The sender operates synchronously when packets are pushed
/// - The receiver runs in a dedicated background thread, continuously listening
///   for incoming packets and dispatching them to stream handlers
pub struct NetworkNode<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    receiver_handle: Option<thread::JoinHandle<()>>,
    shutdown_flag: Arc<AtomicBool>,
    _marker: PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            receiver_handle: None,
            shutdown_flag: Arc::new(AtomicBool::new(false)),
            _marker: PhantomData,
        }
    }

    pub fn shutdown(&mut self) {
        self.shutdown_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.receiver_handle.take() {
            let _ = handle.join();
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Drop
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl<Sample: AudioSample + Clone, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
where
    NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
    NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <NetworkPacket<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived: rkyv::Deserialize<
            NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        >,
{
    /// Starts the network transport layer.
    ///
    /// This initializes the UDP multicast sender and spawns a background thread
    /// for the receiver.
    ///
    /// # Arguments
    ///
    /// * `send_interface_ip` - If Some, bind sender to this interface. If None, use all interfaces.
    ///
    /// # Returns
    ///
    /// A tuple of:
    /// - `Sink` - Push [`NetworkPacket`]s here to broadcast to other peers
    /// - `Arc<RealtimeAudioStream>` - Pull from here to get mixed audio from all peers.
    ///   Each pull returns audio that combines all hosts and all stream IDs,
    ///   with per-buffer jitter buffering already applied.
    pub fn start(
        &mut self,
        realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        state: Arc<AppState>,
        send_interface_ip: Option<Ipv4Addr>,
    ) -> Result<(
        impl Sink<Input = NetworkPacket<Sample, CHANNELS, SAMPLE_RATE>> + Clone + 'static,
        Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    )> {
        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;
        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), MULTICAST_PORT);

        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;
        socket
            .set_reuse_address(true)
            .context("Failed to set reuse address")?;
        socket
            .set_nonblocking(false)
            .context("Failed to set non-blocking")?;
        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast TTL")?;
        socket
            .set_multicast_loop_v4(false)
            .context("Failed to disable multicast loop")?;

        if let Some(iface_ip) = send_interface_ip {
            socket
                .set_multicast_if_v4(&iface_ip)
                .context("Failed to set multicast interface")?;
            info!("Multicast send interface set to {}", iface_ip);
        }

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context("Failed to bind socket")?;

        match if_addrs::get_if_addrs() {
            Ok(interfaces) => {
                for iface in interfaces {
                    if let if_addrs::IfAddr::V4(v4) = &iface.addr {
                        match socket.join_multicast_v4(&multicast_ip, &v4.ip) {
                            Ok(()) => info!(
                                "Joined multicast group on interface {} ({})",
                                iface.name, v4.ip
                            ),
                            Err(e) => warn!(
                                "Failed to join multicast on {} ({}): {}",
                                iface.name, v4.ip, e
                            ),
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to enumerate network interfaces: {}, joining on default interface only",
                    e
                );
                socket
                    .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
                    .context("Failed to join multicast group")?;
            }
        }

        let socket: UdpSocket = socket.into();

        info!(
            "Network socket initialized for multicast group {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        let send_socket = socket
            .try_clone()
            .context("Failed to clone socket for sender")?;

        let sender =
            NetworkSender::<Sample, CHANNELS, SAMPLE_RATE>::new(send_socket, multicast_addr);

        socket
            .set_read_timeout(Some(Duration::from_millis(100)))
            .context("Failed to set socket read timeout")?;

        let local_ips = match get_local_ip() {
            Ok(host_id) => vec![host_id.as_socket_addr().ip()],
            Err(e) => {
                info!("Could not determine local IP for self-filtering: {}", e);
                vec![]
            }
        };

        let realtime_stream_clone = realtime_stream.clone();
        let state_clone = state.clone();
        let shutdown_flag = self.shutdown_flag.clone();
        let receiver_handle = thread::spawn(move || {
            let receiver = NetworkReceiver::<Sample, CHANNELS, SAMPLE_RATE>::new(
                socket,
                state_clone,
                realtime_stream_clone,
                local_ips,
                shutdown_flag,
            );
            receiver.run();
        });

        self.receiver_handle = Some(receiver_handle);

        Ok((sender, realtime_stream))
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for NetworkNode<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}
