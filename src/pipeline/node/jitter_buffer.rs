use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::node::{PullNode, PushNode};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct JitterBuffer<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample> {
    queue: Arc<Mutex<VecDeque<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample: AudioSample>
    JitterBuffer<CHANNELS, SAMPLE_RATE, Sample>
{
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Sample: AudioSample> Default
    for JitterBuffer<CHANNELS, SAMPLE_RATE, Sample>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next, Sample: AudioSample>
    PushNode<Next> for JitterBuffer<CHANNELS, SAMPLE_RATE, Sample>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {
        self.queue.lock().unwrap().push_back(frame);
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next, Sample: AudioSample>
    PullNode<Next> for JitterBuffer<CHANNELS, SAMPLE_RATE, Sample>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        self.queue.lock().unwrap().pop_front()
    }
}
