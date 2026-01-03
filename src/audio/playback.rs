use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig, SampleFormat};
use rtrb::Consumer;
use tracing::{info, error};
use dasp_sample::{Sample as DaspSample, FromSample};

pub struct AudioPlaybackHandler {
    _stream: Stream,
}

impl AudioPlaybackHandler {
    /// Start audio playback
    /// Mixes audio from network (mixer output) and loopback (own voice)
    pub fn start(
        network_consumer: Consumer<Vec<i16>>,
        loopback_consumer: Consumer<Vec<i16>>,
    ) -> Result<Self, String> {
        let host = cpal::default_host();
        
        // Get default output device
        let device = host
            .default_output_device()
            .ok_or_else(|| "No output device available".to_string())?;

        info!("Using output device: {}", device.name().unwrap_or_else(|_| "Unknown".to_string()));

        // Get default config
        let config = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {}", e))?;

        info!("Output config: {:?}", config);

        // Build stream config
        let stream_config = StreamConfig {
            channels: config.channels().min(2), // Limit to stereo
            sample_rate: config.sample_rate(),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = match config.sample_format() {
            SampleFormat::I16 => {
                Self::build_output_stream::<i16>(&device, &stream_config, network_consumer, loopback_consumer)?
            }
            SampleFormat::U16 => {
                Self::build_output_stream::<u16>(&device, &stream_config, network_consumer, loopback_consumer)?
            }
            SampleFormat::F32 => {
                Self::build_output_stream::<f32>(&device, &stream_config, network_consumer, loopback_consumer)?
            }
            format => {
                return Err(format!("Unsupported sample format: {:?}", format));
            }
        };

        stream.play().map_err(|e| format!("Failed to play stream: {}", e))?;
        
        info!("Audio playback started");

        Ok(Self { _stream: stream })
    }

    fn build_output_stream<T>(
        device: &Device,
        config: &StreamConfig,
        mut network_consumer: Consumer<Vec<i16>>,
        mut loopback_consumer: Consumer<Vec<i16>>,
    ) -> Result<Stream, String>
    where
        T: cpal::Sample + cpal::SizedSample,
        T: FromSample<i16>,
    {
        let mut network_buffer: Vec<i16> = Vec::new();
        let mut loopback_buffer: Vec<i16> = Vec::new();
        let mut network_index = 0;
        let mut loopback_index = 0;

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    for sample_slot in data.iter_mut() {
                        // Get network sample
                        let network_sample = if network_index >= network_buffer.len() {
                            // Need new frame
                            match network_consumer.pop() {
                                Ok(frame) => {
                                    network_buffer = frame;
                                    network_index = 0;
                                    if !network_buffer.is_empty() {
                                        let s = network_buffer[network_index];
                                        network_index += 1;
                                        s
                                    } else {
                                        0
                                    }
                                }
                                Err(_) => 0, // No network data
                            }
                        } else {
                            let s = network_buffer[network_index];
                            network_index += 1;
                            s
                        };

                        // Get loopback sample
                        let loopback_sample = if loopback_index >= loopback_buffer.len() {
                            // Need new frame
                            match loopback_consumer.pop() {
                                Ok(frame) => {
                                    loopback_buffer = frame;
                                    loopback_index = 0;
                                    if !loopback_buffer.is_empty() {
                                        let s = loopback_buffer[loopback_index];
                                        loopback_index += 1;
                                        s
                                    } else {
                                        0
                                    }
                                }
                                Err(_) => 0, // No loopback data
                            }
                        } else {
                            let s = loopback_buffer[loopback_index];
                            loopback_index += 1;
                            s
                        };

                        // Mix the two sources (simple addition with clamping)
                        let mixed = (network_sample as i32 + loopback_sample as i32)
                            .clamp(-32768, 32767) as i16;
                        
                        *sample_slot = T::from_sample(mixed);
                    }
                },
                move |err| {
                    error!("Audio playback error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {}", e))?;

        Ok(stream)
    }
}
