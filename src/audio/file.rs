//! Audio file decoding using symphonia.
//!
//! Provides [`AudioFileReader`] for decoding common audio formats (mp3, flac, wav, ogg, aac)
//! and resampling to the target sample rate.
#![allow(dead_code)]

use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use rubato::{FftFixedIn, Resampler};
use symphonia::core::audio::{AudioBufferRef, Signal};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::AudioSample;
use super::frame::AudioBuffer;

fn extract_samples<const CHANNELS: usize>(decoded: &AudioBufferRef, output: &mut [Vec<f64>]) {
    match decoded {
        AudioBufferRef::F32(buf) => {
            let num_channels = buf.spec().channels.count();
            let num_frames = buf.frames();

            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    let sample = buf.chan(src_ch)[frame_idx] as f64;
                    output[ch].push(sample);
                }
            }
        }
        AudioBufferRef::S16(buf) => {
            let num_channels = buf.spec().channels.count();
            let num_frames = buf.frames();

            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    let sample = buf.chan(src_ch)[frame_idx] as f64 / 32768.0;
                    output[ch].push(sample);
                }
            }
        }
        AudioBufferRef::S32(buf) => {
            let num_channels = buf.spec().channels.count();
            let num_frames = buf.frames();

            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    let sample = buf.chan(src_ch)[frame_idx] as f64 / 2147483648.0;
                    output[ch].push(sample);
                }
            }
        }
        AudioBufferRef::U8(buf) => {
            let num_channels = buf.spec().channels.count();
            let num_frames = buf.frames();

            for frame_idx in 0..num_frames {
                for ch in 0..CHANNELS {
                    let src_ch = ch % num_channels;
                    let sample = (buf.chan(src_ch)[frame_idx] as f64 - 128.0) / 128.0;
                    output[ch].push(sample);
                }
            }
        }
        _ => {}
    }
}

pub struct AudioFileInfo {
    pub sample_rate: u32,
    pub channels: usize,
    pub duration_secs: Option<f64>,
    pub file_name: String,
}
pub struct AudioFileReader {
    format: Box<dyn symphonia::core::formats::FormatReader>,
    decoder: Box<dyn symphonia::core::codecs::Decoder>,
    track_id: u32,
    source_sample_rate: u32,
    source_channels: usize,
    pub info: AudioFileInfo,
    resampler: Option<FftFixedIn<f64>>,
    input_buffer: Vec<Vec<f64>>,
    output_buffer: Vec<f64>,
}

impl AudioFileReader {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let file = File::open(path).context("Failed to open audio file")?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let format_opts = FormatOptions::default();
        let metadata_opts = MetadataOptions::default();
        let decoder_opts = DecoderOptions::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .context("Failed to probe audio format")?;

        let format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("No supported audio track found"))?;

        let track_id = track.id;

        let source_sample_rate = track
            .codec_params
            .sample_rate
            .ok_or_else(|| anyhow!("Unknown sample rate"))?;

        let source_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

        let duration_secs = track
            .codec_params
            .n_frames
            .map(|frames| frames as f64 / source_sample_rate as f64);

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &decoder_opts)
            .context("Failed to create decoder")?;

        let info = AudioFileInfo {
            sample_rate: source_sample_rate,
            channels: source_channels,
            duration_secs,
            file_name,
        };

        Ok(Self {
            format,
            decoder,
            track_id,
            source_sample_rate,
            source_channels,
            info,
            resampler: None,
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        })
    }

    pub fn next_buffer<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        &mut self,
    ) -> Result<Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>> {
        if self.input_buffer.is_empty() {
            self.input_buffer = vec![Vec::new(); CHANNELS];
        }

        let frame_samples = 960 * CHANNELS;

        while self.output_buffer.len() < frame_samples {
            let packet = match self.format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    if self.output_buffer.is_empty() {
                        return Ok(None);
                    }
                    let mut padded = self.output_buffer.clone();
                    padded.resize(frame_samples, 0.0);
                    self.output_buffer.clear();
                    let samples: Vec<Sample> = padded
                        .into_iter()
                        .map(Sample::from_f64_normalized)
                        .collect();
                    return Ok(Some(AudioBuffer::new(samples)?));
                }
                Err(e) => return Err(e.into()),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
                Err(e) => return Err(e.into()),
            };

            extract_samples::<CHANNELS>(&decoded, &mut self.input_buffer);

            if self.source_sample_rate == SAMPLE_RATE && self.source_channels == CHANNELS {
                let num_frames = self.input_buffer[0].len();
                for frame_idx in 0..num_frames {
                    for ch in 0..CHANNELS {
                        self.output_buffer.push(self.input_buffer[ch][frame_idx]);
                    }
                }
                for ch in 0..CHANNELS {
                    self.input_buffer[ch].clear();
                }
            } else {
                let chunk_size = 1024;
                if self.resampler.is_none() {
                    self.resampler = Some(FftFixedIn::<f64>::new(
                        self.source_sample_rate as usize,
                        SAMPLE_RATE as usize,
                        chunk_size,
                        2,
                        CHANNELS,
                    )?);
                }

                let resampler = self.resampler.as_mut().unwrap();
                while self.input_buffer[0].len() >= chunk_size {
                    let chunk: Vec<Vec<f64>> = (0..CHANNELS)
                        .map(|ch| self.input_buffer[ch].drain(0..chunk_size).collect())
                        .collect();

                    let resampled = resampler.process(&chunk, None)?;
                    let out_frames = resampled[0].len();
                    for frame_idx in 0..out_frames {
                        for ch in 0..CHANNELS {
                            self.output_buffer.push(resampled[ch][frame_idx]);
                        }
                    }
                }
            }
        }

        let chunk: Vec<f64> = self.output_buffer.drain(0..frame_samples).collect();
        let samples: Vec<Sample> = chunk.into_iter().map(Sample::from_f64_normalized).collect();
        Ok(Some(AudioBuffer::new(samples)?))
    }

    pub fn seek(&mut self, time_secs: f64) -> Result<()> {
        use symphonia::core::formats::{SeekMode, SeekTo};

        let timestamp = (time_secs * self.source_sample_rate as f64) as u64;
        self.format.seek(
            SeekMode::Accurate,
            SeekTo::TimeStamp {
                ts: timestamp,
                track_id: self.track_id,
            },
        )?;
        self.decoder.reset();
        if let Some(resampler) = self.resampler.as_mut() {
            resampler.reset();
        }
        for ch in 0..self.input_buffer.len() {
            self.input_buffer[ch].clear();
        }
        self.output_buffer.clear();
        Ok(())
    }

    pub fn seek_micros(&mut self, micros: u64) -> Result<()> {
        let time_secs = micros as f64 / 1_000_000.0;
        self.seek(time_secs)
    }

    pub fn decode_all_resampled<
        Sample: AudioSample,
        const CHANNELS: usize,
        const SAMPLE_RATE: u32,
    >(
        mut self,
    ) -> Result<Vec<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>> {
        let mut buffers = Vec::new();
        while let Some(buf) = self.next_buffer::<Sample, CHANNELS, SAMPLE_RATE>()? {
            buffers.push(buf);
        }
        Ok(buffers)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_file_info() {
        let info = AudioFileInfo {
            sample_rate: 44100,
            channels: 2,
            duration_secs: Some(180.0),
            file_name: "test.mp3".to_string(),
        };

        assert_eq!(info.sample_rate, 44100);
        assert_eq!(info.channels, 2);
    }
}
