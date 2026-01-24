//! Realtime audio stream for network transport.
//!
//! This module handles realtime audio streams (mic, system audio) that are
//! played immediately upon receipt with minimal latency.
//!
//! For synchronized music playback, see [`sync_stream`](super::sync_stream).

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rkyv::{Archive, Deserialize, Serialize};
use tracing::{info, warn};

use crate::audio::frame::AudioBuffer;
use crate::audio::opus::OpusPacket;
use crate::audio::{AudioSample, JitterBuffer, PullSnapshot};
use crate::party::ntp::NtpPacket;
use crate::party::sync_stream::{SyncedFrame, SyncedStreamMeta};
use crate::pipeline::{Sink, Source};
use crate::state::HostId;

pub use crate::audio::PullSnapshot as StreamSnapshot;

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const JITTER_BUFFER_CAPACITY: usize = 64;

/// Identifies a realtime audio stream instance.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[rkyv(compare(PartialEq))]
pub enum RealtimeStreamId {
    Mic,
    System,
}

impl std::fmt::Display for RealtimeStreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RealtimeStreamId::Mic => write!(f, "Mic"),
            RealtimeStreamId::System => write!(f, "System"),
        }
    }
}

/// Frame format for realtime audio streams (Opus-encoded).
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct RealtimeFrame {
    pub stream_id: RealtimeStreamId,
    pub sequence_number: u64,
    pub timestamp: u64,
    pub opus_data: Vec<u8>,
    pub frame_size: u32,
}

impl RealtimeFrame {
    pub fn new(stream_id: RealtimeStreamId, sequence_number: u64, opus_packet: OpusPacket) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            stream_id,
            sequence_number,
            timestamp,
            opus_data: opus_packet.data,
            frame_size: opus_packet.frame_size as u32,
        }
    }

    pub fn to_opus_packet(&self) -> OpusPacket {
        OpusPacket {
            data: self.opus_data.clone(),
            frame_size: self.frame_size as usize,
        }
    }
}

/// Top-level network packet enum.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum NetworkPacket {
    Realtime(RealtimeFrame),
    Synced(SyncedFrame),
    SyncedMeta(SyncedStreamMeta),
    Ntp(NtpPacket),
}

/// Key for identifying a specific jitter buffer.
/// We use SocketAddr (IP + Port) here to distinguish between multiple
/// instances running on the same machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BufferKey {
    source_addr: SocketAddr,
    stream_id: RealtimeStreamId,
}

/// Metadata for a buffer entry with Opus decoder.
struct BufferEntry<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffer: JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    decoder: crate::audio::opus::OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>,
    last_seen: Instant,
}

