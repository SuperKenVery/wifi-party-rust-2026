use crate::audio::AudioFrame;
use crate::pipeline::frame::PipelineFrame;
use crate::pipeline::node::{PullNode, PushNode};
use crate::state::AppState;
use anyhow::Result;
use rtrb::{Consumer, Producer};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tracing::warn;

/// A jitter buffer that can act as both a PushNode and PullNode.
/// This is the "pivot" point between push and pull pipelines.
#[derive(Clone)]
pub struct JitterBuffer {
    queue: Arc<Mutex<VecDeque<PipelineFrame>>>,
}

impl JitterBuffer {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl PushNode for JitterBuffer {
    fn push(&mut self, frame: PipelineFrame) {
        self.queue.lock().unwrap().push_back(frame);
    }
}

impl PullNode for JitterBuffer {
    fn pull(&mut self) -> Option<PipelineFrame> {
        self.queue.lock().unwrap().pop_front()
    }
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// A node that pushes frames to a network queue (serialized AudioFrame)
pub struct NetworkPushNode {
    producer: Producer<Vec<u8>>,
    state: Arc<AppState>,
    sample_rate: u32,
    channels: u8,
}

impl NetworkPushNode {
    pub fn new(
        producer: Producer<Vec<u8>>,
        state: Arc<AppState>,
        sample_rate: u32,
        channels: u8,
    ) -> Self {
        Self {
            producer,
            state,
            sample_rate,
            channels,
        }
    }
}

impl PushNode for NetworkPushNode {
    fn push(&mut self, frame: PipelineFrame) {
        // Convert to i16
        let i16_samples = frame.to_i16();

        // Get sequence number and increment
        let seq = self
            .state
            .sequence_number
            .fetch_add(1, Ordering::Relaxed);

        // Create AudioFrame for serialization
        if let Ok(audio_frame) = AudioFrame::new(seq, i16_samples) {
            if let Ok(serialized) = audio_frame.serialize() {
                if self.producer.push(serialized).is_err() {
                    warn!("Network send queue full, dropping frame");
                }
            }
        }
    }
}

/// A node that pulls frames from a network queue (deserialized AudioFrame)
pub struct NetworkPullNode {
    consumer: Consumer<Vec<u8>>,
    sample_rate: u32,
    channels: u8,
}

impl NetworkPullNode {
    pub fn new(consumer: Consumer<Vec<u8>>, sample_rate: u32, channels: u8) -> Self {
        Self {
            consumer,
            sample_rate,
            channels,
        }
    }
}

impl PullNode for NetworkPullNode {
    fn pull(&mut self) -> Option<PipelineFrame> {
        match self.consumer.pop() {
            Ok(serialized) => {
                // Deserialize AudioFrame
                if let Ok(audio_frame) = AudioFrame::deserialize(&serialized) {
                    if audio_frame.validate() {
                        let i16_samples = audio_frame.samples.data().to_vec();
                        return Some(PipelineFrame::from_i16(
                            i16_samples,
                            self.sample_rate,
                            self.channels,
                        ));
                    }
                }
                None
            }
            Err(_) => None,
        }
    }
}

/// A node that pushes frames to a playback queue (i16 samples)
pub struct QueuePushNode {
    producer: Producer<Vec<i16>>,
}

impl QueuePushNode {
    pub fn new(producer: Producer<Vec<i16>>) -> Self {
        Self { producer }
    }
}

impl PushNode for QueuePushNode {
    fn push(&mut self, frame: PipelineFrame) {
        let i16_samples = frame.to_i16();
        if self.producer.push(i16_samples).is_err() {
            warn!("Playback queue full, dropping frame");
        }
    }
}

/// A node that pulls frames from a queue (i16 samples)
pub struct QueuePullNode {
    consumer: Consumer<Vec<i16>>,
    buffer: Vec<i16>,
    buffer_index: usize,
    sample_rate: u32,
    channels: u8,
}

impl QueuePullNode {
    pub fn new(consumer: Consumer<Vec<i16>>, sample_rate: u32, channels: u8) -> Self {
        Self {
            consumer,
            buffer: Vec::new(),
            buffer_index: 0,
            sample_rate,
            channels,
        }
    }
}

impl PullNode for QueuePullNode {
    fn pull(&mut self) -> Option<PipelineFrame> {
        // If buffer is empty or exhausted, try to get a new frame
        if self.buffer_index >= self.buffer.len() {
            match self.consumer.pop() {
                Ok(frame) => {
                    self.buffer = frame;
                    self.buffer_index = 0;
                }
                Err(_) => return None,
            }
        }

        // Return a frame from the buffer
        if !self.buffer.is_empty() {
            let samples = self.buffer.drain(..).collect();
            self.buffer_index = 0;
            Some(PipelineFrame::from_i16(samples, self.sample_rate, self.channels))
        } else {
            None
        }
    }
}

/// A node that captures audio from the microphone
/// This is a special node that needs to be driven by cpal's audio callback
pub struct MicrophoneNode {
    pipeline: Arc<Mutex<Option<Box<dyn PushNode + Send>>>>,
    sample_rate: u32,
    channels: u8,
}

impl MicrophoneNode {
    pub fn new(sample_rate: u32, channels: u8) -> Self {
        Self {
            pipeline: Arc::new(Mutex::new(None)),
            sample_rate,
            channels,
        }
    }

