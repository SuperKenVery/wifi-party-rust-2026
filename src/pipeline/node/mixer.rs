use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::PullNode;
use crate::state::AppState;
use std::sync::Arc;

pub struct MixerNode<const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Arc<AppState>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> MixerNode<CHANNELS, SAMPLE_RATE> {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PullNode<Next>
    for MixerNode<CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;

    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        let host_ids: Vec<crate::state::HostId> = {
            self.state
                .active_hosts
                .lock()
                .unwrap()
                .keys()
                .copied()
                .collect()
        };

        if host_ids.is_empty() {
            return None;
        }

        let mut host_frames: Vec<Vec<i16>> = Vec::new();
        for host_id in &host_ids {
            if let Some(samples) = self.state.jitter_buffers.pop_frame(*host_id) {
                host_frames.push(samples);
            }
        }

        if host_frames.is_empty() {
            return None;
        }

        let sample_count = host_frames.iter().map(|f| f.len()).min().unwrap_or(0);
        if sample_count == 0 {
            return None;
        }

        let mut mixed: Vec<i32> = vec![0; sample_count];
        let hosts = self.state.active_hosts.lock().unwrap();

        for (host_id, samples) in host_ids.iter().zip(host_frames.iter()) {
            if samples.len() != sample_count {
                continue;
            }
            let volume = hosts.get(host_id).map(|info| info.volume).unwrap_or(1.0);
            for (i, &sample) in samples.iter().enumerate() {
                mixed[i] += (sample as f32 * volume) as i32;
            }
        }

        let f32_samples: Vec<f32> = mixed.iter().map(|&s| s as f32 / 32768.0).collect();

        AudioBuffer::new(f32_samples).ok()
    }
}
