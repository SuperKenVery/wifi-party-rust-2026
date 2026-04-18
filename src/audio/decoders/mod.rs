//! Audio decoders and resamplers for synced stream playback.
//!
//! Provides pull-based pipeline nodes:
//! - [`CompressedPacketQueue`] — receives compressed packets, serves them on pull
//! - [`SymphoniaDecoder`] — pulls compressed packets, decodes to per-channel f32 PCM
//! - [`Interleaver`] — pulls decoded PCM, interleaves to AudioBuffer (no resampling)
//! - [`FftResampler`] — pulls decoded PCM, resamples to target sample rate

pub mod compressed_packet_queue;
pub mod fft_resampler;
pub mod interleaver;
pub mod symphonia_decoder;

pub use compressed_packet_queue::CompressedPacketQueue;
pub use fft_resampler::FftResampler;
pub use interleaver::Interleaver;
pub use symphonia_decoder::{DecodedAudio, SymphoniaDecoder};
