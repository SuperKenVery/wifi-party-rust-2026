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
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

use crate::party::ntp::NtpService;
use crate::party::stream::NetworkPacket;
use crate::party::sync_stream::SyncedAudioStream;
use crate::pipeline::Sink;
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
    network_sender: NetworkSender,
    realtime_stream: Arc<crate::party::stream::RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    synced_stream: Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
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
        synced_stream: Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
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

    pub fn run(mut self) {
        info!("Network receive thread started");

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for network receiver");

        // Enter the runtime context so tokio::spawn works
        let _guard = rt.enter();

        // Start the NTP service within this runtime
        self.ntp_service.start();

        *self.state.connection_status.lock().unwrap() = ConnectionStatus::Connected;

        let mut buf = [0u8; 65536];
        let mut last_cleanup = Instant::now();
        let mut last_retransmit_request = Instant::now();

        while !self.shutdown_flag.load(Ordering::SeqCst) {
            // Poll the runtime to process any pending async tasks (like NTP timers)
            rt.block_on(async {
                tokio::task::yield_now().await;
            });

            if let Err(e) = self.handle_packet(&mut buf)
                && !self.shutdown_flag.load(Ordering::SeqCst)
            {
                warn!("Error processing packet: {:?}", e);
            }

            if last_cleanup.elapsed() > Duration::from_secs(1) {
                self.realtime_stream.cleanup_stale();
                self.synced_stream.cleanup_stale();
                last_cleanup = Instant::now();
            }

            if last_retransmit_request.elapsed() > Duration::from_millis(200) {
                for (_addr, stream_id, seqs) in self.synced_stream.get_missing_frames() {
                    self.network_sender
                        .push(NetworkPacket::RequestFrames { stream_id, seqs });
                }
                last_retransmit_request = Instant::now();
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

        let packet: NetworkPacket =
            rkyv::from_bytes::<NetworkPacket, rkyv::rancor::Error>(received_data)
                .map_err(|e| anyhow::anyhow!("Deserialization error: {:?}", e))?;

        match packet {
            NetworkPacket::Realtime(frame) => {
                self.realtime_stream.receive(source_addr, frame);
            }
            NetworkPacket::Synced(frame) => {
                self.synced_stream.receive(source_addr, frame);
            }
            NetworkPacket::SyncedMeta(meta) => {
                self.synced_stream.receive_meta(source_addr, meta);
            }
            NetworkPacket::SyncedControl(control) => {
                self.synced_stream.receive_control(source_addr, control);
            }
            NetworkPacket::RequestFrames { stream_id, seqs } => {
                // Handle retransmission requests if we are the sender
                if let Ok(party) = self.state.party.lock()
                    && let Some(party) = party.as_ref()
                {
                    party.handle_retransmission_request(stream_id, seqs);
                }
            }
            NetworkPacket::Ntp(ntp_packet) => {
                self.ntp_service.handle_packet(ntp_packet);
            }
        }

        Ok(())
    }
}
