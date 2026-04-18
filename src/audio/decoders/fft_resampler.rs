use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, RwLock};

use rubato::{FftFixedIn, Resampler};

use super::symphonia_decoder::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::Pullable;

/// Pulls per-channel decoded PCM from upstream, resamples to target sample rate,
/// outputs interleaved `AudioBuffer<Sample>`.
///
/// Uses `FftFixedIn` from rubato for high-quality FFT-based resampling.
pub struct FftResampler<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    resampler: Mutex<FftFixedIn<f32>>,
    source: RwLock<Option<Arc<dyn Pullable<DecodedAudio>>>>,
    /// Leftover interleaved f32 samples from previous resample operations.
    leftover: Mutex<VecDeque<f32>>,
    _sample: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    FftResampler<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(resampler: FftFixedIn<f32>) -> Self {
        Self {
            resampler: Mutex::new(resampler),
            source: RwLock::new(None),
            leftover: Mutex::new(VecDeque::new()),
            _sample: PhantomData,
        }
    }

    /// Set the upstream decoded audio source.
    pub fn set_source(&self, source: Arc<dyn Pullable<DecodedAudio>>) {
        *self.source.write().unwrap() = Some(source);
    }

    /// Clear the leftover buffer (for seek).
    /// Resampler state is preserved as it's not resettable,
    /// but clearing the leftover ensures clean start.
    pub fn reset(&self) {
        self.leftover.lock().unwrap().clear();
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>
    for FftResampler<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Pull resampled interleaved audio.
    ///
    /// `len` is the number of interleaved samples requested.
    /// Pulls from upstream, resamples, interleaves, and accumulates in `leftover`.
    /// Returns `None` if nothing is available.
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let source = self.source.read().unwrap();
        let source = source.as_ref()?;

        let mut resampler = self.resampler.lock().unwrap();
        let mut leftover = self.leftover.lock().unwrap();

        // Pull and resample until we have enough interleaved samples
        while leftover.len() < len {
            let needed_frames = resampler.input_frames_next();
            let Some(decoded) = source.pull(needed_frames) else {
                break;
            };

            let resampled = resample_chunk(&mut resampler, &decoded.channels, CHANNELS);

            // Interleave resampled channels into leftover
            let num_frames = resampled.first().map_or(0, |ch| ch.len());
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    leftover.push_back(resampled[ch][f]);
                }
            }
        }

        if leftover.is_empty() {
            return None;
        }

        // Drain requested amount, rounded down to CHANNELS multiple
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

/// Resample per-channel audio through `FftFixedIn`, handling variable
/// `input_frames_next()` and short tails with zero-padding.
fn resample_chunk(r: &mut FftFixedIn<f32>, input: &[Vec<f32>], channels: usize) -> Vec<Vec<f32>> {
    let total_frames = input[0].len();
    let mut output: Vec<Vec<f32>> = vec![Vec::new(); channels];
    let mut offset = 0;

    while offset < total_frames {
        let needed = r.input_frames_next();
        let available = total_frames - offset;

        if available >= needed {
            // Enough frames for a full chunk
            let chunks: Vec<&[f32]> = input.iter().map(|c| &c[offset..offset + needed]).collect();
            let resampled = r.process(&chunks, None).expect("Resampling failed");
            for (ch, data) in output.iter_mut().enumerate() {
                data.extend_from_slice(&resampled[ch]);
            }
            offset += needed;
        } else {
            // Tail shorter than needed: zero-pad input, trim output proportionally.
            let padded: Vec<Vec<f32>> = input
                .iter()
                .map(|c| {
                    let mut v = c[offset..].to_vec();
                    v.resize(needed, 0.0);
                    v
                })
                .collect();
            let chunks: Vec<&[f32]> = padded.iter().map(|v| v.as_slice()).collect();
            let resampled = r.process(&chunks, None).expect("Resampling failed");
            let output_len = resampled[0].len();
            let keep = output_len * available / needed;
            for (ch, data) in output.iter_mut().enumerate() {
                data.extend_from_slice(&resampled[ch][..keep]);
            }
            offset = total_frames;
        }
    }
    output
}
