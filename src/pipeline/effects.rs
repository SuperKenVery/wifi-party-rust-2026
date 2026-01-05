use crate::pipeline::effect::AudioEffect;
use crate::pipeline::frame::PipelineFrame;

/// Gain/volume control effect
pub struct Gain(pub f32);

impl AudioEffect for Gain {
    fn process(&self, frame: &mut PipelineFrame) {
        for sample in &mut frame.samples {
            *sample *= self.0;
        }
    }
}

/// Mute effect (sets all samples to zero)
pub struct Mute;

impl AudioEffect for Mute {
    fn process(&self, frame: &mut PipelineFrame) {
        for sample in &mut frame.samples {
            *sample = 0.0;
        }
    }
}

/// Noise gate effect - suppresses audio below threshold
pub struct NoiseGate {
    pub threshold: f32,
}

impl AudioEffect for NoiseGate {
    fn process(&self, frame: &mut PipelineFrame) {
        for sample in &mut frame.samples {
            if sample.abs() < self.threshold {
                *sample = 0.0;
            }
        }
    }
}
