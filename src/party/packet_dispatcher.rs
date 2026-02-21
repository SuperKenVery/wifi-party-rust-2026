//! Network packet receiving and dispatching.
//!
//! This module provides [`PacketDispatcher`] which runs a background task that:
//! 1. Receives UDP packets from the multicast socket
//! 2. Deserializes them into [`NetworkPacket`]
//! 3. Dispatches to the appropriate handler based on packet type

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::audio::AudioSample;
use crate::party::ntp::NtpService;
use crate::party::realtime_stream::{NetworkPacket, RealtimeAudioStream};
use crate::party::sync_stream::SyncedAudioStreamManager;
use crate::state::{AppState, ConnectionStatus};

/// Dispatches incoming network packets to appropriate handlers.
///
/// Runs a tokio task that listens for UDP packets and routes them:
/// - `Realtime` → `RealtimeAudioStream`
/// - `Synced` / `SyncedMeta` / `SyncedControl` → `SyncedAudioStreamManager`
/// - `RequestFrames` → `Party` (via AppState)
/// - `Ntp` → `NtpService`
pub struct PacketDispatcher;

impl PacketDispatcher {
    /// Starts the packet dispatcher background task.
    ///
    /// Returns a `JoinHandle` that can be used to await task completion.
    /// The task will run until `shutdown` is set to true.
    pub fn start<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        socket: UdpSocket,
        local_ips: Vec<IpAddr>,
        state: Arc<AppState>,
        realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        ntp_service: Arc<NtpService>,
        shutdown: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            Self::run(
                socket,
                local_ips,
                state,
                realtime_stream,
                synced_stream,
                ntp_service,
                shutdown,
            )
            .await;
        })
    }

    async fn run<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        socket: UdpSocket,
        local_ips: Vec<IpAddr>,
        state: Arc<AppState>,
        realtime_stream: Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        ntp_service: Arc<NtpService>,
        shutdown: Arc<AtomicBool>,
    ) {
        info!("Packet dispatcher started, local IPs: {:?}", local_ips);

        let socket = Arc::new(
            tokio::net::UdpSocket::from_std(socket).expect("Failed to convert to tokio UdpSocket"),
        );

        *state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        while !shutdown.load(Ordering::Relaxed) {
            match socket.recv_from(&mut buf).await {
                Ok((size, source_addr)) => {
                    Self::handle_packet(
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

        info!("Packet dispatcher shutting down");
    }

    fn handle_packet<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        data: &[u8],
        source_addr: SocketAddr,
        local_ips: &[IpAddr],
        state: &Arc<AppState>,
        realtime_stream: &Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        synced_stream: &Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        ntp_service: &Arc<NtpService>,
    ) {
        if local_ips.contains(&source_addr.ip()) {
            return;
        }

        let packet: NetworkPacket =
            match rkyv::from_bytes::<NetworkPacket, rkyv::rancor::Error>(data) {
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
}
