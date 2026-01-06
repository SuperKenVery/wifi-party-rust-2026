use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use crate::pipeline::effect::AudioEffect;

/// An effect that silences all samples.
#[derive(Debug, Clone, Copy)]
pub struct Mute;

impl<T, const CHANNELS: usize, const SAMPLE_RATE: u32> AudioEffect<T, CHANNELS, SAMPLE_RATE>
    for Mute
where
    T: AudioSample,
{
    fn process(&mut self, frame: &mut AudioBuffer<T, CHANNELS, SAMPLE_RATE>) {
        for sample in frame.data_mut() {
            *sample = T::silence();
        }
    }
}
