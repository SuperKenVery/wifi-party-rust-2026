use std::marker::PhantomData;
use std::sync::Mutex;

use rubato::{FftFixedIn, Resampler};

use super::symphonia_decoder::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::Node;

/// Resamples per-channel decoded PCM to target sample rate,
/// outputs interleaved `AudioBuffer<Sample>`.
///
/// Uses `FftFixedIn` from rubato for high-quality FFT-based resampling.
///
/// Decoded frames are accumulated in `pre_resample` until there are enough for a
/// full resampler chunk (`input_frames_next()`). This avoids zero-padding partial
/// chunks which would corrupt the FFT overlap-add state with discontinuities.
pub struct FftResampler<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    resampler: Mutex<FftFixedIn<f32>>,
    /// Per-channel decoded frames waiting to be resampled.
    /// Accumulated until we have enough for a full resampler chunk.
    pre_resample: Mutex<Vec<Vec<f32>>>,
    _sample: PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    FftResampler<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(resampler: FftFixedIn<f32>) -> Self {
        Self {
            resampler: Mutex::new(resampler),
            pre_resample: Mutex::new(vec![Vec::new(); CHANNELS]),
            _sample: PhantomData,
        }
    }

    /// Clear internal buffers (for seek).
    /// Resampler state is preserved as it's not resettable,
    /// but clearing the buffers ensures clean start.
    pub fn reset(&self) {
        for ch in self.pre_resample.lock().unwrap().iter_mut() {
            ch.clear();
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for FftResampler<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = DecodedAudio;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    /// Accumulate input into pre_resample buffer, process all complete resampler chunks,
    /// and return interleaved output. Returns `None` when still accumulating
    /// (not enough frames for a full resampler chunk yet).
    fn process(&self, input: DecodedAudio) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut resampler = self.resampler.lock().unwrap();
        let mut pre = self.pre_resample.lock().unwrap();

        // Append input to pre-resample buffer
        for (ch, samples) in input.channels.iter().enumerate() {
            if ch < CHANNELS {
                pre[ch].extend_from_slice(samples);
            }
        }

        // Process all complete chunks
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

        let num_frames = all_resampled[0].len();
        if num_frames == 0 {
            return None;
        }

        // Interleave into output
        let mut interleaved = Vec::with_capacity(num_frames * CHANNELS);
        for f in 0..num_frames {
            for ch in 0..CHANNELS {
                interleaved.push(Sample::from_f64_normalized(all_resampled[ch][f] as f64));
            }
        }

        AudioBuffer::new(interleaved).ok()
    }
}
