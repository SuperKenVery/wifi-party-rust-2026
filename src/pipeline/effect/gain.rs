//! Gain (volume) effect.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;

/// Applies a gain (volume multiplier) to all samples.
///
/// # Example
///
/// ```ignore
/// let gain = Gain::<f32, 2, 48000>::new(0.5); // 50% volume
/// let pipeline = source.pipe(gain);
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Gain<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    factor: Sample,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Gain<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new(factor: Sample) -> Self {
        Self { factor }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Gain<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, mut input: Self::Input) -> Option<Self::Output> {
        let center = Sample::silence();
        for sample in input.data_mut() {
            *sample = (*sample - center) * self.factor + center;
        }
        Some(input)
    }
}