/// Manages all realtime audio streams across all hosts.
pub struct RealtimeAudioStream<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffers: DashMap<BufferKey, BufferEntry<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            buffers: DashMap::new(),
        }
    }

    /// Receives a realtime frame and routes it to the appropriate jitter buffer.
    pub fn receive(&self, source_addr: SocketAddr, frame: RealtimeFrame) {
        let key = BufferKey {
            source_addr,
            stream_id: frame.stream_id,
        };

        let mut entry = self.buffers.entry(key).or_insert_with(|| {
            info!(
                "Creating jitter buffer for source {} stream {:?}",
                source_addr, frame.stream_id
            );
            let decoder =
                crate::audio::opus::OpusDecoder::new().expect("Failed to create Opus decoder");
            BufferEntry {
                buffer: JitterBuffer::new(JITTER_BUFFER_CAPACITY),
                decoder,
                last_seen: Instant::now(),
            }
        });

        entry.last_seen = Instant::now();

        let opus_packet = frame.to_opus_packet();
        if let Some(pcm_buffer) = entry.decoder.decode_packet(&opus_packet) {
            use crate::audio::frame::AudioFrame;
            let jitter_frame = AudioFrame {
                sequence_number: frame.sequence_number,
                timestamp: frame.timestamp,
                samples: pcm_buffer,
            };
            entry.buffer.push(jitter_frame);
        }
    }

    /// Pulls `len` samples from all buffers and mixes them together.
    pub fn pull_and_mix(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut mixed: Vec<i64> = vec![0; len];
        let mut source_count = 0usize;

        for entry in self.buffers.iter() {
            if let Some(frame) = entry.value().buffer.pull(len) {
                source_count += 1;
                for (i, sample) in frame.samples.data().iter().enumerate() {
                    if i < len {
                        mixed[i] += sample.to_i64_for_mix();
                    }
                }
            }
        }

        if source_count == 0 {
            return None;
        }

        let samples: Vec<Sample> = mixed
            .into_iter()
            .map(|s| Sample::from_i64_mixed(s, source_count))
            .collect();
        AudioBuffer::new(samples).ok()
    }

    /// Removes buffers that haven't received data within the timeout period.
    pub fn cleanup_stale(&self) {
        let now = Instant::now();
        self.buffers.retain(|key, entry| {
            let alive = now.duration_since(entry.last_seen) < HOST_TIMEOUT;
            if !alive {
                info!(
                    "Removing stale buffer for source {} stream {:?}",
                    key.source_addr, key.stream_id
                );
            }
            alive
        });
    }

    /// Returns the number of active buffers.
    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    /// Returns a list of unique active host IDs (IP addresses).
    pub fn active_hosts(&self) -> Vec<HostId> {
        let mut hosts = Vec::new();
        for entry in self.buffers.iter() {
            let host_id = HostId::from(entry.key().source_addr.ip());
            if !hosts.contains(&host_id) {
                hosts.push(host_id);
            }
        }
        hosts
    }

    /// Returns stats for all streams belonging to a specific host (IP).
    pub fn host_stream_stats(&self, host_id: HostId) -> Vec<StreamStats> {
        let mut result = Vec::new();

        for entry in self.buffers.iter() {
            if entry.key().source_addr.ip() != host_id.ip() {
                continue;
            }

            let stream_id = entry.key().stream_id;
            let stats = entry.value().buffer.stats();

            let stream_name =
                if self.has_multiple_instances(entry.key().source_addr.ip(), stream_id) {
                    format!("{} (:{})", stream_id, entry.key().source_addr.port())
                } else {
                    stream_id.to_string()
                };

            result.push(StreamStats {
                stream_id: stream_name,
                packet_loss: stats.loss_rate() as f32,
                target_latency: stats.target_latency() as f32,
                audio_level: stats.audio_level(),
            });
        }

        result
    }

    fn has_multiple_instances(&self, ip: std::net::IpAddr, stream_id: RealtimeStreamId) -> bool {
        let mut count = 0;
        for entry in self.buffers.iter() {
            if entry.key().source_addr.ip() == ip && entry.key().stream_id == stream_id {
                count += 1;
                if count > 1 {
                    return true;
                }
            }
        }
        false
    }

    /// Returns snapshots for a specific stream, identified by stream_id string.
    /// The stream_id should match the format returned by host_stream_stats().
    pub fn stream_snapshots(&self, host_id: HostId, stream_id: &str) -> Vec<PullSnapshot> {
        for entry in self.buffers.iter() {
            if entry.key().source_addr.ip() != host_id.ip() {
                continue;
            }

            let sid = entry.key().stream_id;
            let stream_name = if self.has_multiple_instances(entry.key().source_addr.ip(), sid) {
                format!("{} (:{})", sid, entry.key().source_addr.port())
            } else {
                sid.to_string()
            };

            if stream_name == stream_id {
                return entry.value().buffer.stats().recent_snapshots();
            }
        }
        Vec::new()
    }
}

/// Statistics for a single audio stream.
#[derive(Debug, Clone)]
pub struct StreamStats {
    pub stream_id: String,
    pub packet_loss: f32,
    pub target_latency: f32,
    pub audio_level: u32,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for std::sync::Arc<RealtimeAudioStream<Sample, CHANNELS, SAMPLE_RATE>>
{
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pull_and_mix(len)
    }
}

