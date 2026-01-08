use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::PullNode;

pub struct MixPullNode<A, B> {
    a: A,
    b: B,
}

impl<A, B> MixPullNode<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, A, B, Next> PullNode<Next>
    for MixPullNode<A, B>
where
    A: PullNode<(), Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>>,
    B: PullNode<(), Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>>,
{
    type Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;

    fn pull(&mut self, _next: &mut Next) -> Option<AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> {
        let mut null_a = ();
        let mut null_b = ();
        let frame_a = self.a.pull(&mut null_a);
        let frame_b = self.b.pull(&mut null_b);

        match (frame_a, frame_b) {
            (Some(mut a), Some(b)) => {
                if a.data().len() == b.data().len() {
                    for (a_sample, b_sample) in a.data_mut().iter_mut().zip(b.data().iter()) {
                        *a_sample = (*a_sample + *b_sample).clamp(-1.0, 1.0);
                    }
                }
                Some(a)
            }
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }
}
