use anyhow::{Context, Result};
use crossbeam_channel::Receiver;
use rtrb::Producer;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

use crate::audio::AudioFrame;
use crate::state::{AppState, HostId};

pub struct AudioMixer;

impl AudioMixer {
    /// Start the mixer thread
    pub fn start(
        state: Arc<AppState>,
        frame_receiver: Receiver<(HostId, AudioFrame)>,
        playback_producer: Producer<Vec<i16>>,
    ) -> Result<std::thread::JoinHandle<()>> {
        let handle = std::thread::Builder::new()
            .name("audio-mixer".to_string())
            .spawn(move || {
                Self::run(state, frame_receiver, playback_producer);
            })
            .context("Failed to spawn mixer thread")?;

        Ok(handle)
    }

    /// Run the mixer loop
    fn run(
        state: Arc<AppState>,
        frame_receiver: Receiver<(HostId, AudioFrame)>,
        mut playback_producer: Producer<Vec<i16>>,
    ) {
        info!("Audio mixer thread started");

        // Buffer for collecting frames from different hosts
        let mut host_frames: HashMap<HostId, AudioFrame> = HashMap::new();

        // Timeout for host cleanup
        let host_timeout = std::time::Duration::from_secs(5);

        loop {
            // Receive frames with timeout
            match frame_receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok((host_id, frame)) => {
                    host_frames.insert(host_id, frame);
                }
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                    // No new frames, continue to mixing
                }
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    info!("Frame receiver disconnected, mixer stopping");
                    break;
                }
            }

            // Clean up old hosts
            let now = std::time::Instant::now();
            {
                let hosts = state.active_hosts.lock().unwrap();
                host_frames.retain(|host_id, _| {
                    hosts
                        .get(host_id)
                        .map(|info| now.duration_since(info.last_seen) < host_timeout)
                        .unwrap_or(false)
                });
            }

            // Mix frames if we have any
            if !host_frames.is_empty() {
                if let Some(mixed) = Self::mix_frames(&state, &host_frames) {
                    // Try to push to playback queue
                    if playback_producer.push(mixed).is_err() {
                        warn!("Playback queue full, dropping mixed frame");
                    }
                }
            }
        }

        info!("Audio mixer thread stopped");
    }

    /// Mix multiple audio frames into one
    /// Simple summation with soft clipping for Phase 1
    fn mix_frames(
        state: &Arc<AppState>,
        host_frames: &HashMap<HostId, AudioFrame>,
    ) -> Option<Vec<i16>> {
        if host_frames.is_empty() {
            return None;
        }

        // Get the first frame to determine output format
        let first_frame = host_frames.values().next()?;
        let sample_count = first_frame.samples.len();

        // Initialize accumulator with zeros
        let mut mixed: Vec<i32> = vec![0; sample_count];

        // Get host volumes
        let hosts = state.active_hosts.lock().unwrap();

        // Sum all frames with volume control
        for (host_id, frame) in host_frames.iter() {
            // Get volume for this host (default 1.0)
            let volume = hosts.get(host_id).map(|info| info.volume).unwrap_or(1.0);

            // Ensure frames are same length
            if frame.samples.len() != sample_count {
                warn!("Frame size mismatch, skipping host {:?}", host_id);
                continue;
            }

            // Add samples with volume
            for (i, &sample) in frame.samples.iter().enumerate() {
                mixed[i] += (sample as f32 * volume) as i32;
            }
        }

        // Apply soft clipping and convert to i16
        let output: Vec<i16> = mixed
            .iter()
            .map(|&sample| Self::soft_clip(sample))
            .collect();

        Some(output)
    }

    /// Soft clipping function using tanh-like curve
    /// Prevents harsh distortion when sum exceeds i16 range
    fn soft_clip(sample: i32) -> i16 {
        const MAX: i32 = 32767;
        const MIN: i32 = -32768;

        if sample > MAX {
            // Apply soft clipping to positive values
            let excess = (sample - MAX) as f32;
            let compressed = (excess / 10000.0).tanh() * 1000.0;
            (MAX as f32 + compressed).min(32767.0) as i16
        } else if sample < MIN {
            // Apply soft clipping to negative values
            let excess = (sample - MIN) as f32;
            let compressed = (excess / 10000.0).tanh() * 1000.0;
            (MIN as f32 + compressed).max(-32768.0) as i16
        } else {
            sample as i16
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_clip() {
        // Normal range - no clipping
        assert_eq!(AudioMixer::soft_clip(0), 0);
        assert_eq!(AudioMixer::soft_clip(16384), 16384);
        assert_eq!(AudioMixer::soft_clip(-16384), -16384);

        // At boundaries - no clipping
        assert_eq!(AudioMixer::soft_clip(32767), 32767);
        assert_eq!(AudioMixer::soft_clip(-32768), -32768);

        // Beyond boundaries - soft clipping applied
        let clipped_positive = AudioMixer::soft_clip(40000);
        assert!(clipped_positive > 32767);
        assert!(clipped_positive <= 32767 + 1000);

        let clipped_negative = AudioMixer::soft_clip(-40000);
        assert!(clipped_negative < -32768);
        assert!(clipped_negative >= -32768 - 1000);
    }

    #[test]
    fn test_mix_frames_empty() {
        let state = Arc::new(AppState::new());
        let host_frames = HashMap::new();

        let result = AudioMixer::mix_frames(&state, &host_frames);
        assert!(result.is_none());
    }
}
