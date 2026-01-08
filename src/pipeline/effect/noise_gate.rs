//! Noise gate effect.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;
use std::collections::VecDeque;

/// A stateful noise gate that silences samples based on RMS energy of a sliding window.
///
/// # Example
///
/// ```ignore
/// let gate = NoiseGate::<f32, 2, 48000>::new(0.01, 1024);
/// let pipeline = source.pipe(gate);
/// ```
#[derive(Debug, Clone)]
pub struct NoiseGate<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    threshold: f32,
    window_size: usize,
    window: VecDeque<f32>,
    sum_sq: f32,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NoiseGate<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(threshold: f32, window_size: usize) -> Self {
        Self {
            threshold,
            window_size,
            window: VecDeque::with_capacity(window_size),
            sum_sq: 0.0,
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for NoiseGate<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&mut self, mut input: Self::Input) -> Option<Self::Output> {
        let center = Sample::silence().to_f32().unwrap_or(0.0);

        for sample in input.data_mut() {
            let val = sample.to_f32().unwrap_or(0.0) - center;
            let sq = val * val;

            self.window.push_back(sq);
            self.sum_sq += sq;

            if self.window.len() > self.window_size {
                if let Some(old_sq) = self.window.pop_front() {
                    self.sum_sq -= old_sq;
                }
            }

            let count = self.window.len() as f32;
            let rms = if count > 0.0 {
                (self.sum_sq / count).sqrt()
            } else {
                0.0
            };

            if rms < self.threshold {
                *sample = Sample::silence();
            }
        }

        Some(input)
    }
}
