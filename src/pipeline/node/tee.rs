use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::node::PushNode;

pub struct TeeNode<A, B> {
    a: A,
    b: B,
}

impl<A, B> TeeNode<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, A, B, Next, Sample>
    PushNode<CHANNELS, SAMPLE_RATE, Sample, Next> for TeeNode<A, B>
where
    A: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
    B: PushNode<CHANNELS, SAMPLE_RATE, Sample, ()>,
    Sample: AudioSample,
{
    fn push(&mut self, frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {
        self.a.push(frame.clone(), &mut ());
        self.b.push(frame, &mut ());
    }
}
