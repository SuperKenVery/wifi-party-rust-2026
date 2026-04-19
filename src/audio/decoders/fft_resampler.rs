use std::sync::Mutex;

use rubato::{FftFixedIn, Resampler, ResamplerConstructionError};

use super::symphonia_decoder::DecodedAudio;
use crate::pipeline::Node;

/// Resamples per-channel decoded PCM to the target sample rate, or passes
/// through unchanged when source rate already matches the target.
///
/// Uses `FftFixedIn` from rubato for high-quality FFT-based resampling.
///
/// Decoded frames are accumulated in `pre_resample` until there are enough for a
/// full resampler chunk (`input_frames_next()`). This avoids zero-padding partial
/// chunks which would corrupt the FFT overlap-add state with discontinuities.
///
/// Interleaving is NOT performed here — use `Interleaver` as the next pipeline node.
pub struct FftResampler<const CHANNELS: usize, const SAMPLE_RATE: u32> {
    /// None when src_sample_rate == SAMPLE_RATE (pass-through mode).
    resampler: Option<Mutex<FftFixedIn<f32>>>,
    /// Per-channel decoded frames waiting to be resampled.
    pre_resample: Mutex<Vec<Vec<f32>>>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> FftResampler<CHANNELS, SAMPLE_RATE> {
    pub fn new(src_sample_rate: u32) -> Result<Self, ResamplerConstructionError> {
        let resampler = if src_sample_rate != SAMPLE_RATE {
            Some(Mutex::new(FftFixedIn::<f32>::new(
                src_sample_rate as usize,
                SAMPLE_RATE as usize,
                1024,
                1,
                CHANNELS,
            )?))
        } else {
            None
        };
        Ok(Self {
            resampler,
            pre_resample: Mutex::new(vec![Vec::new(); CHANNELS]),
        })
    }

    pub fn reset(&self) {
        for ch in self.pre_resample.lock().unwrap().iter_mut() {
            ch.clear();
        }
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> Node for FftResampler<CHANNELS, SAMPLE_RATE> {
    type Input = DecodedAudio;
    type Output = DecodedAudio;

    fn process(&self, input: DecodedAudio) -> Option<DecodedAudio> {
        let Some(resampler_lock) = &self.resampler else {
            return Some(input);
        };

        let mut resampler = resampler_lock.lock().unwrap();
        let mut pre = self.pre_resample.lock().unwrap();

        for (ch, samples) in input.channels.iter().enumerate() {
            if ch < CHANNELS {
                pre[ch].extend_from_slice(samples);
            }
        }

        let mut all_resampled: Vec<Vec<f32>> = (0..CHANNELS).map(|_| Vec::new()).collect();

        loop {
            let needed = resampler.input_frames_next();
            if pre[0].len() < needed {
                break;
            }
            let chunks: Vec<&[f32]> = pre.iter().map(|c| &c[..needed]).collect();
            let resampled = resampler.process(&chunks, None).expect("Resampling failed");
            for ch in pre.iter_mut() {
                ch.drain(..needed);
            }
            for ch in 0..CHANNELS {
                all_resampled[ch].extend_from_slice(&resampled[ch]);
            }
        }

        if all_resampled[0].is_empty() {
            return None;
        }

        Some(DecodedAudio { channels: all_resampled })
    }
}
