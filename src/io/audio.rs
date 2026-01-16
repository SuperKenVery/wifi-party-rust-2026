//! Audio device I/O using cpal.
//!
//! Provides `AudioInput` for microphone capture and `AudioOutput` for speaker playback.

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Sink, Source};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleRate, StreamConfig};
use std::sync::Arc;
use tracing::{debug, error, warn};

pub struct AudioInput<S> {
    sink: Arc<S>,
}

impl<S> AudioInput<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink: Arc::new(sink),
        }
    }

    pub fn start<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        self,
    ) -> Result<cpal::Stream>
    where
        Sample: AudioSample + cpal::SizedSample,
        S: Sink<Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("No input device available")?;
        let input_config = input_device.default_input_config()?;
        debug!("Default input config: {input_config:#?}");

        const MIN_BUFFER_MS: u32 = 3;
        let min_buffer_size = SAMPLE_RATE * MIN_BUFFER_MS / 1000;

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: match input_config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, .. } => {
                    BufferSize::Fixed((*min).max(min_buffer_size))
                }
                cpal::SupportedBufferSize::Unknown => {
                    warn!("Supported buffer size range unknown, using default");
                    BufferSize::Default
                }
            },
        };

        let sink = self.sink;
        let stream = input_device.build_input_stream(
            &config,
            move |data: &[Sample], _: &cpal::InputCallbackInfo| {
                let owned: Vec<Sample> = Vec::from(data);
                if let Ok(frame) = AudioBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new(owned) {
                    sink.push(frame);
                }
            },
            |err| error!("An error occurred on the input audio stream: {}", err),
            None,
        )?;
        stream.play()?;
        Ok(stream)
    }
}

pub struct AudioOutput<S> {
    source: Arc<S>,
}

impl<S> AudioOutput<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    pub fn start<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(
        self,
    ) -> Result<cpal::Stream>
    where
        Sample: AudioSample + cpal::SizedSample,
        S: Source<Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .context("No output device available")?;
        let output_config = output_device.default_output_config()?;
        debug!("Default output config: {output_config:#?}");

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: match output_config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, .. } => BufferSize::Fixed(*min),
                cpal::SupportedBufferSize::Unknown => {
                    warn!("Supported buffer size range unknown, using default");
                    BufferSize::Default
                }
            },
        };

        let source = self.source;
        debug!("Building output stream");
        let stream = output_device.build_output_stream(
            &config,
            move |data: &mut [Sample], _: &cpal::OutputCallbackInfo| {
                if let Some(frame) = source.pull(data.len()) {
                    let src = frame.data();
                    let len = src.len().min(data.len());
                    data[..len].copy_from_slice(&src[..len]);
                } else {
                    for sample in data {
                        *sample = Sample::silence();
                    }
                }
            },
            |err| error!("An error occurred on the output audio stream: {}", err),
            None,
        )?;
        stream.play()?;
        Ok(stream)
    }
}
