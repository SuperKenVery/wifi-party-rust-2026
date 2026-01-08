use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Sink, Source};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};
use tracing::error;

pub struct AudioInput<S> {
    sink: Arc<Mutex<S>>,
    stream: Option<cpal::Stream>,
}

impl<S> AudioInput<S> {
    pub fn new(sink: S) -> Self {
        Self {
            sink: Arc::new(Mutex::new(sink)),
            stream: None,
        }
    }

    pub fn start<const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self) -> Result<()>
    where
        S: Sink<Input = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("No input device available")?;
        let input_config = input_device.default_input_config()?;

        if input_config.sample_rate().0 != SAMPLE_RATE
            || input_config.channels() as usize != CHANNELS
        {
            error!(
                "Default input device format {:?} does not match required format ({}ch @ {}Hz)",
                input_config, CHANNELS, SAMPLE_RATE
            );
        }

        let sink = self.sink.clone();
        let stream = input_device.build_input_stream(
            &input_config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(frame) = AudioBuffer::<f32, CHANNELS, SAMPLE_RATE>::new(data.to_vec()) {
                    if let Ok(mut sink) = sink.lock() {
                        sink.push(frame);
                    }
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
    source: Arc<Mutex<S>>,
    stream: Option<cpal::Stream>,
}

impl<S> AudioOutput<S> {
    pub fn new(source: S) -> Self {
        Self {
            source: Arc::new(Mutex::new(source)),
            stream: None,
        }
    }

    pub fn start<const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self) -> Result<()>
    where
        S: Source<Output = AudioBuffer<f32, CHANNELS, SAMPLE_RATE>> + 'static,
    {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .context("No output device available")?;
        let output_config = output_device.default_output_config()?;

        let source = self.source.clone();
        let stream = output_device.build_output_stream(
            &output_config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if let Ok(mut source) = source.lock() {
                    if let Some(frame) = source.pull() {
                        data.copy_from_slice(frame.data());
                    } else {
                        for sample in data {
                            *sample = 0.0;
                        }
                    }
                } else {
                    for sample in data {
                        *sample = 0.0;
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
