//! Host management for multi-peer audio mixing.
//!
//! This module manages audio pipelines for multiple remote hosts (peers) and provides
//! mixed output combining all their audio streams.
//!
//! # Architecture
//!
//! Each connected host gets its own [`JitterBuffer`] to handle network jitter and
//! packet reordering. When audio is requested, frames from all hosts are pulled
//! and mixed together into a single output stream.
//!
//! ```text
//! Host A ──► JitterBuffer A ──┐
//!                             │
//! Host B ──► JitterBuffer B ──┼──► Mixer ──► Mixed Output
//!                             │
//! Host C ──► JitterBuffer C ──┘
//! ```
//!
//! # Components
//!
//! - [`HostPipelineManager`] - Manages per-host jitter buffers and mixing
//! - [`NetworkSource`] - A [`Source`] that pulls mixed audio from all hosts

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use tracing::info;

use crate::audio::AudioSample;
use crate::audio::frame::AudioFrame;
use crate::pipeline::node::JitterBuffer;
use crate::pipeline::{Sink, Source};
use crate::state::{HostId, HostInfo};

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const JITTER_BUFFER_CAPACITY: usize = 64;

struct HostPipeline<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    jitter_buffer: JitterBuffer<Sample, CHANNELS, SAMPLE_RATE>,
    info: HostInfo,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    HostPipeline<Sample, CHANNELS, SAMPLE_RATE>
{
    fn new(host_id: HostId) -> Self {
        Self {
            jitter_buffer: JitterBuffer::new(JITTER_BUFFER_CAPACITY),
            info: HostInfo::new(host_id),
        }
    }
}

/// Manages audio pipelines for multiple remote hosts.
///
/// Each host has its own jitter buffer to compensate for network delay and packet
/// reordering. The manager handles:
///
/// - Creating pipelines for new hosts on first packet
/// - Routing incoming frames to the correct host's buffer
/// - Mixing audio from all hosts into a single output stream
/// - Cleaning up stale hosts that haven't sent data recently
///
/// # Thread Safety
///
/// Uses [`DashMap`] internally for lock-free concurrent access. Can be shared
/// directly via `Arc<HostPipelineManager>` between:
/// - The network receiver thread (pushes frames)
/// - The audio output thread (pulls mixed frames via [`NetworkSource`])
pub struct HostPipelineManager<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pipelines: DashMap<HostId, HostPipeline<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            pipelines: DashMap::new(),
        }
    }

    /// Pushes an audio frame from a specific host into its jitter buffer.
    ///
    /// If this is the first frame from this host, a new pipeline is created.
    pub fn push_frame(&self, host_id: HostId, frame: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>) {
        let mut pipeline = self.pipelines.entry(host_id).or_insert_with(|| {
            info!("Creating pipeline for new host: {}", host_id.to_string());
            HostPipeline::new(host_id)
        });
        pipeline.info.last_seen = Instant::now();
        pipeline.jitter_buffer.push(frame);
    }

    /// Pulls `len` samples from each host's jitter buffer and mixes them together.
    ///
    /// Returns `None` if no hosts have data available. The mixing is done by
    /// summing normalized sample values from all hosts.
    ///
    /// The returned AudioFrame will contain exactly `len` samples (padded with silence
    /// if any host returns fewer samples).
    pub fn pull_and_mix(&self, len: usize) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut mixed: Vec<f64> = vec![0.0; len];
        let mut has_data = false;
        let mut result_seq = 0u64;

        for pipeline in self.pipelines.iter_mut() {
            if let Some(frame) = pipeline.jitter_buffer.pull(len) {
                has_data = true;
                result_seq = result_seq.max(frame.sequence_number);

                for (i, sample) in frame.samples.data().iter().enumerate() {
                    if i < len {
                        mixed[i] += sample.to_f64_normalized();
                    }
                }
            }
        }

        if !has_data {
            return None;
        }

        let samples: Vec<Sample> = mixed.into_iter().map(Sample::from_f64_normalized).collect();
        AudioFrame::new(result_seq, samples).ok()
    }

    /// Removes hosts that haven't sent data within the timeout period.
    pub fn cleanup_stale_hosts(&self) {
        let now = Instant::now();
        self.pipelines.retain(|host_id, pipeline| {
            let alive = now.duration_since(pipeline.info.last_seen) < HOST_TIMEOUT;
            if !alive {
                info!("Removing stale host pipeline: {}", host_id.to_string());
            }
            alive
        });
    }

    pub fn host_count(&self) -> usize {
        self.pipelines.len()
    }

    pub fn get_host_infos(&self) -> Vec<HostInfo> {
        self.pipelines.iter().map(|p| p.info.clone()).collect()
    }

    pub fn get_host_info(&self, host_id: &HostId) -> Option<HostInfo> {
        self.pipelines.get(host_id).map(|p| p.info.clone())
    }

    pub fn update_host_volume(&self, host_id: &HostId, volume: f32) {
        if let Some(mut pipeline) = self.pipelines.get_mut(host_id) {
            pipeline.info.volume = volume;
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

/// A [`Source`] that provides mixed audio from all connected hosts.
///
/// Each call to [`pull()`](Source::pull) returns a single audio frame that combines
/// audio from all hosts managed by the underlying [`HostPipelineManager`].
/// Per-host jitter buffering is applied before mixing.
///
/// Returns `None` when no hosts have data available.
pub struct NetworkSource<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSource<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(pipeline_manager: Arc<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>) -> Self {
        Self { pipeline_manager }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for NetworkSource<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self, len: usize) -> Option<Self::Output> {
        self.pipeline_manager.pull_and_mix(len)
    }
}
