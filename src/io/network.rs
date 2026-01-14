//! Network I/O using UDP multicast.
//!
//! This module provides low-level network transport for audio frames:
//!
//! - [`NetworkSender`] - Broadcasts audio frames to all peers via UDP multicast
//! - [`NetworkReceiver`] - Receives frames from peers and dispatches to per-host pipelines
//! - [`get_local_ip`] - Utility to discover local IP address for multicast
//!
//! # Protocol
//!
//! Audio frames are serialized using `rkyv` (zero-copy deserialization) and sent
//! over UDP multicast. Each packet contains a single [`AudioFrame`] with:
//! - Sequence number for ordering and jitter buffer management
//! - Timestamp for synchronization
//! - Audio sample data
//!
//! # Multicast Configuration
//!
//! - Address: `239.255.43.2` (link-local multicast)
//! - Port: `7667`
//! - TTL: `1` (local network only)

use anyhow::{Context, Result};
use std::marker::PhantomData;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crate::party::host::HostPipelineManager;
use crate::pipeline::Sink;
use crate::state::{AppState, ConnectionStatus, HostId};

pub const MULTICAST_ADDR: &str = "239.255.43.2";
pub const MULTICAST_PORT: u16 = 7667;
pub const TTL: u32 = 1;

/// Sends audio frames to all peers via UDP multicast.
///
/// Implements [`Sink`] so it can be used directly in the audio pipeline.
/// Frames are serialized with `rkyv` and broadcast to the multicast group.
///
/// # Error Handling
///
/// Send failures are logged but don't propagate errors - audio streaming
/// continues even if individual packets are lost (UDP is unreliable by design).
pub struct NetworkSender<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    socket: UdpSocket,
    multicast_addr: SocketAddr,
    _marker: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Creates a new NetworkSender using the provided UDP socket.
    ///
    /// The caller is responsible for configuring the socket appropriately
    /// (e.g., setting non-blocking mode, multicast TTL).
    pub fn new(socket: UdpSocket, multicast_addr: SocketAddr) -> Self {
        info!(
            "Network sender initialized for {:?}",
            multicast_addr
        );

        Self {
            socket,
            multicast_addr,
            _marker: PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    fn send_frame(&self, frame: &AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        match rkyv::to_bytes::<rkyv::rancor::Error>(frame) {
            Ok(serialized) => {
                match self.socket.send_to(&serialized, self.multicast_addr) {
                    Ok(bytes_sent) => {
                        if bytes_sent != serialized.len() {
                            warn!("Partial send: {} of {} bytes", bytes_sent, serialized.len());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        warn!("Socket would block, dropping frame");
                    }
                    Err(e) => {
                        warn!("Failed to send packet: {}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Failed to serialize frame: {}", e);
            }
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Sink
    for NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: for<'a> rkyv::Serialize<
            rkyv::api::high::HighSerializer<
                rkyv::util::AlignedVec,
                rkyv::ser::allocator::ArenaHandle<'a>,
                rkyv::rancor::Error,
            >,
        >,
{
    type Input = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&self, input: Self::Input) {
        self.send_frame(&input);
    }
}

/// Receives audio frames from peers and dispatches them to per-host pipelines.
///
/// Runs in a dedicated thread (see [`NetworkNode`](crate::party::network::NetworkNode)),
/// continuously listening for UDP multicast packets. Each received frame is:
///
/// 1. Deserialized from `rkyv` format
/// 2. Filtered (packets from self are ignored based on source IP)
/// 3. Dispatched to the appropriate host's jitter buffer via [`HostPipelineManager`]
///
/// The receiver also periodically cleans up stale host pipelines.
pub struct NetworkReceiver<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    socket: UdpSocket,
    state: Arc<AppState>,
    pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>,
    local_ips: Vec<std::net::IpAddr>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkReceiver<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Creates a new NetworkReceiver using the provided UDP socket.
    ///
    /// The caller is responsible for configuring the socket appropriately
    /// (e.g., binding, joining multicast group, setting options).
    ///
    /// `local_ips` should contain all IP addresses of the local machine,
    /// used to filter out packets originating from this device.
    pub fn new(
        socket: UdpSocket,
        state: Arc<AppState>,
        pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>,
        local_ips: Vec<std::net::IpAddr>,
    ) -> Self {
        info!("Network receiver initialized, local IPs: {:?}", local_ips);

        Self {
            socket,
            state,
            pipeline_manager,
            local_ips,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkReceiver<Sample, CHANNELS, SAMPLE_RATE>
where
    AudioFrame<Sample, CHANNELS, SAMPLE_RATE>: rkyv::Archive,
    <AudioFrame<Sample, CHANNELS, SAMPLE_RATE> as rkyv::Archive>::Archived: rkyv::Deserialize<
            AudioFrame<Sample, CHANNELS, SAMPLE_RATE>,
            rkyv::api::high::HighDeserializer<rkyv::rancor::Error>,
        >,
{
    /// Runs the receive loop. This blocks forever, processing incoming packets.
    pub fn run(mut self) {
        info!("Network receive thread started");

        *self.state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        let mut last_cleanup = Instant::now();

        loop {
            if let Err(e) = self.handle_packet(&mut buf) {
                warn!("Error processing packet: {:?}", e);
            }

            if last_cleanup.elapsed() > Duration::from_secs(1) {
                self.pipeline_manager.cleanup_stale_hosts();
                last_cleanup = Instant::now();
            }
        }
    }

    fn handle_packet(&mut self, buf: &mut [u8]) -> Result<()> {
        let (size, source_addr) = self
            .socket
            .recv_from(buf)
            .context("Failed to receive UDP packet")?;

        // Filter out packets from our own device (router may echo multicast back to us)
        if self.local_ips.contains(&source_addr.ip()) {
            return Ok(());
        }

        let received_data = &buf[..size];
        let host_id = HostId::from(source_addr);

        let frame: AudioFrame<Sample, CHANNELS, SAMPLE_RATE> =
            unsafe { rkyv::from_bytes_unchecked(received_data) }
                .map_err(|e| anyhow::anyhow!("Deserialization error: {:?}", e))?;

        debug!(
            "Receive packet from {:?}, seq num {}, is_v4: {}",
            source_addr,
            frame.sequence_number,
            source_addr.is_ipv4()
        );

        self.pipeline_manager.push_frame(host_id, frame);

        Ok(())
    }
}

/// Get the local IP address by creating a socket.
///
/// This doesn't actually send any data, just queries the local routing table
/// to determine which interface would be used for multicast traffic.
pub fn get_local_ip() -> Result<HostId> {
    let socket = UdpSocket::bind("0.0.0.0:0").context("Failed to create socket")?;

    socket
        .connect(format!("{}:{}", MULTICAST_ADDR, MULTICAST_PORT))
        .context("Failed to connect socket")?;

    let local_addr = socket.local_addr().context("Failed to get local address")?;

    Ok(HostId::from(local_addr))
}
