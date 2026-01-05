/// Audio frame for pipeline processing.
/// Uses f32 samples for high-quality processing.
#[derive(Debug, Clone)]
pub struct PipelineFrame {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u8,
}

impl PipelineFrame {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: u8) -> Self {
        Self {
            samples,
            sample_rate,
            channels,
        }
    }

    pub fn from_i16(samples: Vec<i16>, sample_rate: u32, channels: u8) -> Self {
        let f32_samples: Vec<f32> = samples
            .iter()
            .map(|&s| s as f32 / 32768.0)
            .collect();
        Self {
            samples: f32_samples,
            sample_rate,
            channels,
        }
    }

    pub fn to_i16(&self) -> Vec<i16> {
        self.samples
            .iter()
            .map(|&s| (s * 32768.0).clamp(-32768.0, 32767.0) as i16)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}
