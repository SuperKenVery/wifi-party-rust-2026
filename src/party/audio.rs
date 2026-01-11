use crate::audio::frame::AudioBuffer;
use crate::audio::AudioSample;
use crate::pipeline::{Sink, Source};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, SampleRate, StreamConfig};
use std::sync::Arc;
use tracing::{error, warn};

pub struct AudioInput<S> {
    sink: Arc<S>,
    stream: Option<cpal::Stream>,
}

impl<S> AudioInput<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink: Arc::new(sink),
            stream: None,
        }
    }

    pub fn start<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self) -> Result<()>
    where
        Sample: AudioSample + cpal::SizedSample,
        S: Sink<Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("No input device available")?;
        let input_config = input_device.default_input_config()?;

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SampleRate(SAMPLE_RATE),
            buffer_size: match input_config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, .. } => BufferSize::Fixed(*min),
                cpal::SupportedBufferSize::Unknown => {
                    warn!("Supported buffer size range unknown, using default");
                    BufferSize::Default
                }
            },
        };

        let sink = self.sink.clone();
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
        self.stream = Some(stream);
        Ok(())
    }
}

pub struct AudioOutput<S> {
    source: Arc<S>,
    stream: Option<cpal::Stream>,
}

impl<S> AudioOutput<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(source),
            stream: None,
        }
    }

    pub fn start<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self) -> Result<()>
    where
        Sample: AudioSample + cpal::SizedSample,
        S: Source<Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .context("No output device available")?;
        let output_config = output_device.default_output_config()?;

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

        let source = self.source.clone();
        let stream = output_device.build_output_stream(
            &config,
            move |data: &mut [Sample], _: &cpal::OutputCallbackInfo| {
                if let Some(frame) = source.pull() {
                    for (i, sample) in frame.data().iter().enumerate() {
                        if i < data.len() {
                            data[i] = *sample;
                        }
                    }
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
        self.stream = Some(stream);
        Ok(())
    }
}
