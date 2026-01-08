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
    PushNode<Next> for TeeNode<A, B>
where
    A: PushNode<(), Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
    B: PushNode<(), Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>,
    Sample: AudioSample + Clone,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn push(&mut self, frame: AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>, _next: &mut Next) {
        let mut null_a = ();
        let mut null_b = ();
        self.a.push(frame.clone(), &mut null_a);
        self.b.push(frame, &mut null_b);
    }
}
