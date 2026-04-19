//! Interleaves per-channel decoded audio into an AudioBuffer.
//!
//! Used in the synced stream pipeline when no resampling is needed
//! (source sample rate == target sample rate).

use std::marker::PhantomData;

use super::symphonia_decoder::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::Node;

/// Interleaves per-channel decoded PCM into `AudioBuffer<Sample>`.
///
/// Pure stateless transform — no accumulation or buffering.
pub struct Interleaver<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    _sample: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Interleaver<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            _sample: PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for Interleaver<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = DecodedAudio;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    /// Interleave all channels from the input into a single AudioBuffer.
    fn process(&self, input: DecodedAudio) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let num_frames = input.channels.first().map_or(0, |c| c.len());
        if num_frames == 0 {
            return None;
        }

        let mut interleaved = Vec::with_capacity(num_frames * CHANNELS);
        for f in 0..num_frames {
            for ch in 0..CHANNELS {
                interleaved.push(Sample::from_f64_normalized(input.channels[ch][f] as f64));
            }
        }

        AudioBuffer::new(interleaved).ok()
    }
}
