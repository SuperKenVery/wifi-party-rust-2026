//! Audio sample batcher for reducing packet frequency.

use std::sync::Mutex;

use super::Node;
use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;

/// Batches audio samples and outputs when minimum duration is reached.
///
/// Useful for reducing network packet frequency when input chunks are small.
/// Accumulates incoming samples and only outputs when the buffer reaches
/// the minimum sample count (calculated from min_ms at construction).
pub struct AudioBatcher<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    buffer: Mutex<Vec<Sample>>,
    min_samples: usize,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    AudioBatcher<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(min_ms: u32) -> Self {
        let min_samples = (SAMPLE_RATE * CHANNELS as u32 * min_ms / 1000) as usize;
        Self {
            buffer: Mutex::new(Vec::with_capacity(min_samples * 2)),
            min_samples,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for AudioBatcher<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let mut buffer = self.buffer.lock().unwrap();
        buffer.extend(input.into_inner());

        if buffer.len() >= self.min_samples {
            let samples = std::mem::take(&mut *buffer);
            AudioBuffer::new(samples).ok()
        } else {
            None
        }
    }
}
