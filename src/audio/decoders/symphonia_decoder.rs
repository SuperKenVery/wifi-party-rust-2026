use std::sync::Mutex;

use symphonia::core::audio::{AudioBufferRef, Signal};

use super::compressed_packet_queue::CompressedPacket;
use crate::pipeline::Node;

/// Per-channel f32 PCM at the source sample rate.
///
/// We don't encode sample rate in type because it's dynamic (unknown, from network)
#[derive(Clone)]
pub struct DecodedAudio {
    pub channels: Vec<Vec<f32>>,
}

/// Decodes a single compressed packet into per-channel f32 PCM using symphonia.
///
/// Uses interior mutability (Mutex) for thread safety.
/// The `CHANNELS` const generic specifies the number of output channels.
pub struct SymphoniaDecoder<const CHANNELS: usize> {
    decoder: Mutex<Box<dyn symphonia::core::codecs::Decoder>>,
}

impl<const CHANNELS: usize> SymphoniaDecoder<CHANNELS> {
    pub fn new(decoder: Box<dyn symphonia::core::codecs::Decoder>) -> Self {
        Self {
            decoder: Mutex::new(decoder),
        }
    }

    /// Reset decoder state (for seek).
    pub fn reset(&self) {
        self.decoder.lock().unwrap().reset();
    }
}

impl<const CHANNELS: usize> Node for SymphoniaDecoder<CHANNELS> {
    type Input = CompressedPacket;
    type Output = DecodedAudio;

    /// Decode one compressed packet into per-channel f32 PCM.
    fn process(&self, input: CompressedPacket) -> Option<DecodedAudio> {
        let mut decoder = self.decoder.lock().unwrap();

        let symphonia_packet =
            symphonia::core::formats::Packet::new_from_slice(0, 0, input.dur as u64, &input.data);

        match decoder.decode(&symphonia_packet) {
            Ok(decoded) => {
                let channels = extract_f32_channels::<CHANNELS>(&decoded);
                Some(DecodedAudio { channels })
            }
            Err(e) => {
                tracing::error!("Failed to decode compressed packet: {}", e);
                None
            }
        }
    }
}

/// Extract per-channel f32 samples from symphonia's `AudioBufferRef`.
///
/// Handles all common sample formats (f32, s16, s32, u8) with proper normalization.
/// If the source has fewer channels than `CHANNELS`, channels wrap around (modulo).
fn extract_f32_channels<const CHANNELS: usize>(decoded: &AudioBufferRef) -> Vec<Vec<f32>> {
    let (num_frames, num_src_channels) = match decoded {
        AudioBufferRef::F32(buf) => (buf.frames(), buf.spec().channels.count()),
        AudioBufferRef::S16(buf) => (buf.frames(), buf.spec().channels.count()),
        AudioBufferRef::S32(buf) => (buf.frames(), buf.spec().channels.count()),
        AudioBufferRef::U8(buf) => (buf.frames(), buf.spec().channels.count()),
        _ => return vec![Vec::new(); CHANNELS],
    };

    let mut channels: Vec<Vec<f32>> = (0..CHANNELS)
        .map(|_| Vec::with_capacity(num_frames))
        .collect();

    match decoded {
        AudioBufferRef::F32(buf) => {
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    channels[ch].push(buf.chan(ch % num_src_channels)[f]);
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    channels[ch].push(buf.chan(ch % num_src_channels)[f] as f32 / 32768.0);
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    channels[ch].push(buf.chan(ch % num_src_channels)[f] as f32 / 2147483648.0);
                }
            }
        }
        AudioBufferRef::U8(buf) => {
            for f in 0..num_frames {
                for ch in 0..CHANNELS {
                    channels[ch].push((buf.chan(ch % num_src_channels)[f] as f32 - 128.0) / 128.0);
                }
            }
        }
        _ => {}
    }
    channels
}
