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
///
/// Decoded frames are accumulated in `pre_resample` until there are enough for a
/// full resampler chunk (`input_frames_next()`). This avoids zero-padding partial
/// chunks which would corrupt the FFT overlap-add state with discontinuities.
pub struct FftResampler<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    resampler: Mutex<FftFixedIn<f32>>,
    source: RwLock<Option<Arc<dyn Pullable<DecodedAudio>>>>,
    /// Per-channel decoded frames waiting to be resampled.
    /// Accumulated until we have enough for a full resampler chunk.
    pre_resample: Mutex<Vec<Vec<f32>>>,
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
            pre_resample: Mutex::new(vec![Vec::new(); CHANNELS]),
            leftover: Mutex::new(VecDeque::new()),
            _sample: PhantomData,
        }
    }

    /// Set the upstream decoded audio source.
    pub fn set_source(&self, source: Arc<dyn Pullable<DecodedAudio>>) {
        *self.source.write().unwrap() = Some(source);
    }

    /// Clear internal buffers (for seek).
    /// Resampler state is preserved as it's not resettable,
    /// but clearing the buffers ensures clean start.
    pub fn reset(&self) {
        for ch in self.pre_resample.lock().unwrap().iter_mut() {
            ch.clear();
        }
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
    /// Pulls decoded frames from upstream into `pre_resample`, then feeds full
    /// chunks to the resampler. Short tails are kept in `pre_resample` for the
    /// next call — never zero-padded, since there is no end-of-stream
    /// (streams are torn down by removing the pipeline).
    fn pull(&self, len: usize) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let source = self.source.read().unwrap();
        let source = source.as_ref()?;

        let mut resampler = self.resampler.lock().unwrap();
        let mut pre = self.pre_resample.lock().unwrap();
        let mut leftover = self.leftover.lock().unwrap();

        while leftover.len() < len {
            // 1. Pull decoded frames into the pre-resample buffer
            let needed = resampler.input_frames_next();
            let buffered = pre[0].len();
            if buffered < needed {
                let Some(decoded) = source.pull(needed - buffered) else {
                    break;
                };
                for (ch, samples) in decoded.channels.iter().enumerate() {
                    if ch < CHANNELS {
                        pre[ch].extend_from_slice(samples);
                    }
                }
            }

            // 2. Process full chunks only — leave remainder for next call
            if pre[0].len() < needed {
                break;
            }
            let chunks: Vec<&[f32]> = pre.iter().map(|c| &c[..needed]).collect();
            let resampled = resampler.process(&chunks, None).expect("Resampling failed");
            for ch in pre.iter_mut() {
                ch.drain(..needed);
            }

            // 3. Interleave into leftover
            let num_frames = resampled[0].len();
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    leftover.push_back(resampled[ch][f]);
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
