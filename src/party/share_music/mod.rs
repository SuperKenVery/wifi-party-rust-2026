//! Shared music streaming types and orchestration.
//!
//! This module coordinates synchronized music playback across devices.
//! It contains shared wire types used by both sender and receiver,
//! and [`ShareMusicService`] which combines sender and receiver into
//! a single [`NetworkStream`] implementation.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use rkyv::{Archive, Deserialize, Serialize};
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::symphonia_compat::WireCodecParams;
use crate::io::NetworkSender;
use crate::party::network_stream::{NetworkStream, NetworkStreamContext};
use crate::party::ntp::NtpService;
use crate::party::tagged_packet::{
    PacketTag, REQUEST_FRAMES_TAG, SYNCED_CONTROL_TAG, SYNCED_META_TAG, SYNCED_TAG,
};
use crate::state::MusicStreamProgress;

pub mod receiver;
pub mod sender;

// ---------------------------------------------------------------------------
//  Stream ID
// ---------------------------------------------------------------------------

pub type SyncedStreamId = u64;

static NEXT_STREAM_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn new_stream_id() -> SyncedStreamId {
    NEXT_STREAM_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
//  Wire types
// ---------------------------------------------------------------------------

/// Metadata about a synced stream, sent over the network.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedStreamMeta {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
    pub total_samples: u64,
    pub codec_params: WireCodecParams,
}

/// A single compressed audio packet for synced playback, sent over the network.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct SyncedFrame {
    pub stream_id: SyncedStreamId,
    pub sequence_number: u64,
    pub dur: u32,
    pub fragment_idx: u16,
    pub fragment_total: u16,
    pub data: Vec<u8>,
}

/// Max bytes of compressed audio per fragment. Leaves headroom under a
/// conservative 1400-byte UDP payload for the rest of `SyncedFrame`,
/// the `NetworkPacket::Synced` wrapper, and rkyv framing overhead.
pub const MAX_FRAGMENT_DATA: usize = 1200;

/// Raw packet stored for sender-side retransmission.
#[derive(Clone)]
pub struct RawPacket {
    pub dur: u32,
    pub data: Vec<u8>,
}

impl SyncedFrame {
    /// Build a non-fragmented frame (`fragment_total = 1`).
    pub fn whole(stream_id: SyncedStreamId, sequence_number: u64, dur: u32, data: Vec<u8>) -> Self {
        Self {
            stream_id,
            sequence_number,
            dur,
            fragment_idx: 0,
            fragment_total: 1,
            data,
        }
    }
}

/// Playback progress for a synced stream (output type for GUI).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncedStreamProgress {
    pub samples_played: u64,
    pub total_samples: u64,
    pub buffered_frames: u64,
    pub is_playing: bool,
    pub highest_seq_received: u64,
}

/// Complete state of a synced stream (output type for GUI).
#[derive(Debug, Clone)]
pub struct SyncedStreamState {
    pub stream_id: SyncedStreamId,
    pub meta: SyncedStreamMeta,
    pub progress: SyncedStreamProgress,
    pub is_local_sender: bool,
}

impl PartialEq for SyncedStreamState {
    fn eq(&self, other: &Self) -> bool {
        self.stream_id == other.stream_id
            && self.meta.stream_id == other.meta.stream_id
            && self.meta.file_name == other.meta.file_name
            && self.meta.total_frames == other.meta.total_frames
            && self.progress == other.progress
            && self.is_local_sender == other.is_local_sender
    }
}

/// Wire payload for retransmission requests.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub struct RequestFramesPayload {
    pub stream_id: SyncedStreamId,
    pub seqs: Vec<u64>,
}

/// Control commands for synced streams.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub enum SyncedControl {
    Start {
        stream_id: SyncedStreamId,
        party_clock_time: u64,
        seq: u64,
    },
    Pause {
        stream_id: SyncedStreamId,
    },
}

// ---------------------------------------------------------------------------
//  ShareMusicService — combines sender + receiver into one NetworkStream
// ---------------------------------------------------------------------------

/// Unified service for synchronized music sharing.
///
/// Combines [`sender::MusicStreamRegistry`] (outgoing streams) and
/// [`receiver::SyncedAudioStreamManager`] (incoming streams) into a single
/// [`NetworkStream`] implementation. This ensures all synced-music packet
/// tags are handled by one registration entry.
pub struct ShareMusicService<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sender: sender::MusicStreamRegistry<Sample, CHANNELS, SAMPLE_RATE>,
    receiver: Arc<receiver::SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>
    ShareMusicService<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        ntp_service: Arc<NtpService>,
        network_sender: NetworkSender,
        party_now_fn: impl Fn() -> u64 + Send + Sync + 'static,
        vocal_removal_enabled: Arc<AtomicBool>,
    ) -> Self {
        let receiver = Arc::new(receiver::SyncedAudioStreamManager::new(
            party_now_fn,
            vocal_removal_enabled,
        ));
        let sender =
            sender::MusicStreamRegistry::new(ntp_service, network_sender, receiver.clone());
        info!("ShareMusicService created");
        Self { sender, receiver }
    }

    /// Start streaming a local music file.
    pub fn start_stream(
        &self,
        data: Vec<u8>,
        file_name: String,
        progress: Arc<MusicStreamProgress>,
    ) -> anyhow::Result<()> {
        self.sender.start_stream(data, file_name, progress)
    }

    /// Pause a playing stream by ID.
    pub fn pause(&self, stream_id: SyncedStreamId) -> anyhow::Result<()> {
        self.sender.pause(stream_id)
    }

    /// Resume a paused stream by ID.
    pub fn resume(&self, stream_id: SyncedStreamId) -> anyhow::Result<()> {
        self.sender.resume(stream_id)
    }

    /// Seek to a position (in milliseconds) in a stream by ID.
    pub fn seek(&self, stream_id: SyncedStreamId, position_ms: u64) -> anyhow::Result<()> {
        self.sender.seek(stream_id, position_ms)
    }

    /// Clear all outgoing streams.
    pub fn clear(&self) {
        self.sender.clear();
    }

    /// Access the receiver for wiring into the audio output mixer.
    pub fn receiver(
        &self,
    ) -> Arc<receiver::SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>> {
        self.receiver.clone()
    }
}

impl<S: AudioSample + 'static, const C: usize, const SR: u32> NetworkStream<S, C, SR>
    for ShareMusicService<S, C, SR>
{
    fn tags(&self) -> &'static [PacketTag] {
        &[
            SYNCED_TAG,
            SYNCED_META_TAG,
            SYNCED_CONTROL_TAG,
            REQUEST_FRAMES_TAG,
        ]
    }

    fn handle(&self, source: SocketAddr, tag: PacketTag, bytes: &[u8]) -> anyhow::Result<()> {
        match tag {
            REQUEST_FRAMES_TAG => self.sender.handle(source, tag, bytes),
            SYNCED_TAG | SYNCED_META_TAG | SYNCED_CONTROL_TAG => {
                self.receiver.handle(source, tag, bytes)
            }
            _ => unreachable!("ShareMusicService received unexpected tag {tag}"),
        }
    }

    fn start(self: Arc<Self>, ctx: NetworkStreamContext) {
        // Receiver owns the background tasks (cleanup, retransmit, view).
        self.receiver.clone().start(ctx);
    }
}
