use anyhow::Result;
use neteq::{AudioPacket, NetEq, NetEqConfig, RtpHeader};

use crate::audio::AudioFrame;

pub struct HostJitterBuffer {
    neteq: NetEq,
    sample_rate: u32,
    channels: u8,
}

impl std::fmt::Debug for HostJitterBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostJitterBuffer")
            .field("sample_rate", &self.sample_rate)
            .field("channels", &self.channels)
            .finish()
    }
}

impl HostJitterBuffer {
    pub fn new(sample_rate: u32, channels: u8) -> Result<Self> {
        let config = NetEqConfig {
            sample_rate,
            channels,
            ..Default::default()
        };

        let neteq = NetEq::new(config)?;

        Ok(Self {
            neteq,
            sample_rate,
            channels,
        })
    }

    pub fn push(&mut self, frame: AudioFrame) -> Result<()> {
        let header = RtpHeader::new(
            frame.sequence_number as u16,
            frame.timestamp as u32,
            0,
            0,
            false,
        );

        let payload: Vec<u8> = frame.samples.iter().flat_map(|s| s.to_le_bytes()).collect();

        let duration_ms =
            (frame.samples.len() as u32 * 1000) / (self.sample_rate * self.channels as u32);

        let packet = AudioPacket::new(
            header,
            payload,
            self.sample_rate,
            self.channels,
            duration_ms,
        );

        self.neteq.insert_packet(packet)?;
        Ok(())
    }

    pub fn pop(&mut self) -> Result<Option<Vec<i16>>> {
        match self.neteq.get_audio() {
            Ok(audio_frame) => {
                let i16_samples: Vec<i16> = audio_frame
                    .samples
                    .iter()
                    .map(|&sample| (sample * 32767.0) as i16)
                    .collect();
                Ok(Some(i16_samples))
            }
            Err(_) => Ok(None),
        }
    }
}
