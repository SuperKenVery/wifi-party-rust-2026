//! Noise gate effect.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;
use std::collections::VecDeque;
use std::sync::Mutex;

struct NoiseGateState {
    window: VecDeque<f64>,
    sum_sq: f64,
}

/// A stateful noise gate that silences samples based on RMS energy of a sliding window.
///
/// # Example
///
/// ```ignore
/// let gate = NoiseGate::<f32, 2, 48000>::new(0.01, 1024);
/// let pipeline = source.pipe(gate);
/// ```
pub struct NoiseGate<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    threshold: f64,
    window_size: usize,
    state: Mutex<NoiseGateState>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    NoiseGate<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(threshold: f64, window_size: usize) -> Self {
        Self {
            threshold,
            window_size,
            state: Mutex::new(NoiseGateState {
                window: VecDeque::with_capacity(window_size),
                sum_sq: 0.0,
            }),
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

    fn process(&self, mut input: Self::Input) -> Option<Self::Output> {
        let mut state = self.state.lock().unwrap();

        for sample in input.data_mut() {
            let val = sample.to_f64_normalized();
            let sq = val * val;

            state.window.push_back(sq);
            state.sum_sq += sq;

            if state.window.len() > self.window_size {
                if let Some(old_sq) = state.window.pop_front() {
                    state.sum_sq -= old_sq;
                }
            }

            let count = state.window.len() as f64;
            let rms = if count > 0.0 {
                (state.sum_sq / count).sqrt()
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
