use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::effect::AudioEffect;

/// An effect that applies a given gain (multiplier) to all samples.
#[derive(Debug, Clone, Copy)]
pub struct Gain<T>(pub T);

impl<T, const CHANNELS: usize, const SAMPLE_RATE: u32> AudioEffect<T, CHANNELS, SAMPLE_RATE>
    for Gain<T>
where
    T: AudioSample,
{
    fn process(&mut self, frame: &mut AudioBuffer<T, CHANNELS, SAMPLE_RATE>) {
        // Simple multiplication gain assumes the signal is centered at zero.
        // For unsigned types (centered at 128, etc.), this simple multiplication will cause DC offset shift.
        // A proper generic implementation would need to subtract silence, multiply, and add silence back.
        // For now, we assume this is acceptable or T is signed/float.
        let center = T::silence();
        for sample in frame.data_mut() {
             *sample = (*sample - center) * self.0 + center;
        }
    }
}
