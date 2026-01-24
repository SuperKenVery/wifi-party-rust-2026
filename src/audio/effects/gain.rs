//! Gain (volume) effect.

use std::sync::{Arc, Mutex};

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;

/// Applies a dynamic gain (volume multiplier) to all samples.
///
/// The gain factor is read from an `Arc<Mutex<f32>>` on each process call,
/// allowing real-time volume control from the UI.
///
/// # Example
///
/// ```ignore
/// let volume = Arc::new(Mutex::new(1.0f32));
/// let gain = Gain::<f32, 2, 48000>::new(volume.clone());
/// let pipeline = source.pipe(gain);
/// // Later, adjust volume dynamically:
/// *volume.lock().unwrap() = 0.5; // 50% volume
/// ```
pub struct Gain<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    factor: Arc<Mutex<f32>>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Gain<Sample, CHANNELS, SAMPLE_RATE> {
    pub fn new(factor: Arc<Mutex<f32>>) -> Self {
        Self {
            factor,
            _marker: std::marker::PhantomData,
        }
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
        let factor = *self.factor.lock().unwrap();
        for sample in input.data_mut() {
            *sample = Sample::from_f64_normalized(sample.to_f64_normalized() * factor as f64);
        }
        Some(input)
    }
}
