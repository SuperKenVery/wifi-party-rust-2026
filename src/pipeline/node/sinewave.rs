use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::PushNode;
use std::f32::consts::PI;

/// A node that generates a sinewave and overwrites the incoming audio frame.
/// This is useful for testing audio pipelines with a known signal.
pub struct SinewaveNode {
    frequency: f32,
    amplitude: f32,
    phase: f32,
}

impl SinewaveNode {
    /// Create a new SinewaveNode with the given frequency (Hz) and amplitude (0.0 to 1.0).
    pub fn new(frequency: f32, amplitude: f32) -> Self {
        Self {
            frequency,
            amplitude,
            phase: 0.0,
        }
    }

    /// Set the frequency of the sinewave.
    pub fn set_frequency(&mut self, frequency: f32) {
        self.frequency = frequency;
    }

    /// Set the amplitude of the sinewave.
    pub fn set_amplitude(&mut self, amplitude: f32) {
        self.amplitude = amplitude;
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32, Next>
    PushNode<Next> for SinewaveNode
where
    Next: PushNode<(), Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>>,
{
    type Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>;

    fn push(&mut self, mut frame: AudioBuffer<f32, CHANNELS, SAMPLE_RATE>, next: &mut Next) {
        let phase_inc = 2.0 * PI * self.frequency / SAMPLE_RATE as f32;
        let samples_per_channel = frame.samples_per_channel();
        let data = frame.data_mut();

        // We iterate through each sample position (all channels at once)
        for i in 0..samples_per_channel {
            let val = self.phase.sin() * self.amplitude;

            // Fill all channels with the same value
            for channel in 0..CHANNELS {
                data[i * CHANNELS + channel] = val;
            }

            // Update phase and wrap it to [0, 2*PI] to maintain precision
            self.phase = (self.phase + phase_inc) % (2.0 * PI);
        }

        // Pass the modified frame to the next node in the pipeline
        let mut null = ();
        next.push(frame, &mut null);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sinewave_generation() {
        let mut node = SinewaveNode::new(440.0, 1.0);
        let frame = AudioBuffer::<f32, 1, 44100>::new(vec![0.0; 100]).unwrap();
        let mut next = ();

        node.push(frame, &mut next);
        // We don't have an easy way to check the output here without a mock Next,
        // but we can at least verify it doesn't crash and phase updates.
        assert!(node.phase > 0.0);
    }
}
