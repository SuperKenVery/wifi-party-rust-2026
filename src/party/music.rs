//! Music streaming pipeline for synced playback.
//!
//! Provides [`MusicStream`] which handles:
//! - Reading compressed audio packets from files (no re-encoding)
//! - Fast-than-realtime streaming (2x speed)
//! - Redundant packet transmission (2x redundancy)
//! - Handling retransmission requests from peers
//! - Playback control (Play, Pause, Seek)

use std::collections::VecDeque;
use std::io::Cursor;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use dashmap::DashMap;
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::{info, warn};

use crate::audio::AudioSample;
use crate::audio::symphonia_compat::WireCodecParams;
use crate::io::NetworkSender;
use crate::party::ntp::NtpService;
use crate::party::realtime_stream::NetworkPacket;
use crate::party::sync_stream::SyncedControl;
use crate::party::sync_stream::{
    RawPacket, SyncedAudioStreamManager, SyncedFrame, SyncedStreamId, SyncedStreamMeta,
    new_stream_id,
};
use crate::pipeline::Pushable;
use crate::state::MusicStreamProgress;

const LOCAL_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 0);
const SEND_RATE_MULTIPLIER: u32 = 2;
const REDUNDANCY_COUNT: usize = 2;

enum MusicCommand {
    Retransmit(Vec<u64>),
    Pause,
    Resume,
    Seek(u64),
}

/// Handle for controlling an active music stream.
/// Created by `MusicStream::start()`, kept by caller.
pub struct MusicStream {
    stream_id: SyncedStreamId,
    is_running: Arc<AtomicBool>,
    command_tx: std::sync::mpsc::Sender<MusicCommand>,
    _handle: Option<thread::JoinHandle<()>>,
}

