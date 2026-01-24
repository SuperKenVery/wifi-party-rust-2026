//! Music streaming pipeline for synced playback.
//!
//! Provides [`MusicStream`] which handles:
//! - On-the-fly decoding of audio files
//! - Fast-than-realtime encoding and sending (2x speed)
//! - Redundant packet transmission (2x redundancy)
//! - Handling retransmission requests from peers
//! - Playback control (Play, Pause, Seek)

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use dashmap::DashMap;
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::audio::file::AudioFileReader;
use crate::audio::opus::{OpusEncoder, OpusPacket};
use crate::io::NetworkSender;
use crate::party::ntp::NtpService;
use crate::party::stream::{NetworkPacket, SyncedControl};
use crate::party::sync_stream::{
    SyncedAudioStream, SyncedFrame, SyncedStreamId, SyncedStreamMeta, new_stream_id,
};
use crate::pipeline::{Node, Sink};
use crate::state::MusicStreamProgress;

const LOCAL_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
const FRAME_DURATION_US: u64 = 20_000; // 20ms
const SEND_RATE_MULTIPLIER: u32 = 2; // Send at 2x speed
const REDUNDANCY_COUNT: usize = 2; // Send every packet twice

pub struct MusicStreamInfo {
    pub stream_id: SyncedStreamId,
    pub file_name: String,
    pub total_frames: u64,
    pub frames_sent: u64,
    pub is_complete: bool,
}

enum MusicCommand {
    Retransmit(Vec<u64>),
    Pause,
    Resume,
    Seek(u64), // position in ms
}

pub struct MusicStream {
    stream_id: SyncedStreamId,
    file_name: String,
    total_frames: Arc<AtomicU64>,
    frames_encoded: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
    is_complete: Arc<AtomicBool>,
    command_tx: std::sync::mpsc::Sender<MusicCommand>,
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
        let total_frames = Arc::new(AtomicU64::new(0));
        let frames_encoded = Arc::new(AtomicU64::new(0));
        let is_running = Arc::new(AtomicBool::new(true));
        let is_complete = Arc::new(AtomicBool::new(false));
        let vault = Arc::new(DashMap::new());

        let (command_tx, command_rx) = std::sync::mpsc::channel();

        let ctx = StreamContext {
            path,
            stream_id,
            file_name: file_name.clone(),
            ntp_service,
            network_sender,
            synced_stream,
            progress,
            total_frames: total_frames.clone(),
            frames_encoded: frames_encoded.clone(),
            is_running: is_running.clone(),
            is_complete: is_complete.clone(),
            vault,
            command_rx,
        };

        let handle = thread::spawn(move || {
            if let Err(e) = ctx.run() {
                warn!("Music stream error: {}", e);
            }
        });