/// Packs OpusPacket into NetworkPacket::Realtime with the given stream ID.
///
/// Each instance maintains its own sequence counter for independent
/// packet ordering per stream.
pub struct RealtimeFramePacker {
    stream_id: RealtimeStreamId,
    sequence_number: AtomicU64,
}

impl RealtimeFramePacker {
    pub fn new(stream_id: RealtimeStreamId) -> Self {
        Self {
            stream_id,
            sequence_number: AtomicU64::new(0),
        }
    }
}

impl crate::pipeline::Node for RealtimeFramePacker {
    type Input = OpusPacket;
    type Output = NetworkPacket;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let seq = self.sequence_number.fetch_add(1, Ordering::Relaxed) + 1;
        let frame = RealtimeFrame::new(self.stream_id, seq, input);
        Some(NetworkPacket::Realtime(frame))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::OpusEncoder;
    use crate::audio::frame::AudioBuffer;
    use crate::pipeline::Node;

    #[test]
    fn test_realtime_frame_creation() {
        let opus_packet = OpusPacket {
            data: vec![0u8; 100],
            frame_size: 960 * 2,
        };
        let frame = RealtimeFrame::new(RealtimeStreamId::Mic, 1, opus_packet);

        assert_eq!(frame.stream_id, RealtimeStreamId::Mic);
        assert_eq!(frame.sequence_number, 1);
        assert_eq!(frame.frame_size, 960 * 2);
    }

    #[test]
    fn test_network_packet_realtime() {
        let opus_packet = OpusPacket {
            data: vec![0u8; 100],
            frame_size: 960 * 2,
        };
        let frame = RealtimeFrame::new(RealtimeStreamId::System, 42, opus_packet);
        let packet = NetworkPacket::Realtime(frame);

        match packet {
            NetworkPacket::Realtime(f) => {
                assert_eq!(f.stream_id, RealtimeStreamId::System);
                assert_eq!(f.sequence_number, 42);
            }
            _ => panic!("Expected Realtime packet"),
        }
    }

    #[test]
    fn test_opus_encode_decode_roundtrip() {
        use crate::audio::opus::OpusDecoder;

        let encoder = OpusEncoder::<f32, 2, 48000>::new().unwrap();
        let decoder = OpusDecoder::<f32, 2, 48000>::new().unwrap();

        // Opus has ~120 samples/channel lookahead (240 total for stereo)
        // We need multiple frames to get meaningful decoded data
        let mut all_original: Vec<f32> = Vec::new();
        let mut all_decoded: Vec<f32> = Vec::new();

        for frame_num in 0..5 {
            let samples: Vec<f32> = (0..1920)
                .map(|i| ((i as f32 + frame_num as f32 * 1920.0) * 0.1).sin() * 0.5)
                .collect();
            all_original.extend(&samples);

            let input = AudioBuffer::<f32, 2, 48000>::new(samples).unwrap();
            let opus_packet = encoder.process(input).unwrap();
            let decoded = decoder.decode_packet(&opus_packet).unwrap();
            all_decoded.extend(decoded.data());
        }

        // Compare with lookahead offset (120 samples/channel * 2 channels = 240)
        let offset = 240;
        let compare_len = all_original.len() - offset;

        let mut max_diff: f32 = 0.0;
        let mut large_diff_count = 0;
        for i in 0..compare_len {
            let orig = all_original[i];
            let dec = all_decoded[i + offset];
            let diff = (orig - dec).abs();
            if diff > max_diff {
                max_diff = diff;
            }
            if diff > 0.3 {
                large_diff_count += 1;
            }
        }

        eprintln!(
            "Max diff: {}, large diff count: {}/{}",
            max_diff, large_diff_count, compare_len
        );
        assert!(
            large_diff_count < compare_len / 10,
            "Too many samples differ significantly: {}/{}",
            large_diff_count,
            compare_len
        );
    }

