//! Audio decoders and resamplers for synced stream playback.
//!
//! Provides push-based pipeline nodes (implementing `Node` trait):
//! - [`PacketCounter`] — tracks packet progress counters
//! - [`SymphoniaDecoder`] — decodes compressed packets to per-channel f32 PCM
//! - [`Interleaver`] — interleaves decoded PCM to AudioBuffer (no resampling)
//! - [`FftResampler`] — resamples decoded PCM to target sample rate, or passes through when rates match

pub mod compressed_packet_queue;
pub mod fft_resampler;
pub mod interleaver;
pub mod symphonia_decoder;

pub use compressed_packet_queue::{CompressedPacket, PacketCounter};
pub use fft_resampler::FftResampler;
pub use interleaver::Interleaver;
pub use symphonia_decoder::{DecodedAudio, SymphoniaDecoder};
