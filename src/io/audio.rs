//! Audio device I/O using cpal.
//!
//! Provides:
//! - [`AudioInput`] for microphone capture
//! - [`LoopbackInput`] for system audio capture (loopback recording)
//! - [`AudioOutput`] for speaker playback

use crate::audio::AudioSample;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::{Pullable, Pushable};
use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, DeviceId, StreamConfig};
use std::sync::{Arc, Mutex};
use tracing::{debug, error, info, warn};

fn find_device_by_id<I: Iterator<Item = Device>>(
    devices: I,
    device_id: &DeviceId,
) -> Option<Device> {
    devices
        .filter_map(|d| d.id().ok().map(|id| (d, id)))
        .find(|(_, id)| id == device_id)
        .map(|(d, _)| d)
}

fn get_input_device(device_id: Option<&DeviceId>) -> Result<Device> {
    let host = cpal::default_host();
    match device_id {
        Some(id) => {
            let devices = host
                .input_devices()
                .context("Failed to enumerate input devices")?;
            find_device_by_id(devices, id).context("Input device not found")
        }
        None => host
            .default_input_device()
            .context("No default input device available"),
    }
}

fn get_output_device(device_id: Option<&DeviceId>) -> Result<Device> {
    let host = cpal::default_host();
    match device_id {
        Some(id) => {
            let devices = host
                .output_devices()
                .context("Failed to enumerate output devices")?;
            find_device_by_id(devices, id).context("Output device not found")
        }
        None => host
            .default_output_device()
            .context("No default output device available"),
    }
}

/// Captures audio from an input device (microphone).
///
/// Supports enable/disable to start/stop the device on demand.
pub struct AudioInput<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sink: Arc<dyn Pushable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
    device_id: Option<DeviceId>,
    stream: Mutex<Option<cpal::Stream>>,
}

impl<Sample: AudioSample + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    AudioInput<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(
        sink: Arc<dyn Pushable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
        device_id: Option<DeviceId>,
    ) -> Self {
        Self {
            sink,
            device_id,
            stream: Mutex::new(None),
        }
    }

    pub fn enable(&self) -> Result<()> {
        let mut stream_guard = self.stream.lock().unwrap();
        if stream_guard.is_some() {
            return Ok(());
        }

        let input_device = get_input_device(self.device_id.as_ref())?;
        let input_config = input_device.default_input_config()?;
        debug!("Input config: {input_config:#?}");

        const MIN_BUFFER_MS: u32 = 3;
        let min_buffer_size = SAMPLE_RATE * MIN_BUFFER_MS / 1000;

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE,
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
        info!("Microphone input enabled");
        *stream_guard = Some(stream);
        Ok(())
    }

    pub fn disable(&self) {
        let mut stream_guard = self.stream.lock().unwrap();
        if stream_guard.take().is_some() {
            info!("Microphone input disabled");
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.stream.lock().unwrap().is_some()
    }
}

/// Captures system audio via loopback recording.
///
/// This works by building an input stream on the default output device,
/// which cpal supports as loopback recording on supported platforms.
pub struct LoopbackInput<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    sink: Arc<dyn Pushable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
}

impl<Sample: AudioSample + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    LoopbackInput<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(sink: Arc<dyn Pushable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>) -> Self {
        Self { sink }
    }

    pub fn start(self, device_id: Option<&DeviceId>) -> Result<cpal::Stream> {
        let output_device = get_output_device(device_id)?;

        info!("Setting up loopback recording on output device");

        let output_config = output_device.default_output_config()?;

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE,
            buffer_size: match output_config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => {
                    let target = 256u32;
                    let size = target.clamp(*min, *max);
                    debug!("Using buffer size: {} (min={}, max={})", size, min, max);
                    BufferSize::Fixed(size)
                }
                cpal::SupportedBufferSize::Unknown => {
                    warn!("Supported buffer size range unknown, using default");
                    BufferSize::Default
                }
            },
        };
        debug!("Using output config for loopback: {:?}", config);

        let sink = self.sink;
        let stream = output_device.build_input_stream(
            &config,
            move |data: &[Sample], _: &cpal::InputCallbackInfo| {
                let owned: Vec<Sample> = Vec::from(data);
                if let Ok(frame) = AudioBuffer::<Sample, CHANNELS, SAMPLE_RATE>::new(owned) {
                    sink.push(frame);
                }
            },
            |err| error!("An error occurred on the loopback audio stream: {}", err),
            None,
        )?;
        stream.play()?;
        info!("Loopback recording started successfully");
        Ok(stream)
    }
}

/// Plays audio to the default output device (speakers).
pub struct AudioOutput<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    source: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>,
}

impl<Sample: AudioSample + cpal::SizedSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    AudioOutput<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(source: Arc<dyn Pullable<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>>>) -> Self {
        Self { source }
    }

    pub fn start(self, device_id: Option<&DeviceId>) -> Result<cpal::Stream> {
        let output_device = get_output_device(device_id)?;
        let output_config = output_device.default_output_config()?;
        debug!("Output config: {output_config:#?}");

        let config = StreamConfig {
            channels: CHANNELS as u16,
            sample_rate: SAMPLE_RATE,
            buffer_size: match output_config.buffer_size() {
                cpal::SupportedBufferSize::Range { min, max } => {
                    let target = 256u32;
                    let size = target.clamp(*min, *max);
                    debug!(
                        "Using output buffer size: {} (min={}, max={})",
                        size, min, max
                    );
                    BufferSize::Fixed(size)
                }
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
