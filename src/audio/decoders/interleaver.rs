//! Interleaves per-channel decoded audio into an AudioBuffer.
//!
//! Used in the synced stream pipeline when no resampling is needed
//! (source sample rate == target sample rate).

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, RwLock};

use super::symphonia_decoder::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::Pullable;

/// Pulls per-channel decoded PCM from upstream and interleaves it into
/// `AudioBuffer<Sample>` at the source sample rate (no resampling).
pub struct Interleaver<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    source: RwLock<Option<Arc<dyn Pullable<DecodedAudio>>>>,
    leftover: Mutex<VecDeque<f32>>,
    _sample: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Interleaver<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Self {
        Self {
            source: RwLock::new(None),
            leftover: Mutex::new(VecDeque::new()),
            _sample: PhantomData,
        }
    }

    pub fn set_source(&self, source: Arc<dyn Pullable<DecodedAudio>>) {
        *self.source.write().unwrap() = Some(source);
    }

    pub fn reset(&self) {
        self.leftover.lock().unwrap().clear();
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>
    for Interleaver<Sample, CHANNELS, SAMPLE_RATE>
{
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let source = self.source.read().unwrap();
        let source = source.as_ref()?;

        let mut leftover = self.leftover.lock().unwrap();

        // Pull decoded audio and interleave until we have enough samples.
        while leftover.len() < len {
            // Request enough frames to fill the remaining need.
            let needed_frames = (len - leftover.len()) / CHANNELS + 1;
            let Some(decoded) = source.pull(needed_frames) else {
                break;
            };

            let num_frames = decoded.channels.first().map_or(0, |c| c.len());
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    leftover.push_back(decoded.channels[ch][f]);
                }
            }
        }

        if leftover.is_empty() {
            return None;
        }

        let take = len.min(leftover.len());
        let take = take - (take % CHANNELS);
        if take == 0 {
            return None;
        }

        let interleaved_f32: Vec<f32> = leftover.drain(..take).collect();
        let samples: Vec<Sample> = interleaved_f32
            .iter()
            .map(|&s| Sample::from_f64_normalized(s as f64))
            .collect();

        AudioBuffer::new(samples).ok()
    }
}
