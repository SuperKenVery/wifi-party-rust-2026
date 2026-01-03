use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig, SampleFormat};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use rtrb::Producer;
use tracing::{info, warn, error};
use dasp_sample::{Sample as DaspSample, FromSample};

use crate::audio::AudioFrame;
use crate::state::AppState;

pub struct AudioCaptureHandler {
    _stream: Stream,
}

impl AudioCaptureHandler {
    /// Start audio capture
    pub fn start(
        state: Arc<AppState>,
        network_producer: Producer<Vec<u8>>,
        loopback_producer: Producer<Vec<i16>>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        
        // Get default input device
        let device = host
            .default_input_device()
            .ok_or_else(|| "No input device available".to_string())?;

        info!("Using input device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));

        // Get default config
        let config = device
            .default_input_config()
            .map_err(|e| format!("Failed to get default input config: {}", e))?;

        info!("Input config: {:?}", config);

        // Build stream config
        let stream_config = StreamConfig {
            channels: config.channels().min(2), // Limit to stereo
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let channels = stream_config.channels as u8;
        let sample_rate = stream_config.sample_rate.0;

        // Get target frame size from state
        let frame_size = state.audio_config.lock().unwrap().frame_size * channels as usize;
        
        let stream = match config.sample_format() {
            SampleFormat::I16 => {
                Self::build_input_stream::<i16>(
                    &device,
                    &stream_config,
                    state.clone(),
                    network_producer,
                    loopback_producer,
                    frame_size,
                    sample_rate,
                    channels,
                )?
            }
            SampleFormat::U16 => {
                Self::build_input_stream::<u16>(
                    &device,
                    &stream_config,
                    state.clone(),
                    network_producer,
                    loopback_producer,
                    frame_size,
                    sample_rate,
                    channels,
                )?
            }
            SampleFormat::F32 => {
                Self::build_input_stream::<f32>(
                    &device,
                    &stream_config,
                    state.clone(),
                    network_producer,
                    loopback_producer,
                    frame_size,
                    sample_rate,
                    channels,
                )?
            }
            format => {
                return Err(format!("Unsupported sample format: {:?}", format));
            }
        };

        stream.play().map_err(|e| format!("Failed to play stream: {}", e))?;
        
        info!("Audio capture started");

        Ok(Self { _stream: stream })
    }

    fn build_input_stream<T>(
        device: &Device,
        config: &StreamConfig,
        state: Arc<AppState>,
        mut network_producer: Producer<Vec<u8>>,
        mut loopback_producer: Producer<Vec<i16>>,
        frame_size: usize,
        sample_rate: u32,
        channels: u8,
    ) -> Result<Stream, String>
    where
        T: cpal::Sample + cpal::SizedSample,
    {
        let mut buffer = Vec::with_capacity(frame_size);

        let stream = device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    // Convert samples to i16
                    for sample in data {
                        let i16_sample = Self::to_i16_sample::<T>(sample);
                        buffer.push(i16_sample);

                        // When we have a full frame, process it
                        if buffer.len() >= frame_size {
                            let frame_samples = buffer.drain(..frame_size).collect::<Vec<_>>();
                            
                            // Check if mic is muted
                            let is_muted = state.mic_muted.load(Ordering::Relaxed);
                            
                            let samples = if is_muted {
                                vec![0; frame_size]
                            } else {
                                // Apply mic volume
                                let volume = *state.mic_volume.lock().unwrap();
                                frame_samples.iter()
                                    .map(|&s| {
                                        let scaled = (s as f32 * volume) as i32;
                                        scaled.clamp(-32768, 32767) as i16
                                    })
                                    .collect()
                            };

                            // Get sequence number and increment
                            let seq = state.sequence_number.fetch_add(1, Ordering::Relaxed);

                            // If loopback is enabled, send to loopback queue
                            if state.loopback_enabled.load(Ordering::Relaxed) && !is_muted {
                                if loopback_producer.push(samples.clone()).is_err() {
                                    // Loopback queue full, just continue (don't warn, it's not critical)
                                }
                            }

                            // Create audio frame (no host_id, sample_rate is always 48kHz)
                            let frame = AudioFrame::new(
                                seq,
                                channels,
                                samples,
                            );

                            // Serialize frame
                            match frame.serialize() {
                                Ok(serialized) => {
                                    // Try to send to network queue
                                    if network_producer.push(serialized).is_err() {
                                        warn!("Send queue full, dropping frame");
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to serialize frame: {}", e);
                                }
                            }
                        }
                    }
                },
                move |err| {
                    error!("Audio capture error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to build input stream: {}", e))?;

        Ok(stream)
    }

    fn to_i16_sample<T>(sample: &T) -> i16
    where
        T: cpal::Sample,
    {
        // Convert through f32 using dasp_sample
        let f32_val: f32 = sample.to_float_sample().to_sample();
        (f32_val * 32767.0).clamp(-32768.0, 32767.0) as i16
    }
}
