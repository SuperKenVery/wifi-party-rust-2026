//! Mute effect.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;

/// Silences all samples.
///
/// # Example
///
/// ```ignore
/// let mute = Mute::<f32, 2, 48000>::new();
/// let pipeline = source.pipe(mute);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct Mute<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Mute<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Mute<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, mut input: Self::Input) -> Option<Self::Output> {
        for sample in input.data_mut() {
            *sample = Sample::silence();
        }
        Some(input)
    }
}
