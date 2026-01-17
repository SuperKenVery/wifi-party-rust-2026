//! Host pipeline management.
//!
//! This module is deprecated in favor of the stream-based architecture.
//! See [`super::stream::RealtimeAudioStream`] for the new implementation.
//!
//! The old `HostPipelineManager` functionality is now part of `RealtimeAudioStream`,
//! which manages per-(host, stream_id) jitter buffers and mixing.