    pub fn set_pipeline(&self, pipeline: Box<dyn PushNode + Send>) {
        *self.pipeline.lock().unwrap() = Some(pipeline);
    }

    pub fn push_samples(&self, samples: Vec<i16>) {
        if let Ok(mut pipeline_guard) = self.pipeline.lock() {
            if let Some(ref mut pipeline) = *pipeline_guard {
                let frame = PipelineFrame::from_i16(samples, self.sample_rate, self.channels);
                pipeline.push(frame);
            }
        }
    }
}

/// A node that outputs audio to the speaker
/// This is a special node that needs to be driven by cpal's audio callback
pub struct SpeakerNode {
    pipeline: Arc<Mutex<Option<Box<dyn PullNode + Send>>>>,
    sample_rate: u32,
    channels: u8,
}

impl SpeakerNode {
    pub fn new(sample_rate: u32, channels: u8) -> Self {
        Self {
            pipeline: Arc::new(Mutex::new(None)),
            sample_rate,
            channels,
        }
    }

    pub fn set_pipeline(&self, pipeline: Box<dyn PullNode + Send>) {
        *self.pipeline.lock().unwrap() = Some(pipeline);
    }

    pub fn pull_samples(&self) -> Option<Vec<i16>> {
        if let Ok(mut pipeline_guard) = self.pipeline.lock() {
            if let Some(ref mut pipeline) = *pipeline_guard {
                if let Some(frame) = pipeline.pull() {
                    return Some(frame.to_i16());
                }
            }
        }
        None
    }
}

/// A node that mixes audio from multiple jitter buffers (one per host)
/// This pulls from all active hosts' jitter buffers and mixes them together
pub struct MixerNode {
    state: Arc<crate::state::AppState>,
    sample_rate: u32,
    channels: u8,
}

impl MixerNode {
    pub fn new(state: Arc<crate::state::AppState>, sample_rate: u32, channels: u8) -> Self {
        Self {
            state,
            sample_rate,
            channels,
        }
    }
}

impl PullNode for MixerNode {
    fn pull(&mut self) -> Option<PipelineFrame> {
        // Get active host IDs
        let host_ids: Vec<crate::state::HostId> = {
            let hosts = self.state.active_hosts.lock().unwrap();
            hosts.keys().copied().collect()
        };

        if host_ids.is_empty() {
            return None;
        }

        // Pull frames from all jitter buffers
        let mut host_frames: std::collections::HashMap<crate::state::HostId, Vec<i16>> =
            std::collections::HashMap::new();
        for host_id in host_ids {
            if let Some(samples) = self.state.jitter_buffers.pop_frame(host_id) {
                host_frames.insert(host_id, samples);
            }
        }

        if host_frames.is_empty() {
            return None;
        }

        // Get the first frame to determine output format
        let first_samples = host_frames.values().next()?;
        let sample_count = first_samples.len();

        // Initialize accumulator with zeros
        let mut mixed: Vec<i32> = vec![0; sample_count];

        // Get host volumes
        let hosts = self.state.active_hosts.lock().unwrap();

        // Sum all frames with volume control
        for (host_id, samples) in host_frames.iter() {
            // Get volume for this host (default 1.0)
            let volume = hosts.get(host_id).map(|info| info.volume).unwrap_or(1.0);

            // Ensure frames are same length
            if samples.len() != sample_count {
                tracing::warn!("Frame size mismatch, skipping host {:?}", host_id);
                continue;
            }

            // Add samples with volume
            for (i, &sample) in samples.iter().enumerate() {
                mixed[i] += (sample as f32 * volume) as i32;
            }
        }

        // Apply soft clipping and convert to i16
        const MAX: i32 = 32767;
        const MIN: i32 = -32768;
        let output: Vec<i16> = mixed
            .iter()
            .map(|&sample| {
                if sample > MAX {
                    // Apply soft clipping to positive values
                    let excess = (sample - MAX) as f32;
                    let compressed = (excess / 10000.0).tanh() * 1000.0;
                    (MAX as f32 + compressed).min(32767.0) as i16
                } else if sample < MIN {
                    // Apply soft clipping to negative values
                    let excess = (sample - MIN) as f32;
                    let compressed = (excess / 10000.0).tanh() * 1000.0;
                    (MIN as f32 + compressed).max(-32768.0) as i16
                } else {
                    sample as i16
                }
            })
            .collect();

        Some(PipelineFrame::from_i16(output, self.sample_rate, self.channels))
    }
}
