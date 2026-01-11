//! Host management for multi-peer audio mixing.
//!
//! Manages per-host audio pipelines with jitter buffering and provides
//! mixed output from all connected hosts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::info;

use crate::audio::frame::AudioFrame;
use crate::audio::AudioSample;
use crate::pipeline::node::{jitter_buffer, JitterBufferConsumer, JitterBufferProducer};
use crate::pipeline::{Sink, Source};
use crate::state::{HostId, HostInfo};

const HOST_TIMEOUT: Duration = Duration::from_secs(5);
const JITTER_BUFFER_CAPACITY: usize = 16;

struct HostPipeline<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    producer: JitterBufferProducer<Sample, CHANNELS, SAMPLE_RATE>,
    consumer: JitterBufferConsumer<Sample, CHANNELS, SAMPLE_RATE>,
    info: HostInfo,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    HostPipeline<Sample, CHANNELS, SAMPLE_RATE>
{
    fn new(host_id: HostId) -> Self {
        let (producer, consumer) = jitter_buffer(JITTER_BUFFER_CAPACITY);
        Self {
            producer,
            consumer,
            info: HostInfo::new(host_id),
        }
    }
}

pub struct HostPipelineManager<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pipelines: HashMap<HostId, HostPipeline<Sample, CHANNELS, SAMPLE_RATE>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
        }
    }

    pub fn push_frame(
        &mut self,
        host_id: HostId,
        frame: AudioFrame<Sample, CHANNELS, SAMPLE_RATE>,
    ) {
        let pipeline = self.pipelines.entry(host_id).or_insert_with(|| {
            info!("Creating pipeline for new host: {}", host_id.to_string());
            HostPipeline::new(host_id)
        });
        pipeline.info.last_seen = Instant::now();
        pipeline.producer.push(frame);
    }

    pub fn pull_and_mix(&mut self) -> Option<AudioFrame<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut mixed_samples: Option<Vec<Sample>> = None;
        let mut result_seq = 0u64;
        let mut result_timestamp = 0u64;

        for pipeline in self.pipelines.values_mut() {
            if let Some(frame) = pipeline.consumer.pull() {
                result_seq = result_seq.max(frame.sequence_number);
                result_timestamp = result_timestamp.max(frame.timestamp);

                match &mut mixed_samples {
                    None => {
                        mixed_samples = Some(frame.samples.data().to_vec());
                    }
                    Some(mixed) => {
                        for (i, sample) in frame.samples.data().iter().enumerate() {
                            if i < mixed.len() {
                                let sum =
                                    mixed[i].to_f64_normalized() + sample.to_f64_normalized();
                                mixed[i] = Sample::from_f64_normalized(sum);
                            }
                        }
                    }
                }
            }
        }

        mixed_samples.and_then(|samples| AudioFrame::new(result_seq, samples).ok())
    }

    pub fn cleanup_stale_hosts(&mut self) {
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
        self.pipelines.values().map(|p| p.info.clone()).collect()
    }

    pub fn get_host_info(&self, host_id: &HostId) -> Option<HostInfo> {
        self.pipelines.get(host_id).map(|p| p.info.clone())
    }

    pub fn update_host_volume(&mut self, host_id: &HostId, volume: f32) {
        if let Some(pipeline) = self.pipelines.get_mut(host_id) {
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

pub struct NetworkSource<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NetworkSource<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        pipeline_manager: Arc<Mutex<HostPipelineManager<Sample, CHANNELS, SAMPLE_RATE>>>,
    ) -> Self {
        Self { pipeline_manager }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Source
    for NetworkSource<Sample, CHANNELS, SAMPLE_RATE>
{
    type Output = AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&self) -> Option<Self::Output> {
        self.pipeline_manager.lock().unwrap().pull_and_mix()
    }
}
