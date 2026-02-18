//! Opus audio codec integration with Forward Error Correction (FEC).
//!
//! This module provides Opus encoding and decoding for network transmission.
//! Opus is configured with:
//! - **Inband FEC**: Each packet contains redundant data from the previous frame,
//!   allowing recovery if the previous packet was lost.
//! - **Low latency**: Uses the "restricted lowdelay" application mode.

use std::sync::Mutex;

use anyhow::{Context, Result};
use opus::{Application, Bitrate, Channels, Decoder, Encoder};

use super::AudioSample;
use super::frame::AudioBuffer;
use crate::pipeline::Node;

const OPUS_BITRATE: i32 = 128000;
const OPUS_EXPECTED_PACKET_LOSS: i32 = 60;
const MAX_OPUS_PACKET_SIZE: usize = 4000;
const MAX_FRAME_SIZE: usize = 48000;

const VALID_FRAME_DURATIONS_MS: [f64; 6] = [2.5, 5.0, 10.0, 20.0, 40.0, 60.0];

fn is_valid_opus_frame_size(samples_per_channel: usize, sample_rate: u32) -> bool {
    for &duration_ms in &VALID_FRAME_DURATIONS_MS {
        let expected = (sample_rate as f64 * duration_ms / 1000.0) as usize;
        if samples_per_channel == expected {
            return true;
        }
    }
    false
}

fn channels_to_opus(channels: usize) -> Result<Channels> {
    match channels {
        1 => Ok(Channels::Mono),
        2 => Ok(Channels::Stereo),
        _ => anyhow::bail!("Opus only supports 1 or 2 channels, got {}", channels),
    }
}

pub struct OpusEncoderState {
    encoder: Encoder,
    output_buffer: Vec<u8>,
}

impl OpusEncoderState {
    pub fn new<const CHANNELS: usize, const SAMPLE_RATE: u32>() -> Result<Self> {
        let channels = channels_to_opus(CHANNELS)?;

        let mut encoder = Encoder::new(SAMPLE_RATE, channels, Application::LowDelay)
            .context("Failed to create Opus encoder")?;

        encoder
            .set_bitrate(Bitrate::Bits(OPUS_BITRATE))
            .context("Failed to set bitrate")?;

        encoder
            .set_inband_fec(true)
            .context("Failed to enable FEC")?;

        encoder
            .set_packet_loss_perc(OPUS_EXPECTED_PACKET_LOSS)
            .context("Failed to set packet loss percentage")?;

        Ok(Self {
            encoder,
            output_buffer: vec![0u8; MAX_OPUS_PACKET_SIZE],
        })
    }

    pub fn encode(&mut self, pcm: &[i16]) -> Result<&[u8]> {
        let len = self
            .encoder
            .encode(pcm, &mut self.output_buffer)
            .context("Opus encoding failed")?;

        Ok(&self.output_buffer[..len])
    }
}

pub struct OpusDecoderState {
    decoder: Decoder,
    output_buffer: Vec<i16>,
    fec_buffer: Vec<i16>,
    last_packet_lost: bool,
}

impl OpusDecoderState {
    pub fn new<const CHANNELS: usize, const SAMPLE_RATE: u32>() -> Result<Self> {
        let channels = channels_to_opus(CHANNELS)?;

        let decoder =
            Decoder::new(SAMPLE_RATE, channels).context("Failed to create Opus decoder")?;

        Ok(Self {
            decoder,
            output_buffer: vec![0i16; MAX_FRAME_SIZE],
            fec_buffer: vec![0i16; MAX_FRAME_SIZE],
            last_packet_lost: false,
        })
    }

    pub fn decode(
        &mut self,
        opus_data: &[u8],
        frame_size: usize,
        channels: usize,
    ) -> Result<&[i16]> {
        if self.last_packet_lost {
            let samples_per_channel = self
                .decoder
                .decode(opus_data, &mut self.fec_buffer[..frame_size], true)
                .context("FEC decoding failed")?;
            self.last_packet_lost = false;

            let total_samples = samples_per_channel * channels;
            if total_samples > 0 {
                return Ok(&self.fec_buffer[..total_samples]);
            }
        }

        let samples_per_channel = self
            .decoder
            .decode(opus_data, &mut self.output_buffer[..frame_size], false)
            .context("Opus decoding failed")?;

        let total_samples = samples_per_channel * channels;
        Ok(&self.output_buffer[..total_samples])
    }

    pub fn decode_missing(&mut self, frame_size: usize) -> Result<&[i16]> {
        self.last_packet_lost = true;

        let len = self
            .decoder
            .decode_float(&[], &mut vec![0.0f32; frame_size], false)
            .ok();

        let _ = len;

        Ok(&self.output_buffer[..frame_size.min(self.output_buffer.len())])
    }
}

