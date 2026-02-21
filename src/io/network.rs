//! Network I/O using UDP multicast.
//!
//! This module provides low-level network transport for audio packets:
//!
//! - [`NetworkSender`] - Broadcasts audio packets to all peers via UDP multicast
//! - [`NetworkReceiver`] - Receives packets from peers and dispatches to stream handlers
//!
//! # Protocol
//!
//! Audio data is wrapped in [`NetworkPacket`] (Opus-encoded) and serialized using `rkyv`.
//! Each packet is sent over UDP multicast.
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
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::party::ntp::NtpService;
use crate::party::stream::NetworkPacket;
use crate::party::sync_stream::SyncedAudioStreamManager;
use crate::pipeline::Pushable;
use crate::state::{AppState, ConnectionStatus};

pub const MULTICAST_ADDR_V4: &str = "239.255.43.2";
pub const MULTICAST_ADDR_V6: &str = "ff02::7667";
pub const MULTICAST_PORT: u16 = 7667;
pub const TTL: u32 = 1;

/// Sends audio packets to all peers via UDP multicast.
///
/// Implements [`Sink`] so it can be used directly in the audio pipeline.
/// Packets are serialized with `rkyv` and broadcast to the multicast group.
///
/// # Error Handling
///
/// Send failures are logged but don't propagate errors.
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
        if let Err(error) = self.send_inner(packet) {
            error!("{}", error);
        }
    }

    fn send_inner(&self, packet: &NetworkPacket) -> Result<()> {
        let serialized =
            rkyv::to_bytes::<rkyv::rancor::Error>(packet).context("Failed to serialize packet")?;

        let addr = self.multicast_addr;
        let sent_length = self
            .socket
            .send_to(&serialized, addr)
            .context(format!("Failed to send packet to {addr:?}"))?;

        if sent_length < serialized.len() {
            warn!("Partial sent: {}/{}", sent_length, serialized.len());
        }

        Ok(())
    }
}

impl Pushable<NetworkPacket> for NetworkSender {
    fn push(&self, input: NetworkPacket) {
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
    network_sender: NetworkSender,
    realtime_stream: Arc<crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
    ntp_service: Arc<NtpService>,
    local_ips: Vec<std::net::IpAddr>,
    shutdown_flag: Arc<AtomicBool>,
}

impl<Sample: crate::audio::AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkReceiver<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        socket: UdpSocket,
        state: Arc<AppState>,
        network_sender: NetworkSender,
        realtime_stream: Arc<
            crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>,
        >,
        synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        ntp_service: Arc<NtpService>,
        local_ips: Vec<std::net::IpAddr>,
        shutdown_flag: Arc<AtomicBool>,
    ) -> Self {
        info!("Network receiver initialized, local IPs: {:?}", local_ips);

        Self {
            socket,
            state,
            network_sender,
            realtime_stream,
            synced_stream,
            ntp_service,
            local_ips,
            shutdown_flag,
        }
    }

    pub fn run(self) {
        info!("Network receiver started");

        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for network receiver");

        rt.block_on(self.run_async());

        info!("Network receiver shutting down");
    }

    async fn run_async(self) {
        let Self {
            socket,
            state,
            network_sender,
            realtime_stream,
            synced_stream,
            ntp_service,
            local_ips,
            shutdown_flag,
        } = self;

        let socket = Arc::new(
            tokio::net::UdpSocket::from_std(socket).expect("Failed to convert to tokio UdpSocket"),
        );

        ntp_service.start();

        *state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        // Clean up stale hosts
        let shutdown = shutdown_flag.clone();
        let rt_clone = realtime_stream.clone();
        let sync_clone = synced_stream.clone();
        let cleanup_task = tokio::spawn(async move {
            let mut cleanup_interval = interval(Duration::from_secs(1));
            while !shutdown.load(Ordering::Relaxed) {
                cleanup_interval.tick().await;
                rt_clone.cleanup_stale();
                sync_clone.cleanup_stale();
            }
        });

        // Find missing frames in synced stream, and request a resend
        let shutdown = shutdown_flag.clone();
        let sync_clone = synced_stream.clone();
        let sender_clone = network_sender.clone();
        let retransmit_task = tokio::spawn(async move {
            let mut retransmit_interval = interval(Duration::from_millis(200));
            while !shutdown.load(Ordering::Relaxed) {
                retransmit_interval.tick().await;
                for (_addr, stream_id, seqs) in sync_clone.get_missing_frames() {
                    sender_clone.push(NetworkPacket::RequestFrames { stream_id, seqs });
                }
            }
        });

        // Handle realtime stream packets
        let shutdown = shutdown_flag.clone();
        let recv_task = tokio::spawn(async move {
            let mut buf = [0u8; 65536];
            while !shutdown.load(Ordering::Relaxed) {
                match socket.recv_from(&mut buf).await {
                    Ok((size, source_addr)) => {
                        handle_packet(
                            &buf[..size],
                            source_addr,
                            &local_ips,
                            &state,
                            &realtime_stream,
                            &synced_stream,
                            &ntp_service,
                        );
                    }
                    Err(e) => {
                        error!("Failed to receive UDP packet: {:?}", e);
                    }
                }
            }
        });

        let _ = tokio::join!(cleanup_task, retransmit_task, recv_task);
    }
}

fn handle_packet<
    Sample: crate::audio::AudioSample,
    const CHANNELS: usize,
    const SAMPLE_RATE: u32,
>(
    data: &[u8],
    source_addr: SocketAddr,
    local_ips: &[std::net::IpAddr],
    state: &Arc<AppState>,
    realtime_stream: &Arc<crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    synced_stream: &Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
    ntp_service: &Arc<NtpService>,
) {
    if local_ips.contains(&source_addr.ip()) {
        return;
    }

    let packet: NetworkPacket = match rkyv::from_bytes::<NetworkPacket, rkyv::rancor::Error>(data) {
        Ok(p) => p,
        Err(e) => {
            warn!("Deserialization error: {:?}", e);
            return;
        }
    };

    match packet {
        NetworkPacket::Realtime(frame) => {
            realtime_stream.receive(source_addr, frame);
        }
        NetworkPacket::Synced(frame) => {
            synced_stream.receive(source_addr, frame);
        }
        NetworkPacket::SyncedMeta(meta) => {
            synced_stream.receive_meta(source_addr, meta);
        }
        NetworkPacket::SyncedControl(control) => {
            synced_stream.receive_control(source_addr, control);
        }
        NetworkPacket::RequestFrames { stream_id, seqs } => {
            if let Ok(party) = state.party.lock()
                && let Some(party) = party.as_ref()
            {
                party.handle_retransmission_request(stream_id, seqs);
            }
        }
        NetworkPacket::Ntp(ntp_packet) => {
            ntp_service.handle_packet(ntp_packet);
        }
    }
}