        Ok(Self {
            stream_id,
            file_name,
            total_frames,
            frames_encoded,
            is_running,
            is_complete,
            command_tx,
            _handle: Some(handle),
        })
    }

    pub fn handle_retransmission_request(&self, seqs: Vec<u64>) {
        let _ = self.command_tx.send(MusicCommand::Retransmit(seqs));
    }

    pub fn pause(&self) -> Result<()> {
        self.command_tx
            .send(MusicCommand::Pause)
            .context("Failed to send pause command")
    }

    pub fn resume(&self) -> Result<()> {
        self.command_tx
            .send(MusicCommand::Resume)
            .context("Failed to send resume command")
    }

    pub fn seek(&self, position_ms: u64) -> Result<()> {
        self.command_tx
            .send(MusicCommand::Seek(position_ms))
            .context("Failed to send seek command")
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::Relaxed);
    }

    pub fn info(&self) -> MusicStreamInfo {
        MusicStreamInfo {
            stream_id: self.stream_id,
            file_name: self.file_name.clone(),
            total_frames: self.total_frames.load(Ordering::Relaxed),
            frames_sent: self.frames_encoded.load(Ordering::Relaxed),
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

struct StreamContext<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    path: PathBuf,
    stream_id: SyncedStreamId,
    file_name: String,
    ntp_service: Arc<NtpService>,
    network_sender: NetworkSender,
    synced_stream: Arc<SyncedAudioStream<Sample, CHANNELS, SAMPLE_RATE>>,
    progress: Arc<MusicStreamProgress>,
    total_frames: Arc<AtomicU64>,
    frames_encoded: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
    is_complete: Arc<AtomicBool>,
    vault: Arc<DashMap<u64, OpusPacket>>,
    command_rx: std::sync::mpsc::Receiver<MusicCommand>,
}

impl<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>
    StreamContext<Sample, CHANNELS, SAMPLE_RATE>
{
    fn run(self) -> Result<()> {
        let mut reader = AudioFileReader::open(&self.path)?;
        let encoder = OpusEncoder::<Sample, CHANNELS, SAMPLE_RATE>::new()?;

        // Calculate total frames if possible
        if let Some(duration) = reader.info.duration_secs {
            let total = (duration * 1_000_000.0 / FRAME_DURATION_US as f64) as u64;
            self.total_frames.store(total, Ordering::Relaxed);
            self.progress.encoding_total.store(total, Ordering::Relaxed);
            self.progress
                .streaming_total
                .store(total, Ordering::Relaxed);
        }

        self.progress.is_encoding.store(true, Ordering::Relaxed);
        *self.progress.file_name.lock().unwrap() = Some(self.file_name.clone());

        // Send metadata
        let meta = SyncedStreamMeta {
            stream_id: self.stream_id,
            file_name: self.file_name.clone(),
            total_frames: self.total_frames.load(Ordering::Relaxed),
        };
        self.network_sender
            .push(NetworkPacket::SyncedMeta(meta.clone()));
        self.synced_stream.receive_meta(LOCAL_ADDR, meta);

        // Start with a "Start" command to begin playback in 500ms
        let start_at = self.ntp_service.party_now() + 500_000;
        let control = SyncedControl::Start {
            stream_id: self.stream_id,
            party_clock_time: start_at,
            seq: 1,
        };
        self.network_sender
            .push(NetworkPacket::SyncedControl(control.clone()));
        self.synced_stream.receive_control(LOCAL_ADDR, control);

        let mut next_seq_to_send = 1u64;
        let mut last_send_time = Instant::now();
        let mut retransmit_queue = VecDeque::<u64>::new();

        let mut last_pause_seq = 1u64;
        let mut last_start_party_time = start_at;
        let mut last_start_seq = 1u64;
        let mut _is_playing = true;

        while self.is_running.load(Ordering::Relaxed) {
            // 0. Conflict Detection
            // If any stream exists with a different ID, someone else (or a newer song) has taken over
            if self
                .synced_stream
                .active_streams()
                .iter()
                .any(|s| s.stream_id != self.stream_id)
            {
                info!(
                    "Another stream detected, stopping our stream {}",
                    self.stream_id
                );
                break;
            }

            // 1. Process Commands
            while let Ok(cmd) = self.command_rx.try_recv() {
                match cmd {
                    MusicCommand::Retransmit(seqs) => {
                        retransmit_queue.extend(seqs);
                    }
                    MusicCommand::Pause => {
                        let control = SyncedControl::Pause {
                            stream_id: self.stream_id,
                        };
                        self.network_sender
                            .push(NetworkPacket::SyncedControl(control.clone()));
                        self.synced_stream.receive_control(LOCAL_ADDR, control);

                        // Calculate where we paused
                        let party_now = self.ntp_service.party_now();
                        if party_now > last_start_party_time {
                            let elapsed = party_now - last_start_party_time;
                            last_pause_seq = last_start_seq + (elapsed / FRAME_DURATION_US);
                        } else {
                            last_pause_seq = last_start_seq;
                        }
                        _is_playing = false;
                    }
                    MusicCommand::Resume => {
                        let resume_at = self.ntp_service.party_now() + 200_000;
                        let control = SyncedControl::Start {
                            stream_id: self.stream_id,
                            party_clock_time: resume_at,
                            seq: last_pause_seq,
                        };
                        self.network_sender
                            .push(NetworkPacket::SyncedControl(control.clone()));
                        self.synced_stream.receive_control(LOCAL_ADDR, control);

                        last_start_party_time = resume_at;
                        last_start_seq = last_pause_seq;
                        _is_playing = true;
                    }
                    MusicCommand::Seek(pos_ms) => {
                        let seek_at = self.ntp_service.party_now() + 300_000;
                        let seq = ((pos_ms * 1000) / FRAME_DURATION_US).max(1);

                        // If we haven't encoded this far yet, seek the reader
                        let current_encoded = self.frames_encoded.load(Ordering::Relaxed);
                        if seq > current_encoded {
                            let seek_micros = (seq.saturating_sub(1)) * FRAME_DURATION_US;
                            if let Err(e) = reader.seek_micros(seek_micros) {
                                warn!(
                                    "Failed to seek reader to {} micros (seq {}): {}",
                                    seek_micros, seq, e
                                );
                            } else {
                                info!("Seeked reader to {} micros (seq {})", seek_micros, seq);
                                self.frames_encoded.store(seq - 1, Ordering::Relaxed);
                                self.is_complete.store(false, Ordering::Relaxed);
                                self.progress.is_encoding.store(true, Ordering::Relaxed);
                                // Note: This might leave gaps in the vault if we seek forward,
                                // but the protocol handles missing frames via retransmission
                                // or simply skipping them if they never arrive.
                            }
                        }

                        let control = SyncedControl::Start {
                            stream_id: self.stream_id,
                            party_clock_time: seek_at,
                            seq,
                        };
                        self.network_sender
                            .push(NetworkPacket::SyncedControl(control.clone()));
                        self.synced_stream.receive_control(LOCAL_ADDR, control);

                        last_start_party_time = seek_at;
                        last_start_seq = seq;
                        last_pause_seq = seq;
                        _is_playing = true;

                        // Reset next_seq_to_send to the seek position to prioritize sending new frames
                        next_seq_to_send = seq;
                    }
                }
            }

            // 2. Decode/Encode on-the-fly (as fast as possible to fill the vault)
            if !self.is_complete.load(Ordering::Relaxed) {
                for _ in 0..100 {
                    match reader.next_buffer::<Sample, CHANNELS, SAMPLE_RATE>()? {
                        Some(buffer) => {
                            if let Some(packet) = encoder.process(buffer) {
                                let seq = self.frames_encoded.fetch_add(1, Ordering::Relaxed) + 1;
                                self.vault.insert(seq, packet);
                                self.progress.encoding_current.store(seq, Ordering::Relaxed);
                            }
                        }
                        None => {
                            self.is_complete.store(true, Ordering::Relaxed);
                            self.progress.is_encoding.store(false, Ordering::Relaxed);
                            let final_total = self.frames_encoded.load(Ordering::Relaxed);
                            self.total_frames.store(final_total, Ordering::Relaxed);
                            // Update meta with final total
                            let meta = SyncedStreamMeta {
                                stream_id: self.stream_id,
                                file_name: self.file_name.clone(),
                                total_frames: final_total,
                            };
                            self.network_sender.push(NetworkPacket::SyncedMeta(meta));
                            break;
                        }
                    }
                }
            }

            // 3. Handle Retransmissions (High Priority)
            for _ in 0..10 {
                if let Some(seq) = retransmit_queue.pop_front() {
                    if let Some(packet) = self.vault.get(&seq) {
                        let frame = SyncedFrame::new(self.stream_id, seq, packet.value().clone());
                        self.network_sender.push(NetworkPacket::Synced(frame));
                    }
                } else {
                    break;
                }
            }

            // 4. Send New Frames (2x speed with redundancy)
            let now = Instant::now();
            let elapsed_us = now.duration_since(last_send_time).as_micros() as u64;
            let frames_to_send = (elapsed_us * SEND_RATE_MULTIPLIER as u64) / FRAME_DURATION_US;

            if frames_to_send > 0 {
                for _ in 0..frames_to_send {
                    if let Some(packet) = self.vault.get(&next_seq_to_send) {
                        let frame = SyncedFrame::new(
                            self.stream_id,
                            next_seq_to_send,
                            packet.value().clone(),
                        );

                        // Send with redundancy
                        for _ in 0..REDUNDANCY_COUNT {
                            self.network_sender
                                .push(NetworkPacket::Synced(frame.clone()));
                        }

                        // Also feed local stream
                        self.synced_stream.receive(LOCAL_ADDR, frame);

                        self.progress
                            .streaming_current
                            .store(next_seq_to_send, Ordering::Relaxed);
                        next_seq_to_send += 1;
                    } else if self.is_complete.load(Ordering::Relaxed)
                        && next_seq_to_send > self.total_frames.load(Ordering::Relaxed)
                    {
                        break;
                    } else {
                        // Waiting for encoder
                        break;
                    }
                }
                last_send_time = now;
            }

            thread::sleep(Duration::from_millis(10));
        }

        self.progress.reset();
        Ok(())
    }
}

impl Drop for MusicStream {
    fn drop(&mut self) {
        self.stop();
    }
}