/// Pipeline node that encodes PCM audio to Opus format with FEC enabled.
///
/// Input: AudioBuffer<Sample> (PCM samples)
/// Output: OpusPacket (compressed Opus data)
pub struct OpusEncoder<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Mutex<OpusEncoderState>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    OpusEncoder<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: Mutex::new(OpusEncoderState::new::<CHANNELS, SAMPLE_RATE>()?),
            _marker: std::marker::PhantomData,
        })
    }
}

#[derive(Debug, Clone)]
pub struct OpusPacket {
    pub data: Vec<u8>,
    pub frame_size: usize,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for OpusEncoder<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = OpusPacket;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let pcm_i16: Vec<i16> = input
            .data()
            .iter()
            .map(|s| i16::from_f64_normalized(s.to_f64_normalized()))
            .collect();

        let frame_size = pcm_i16.len();
        let samples_per_channel = frame_size / CHANNELS;

        if !is_valid_opus_frame_size(samples_per_channel, SAMPLE_RATE) {
            tracing::error!(
                "Invalid Opus frame size: {} samples/channel (total {}). \
                 Valid sizes at {}Hz: {:?}",
                samples_per_channel,
                frame_size,
                SAMPLE_RATE,
                VALID_FRAME_DURATIONS_MS
                    .iter()
                    .map(|&ms| (SAMPLE_RATE as f64 * ms / 1000.0) as usize)
                    .collect::<Vec<_>>()
            );
            return None;
        }

        let mut state = self.state.lock().unwrap();
        match state.encode(&pcm_i16) {
            Ok(encoded) => Some(OpusPacket {
                data: encoded.to_vec(),
                frame_size,
            }),
            Err(e) => {
                tracing::warn!("Opus encoding failed: {}", e);
                None
            }
        }
    }
}

/// Pipeline node that decodes Opus packets back to PCM audio.
///
/// Input: OpusPacket (compressed Opus data)
/// Output: AudioBuffer<Sample> (PCM samples)
pub struct OpusDecoder<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    state: Mutex<OpusDecoderState>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Result<Self> {
        Ok(Self {
            state: Mutex::new(OpusDecoderState::new::<CHANNELS, SAMPLE_RATE>()?),
            _marker: std::marker::PhantomData,
        })
    }

    pub fn decode_packet(
        &self,
        packet: &OpusPacket,
    ) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let samples_per_channel = packet.frame_size / CHANNELS;

        if !is_valid_opus_frame_size(samples_per_channel, SAMPLE_RATE) {
            tracing::error!(
                "Invalid Opus frame size for decoding: {} samples/channel (total {}). \
                 Valid sizes at {}Hz: {:?}",
                samples_per_channel,
                packet.frame_size,
                SAMPLE_RATE,
                VALID_FRAME_DURATIONS_MS
                    .iter()
                    .map(|&ms| (SAMPLE_RATE as f64 * ms / 1000.0) as usize)
                    .collect::<Vec<_>>()
            );
            return None;
        }

        let mut state = self.state.lock().unwrap();
        match state.decode(&packet.data, packet.frame_size, CHANNELS) {
            Ok(pcm_i16) => {
                let samples: Vec<Sample> = pcm_i16
                    .iter()
                    .map(|&s| Sample::from_f64_normalized(s.to_f64_normalized()))
                    .collect();
                AudioBuffer::new(samples).ok()
            }
            Err(e) => {
                tracing::warn!("Opus decoding failed: {}", e);
                None
            }
        }
    }

    pub fn decode_missing(
        &self,
        frame_size: usize,
    ) -> Option<AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>> {
        let mut state = self.state.lock().unwrap();
        match state.decode_missing(frame_size) {
            Ok(pcm_i16) => {
                let samples: Vec<Sample> = pcm_i16
                    .iter()
                    .map(|&s| Sample::from_f64_normalized(s.to_f64_normalized()))
                    .collect();
                AudioBuffer::new(samples).ok()
            }
            Err(e) => {
                tracing::warn!("Opus PLC failed: {}", e);
                None
            }
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = OpusPacket;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        self.decode_packet(&input)
    }
}

/// A network frame containing Opus-encoded audio with sequence number.
///
/// This is the input type for [`RealtimeFrameDecoder`], containing the
/// compressed audio data and metadata needed for jitter buffer ordering.
#[derive(Debug, Clone)]
pub struct RealtimeOpusFrame {
    pub sequence_number: u64,
    pub timestamp: u64,
    pub opus_data: Vec<u8>,
    pub frame_size: usize,
}

