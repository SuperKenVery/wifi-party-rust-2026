//! Music streaming pipeline for synced playback.
//!
//! Provides [`MusicStream`] which handles:
//! - Decoding audio files
//! - Encoding to Opus
//! - Packing into SyncedFrames with play_at timestamps
//! - Sending to network
//! - Local playback (sharer hears their own music)

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use crate::audio::file::AudioFileReader;
use crate::audio::frame::AudioBuffer;
use crate::audio::{AudioSample, OpusEncoder};
use crate::io::NetworkSender;
use crate::party::ntp::NtpService;
use crate::party::stream::NetworkPacket;
use crate::party::sync_stream::{
    new_stream_id, SyncedAudioStream, SyncedFrame, SyncedStreamId, SyncedStreamMeta,
};
use crate::pipeline::{Node, Sink};
use crate::state::MusicStreamProgress;

const LOCAL_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);

const BUFFER_AHEAD_MS: u64 = 2000;
const FRAME_DURATION_MS: u64 = 20;

pub struct MusicStreamInfo {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
    pub frames_sent: u64,
    pub is_complete: bool,
}

pub struct MusicStream {
    stream_id: SyncedStreamId,
    file_name: String,
    total_frames: u64,
    frames_sent: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
    is_complete: Arc<AtomicBool>,
    _handle: Option<thread::JoinHandle<()>>,
}

impl MusicStream {
    pub fn start<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        path: PathBuf,
        ntp_service: Arc<NtpService>,
        network_sender: NetworkSender,
        synced_stream: Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        progress: Arc<MusicStreamProgress>,
    ) -> Result<Self> {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!("Starting music stream for: {}", file_name);

        let stream_id = new_stream_id();
        let frames_sent = Arc::new(AtomicU64::new(0));
        let is_running = Arc::new(AtomicBool::new(true));
        let is_complete = Arc::new(AtomicBool::new(false));

        let frames_sent_clone = frames_sent.clone();
        let is_running_clone = is_running.clone();
        let is_complete_clone = is_complete.clone();
        let file_name_clone = file_name.clone();

        progress.reset();
        *progress.file_name.lock().unwrap() = Some(file_name.clone());
        progress.is_encoding.store(true, Ordering::Relaxed);

        let handle = thread::spawn(move || {
            if let Err(e) = Self::decode_and_send_loop::<Sample, CHANNELS, SAMPLE_RATE>(
                path,
                stream_id,
                file_name_clone,
                ntp_service,
                network_sender,
                synced_stream,
                progress,
                frames_sent_clone,
                is_running_clone,
                is_complete_clone,
            ) {
                warn!("Music stream error: {}", e);
            }
        });

        Ok(Self {
            stream_id,
            file_name,
            total_frames: 0,
            frames_sent,
            is_running,
            is_complete,
            _handle: Some(handle),
        })
    }

    fn decode_and_send_loop<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        path: PathBuf,
        stream_id: SyncedStreamId,
        file_name: String,
        ntp_service: Arc<NtpService>,
        network_sender: NetworkSender,
        synced_stream: Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
        progress: Arc<MusicStreamProgress>,
        frames_sent: Arc<AtomicU64>,
        is_running: Arc<AtomicBool>,
        is_complete: Arc<AtomicBool>,
    ) -> Result<()> {
        let reader = AudioFileReader::open(&path)?;
        info!("Decoding audio file: {}", file_name);

        let buffers: Vec<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> =
            reader.decode_all_resampled()?;

        let total_frames = buffers.len() as u64;
        info!("Decoded {} frames from {}", total_frames, file_name);

        progress.encoding_total.store(total_frames, Ordering::Relaxed);
        progress.encoding_current.store(total_frames, Ordering::Relaxed);
        progress.is_encoding.store(false, Ordering::Relaxed);
        progress.is_streaming.store(true, Ordering::Relaxed);
        progress.streaming_total.store(total_frames, Ordering::Relaxed);

        let encoder = OpusEncoder::<Sample, CHANNELS, SAMPLE_RATE>::new()?;

        while !ntp_service.is_synced() {
            if !is_running.load(Ordering::Relaxed) {
                progress.reset();
                return Ok(());
            }
            thread::sleep(Duration::from_millis(100));
        }

        let meta = SyncedStreamMeta {
            stream_id,
            file_name: file_name.clone(),
            total_frames,
        };
        network_sender.push(NetworkPacket::SyncedMeta(meta.clone()));
        synced_stream.receive_meta(LOCAL_ADDR, meta);

        let start_play_at = ntp_service.party_now() + BUFFER_AHEAD_MS * 1000;

        info!(
            "Music stream {} starting playback in {}ms",
            stream_id, BUFFER_AHEAD_MS
        );

        for (seq, buffer) in buffers.into_iter().enumerate() {
            if !is_running.load(Ordering::Relaxed) {
                info!("Music stream {} stopped", stream_id);
                progress.reset();
                return Ok(());
            }

            let opus_packet = match encoder.process(buffer) {
                Some(p) => p,
                None => continue,
            };

            let play_at = start_play_at + (seq as u64 * FRAME_DURATION_MS * 1000);

            let frame = SyncedFrame::new(stream_id, seq as u64 + 1, play_at, opus_packet);

            network_sender.push(NetworkPacket::Synced(frame.clone()));
            synced_stream.receive(LOCAL_ADDR, frame);

            let current = seq as u64 + 1;
            frames_sent.store(current, Ordering::Relaxed);
            progress.streaming_current.store(current, Ordering::Relaxed);

            let now = ntp_service.party_now();
            if play_at > now + BUFFER_AHEAD_MS * 1000 * 2 {
                let sleep_us = (play_at - now - BUFFER_AHEAD_MS * 1000) as u64;
                thread::sleep(Duration::from_micros(sleep_us.min(100_000)));
            }
        }

        is_complete.store(true, Ordering::Relaxed);
        progress.is_streaming.store(false, Ordering::Relaxed);
        info!("Music stream {} completed ({} frames)", stream_id, total_frames);

        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    pub fn info(&self) -> MusicStreamInfo {
        MusicStreamInfo {
            stream_id: self.stream_id,
            file_name: self.file_name.clone(),
            total_frames: self.total_frames,
            frames_sent: self.frames_sent.load(Ordering::Relaxed),
            is_complete: self.is_complete.load(Ordering::Relaxed),
        }
    }

    pub fn is_complete(&self) -> bool {
        self.is_complete.load(Ordering::Relaxed)
    }

    pub fn stream_id(&self) -> SyncedStreamId {
        self.stream_id
    }
}

impl Drop for MusicStream {
    fn drop(&mut self) {
        self.stop();
    }
}
