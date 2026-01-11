use anyhow::{Context, Result};
use rkyv::{Archive, Deserialize, Serialize};

/// A type-safe audio buffer with compile-time channel count and sample rate.
///
/// This structure ensures that audio processing logic (like channel iteration)
/// is checked at compile time and can be heavily optimized by the compiler.
#[derive(Archive, Deserialize, Serialize, Debug, Clone, PartialEq)]
#[rkyv(compare(PartialEq))]
pub struct AudioBuffer<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    data: Vec<Sample>,
}

impl<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>
{
    /// Create a new audio buffer from raw samples.
    ///
    /// Returns an error if the data length is not a multiple of the channel count.
    pub fn new(data: Vec<Sample>) -> Result<Self> {
        if !data.is_empty() && data.len() % CHANNELS != 0 {
            anyhow::bail!(
                "Data length {} must be a multiple of channels {}",
                data.len(),
                CHANNELS
            );
        }
        Ok(Self { data })
    }

    /// Returns an iterator over the samples of a specific channel.
    ///
    /// This is fully static and compiler-optimized.
    pub fn iter_channel(&self, channel_idx: usize) -> impl Iterator<Item = &Sample> {
        assert!(
            channel_idx < CHANNELS,
            "Channel index {} out of bounds (max {})",
            channel_idx,
            CHANNELS - 1
        );
        self.data.iter().skip(channel_idx).step_by(CHANNELS)
    }

    /// Returns the number of samples per channel.
    pub fn samples_per_channel(&self) -> usize {
        self.data.len() / CHANNELS
    }

    /// Returns the number of channels.
    pub const fn channels(&self) -> usize {
        CHANNELS
    }

    /// Returns the sample rate.
    pub const fn sample_rate(&self) -> u32 {
        SAMPLE_RATE
    }

    /// Access the underlying raw sample data.
    pub fn data(&self) -> &[Sample] {
        &self.data
    }

    /// Access the underlying raw sample data mutably.
    pub fn data_mut(&mut self) -> &mut [Sample] {
        &mut self.data
    }

    /// Consumes the buffer and returns the raw vector.
    pub fn into_inner(self) -> Vec<Sample> {
        self.data
    }
}

/// Audio frame structure for network transmission.
/// Standardized to 48kHz Stereo 16-bit PCM.
#[derive(Archive, Deserialize, Serialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct AudioFrame {
    /// Monotonic sequence number for packet ordering and loss detection
    pub sequence_number: u64,

    /// Capture timestamp in microseconds
    pub timestamp: u64,

    /// Interleaved 16-bit PCM samples at 48kHz stereo
    pub samples: AudioBuffer<i16, 2, 48000>,
}

impl AudioFrame {
    /// Create a new audio frame.
    /// Expects interleaved stereo samples at 48kHz.
    pub fn new(sequence_number: u64, samples: Vec<i16>) -> Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        Ok(Self {
            sequence_number,
            timestamp,
            samples: AudioBuffer::new(samples)?,
        })
    }

    /// Get the number of samples per channel
    pub fn samples_per_channel(&self) -> usize {
        self.samples.samples_per_channel()
    }

    /// Serialize the frame using rkyv
    pub fn serialize(&self) -> Result<Vec<u8>> {
        rkyv::to_bytes::<rkyv::rancor::Error>(self)
            .map(|bytes| bytes.to_vec())
            .context("Serialization error")
    }

    /// Deserialize a frame from bytes using rkyv
    pub fn deserialize(bytes: &[u8]) -> Result<Self> {
        rkyv::from_bytes::<AudioFrame, rkyv::rancor::Error>(bytes).context("Deserialization error")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_frame_creation() {
        let samples = vec![100, -100, 200, -200];
        let frame = AudioFrame::new(1, samples.clone()).unwrap();

        assert_eq!(frame.sequence_number, 1);
        assert_eq!(frame.samples.channels(), 2);
        assert_eq!(frame.samples.data(), &samples);
        assert_eq!(frame.samples_per_channel(), 2);
    }

    #[test]
    fn test_audio_buffer_channel_iter() {
        let samples = vec![1, 10, 2, 20, 3, 30]; // L1, R1, L2, R2, L3, R3
        let buffer = AudioBuffer::<i16, 2, 48000>::new(samples).unwrap();

        let left: Vec<_> = buffer.iter_channel(0).cloned().collect();
        let right: Vec<_> = buffer.iter_channel(1).cloned().collect();

        assert_eq!(left, vec![1, 2, 3]);
        assert_eq!(right, vec![10, 20, 30]);
    }

    #[test]
    fn test_audio_frame_serialization() {
        let samples = vec![100, -100, 200, -200];
        let frame = AudioFrame::new(1, samples).unwrap();

        let serialized = frame.serialize().unwrap();
        let deserialized = AudioFrame::deserialize(&serialized).unwrap();

        assert_eq!(frame.sequence_number, deserialized.sequence_number);
        assert_eq!(frame.samples.data(), deserialized.samples.data());
    }

    #[test]
    fn test_audio_frame_validation() {
        let valid_frame = AudioFrame::new(1, vec![0; 960]).unwrap();
        assert!(valid_frame.validate());

        let invalid_samples = AudioBuffer::<i16, 2, 48000>::new(vec![0; 961]);
        assert!(invalid_samples.is_err());
    }
}
