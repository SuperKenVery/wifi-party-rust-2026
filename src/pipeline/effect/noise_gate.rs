use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::effect::AudioEffect;
use std::collections::VecDeque;

/// A stateful noise gate that silences samples based on the energy (RMS) of a sliding window.
#[derive(Debug, Clone)]
pub struct NoiseGate<T> {
    /// The threshold for the energy (RMS value).
    pub threshold: f32,
    /// The size of the sliding window in samples.
    pub window_size: usize,
    /// Internal buffer for the sliding window.
    window: VecDeque<f32>,
    /// Running sum of squares for efficient RMS calculation.
    sum_sq: f32,
    /// Phantom data for T
    _marker: std::marker::PhantomData<T>,
}

impl<T> NoiseGate<T> {
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

impl<T, const CHANNELS: usize, const SAMPLE_RATE: u32> AudioEffect<T, CHANNELS, SAMPLE_RATE>
    for NoiseGate<T>
where
    T: AudioSample,
{
    fn process(&mut self, frame: &mut AudioBuffer<T, CHANNELS, SAMPLE_RATE>) {
        let center = T::silence().to_f32().unwrap_or(0.0);

        for sample in frame.data_mut() {
            let val = sample.to_f32().unwrap_or(0.0) - center;
            let sq = val * val;

            // Update sliding window
            self.window.push_back(sq);
            self.sum_sq += sq;

            if self.window.len() > self.window_size {
                if let Some(old_sq) = self.window.pop_front() {
                    self.sum_sq -= old_sq;
                }
            }

            // Calculate RMS
            // Ideally we check if window is full, but for startup we just use current count
            let count = self.window.len() as f32;
            let rms = if count > 0.0 {
                (self.sum_sq / count).sqrt()
            } else {
                0.0
            };

            if rms < self.threshold {
                *sample = T::silence();
            }
        }
    }
}
