use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::{PullNode, PushNode};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct JitterBuffer<const CHANNELS: usize, const SAMPLE_RATE: u32> {
    queue: Arc<Mutex<VecDeque<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>>>>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> JitterBuffer<CHANNELS, SAMPLE_RATE> {
    pub fn new() -> Self {
        Self {
            queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> Default
    for JitterBuffer<CHANNELS, SAMPLE_RATE>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PushNode<CHANNELS, SAMPLE_RATE, Next>
    for JitterBuffer<CHANNELS, SAMPLE_RATE>
{
    fn push(&mut self, frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {
        self.queue.lock().unwrap().push_back(frame);
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next> PullNode<CHANNELS, SAMPLE_RATE, Next>
    for JitterBuffer<CHANNELS, SAMPLE_RATE>
{
    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        self.queue.lock().unwrap().pop_front()
    }
}
