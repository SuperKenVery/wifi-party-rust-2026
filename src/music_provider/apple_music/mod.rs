//! Apple Music provider.
//!
//! Searches Apple Music, downloads ALAC-encrypted fMP4 from the Apple Music
//! HLS endpoint, and decrypts samples against a local "wrapper" TCP service
//! (typically `127.0.0.1:10020`) that owns the Fairplay keys.
//!
//! Reference implementation: https://github.com/zhaarey/apple-music-downloader
//! Wrapper protocol documented in `mp4_decrypt.rs`.

pub mod api;
pub mod download;
pub mod m3u8;
pub mod mp4_decrypt;
pub mod provider;

pub use provider::factory;
