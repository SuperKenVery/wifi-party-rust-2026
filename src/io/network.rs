//! Network I/O using UDP multicast.
//!
//! This module provides low-level network transport for audio frames:
//!
//! - [`NetworkSender`] - Broadcasts audio frames to all peers via UDP multicast
//! - [`NetworkReceiver`] - Receives frames from peers and dispatches to per-host pipelines
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
use socket2::{Domain, Protocol, Socket, Type};
use std::marker::PhantomData;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crate::party::host::HostPipelineManager;
use crate::pipeline::Sink;
use crate::state::{AppState, ConnectionStatus, HostId};
use std::sync::Mutex;

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
    socket: Socket,
    multicast_addr: SocketAddr,
    _marker: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSender<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        socket
            .set_nonblocking(true)
            .context("Failed to set non-blocking")?;

        socket
            .set_multicast_ttl_v4(TTL)
            .context("Failed to set multicast TTL")?;

        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;

        let multicast_addr = SocketAddr::new(IpAddr::V4(multicast_ip), MULTICAST_PORT);

        info!(
            "Network sender initialized for {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        Ok(Self {
            socket,
            multicast_addr,
            _marker: PhantomData,
        })
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
                match self
                    .socket
                    .send_to(&serialized, &self.multicast_addr.into())
                {
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
/// 2. Filtered (packets from self are ignored)
/// 3. Dispatched to the appropriate host's jitter buffer via [`HostPipelineManager`]
///
/// The receiver also periodically cleans up stale host pipelines.
pub struct NetworkReceiver<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    socket: std::net::UdpSocket,
    state: Arc<AppState>,
    pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkReceiver<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        state: Arc<AppState>,
        pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
    ) -> Result<Self> {
        let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))
            .context("Failed to create socket")?;

        socket
            .set_reuse_address(true)
            .context("Failed to set reuse address")?;

        let bind_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), MULTICAST_PORT);
        socket
            .bind(&bind_addr.into())
            .context("Failed to bind socket")?;

        let multicast_ip: Ipv4Addr = MULTICAST_ADDR
            .parse()
            .context("Invalid multicast address")?;

        socket
            .join_multicast_v4(&multicast_ip, &Ipv4Addr::UNSPECIFIED)
            .context("Failed to join multicast group")?;

        info!(
            "Network receiver joined multicast group {}:{}",
            MULTICAST_ADDR, MULTICAST_PORT
        );

        Ok(Self {
            socket: socket.into(),
            state,
            pipeline_manager,
        })
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
                self.pipeline_manager.lock().unwrap().cleanup_stale_hosts();
                last_cleanup = Instant::now();
            }
        }
    }

    fn handle_packet(&mut self, buf: &mut [u8]) -> Result<()> {
        let (size, source_addr) = self
            .socket
            .recv_from(buf)
            .context("Failed to receive UDP packet")?;

        let received_data = &buf[..size];
        let host_id = HostId::from(source_addr);

        let frame: AudioFrame<Sample, CHANNELS, SAMPLE_RATE> =
            unsafe { rkyv::from_bytes_unchecked(received_data) }
                .map_err(|e| anyhow::anyhow!("Deserialization error: {:?}", e))?;

        // Ignore packets from self
        {
            let local_host_id = self.state.local_host_id.lock().unwrap();
            if let Some(local_id) = *local_host_id {
                if host_id == local_id {
                    return Ok(());
                }
            }
        }

        debug!(
            "Receive packet from {:?}, seq num {}, is_v4: {}",
            source_addr,
            frame.sequence_number,
            source_addr.is_ipv4()
        );

        self.pipeline_manager
            .lock()
            .unwrap()
            .push_frame(host_id, frame);

        Ok(())
    }
}