    #[test]
    fn test_realtime_stream_receive_and_pull_content() {
        use std::net::SocketAddr;

        let encoder = OpusEncoder::<f32, 2, 48000>::new().unwrap();
        let stream = RealtimeAudioStream::<f32, 2, 48000>::new();
        let source_addr = "127.0.0.1:12345".parse::<SocketAddr>().unwrap();

        let mut all_pulled: Vec<f32> = Vec::new();

        for seq in 1..=10u64 {
            let samples: Vec<f32> = (0..1920)
                .map(|i| ((i as f32 + seq as f32 * 1920.0) * 0.1).sin() * 0.5)
                .collect();

            let input = AudioBuffer::<f32, 2, 48000>::new(samples).unwrap();
            let opus_packet = encoder.process(input).unwrap();
            let frame = RealtimeFrame::new(RealtimeStreamId::Mic, seq, opus_packet);
            stream.receive(source_addr, frame);

            let pulled = stream.pull_and_mix(1920);
            assert!(pulled.is_some(), "Pull {} should have data", seq);
            let data = pulled.unwrap();
            assert_eq!(data.data().len(), 1920, "Pull {} wrong length", seq);
            all_pulled.extend(data.data());
        }

        let non_zero_count = all_pulled.iter().filter(|&&x| x.abs() > 0.001).count();
        eprintln!("Non-zero samples: {}/{}", non_zero_count, all_pulled.len());
        assert!(
            non_zero_count > all_pulled.len() / 2,
            "Too many near-zero samples: {}/{}",
            all_pulled.len() - non_zero_count,
            all_pulled.len()
        );

        let mut prev = all_pulled[0];
        let mut change_count = 0;
        for &sample in &all_pulled[1..] {
            if (sample - prev).abs() > 0.001 {
                change_count += 1;
            }
            prev = sample;
        }
        eprintln!("Sample changes: {}/{}", change_count, all_pulled.len() - 1);
        assert!(
            change_count > all_pulled.len() / 2,
            "Data doesn't vary enough: {}/{}",
            change_count,
            all_pulled.len() - 1
        );
    }

    #[test]
    fn test_realtime_stream_pull_exact_length() {
        use std::net::SocketAddr;
        let encoder = OpusEncoder::<f32, 2, 48000>::new().unwrap();
        let stream = RealtimeAudioStream::<f32, 2, 48000>::new();
        let source_addr = "127.0.0.1:12345".parse::<SocketAddr>().unwrap();

        // Send enough frames
        for seq in 1..=10u64 {
            let samples: Vec<f32> = (0..1920).map(|_| 0.1).collect();
            let input = AudioBuffer::<f32, 2, 48000>::new(samples).unwrap();
            let opus_packet = encoder.process(input).unwrap();
            let frame = RealtimeFrame::new(RealtimeStreamId::Mic, seq, opus_packet);
            stream.receive(source_addr, frame);
        }

        // Test various pull lengths
        let test_lengths = [100, 500, 1920, 2000, 3000];
        for &len in &test_lengths {
            let pulled = stream.pull_and_mix(len);
            assert!(pulled.is_some(), "pull_and_mix({}) returned None", len);
            assert_eq!(
                pulled.unwrap().data().len(),
                len,
                "pull_and_mix({}) returned wrong length",
                len
            );
        }
    }

    #[test]
    #[ignore]
    fn test_local_simulation_to_wav() {
        use hound::{WavSpec, WavWriter};
        use std::net::SocketAddr;

        let encoder = OpusEncoder::<f32, 2, 48000>::new().unwrap();
        let stream = RealtimeAudioStream::<f32, 2, 48000>::new();
        let source_addr = "127.0.0.1:12345".parse::<SocketAddr>().unwrap();

        let sample_rate = 48000;
        let duration_secs = 3;
        let total_samples = sample_rate * duration_secs * 2;
        let frame_size = 1920;
        let num_frames = total_samples / frame_size;

        eprintln!(
            "Generating {} seconds of audio ({} frames)",
            duration_secs, num_frames
        );

        let mut original_samples: Vec<f32> = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let freq = 440.0;
            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            original_samples.push(sample);
        }