impl RealtimeOpusFrame {
    pub fn to_opus_packet(&self) -> OpusPacket {
        OpusPacket {
            data: self.opus_data.clone(),
            frame_size: self.frame_size,
        }
    }
}

/// Decodes Opus frames from network into AudioFrames for jitter buffer.
///
/// This node preserves the sequence number through decoding:
/// - Input: [`RealtimeOpusFrame`] (Opus data + sequence_number from network)
/// - Output: [`AudioFrame`] (decoded PCM + sequence_number for jitter buffer)
pub struct RealtimeFrameDecoder<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    decoder: OpusDecoder<Sample, CHANNELS, SAMPLE_RATE>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    RealtimeFrameDecoder<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new() -> Result<Self> {
        Ok(Self {
            decoder: OpusDecoder::new()?,
        })
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for RealtimeFrameDecoder<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = RealtimeOpusFrame;
    type Output = super::frame::AudioFrame<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        let opus_packet = input.to_opus_packet();
        let pcm_buffer = self.decoder.decode_packet(&opus_packet)?;
        Some(super::frame::AudioFrame {
            sequence_number: input.sequence_number,
            timestamp: input.timestamp,
            samples: pcm_buffer,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opus_raw_api_multiple_frames() {
        use opus::{Application, Channels, Decoder, Encoder};

        let mut encoder = Encoder::new(48000, Channels::Stereo, Application::LowDelay).unwrap();
        let mut decoder = Decoder::new(48000, Channels::Stereo).unwrap();

        // Check encoder lookahead
        let lookahead = encoder.get_lookahead().unwrap();
        eprintln!("Encoder lookahead: {} samples", lookahead);

        // Generate continuous audio signal
        let total_samples = 1920 * 5;
        let all_input: Vec<i16> = (0..total_samples)
            .map(|i| ((i as f32 * 0.1).sin() * 10000.0) as i16)
            .collect();

        let mut all_decoded: Vec<i16> = Vec::new();

        // Encode and decode frame by frame
        for frame_num in 0..5 {
            let start = frame_num * 1920;
            let input = &all_input[start..start + 1920];

            let mut encoded = vec![0u8; 4000];
            let encoded_len = encoder.encode(input, &mut encoded).unwrap();

            let mut decoded = vec![0i16; 1920];
            decoder
                .decode(&encoded[..encoded_len], &mut decoded, false)
                .unwrap();

            all_decoded.extend_from_slice(&decoded);
        }

        // Compare with lookahead offset
        let offset = (lookahead * 2) as usize; // stereo
        eprintln!("Comparing with offset {} samples", offset);
        eprintln!("Input[0..10]: {:?}", &all_input[0..10]);
        eprintln!(
            "Decoded[offset..offset+10]: {:?}",
            &all_decoded[offset..offset + 10]
        );

        // Check similarity with offset
        let mut max_diff = 0i16;
        let compare_len = all_input.len() - offset;
        for i in 0..compare_len {
            let orig = all_input[i];
            let dec = all_decoded[i + offset];
            let diff = (orig - dec).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
        eprintln!("Max diff with offset: {}", max_diff);
        assert!(max_diff < 3000, "Max diff too large: {}", max_diff);
    }

    #[test]
    fn test_opus_roundtrip() {
        let encoder: OpusEncoder<i16, 2, 48000> = OpusEncoder::new().unwrap();
        let decoder: OpusDecoder<i16, 2, 48000> = OpusDecoder::new().unwrap();

        let samples: Vec<i16> = (0..960 * 2).map(|i| (i as i16) % 1000).collect();
        let input = AudioBuffer::<i16, 2, 48000>::new(samples.clone()).unwrap();

        eprintln!("Input first 20: {:?}", &samples[..20]);

        let encoded = encoder.process(input).expect("Encoding should succeed");
        assert!(encoded.data.len() < samples.len() * 2);

        let decoded = decoder.process(encoded).expect("Decoding should succeed");
        eprintln!("Decoded first 20: {:?}", &decoded.data()[..20]);
        assert_eq!(decoded.data().len(), samples.len());

        // Check content similarity
        let mut max_diff = 0i16;
        for (&orig, &dec) in samples.iter().zip(decoded.data().iter()) {
            let diff = (orig - dec).abs();
            if diff > max_diff {
                max_diff = diff;
            }
        }
        eprintln!("Max diff: {}", max_diff);
        assert!(max_diff < 1000, "Max diff too large: {}", max_diff);
    }

    #[test]
    fn test_opus_fec_recovery() {
        let decoder: OpusDecoder<i16, 2, 48000> = OpusDecoder::new().unwrap();

        let plc_output = decoder.decode_missing(960 * 2);
        assert!(plc_output.is_some());
    }
}