impl MusicStream {
    pub fn start<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        data: Vec<u8>,
        file_name: String,
        ntp_service: Arc<NtpService>,
        network_sender: NetworkSender,
        synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
        progress: Arc<MusicStreamProgress>,
    ) -> Result<Self> {
        info!("Starting music stream for: {}", file_name);

        let extension = file_name.rsplit('.').next().map(|s| s.to_lowercase());
        let source = AudioSource::open(data, extension.as_deref())?;
        let codec_params = WireCodecParams::from_symphonia(&source.codec_params())
            .ok_or_else(|| anyhow!("Unsupported codec"))?;

        let stream_id = new_stream_id();
        let is_running = Arc::new(AtomicBool::new(true));
        let vault: Arc<DashMap<u64, RawPacket>> = Arc::new(DashMap::new());
        let (command_tx, command_rx) = std::sync::mpsc::channel();

        progress.is_streaming.store(true, Ordering::Relaxed);
        *progress.file_name.lock().unwrap() = Some(file_name.clone());

        let meta = SyncedStreamMeta {
            stream_id,
            file_name,
            total_frames: 0,
            total_samples: 0,
            codec_params,
        };
        network_sender.push(NetworkPacket::SyncedMeta(meta.clone()));
        synced_stream.receive_meta(LOCAL_ADDR, meta.clone());

        let start_at = ntp_service.party_now() + 500_000;
        let control = SyncedControl::Start {
            stream_id,
            party_clock_time: start_at,
            seq: 1,
        };
        network_sender.push(NetworkPacket::SyncedControl(control.clone()));
        synced_stream.receive_control(LOCAL_ADDR, control);

        let ctx = StreamContext {
            source,
            meta,
            ntp_service,
            network_sender,
            synced_stream,
            progress,
            is_running: is_running.clone(),
            vault,
            command_rx,
            frames_read: 0,
            is_complete: false,
            next_seq_to_send: 1,
            last_send_time: Instant::now(),
            retransmit_queue: VecDeque::new(),
            last_pause_seq: 1,
            last_start_party_time: start_at,
            last_start_seq: 1,
        };

        let handle = thread::spawn(move || {
            if let Err(e) = ctx.run() {
                warn!("Music stream error: {}", e);
            }
        });

        Ok(Self {
            stream_id,
            is_running,
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

    pub fn stream_id(&self) -> SyncedStreamId {
        self.stream_id
    }
}

impl Drop for MusicStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Audio source - wraps symphonia format reader for a single track.
struct AudioSource {
    format: Box<dyn FormatReader>,
    track_id: u32,
    duration_secs: Option<f64>,
}

impl AudioSource {
    fn open(data: Vec<u8>, extension: Option<&str>) -> Result<Self> {
        let cursor = Cursor::new(data);
        let mss = MediaSourceStream::new(Box::new(cursor), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = extension {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .context("Failed to probe audio format")?;

        let format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("No supported audio track found"))?;

        let track_id = track.id;
        let sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| anyhow!("Unknown sample rate"))?;

        let duration_secs = track
            .codec_params
            .n_frames
            .map(|frames| frames as f64 / sample_rate as f64);

        Ok(Self {
            format,
            track_id,
            duration_secs,
        })
    }

    fn codec_params(&self) -> &symphonia::core::codecs::CodecParameters {
        &self.format.tracks()[0].codec_params
    }

    fn next_packet(&mut self) -> Result<Option<RawPacket>, symphonia::core::errors::Error> {
        loop {
            match self.format.next_packet() {
                Ok(packet) => {
                    if packet.track_id() != self.track_id {
                        continue;
                    }
                    return Ok(Some(RawPacket {
                        dur: packet.dur as u32,
                        data: packet.data.to_vec(),
                    }));
                }
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn seek(&mut self, timestamp: u64) -> Result<(), symphonia::core::errors::Error> {
        self.format.seek(
            SeekMode::Accurate,
            SeekTo::TimeStamp {
                ts: timestamp,
                track_id: self.track_id,
            },
        )?;
        Ok(())
    }
}

/// Worker context for the streaming thread.
/// Moved into the thread and consumed by `run()`.
struct StreamContext<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    source: AudioSource,
    meta: SyncedStreamMeta,

    ntp_service: Arc<NtpService>,
    network_sender: NetworkSender,
    synced_stream: Arc<SyncedAudioStreamManager<Sample, CHANNELS, SAMPLE_RATE>>,
    progress: Arc<MusicStreamProgress>,

    is_running: Arc<AtomicBool>,
    vault: Arc<DashMap<u64, RawPacket>>,
    command_rx: std::sync::mpsc::Receiver<MusicCommand>,

    frames_read: u64,
    is_complete: bool,
    next_seq_to_send: u64,
    last_send_time: Instant,
    retransmit_queue: VecDeque<u64>,
    last_pause_seq: u64,
    last_start_party_time: u64,
    last_start_seq: u64,
}

impl<Sample: AudioSample + 'static, const CHANNELS: usize, const SAMPLE_RATE: u32>
    StreamContext<Sample, CHANNELS, SAMPLE_RATE>
{
    fn run(mut self) -> Result<()> {
        self.init_with_duration();
        self.progress.is_encoding.store(true, Ordering::Relaxed);

        while self.is_running.load(Ordering::Relaxed) {
            if self.should_stop_for_other_stream() {
                break;
            }

            self.handle_commands();
            self.read_packets();
            self.send_retransmissions();
            self.send_packets();

            thread::sleep(Duration::from_millis(10));
        }

        self.progress.reset();
        Ok(())
    }

    fn sample_rate(&self) -> u32 {
        self.meta.codec_params.sample_rate
    }

    fn init_with_duration(&mut self) {
        let Some(duration) = self.source.duration_secs else {
            return;
        };

        let est_packets = (duration * self.sample_rate() as f64 / 1152.0) as u64;
        let total_samples = (duration * self.sample_rate() as f64) as u64;
        self.meta.total_frames = est_packets;
        self.meta.total_samples = total_samples;
        self.progress
            .encoding_total
            .store(est_packets, Ordering::Relaxed);
        self.progress
            .streaming_total
            .store(est_packets, Ordering::Relaxed);

        self.network_sender
            .push(NetworkPacket::SyncedMeta(self.meta.clone()));
        self.synced_stream
            .receive_meta(LOCAL_ADDR, self.meta.clone());
    }

    fn should_stop_for_other_stream(&self) -> bool {
        let dominated = self
            .synced_stream
            .active_streams()
            .iter()
            .any(|s| s.stream_id != self.meta.stream_id);
        if dominated {
            info!(
                "Another stream detected, stopping our stream {}",
                self.meta.stream_id
            );
        }
        dominated
    }

    fn handle_commands(&mut self) {
        while let Ok(cmd) = self.command_rx.try_recv() {
            match cmd {
                MusicCommand::Retransmit(seqs) => {
                    self.retransmit_queue.extend(seqs);
                }
                MusicCommand::Pause => self.handle_pause(),
                MusicCommand::Resume => self.handle_resume(),
                MusicCommand::Seek(pos_ms) => self.handle_seek(pos_ms),
            }
        }
    }

    fn handle_pause(&mut self) {
        let control = SyncedControl::Pause {
            stream_id: self.meta.stream_id,
        };
        self.network_sender
            .push(NetworkPacket::SyncedControl(control.clone()));
        self.synced_stream.receive_control(LOCAL_ADDR, control);

        let party_now = self.ntp_service.party_now();
        if party_now > self.last_start_party_time {
            let elapsed_us = party_now - self.last_start_party_time;
            let elapsed_samples = elapsed_us * self.sample_rate() as u64 / 1_000_000;
            self.last_pause_seq = self.find_seq_at_samples(self.last_start_seq, elapsed_samples);
        } else {
            self.last_pause_seq = self.last_start_seq;
        }
    }

    fn handle_resume(&mut self) {
        let resume_at = self.ntp_service.party_now() + 200_000;
        let control = SyncedControl::Start {
            stream_id: self.meta.stream_id,
            party_clock_time: resume_at,
            seq: self.last_pause_seq,
        };
        self.network_sender
            .push(NetworkPacket::SyncedControl(control.clone()));
        self.synced_stream.receive_control(LOCAL_ADDR, control);

        self.last_start_party_time = resume_at;
        self.last_start_seq = self.last_pause_seq;
    }

    fn handle_seek(&mut self, pos_ms: u64) {
        let seek_at = self.ntp_service.party_now() + 300_000;
        let target_samples = pos_ms * self.sample_rate() as u64 / 1000;
        let seq = self.find_seq_at_samples(1, target_samples);

        // If seeking beyond what we've read, seek the format reader
        if seq > self.frames_read {
            if let Err(e) = self.source.seek(target_samples) {
                warn!("Failed to seek: {}", e);
            } else {
                info!("Seeked to {} samples (seq {})", target_samples, seq);
                self.frames_read = seq - 1;
                self.is_complete = false;
                self.progress.is_encoding.store(true, Ordering::Relaxed);
            }
        }

        let control = SyncedControl::Start {
            stream_id: self.meta.stream_id,
            party_clock_time: seek_at,
            seq,
        };
        self.network_sender
            .push(NetworkPacket::SyncedControl(control.clone()));
        self.synced_stream.receive_control(LOCAL_ADDR, control);

        self.last_start_party_time = seek_at;
        self.last_start_seq = seq;
        self.last_pause_seq = seq;
        self.next_seq_to_send = seq;
    }

    fn find_seq_at_samples(&self, start_seq: u64, target_samples: u64) -> u64 {
        let mut cum = 0u64;
        let mut seq = start_seq;
        while let Some(pkt) = self.vault.get(&seq) {
            if cum >= target_samples {
                break;
            }
            cum += pkt.dur as u64;
            seq += 1;
        }
        seq
    }

    fn read_packets(&mut self) {
        if self.is_complete {
            return;
        }

        for _ in 0..100 {
            match self.source.next_packet() {
                Ok(Some(raw)) => {
                    self.frames_read += 1;
                    self.vault.insert(self.frames_read, raw);
                    self.progress
                        .encoding_current
                        .store(self.frames_read, Ordering::Relaxed);
                }
                Ok(None) => {
                    // EOF - calculate exact total_samples from all packets
                    self.is_complete = true;
                    self.progress.is_encoding.store(false, Ordering::Relaxed);
                    self.meta.total_frames = self.frames_read;
                    self.meta.total_samples = self.vault.iter().map(|r| r.dur as u64).sum();
                    self.network_sender
                        .push(NetworkPacket::SyncedMeta(self.meta.clone()));
                    break;
                }
                Err(_) => break,
            }
        }
    }

    fn send_retransmissions(&mut self) {
        for _ in 0..10 {
            let Some(seq) = self.retransmit_queue.pop_front() else {
                break;
            };
            if let Some(packet) = self.vault.get(&seq) {
                let frame =
                    SyncedFrame::new(self.meta.stream_id, seq, packet.dur, packet.data.clone());
                self.network_sender.push(NetworkPacket::Synced(frame));
            }
        }
    }

    fn send_packets(&mut self) {
        let now = Instant::now();
        let elapsed_us = now.duration_since(self.last_send_time).as_micros() as u64;
        let avg_frame_dur_us = 20_000u64;
        let frames_to_send = (elapsed_us * SEND_RATE_MULTIPLIER as u64) / avg_frame_dur_us;

        if frames_to_send == 0 {
            return;
        }

        for _ in 0..frames_to_send {
            if let Some(packet) = self.vault.get(&self.next_seq_to_send) {
                let frame = SyncedFrame::new(
                    self.meta.stream_id,
                    self.next_seq_to_send,
                    packet.dur,
                    packet.data.clone(),
                );

                for _ in 0..REDUNDANCY_COUNT {
                    self.network_sender
                        .push(NetworkPacket::Synced(frame.clone()));
                }
                self.synced_stream.receive(LOCAL_ADDR, frame);

                self.progress
                    .streaming_current
                    .store(self.next_seq_to_send, Ordering::Relaxed);
                self.next_seq_to_send += 1;
            } else if self.is_complete && self.next_seq_to_send > self.meta.total_frames {
                break;
            } else {
                break;
            }
        }
        self.last_send_time = now;
    }
}