        let mut pulled_samples: Vec<f32> = Vec::with_capacity(total_samples);
        let pull_size = frame_size;

        for seq in 1..=num_frames as u64 {
            let start = ((seq - 1) as usize) * frame_size;
            let end = start + frame_size;
            let samples = original_samples[start..end].to_vec();

            let input = AudioBuffer::<f32, 2, 48000>::new(samples).unwrap();
            let opus_packet = encoder.process(input).unwrap();
            let frame = RealtimeFrame::new(RealtimeStreamId::Mic, seq, opus_packet);
            stream.receive(source_addr, frame);

            if seq >= 3 {
                if let Some(data) = stream.pull_and_mix(pull_size) {
                    pulled_samples.extend(data.data());
                }
            }
        }

        while pulled_samples.len() < total_samples {
            if let Some(data) = stream.pull_and_mix(pull_size) {
                pulled_samples.extend(data.data());
            } else {
                break;
            }
        }

        eprintln!(
            "Pulled {} samples (expected {})",
            pulled_samples.len(),
            total_samples
        );

        let spec = WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let original_path = "/tmp/original.wav";
        let mut original_writer = WavWriter::create(original_path, spec).unwrap();
        for &sample in &original_samples {
            original_writer.write_sample(sample).unwrap();
        }
        original_writer.finalize().unwrap();
        eprintln!("Wrote original audio to {}", original_path);

        let decoded_path = "/tmp/decoded.wav";
        let mut decoded_writer = WavWriter::create(decoded_path, spec).unwrap();
        for &sample in &pulled_samples {
            decoded_writer.write_sample(sample).unwrap();
        }
        decoded_writer.finalize().unwrap();
        eprintln!("Wrote decoded audio to {}", decoded_path);

        let zero_count = pulled_samples.iter().filter(|&&x| x.abs() < 0.001).count();
        eprintln!(
            "Zero/near-zero samples in pulled: {}/{}",
            zero_count,
            pulled_samples.len()
        );

        let mut discontinuities = 0;
        let mut max_jump: f32 = 0.0;
        for i in 1..pulled_samples.len() {
            let jump = (pulled_samples[i] - pulled_samples[i - 1]).abs();
            if jump > max_jump {
                max_jump = jump;
            }
            if jump > 0.5 {
                discontinuities += 1;
                if discontinuities <= 10 {
                    eprintln!(
                        "Discontinuity at {}: {} -> {} (jump={})",
                        i,
                        pulled_samples[i - 1],
                        pulled_samples[i],
                        jump
                    );
                }
            }
        }
        eprintln!(
            "Discontinuities (jump > 0.5): {}, max_jump: {}",
            discontinuities, max_jump
        );

