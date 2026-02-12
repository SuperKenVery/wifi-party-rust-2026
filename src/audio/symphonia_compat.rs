//! Symphonia compatibility layer.
//!
//! All workarounds for symphonia's limited public API live here.
//! If this file grows complex, consider forking symphonia.

use rkyv::{Archive, Deserialize, Serialize};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CodecParameters, CodecType};

/// Wire-serializable codec type enum.
/// Maps to symphonia's CodecType constants.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[rkyv(compare(PartialEq))]
pub enum WireCodecType {
    Mp3,
    Aac,
    Flac,
    Vorbis,
    Opus,
    PcmS16Le,
    PcmS24Le,
    PcmS32Le,
    PcmF32Le,
    Alac,
}

impl WireCodecType {
    pub fn from_symphonia(ct: CodecType) -> Option<Self> {
        use symphonia::core::codecs::*;
        if ct == CODEC_TYPE_MP3 {
            Some(Self::Mp3)
        } else if ct == CODEC_TYPE_AAC {
            Some(Self::Aac)
        } else if ct == CODEC_TYPE_FLAC {
            Some(Self::Flac)
        } else if ct == CODEC_TYPE_VORBIS {
            Some(Self::Vorbis)
        } else if ct == CODEC_TYPE_OPUS {
            Some(Self::Opus)
        } else if ct == CODEC_TYPE_PCM_S16LE {
            Some(Self::PcmS16Le)
        } else if ct == CODEC_TYPE_PCM_S24LE {
            Some(Self::PcmS24Le)
        } else if ct == CODEC_TYPE_PCM_S32LE {
            Some(Self::PcmS32Le)
        } else if ct == CODEC_TYPE_PCM_F32LE {
            Some(Self::PcmF32Le)
        } else if ct == CODEC_TYPE_ALAC {
            Some(Self::Alac)
        } else {
            None
        }
    }

    pub fn to_symphonia(self) -> CodecType {
        use symphonia::core::codecs::*;
        match self {
            Self::Mp3 => CODEC_TYPE_MP3,
            Self::Aac => CODEC_TYPE_AAC,
            Self::Flac => CODEC_TYPE_FLAC,
            Self::Vorbis => CODEC_TYPE_VORBIS,
            Self::Opus => CODEC_TYPE_OPUS,
            Self::PcmS16Le => CODEC_TYPE_PCM_S16LE,
            Self::PcmS24Le => CODEC_TYPE_PCM_S24LE,
            Self::PcmS32Le => CODEC_TYPE_PCM_S32LE,
            Self::PcmF32Le => CODEC_TYPE_PCM_F32LE,
            Self::Alac => CODEC_TYPE_ALAC,
        }
    }
}

/// Wire-serializable codec parameters.
/// Contains only the fields needed for creating a decoder on the receiver side.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(compare(PartialEq))]
pub struct WireCodecParams {
    pub codec: WireCodecType,
    pub sample_rate: u32,
    pub channels: u8,
    pub extra_data: Option<Vec<u8>>,
}

impl WireCodecParams {
    pub fn from_symphonia(p: &CodecParameters) -> Option<Self> {
        let codec = WireCodecType::from_symphonia(p.codec)?;
        let sample_rate = p.sample_rate?;
        let channels = p.channels.map(|c| c.count() as u8).unwrap_or(2);
        let extra_data = p.extra_data.as_ref().map(|d| d.to_vec());

        Some(Self {
            codec,
            sample_rate,
            channels,
            extra_data,
        })
    }

    pub fn to_symphonia(&self) -> CodecParameters {
        let mut params = CodecParameters::new();
        params
            .for_codec(self.codec.to_symphonia())
            .with_sample_rate(self.sample_rate);

        if let Some(ref extra) = self.extra_data {
            params.with_extra_data(extra.clone().into_boxed_slice());
        }

        params
    }
}

use crate::audio::sample::AudioSample;

fn audio_buffer_frames(decoded: &AudioBufferRef) -> usize {
    match decoded {
        AudioBufferRef::F32(buf) => buf.frames(),
        AudioBufferRef::S16(buf) => buf.frames(),
        AudioBufferRef::S32(buf) => buf.frames(),
        AudioBufferRef::U8(buf) => buf.frames(),
        _ => 0,
    }
}

/// Extract samples from symphonia's AudioBufferRef to any AudioSample type.
/// Handles all symphonia sample formats, normalizes to [-1.0, 1.0], then converts to T.
pub fn extract_samples<T: AudioSample, const CHANNELS: usize>(
    decoded: &AudioBufferRef,
) -> [Vec<T>; CHANNELS] {
    let num_frames = audio_buffer_frames(decoded);
    let mut output: [Vec<T>; CHANNELS] = std::array::from_fn(|_| Vec::with_capacity(num_frames));

    match decoded {
        AudioBufferRef::F32(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    output[ch].push(T::from_f64_normalized(buf.chan(src_ch)[frame_idx] as f64));
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    output[ch]
                        .push(T::from_f64_normalized(buf.chan(src_ch)[frame_idx] as f64 / 32768.0));
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    output[ch].push(T::from_f64_normalized(
                        buf.chan(src_ch)[frame_idx] as f64 / 2147483648.0,
                    ));
                }
            }
        }
        AudioBufferRef::U8(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    output[ch].push(T::from_f64_normalized(
                        (buf.chan(src_ch)[frame_idx] as f64 - 128.0) / 128.0,
                    ));
                }
            }
        }
        _ => {}
    }

    output
}
