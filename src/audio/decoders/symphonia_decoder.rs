use std::sync::{Arc, Mutex, RwLock};

use symphonia::core::audio::{AudioBufferRef, Signal};

use super::compressed_packet_queue::CompressedPacket;
use crate::pipeline::Pullable;

/// Per-channel f32 PCM at the source sample rate.
pub struct DecodedAudio {
    pub channels: Vec<Vec<f32>>,
}

/// Pulls compressed packets from upstream, decodes them using symphonia,
/// outputs per-channel f32 PCM.
///
/// Uses interior mutability (Mutex/RwLock) for thread safety.
/// The `CHANNELS` const generic specifies the number of output channels.
pub struct SymphoniaDecoder<const CHANNELS: usize> {
    decoder: Mutex<Box<dyn symphonia::core::codecs::Decoder>>,
    source: RwLock<Option<Arc<dyn Pullable<CompressedPacket>>>>,
    /// Leftover per-channel samples from the last decode that weren't consumed.
    leftover: Mutex<Vec<Vec<f32>>>,
}

impl<const CHANNELS: usize> SymphoniaDecoder<CHANNELS> {
    pub fn new(decoder: Box<dyn symphonia::core::codecs::Decoder>) -> Self {
        Self {
            decoder: Mutex::new(decoder),
            source: RwLock::new(None),
            leftover: Mutex::new(vec![Vec::new(); CHANNELS]),
        }
    }

    /// Set the upstream compressed packet source.
    pub fn set_source(&self, source: Arc<dyn Pullable<CompressedPacket>>) {
        *self.source.write().unwrap() = Some(source);
    }

    /// Reset decoder state and clear leftover buffer (for seek).
    pub fn reset(&self) {
        self.decoder.lock().unwrap().reset();
        let mut leftover = self.leftover.lock().unwrap();
        for ch in leftover.iter_mut() {
            ch.clear();
        }
    }
}

impl<const CHANNELS: usize> Pullable<DecodedAudio> for SymphoniaDecoder<CHANNELS> {
    /// Pull decoded PCM frames.
    ///
    /// `len` is the number of frames (samples per channel) requested.
    /// Pulls compressed packets from source until enough frames are accumulated.
    /// Returns partial data if source runs out. Returns `None` if nothing available.
    fn pull(&self, len: usize) -> Option<DecodedAudio> {
        let source = self.source.read().unwrap();
        let source = source.as_ref()?;

        let mut decoder = self.decoder.lock().unwrap();
        let mut leftover = self.leftover.lock().unwrap();

        // Pull and decode packets until we have enough frames
        while leftover[0].len() < len {
            let Some(packet) = source.pull(1) else {
                break;
            };

            let symphonia_packet = symphonia::core::formats::Packet::new_from_slice(
                0,
                0,
                packet.dur as u64,
                &packet.data,
            );

            match decoder.decode(&symphonia_packet) {
                Ok(decoded) => {
                    let channels = extract_f32_channels::<CHANNELS>(&decoded);
                    for (ch, samples) in channels.into_iter().enumerate() {
                        if ch < CHANNELS {
                            leftover[ch].extend(samples);
                        }
                    }
                }
                Err(e) => {
                    // Log and skip this packet, try next one
                    tracing::error!("Failed to decode compressed packet: {}", e);
                    continue;
                }
            }
        }

        // Nothing decoded at all
        if leftover[0].is_empty() {
            return None;
        }

        // Drain up to `len` frames from each channel
        let take = len.min(leftover[0].len());
        let channels: Vec<Vec<f32>> = leftover
            .iter_mut()
            .map(|ch| ch.drain(..take).collect())
            .collect();

        Some(DecodedAudio { channels })
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
                    channels[ch]
                        .push((buf.chan(ch % num_src_channels)[f] as f32 - 128.0) / 128.0);
                }
            }
        }
        _ => {}
    }
    channels
}