        eprintln!("Listen to /tmp/original.wav and /tmp/decoded.wav to compare");
    }

    #[test]
    #[ignore]
    fn test_simulated_realtime_to_wav() {
        use hound::{WavSpec, WavWriter};
        use std::net::SocketAddr;
        use std::sync::Arc;
        use std::thread;
        use std::time::{Duration, Instant};

        let encoder = Arc::new(OpusEncoder::<f32, 2, 48000>::new().unwrap());
        let stream = Arc::new(RealtimeAudioStream::<f32, 2, 48000>::new());
        let source_addr = "127.0.0.1:12345".parse::<SocketAddr>().unwrap();

        let sample_rate = 48000;
        let duration_secs = 3;
        let total_samples = sample_rate * duration_secs * 2;
        let frame_size = 1920;
        let frame_duration_ms = (frame_size as f64 / 2.0) / (sample_rate as f64) * 1000.0;

        eprintln!(
            "Frame duration: {:.2}ms, simulating {} seconds",
            frame_duration_ms, duration_secs
        );

        let mut original_samples: Vec<f32> = Vec::with_capacity(total_samples);
        for i in 0..total_samples {
            let t = i as f32 / sample_rate as f32;
            let freq = 440.0;
            let sample = (2.0 * std::f32::consts::PI * freq * t).sin() * 0.5;
            original_samples.push(sample);
        }
        let original_samples = Arc::new(original_samples);

        let stream_clone = Arc::clone(&stream);
        let encoder_clone = Arc::clone(&encoder);
        let original_clone = Arc::clone(&original_samples);

        let sender = thread::spawn(move || {
            let num_frames = total_samples / frame_size;
            let start = Instant::now();

            for seq in 1..=num_frames as u64 {
                let frame_start = ((seq - 1) as usize) * frame_size;
                let frame_end = frame_start + frame_size;
                let samples = original_clone[frame_start..frame_end].to_vec();

                let input = AudioBuffer::<f32, 2, 48000>::new(samples).unwrap();
                let opus_packet = encoder_clone.process(input).unwrap();
                let frame = RealtimeFrame::new(RealtimeStreamId::Mic, seq, opus_packet);
                stream_clone.receive(source_addr, frame);

                let expected_time =
                    Duration::from_secs_f64(seq as f64 * frame_duration_ms / 1000.0);
                let elapsed = start.elapsed();
                if expected_time > elapsed {
                    thread::sleep(expected_time - elapsed);
                }
            }
            eprintln!("Sender finished");
        });

        let stream_clone = Arc::clone(&stream);
        let receiver = thread::spawn(move || {
            let mut pulled_samples: Vec<f32> = Vec::with_capacity(total_samples);
            let pull_size = 256;
            let pull_interval =
                Duration::from_secs_f64(pull_size as f64 / 2.0 / sample_rate as f64);
            let start = Instant::now();

            while stream_clone.buffers.is_empty() {
                thread::sleep(Duration::from_millis(1));
            }

            thread::sleep(Duration::from_millis(50));

            while pulled_samples.len() < total_samples {
                let pull_start = Instant::now();

                if let Some(data) = stream_clone.pull_and_mix(pull_size) {
                    pulled_samples.extend(data.data());
                } else {
                    pulled_samples.extend(std::iter::repeat(0.0f32).take(pull_size));
                }

                let pull_elapsed = pull_start.elapsed();
                if pull_interval > pull_elapsed {
                    thread::sleep(pull_interval - pull_elapsed);
                }
            }

            pulled_samples
        });

        sender.join().unwrap();
        let pulled_samples = receiver.join().unwrap();

        let spec = WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let original_path = "/tmp/original_realtime.wav";
        let mut original_writer = WavWriter::create(original_path, spec).unwrap();
        for &sample in original_samples.iter() {
            original_writer.write_sample(sample).unwrap();
        }
        original_writer.finalize().unwrap();
        eprintln!("Wrote original audio to {}", original_path);

        let decoded_path = "/tmp/decoded_realtime.wav";
        let mut decoded_writer = WavWriter::create(decoded_path, spec).unwrap();
        for &sample in &pulled_samples[..total_samples.min(pulled_samples.len())] {
            decoded_writer.write_sample(sample).unwrap();
        }
        decoded_writer.finalize().unwrap();
        eprintln!("Wrote decoded audio to {}", decoded_path);

        let zero_count = pulled_samples.iter().filter(|&&x| x.abs() < 0.001).count();
        eprintln!(
            "Zero/near-zero samples in pulled: {}/{}",
            zero_count,
            pulled_samples.len()
        );

        let mut discontinuities = 0;
        let mut max_jump: f32 = 0.0;
        for i in 1..pulled_samples.len() {
            let jump = (pulled_samples[i] - pulled_samples[i - 1]).abs();
            if jump > max_jump {
                max_jump = jump;
            }
            if jump > 0.5 {
                discontinuities += 1;
                if discontinuities <= 10 {
                    eprintln!(
                        "Discontinuity at {}: {} -> {} (jump={})",
                        i,
                        pulled_samples[i - 1],
                        pulled_samples[i],
                        jump
                    );
                }
            }
        }
        eprintln!(
            "Discontinuities (jump > 0.5): {}, max_jump: {}",
            discontinuities, max_jump
        );

        eprintln!(
            "Listen to {} and {} to compare",
            original_path, decoded_path
        );
    }
}
