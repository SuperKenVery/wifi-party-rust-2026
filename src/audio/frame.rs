use rkyv::{Archive, Deserialize, Serialize};

/// Audio frame structure for network transmission
/// Uses rkyv for zero-copy serialization/deserialization
/// Sample rate is always 48kHz (resampled before sending if needed)
/// Host ID is extracted from UDP packet source address when receiving
#[derive(Archive, Deserialize, Serialize, Debug, Clone)]
#[rkyv(
    compare(PartialEq),
    derive(Debug),
)]
pub struct AudioFrame {
    /// Monotonic sequence number for packet ordering and loss detection
    pub sequence_number: u64,
    
    /// Capture timestamp in microseconds
    pub timestamp: u64,
    
    /// Number of audio channels (1=mono, 2=stereo)
    pub channels: u8,
    
    /// Interleaved 16-bit PCM samples at 48kHz
    /// For stereo: [L0, R0, L1, R1, ...]
    pub samples: Vec<i16>,
}

impl AudioFrame {
    /// Create a new audio frame
    /// Sample rate is assumed to be 48kHz
    pub fn new(
        sequence_number: u64,
        channels: u8,
        samples: Vec<i16>,
    ) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Self {
            sequence_number,
            timestamp,
            channels,
            samples,
        }
    }

    /// Get the number of samples per channel
    pub fn samples_per_channel(&self) -> usize {
        self.samples.len() / self.channels as usize
    }

    /// Serialize the frame using rkyv
    pub fn serialize(&self) -> Result<Vec<u8>, String> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("Serialization error: {:?}", e))
    }

    /// Deserialize a frame from bytes using rkyv
    pub fn deserialize(bytes: &[u8]) -> Result<Self, String> {
        rkyv::from_bytes::<AudioFrame, rkyv::rancor::Error>(bytes)
            .map_err(|e| format!("Deserialization error: {:?}", e))
    }

    /// Validate the frame
    pub fn validate(&self) -> bool {
        // Basic validation checks
        if self.channels == 0 || self.channels > 2 {
            return false;
        }
        if self.samples.is_empty() {
            return false;
        }
        // Check if samples length is valid for the channel count
        if self.samples.len() % self.channels as usize != 0 {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_frame_creation() {
        let samples = vec![100, -100, 200, -200];
        let frame = AudioFrame::new(1, 2, samples.clone());
        
        assert_eq!(frame.sequence_number, 1);
        assert_eq!(frame.channels, 2);
        assert_eq!(frame.samples, samples);
        assert_eq!(frame.samples_per_channel(), 2);
    }

    #[test]
    fn test_audio_frame_serialization() {
        let samples = vec![100, -100, 200, -200];
        let frame = AudioFrame::new(1, 2, samples);
        
        let serialized = frame.serialize().unwrap();
        let deserialized = AudioFrame::deserialize(&serialized).unwrap();
        
        assert_eq!(frame.sequence_number, deserialized.sequence_number);
        assert_eq!(frame.channels, deserialized.channels);
        assert_eq!(frame.samples, deserialized.samples);
    }

    #[test]
    fn test_audio_frame_validation() {
        let valid_frame = AudioFrame::new(1, 2, vec![0; 960]);
        assert!(valid_frame.validate());

        let invalid_channels = AudioFrame {
            sequence_number: 1,
            timestamp: 0,
            channels: 0,
            samples: vec![0; 960],
        };
        assert!(!invalid_channels.validate());

        let invalid_sample_count = AudioFrame {
            sequence_number: 1,
            timestamp: 0,
            channels: 2,
            samples: vec![0; 961], // Odd number for stereo
        };
        assert!(!invalid_sample_count.validate());
    }
}
