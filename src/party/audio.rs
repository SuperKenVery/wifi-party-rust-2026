use crate::audio::frame::AudioBuffer;
use crate::pipeline::node::{PullNode, PushNode};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use tracing::error;

pub struct AudioInputNode {
    stream: Option<cpal::Stream>,
}

impl AudioInputNode {
    pub fn new() -> Self {
        Self { stream: None }
    }

    pub fn start<P, const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self, mut pipeline: P) -> Result<()>
    where
        P: PushNode<CHANNELS, SAMPLE_RATE> + 'static,
    {
        let host = cpal::default_host();
        let input_device = host
            .default_input_device()
            .context("No input device available")?;
        let input_config = input_device.default_input_config()?;

        if input_config.sample_rate().0 != SAMPLE_RATE || input_config.channels() as usize != CHANNELS {
            error!(
                "Default input device format {:?} does not match required format ({}ch @ {}Hz)",
                input_config, CHANNELS, SAMPLE_RATE
            );
        }

        let stream = input_device.build_input_stream(
            &input_config.config(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(frame) = AudioBuffer::<f32, CHANNELS, SAMPLE_RATE>::new(data.to_vec()) {
                    pipeline.push(frame, &mut ());
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

pub struct AudioOutputNode {
    stream: Option<cpal::Stream>,
}

impl AudioOutputNode {
    pub fn new() -> Self {
        Self { stream: None }
    }

    pub fn start<P, const CHANNELS: usize, const SAMPLE_RATE: u32>(&mut self, mut pipeline: P) -> Result<()>
    where
        P: PullNode<CHANNELS, SAMPLE_RATE> + 'static,
    {
        let host = cpal::default_host();
        let output_device = host
            .default_output_device()
            .context("No output device available")?;
        let output_config = output_device.default_output_config()?;

        let stream = output_device.build_output_stream(
            &output_config.config(),
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                if let Some(frame) = pipeline.pull(&mut ()) {
                    data.copy_from_slice(frame.data());
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
