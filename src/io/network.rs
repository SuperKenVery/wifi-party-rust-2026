//! Network I/O using UDP multicast.
//!
//! This module provides low-level network transport for audio packets:
//!
//! - [`NetworkSender`] - Broadcasts audio packets to all peers via UDP multicast
//! - [`NetworkReceiver`] - Receives packets from peers and dispatches to stream handlers
//! - [`get_local_ip`] / [`get_local_ip_v6`] - Utility to discover local IP address for multicast
//!
//! # Protocol
//!
//! Audio data is wrapped in [`NetworkPacket`] (Opus-encoded) and serialized using `rkyv`
//! (zero-copy deserialization). Each packet is sent over UDP multicast.
//!
//! # Multicast Configuration
//!
//! IPv4:
//! - Address: `239.255.43.2` (administratively scoped multicast)
//! - Port: `7667`
//! - TTL: `1` (local network only)
//!
//! IPv6:
//! - Address: `ff02::7667` (link-local scope multicast)
//! - Port: `7667`
//! - Hop limit: `1` (local network only)

use anyhow::{Context, Result};
use std::io::ErrorKind;
use std::net::{Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::party::stream::NetworkPacket;
use crate::pipeline::Sink;
use crate::state::{AppState, ConnectionStatus, HostId};

pub const MULTICAST_ADDR_V4: &str = "239.255.43.2";
pub const MULTICAST_ADDR_V6: &str = "ff02::fb";
pub const MULTICAST_PORT: u16 = 7667;
pub const TTL: u32 = 1;

/// Sends audio packets to all peers via UDP multicast.
///
/// Implements [`Sink`] so it can be used directly in the audio pipeline.
/// Packets are serialized with `rkyv` and broadcast to the multicast group.
///
/// # Error Handling
///
/// Send failures are logged but don't propagate errors - audio streaming
/// continues even if individual packets are lost (UDP is unreliable by design).
///
/// # Cloning
///
/// `NetworkSender` can be cloned to share the same socket between multiple
/// audio pipelines (e.g., mic and system audio).
#[derive(Clone)]
pub struct NetworkSender {
    socket: Arc<UdpSocket>,
    multicast_addr: SocketAddr,
}

impl NetworkSender {
    pub fn new(socket: UdpSocket, multicast_addr: SocketAddr) -> Self {
        info!("Network sender initialized for {:?}", multicast_addr);

        Self {
            socket: Arc::new(socket),
            multicast_addr,
        }
    }

    fn send_packet(&self, packet: &NetworkPacket) {
        match rkyv::to_bytes::<rkyv::rancor::Error>(packet) {
            Ok(serialized) => match self.socket.send_to(&serialized, self.multicast_addr) {
                Ok(bytes_sent) => {
                    if bytes_sent != serialized.len() {
                        warn!("Partial send: {} of {} bytes", bytes_sent, serialized.len());
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    warn!("Socket would block, dropping packet");
                }
                Err(e) => {
                    warn!("Failed to send packet: {}", e);
                }
            },
            Err(e) => {
                warn!("Failed to serialize packet: {}", e);
            }
        }
    }
}

impl Sink for NetworkSender {
    type Input = NetworkPacket;

    fn push(&self, input: Self::Input) {
        self.send_packet(&input);
    }
}

/// Receives audio packets from peers and dispatches them to stream handlers.
///
/// Runs in a dedicated thread (see [`NetworkNode`](crate::party::network::NetworkNode)),
/// continuously listening for UDP multicast packets. Each received packet is:
///
/// 1. Deserialized from `rkyv` format
/// 2. Filtered (packets from self are ignored based on source IP)
/// 3. Dispatched to the appropriate stream handler based on packet type
///
/// The receiver also periodically cleans up stale stream buffers.
pub struct NetworkReceiver<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    socket: UdpSocket,
    state: Arc<AppState>,
    realtime_stream: Arc<crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    local_ips: Vec<std::net::IpAddr>,
    shutdown_flag: Arc<AtomicBool>,
}

impl<Sample: crate::audio::AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkReceiver<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        socket: UdpSocket,
        state: Arc<AppState>,
        realtime_stream: Arc<
            crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>,
        >,
        local_ips: Vec<std::net::IpAddr>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        info!("Network receiver initialized, local IPs: {:?}", local_ips);

        Self {
            socket,
            state,
            realtime_stream,
            local_ips,
            shutdown_flag,
        }
    }

    pub fn run(mut self) {
        info!("Network receive thread started");

        *self.state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        let mut last_cleanup = Instant::now();

        while !self.shutdown_flag.load(Ordering::SeqCst) {
            if let Err(e) = self.handle_packet(&mut buf) {
                if !self.shutdown_flag.load(Ordering::SeqCst) {
                    warn!("Error processing packet: {:?}", e);
                }
            }

            if last_cleanup.elapsed() > Duration::from_secs(1) {
                self.realtime_stream.cleanup_stale();
                last_cleanup = Instant::now();
            }
        }

        info!("Network receive thread shutting down");
    }

    fn handle_packet(&mut self, buf: &mut [u8]) -> Result<()> {
        let (size, source_addr) = match self.socket.recv_from(buf) {
            Ok(result) => result,
            Err(e) if e.kind() == ErrorKind::WouldBlock || e.kind() == ErrorKind::TimedOut => {
                return Ok(());
            }
            Err(e) => return Err(e).context("Failed to receive UDP packet"),
        };

        if self.local_ips.contains(&source_addr.ip()) {
            return Ok(());
        }

        let received_data = &buf[..size];

        let packet: NetworkPacket = unsafe {
            rkyv::from_bytes_unchecked::<NetworkPacket, rkyv::rancor::Error>(received_data)
        }
        .map_err(|e| anyhow::anyhow!("Deserialization error: {:?}", e))?;

        match packet {
            NetworkPacket::Realtime(frame) => {
                self.realtime_stream.receive(source_addr, frame);
            }
        }

        Ok(())
    }
}

pub fn get_local_ip() -> Result<HostId> {
    let socket = UdpSocket::bind("0.0.0.0:0").context("Failed to create socket")?;

    socket
        .connect(format!("{}:{}", MULTICAST_ADDR_V4, MULTICAST_PORT))
        .context("Failed to connect socket")?;

    let local_addr = socket.local_addr().context("Failed to get local address")?;

    Ok(HostId::from(local_addr))
}

pub fn get_local_ip_v6(scope_id: u32) -> Result<HostId> {
    let bind_addr = SocketAddrV6::new(Ipv6Addr::UNSPECIFIED, 0, 0, scope_id);
    let socket = UdpSocket::bind(bind_addr).context("Failed to create IPv6 socket")?;

    let multicast_ip: Ipv6Addr = MULTICAST_ADDR_V6
        .parse()
        .context("Invalid IPv6 multicast address")?;
    let multicast_addr = SocketAddrV6::new(multicast_ip, MULTICAST_PORT, 0, scope_id);

    socket
        .connect(multicast_addr)
        .context("Failed to connect IPv6 socket")?;

    let local_addr = socket.local_addr().context("Failed to get local address")?;

    Ok(HostId::from(local_addr))
}
