//! Symphonia compatibility layer.
//!
//! All workarounds for symphonia's limited public API live here.
//! If this file grows complex, consider forking symphonia.

use crate::audio::frame::AudioBuffer;
use crate::audio::sample::AudioSample;
use rkyv::{Archive, Deserialize, Serialize};
use rubato::{FftFixedIn, Resampler};
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

fn audio_buffer_frames(decoded: &AudioBufferRef) -> usize {
    match decoded {
        AudioBufferRef::F32(buf) => buf.frames(),
        AudioBufferRef::S16(buf) => buf.frames(),
        AudioBufferRef::S32(buf) => buf.frames(),
        AudioBufferRef::U8(buf) => buf.frames(),
        _ => 0,
    }
}

fn audio_buffer_sample_rate(decoded: &AudioBufferRef) -> u32 {
    match decoded {
        AudioBufferRef::F32(buf) => buf.spec().rate,
        AudioBufferRef::S16(buf) => buf.spec().rate,
        AudioBufferRef::S32(buf) => buf.spec().rate,
        AudioBufferRef::U8(buf) => buf.spec().rate,
        _ => 0,
    }
}

struct ExtractedAudio<const CHANNELS: usize> {
    channels: Vec<Vec<f32>>,
    source_rate: u32,
}

fn extract_audio<const CHANNELS: usize>(decoded: &AudioBufferRef) -> ExtractedAudio<CHANNELS> {
    let num_frames = audio_buffer_frames(decoded);
    let source_rate = audio_buffer_sample_rate(decoded);
    let mut channels: Vec<Vec<f32>> = (0..CHANNELS)
        .map(|_| Vec::with_capacity(num_frames))
        .collect();

    match decoded {
        AudioBufferRef::F32(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    channels[ch].push(buf.chan(src_ch)[frame_idx]);
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    channels[ch].push(buf.chan(src_ch)[frame_idx] as f32 / 32768.0);
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    channels[ch].push(buf.chan(src_ch)[frame_idx] as f32 / 2147483648.0);
                }
            }
        }
        AudioBufferRef::U8(buf) => {
            let num_channels = buf.spec().channels.count();
            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    channels[ch].push((buf.chan(src_ch)[frame_idx] as f32 - 128.0) / 128.0);
                }
            }
        }
        _ => {
            for ch in 0..CHANNELS {
                channels[ch].resize(num_frames, 0.0);
            }
        }
    }

    ExtractedAudio {
        channels,
        source_rate,
    }
}

/// Convert per-channel f32 vectors to interleaved AudioBuffer<T>.
fn channels_to_interleaved<T: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
    channels: &[Vec<f32>],
) -> AudioBuffer<T, CHANNELS, SAMPLE_RATE> {
    let num_frames = channels.first().map(|c| c.len()).unwrap_or(0);
    let mut samples = Vec::with_capacity(num_frames * CHANNELS);

    for frame_idx in 0..num_frames {
        for ch in 0..CHANNELS {
            samples.push(T::from_f64_normalized(channels[ch][frame_idx] as f64));
        }
    }

    AudioBuffer::new(samples).expect("Extracted samples must be a multiple of CHANNELS")
}

/// Extract, resample, and convert in one call.
///
/// Uses high-quality FFT-based sinc resampling via rubato when sample rates differ.
/// Pass a persistent resampler to maintain inter-frame state for smooth transitions.
pub fn extract_and_resample<T: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
    decoded: &AudioBufferRef,
    resampler: Option<&mut FftFixedIn<f32>>,
) -> AudioBuffer<T, CHANNELS, SAMPLE_RATE> {
    let extracted = extract_audio::<CHANNELS>(decoded);

    if extracted.channels.first().map(|c| c.len()).unwrap_or(0) == 0 {
        return AudioBuffer::new(vec![]).unwrap();
    }

    if extracted.source_rate == SAMPLE_RATE {
        return channels_to_interleaved(&extracted.channels);
    }

    match resampler {
        Some(r) => {
            let resampled = r
                .process(&extracted.channels, None)
                .expect("Resampling failed");
            channels_to_interleaved(&resampled)
        }
        None => {
            let num_frames = extracted.channels[0].len();
            let mut temp_resampler = FftFixedIn::<f32>::new(
                extracted.source_rate as usize,
                SAMPLE_RATE as usize,
                num_frames,
                1,
                CHANNELS,
            )
            .expect("Failed to create resampler");

            let resampled = temp_resampler
                .process(&extracted.channels, None)
                .expect("Resampling failed");

            channels_to_interleaved(&resampled)
        }
    }
}
