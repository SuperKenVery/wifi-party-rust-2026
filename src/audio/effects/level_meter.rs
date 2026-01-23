//! Audio level metering.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::Node;

const UPDATE_INTERVAL: u32 = 32;

pub fn calculate_rms_level<Sample: AudioSample>(samples: &[Sample]) -> u32 {
    if samples.is_empty() {
        return 0;
    }
    let sum_sq: f64 = samples
        .iter()
        .map(|s| {
            let v = s.to_f64_normalized();
            v * v
        })
        .sum();
    let rms = (sum_sq / samples.len() as f64).sqrt();
    (rms * 100.0).min(100.0) as u32
}

/// Given an Arc<AtomicU32>, it updates volume to the u32 100 times every second. Range: 0-100.
pub struct LevelMeter<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    level: Arc<AtomicU32>,
    counter: AtomicU32,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    LevelMeter<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(level: Arc<AtomicU32>) -> Self {
        Self {
            level,
            counter: AtomicU32::new(0),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for LevelMeter<Sample, CHANNELS, SAMPLE_RATE>
where
    Sample: AudioSample,
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        if count % UPDATE_INTERVAL != 0 {
            return Some(input);
        }

        let level_percent = calculate_rms_level(input.data());
        self.level.store(level_percent, Ordering::Relaxed);

        Some(input)
    }
}
