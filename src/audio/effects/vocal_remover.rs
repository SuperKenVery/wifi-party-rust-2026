//! Vocal remover effect using the RT-DTT separator from `wifi-party-vocal-model`.
//!
//! ## How it works
//!
//! At **build time** `wifi-party-vocal-model` converts `all_rt.onnx` into Burn
//! Rust code. At **run time** the Wgpu backend runs the
//! separation network, and `gpu-fft` (CubeCL / wgpu) does STFT/iSTFT inside
//! that crate.
//!
//! When the model is not available at build time (no ONNX file found) the node
//! compiles as a pass-through and emits a tracing warning on first use.
//!
use tracing::debug;
#[cfg(feature = "vocal-removal")]
use wifi_party_vocal_model::{GEN_SIZE, MODEL_CHANNELS, MODEL_SAMPLE_RATE, RtDttSeparator};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::AudioSample;
use crate::audio::decoders::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::Node;

#[cfg(not(feature = "vocal-removal"))]
const GEN_SIZE: usize = 31_232;
#[cfg(not(feature = "vocal-removal"))]
const MODEL_SAMPLE_RATE: u32 = 44_100;
#[cfg(not(feature = "vocal-removal"))]
const MODEL_CHANNELS: usize = 2;

// ── State ─────────────────────────────────────────────────────────────────────

struct State {
    #[cfg(feature = "vocal-removal")]
    separator: Option<Box<RtDttSeparator>>,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
}

impl State {
    fn reset(&mut self) {
        self.input_buffer.clear();
        self.output_buffer.clear();
    }

    #[cfg(feature = "vocal-removal")]
    fn process_impl(&mut self, chunk_data: &[f32]) {
        let Some(separator) = self.separator.as_ref() else {
            self.output_buffer.extend_from_slice(chunk_data);
            return;
        };

        let processed = separator.process_interleaved_chunk(chunk_data);
        self.output_buffer.extend_from_slice(&processed);
    }

    #[cfg(not(feature = "vocal-removal"))]
    fn process_impl(&mut self, chunk_data: &[f32]) {
        self.output_buffer.extend_from_slice(chunk_data);
    }

    fn process_interleaved(&mut self, f32_samples: &[f32]) {
        self.input_buffer.extend_from_slice(f32_samples);

        let chunk_samples = GEN_SIZE * MODEL_CHANNELS;
        while self.input_buffer.len() >= chunk_samples {
            let chunk_data: Vec<f32> = self.input_buffer.drain(..chunk_samples).collect();
            self.process_impl(&chunk_data);
        }
    }

    fn drain_interleaved(&mut self, out_len: usize) -> Option<Vec<f32>> {
        if self.output_buffer.len() < out_len {
            return None;
        }
        Some(self.output_buffer.drain(..out_len).collect())
    }

    /// Zero-pad the remaining input to a full chunk, run inference, and return
    /// everything in the output buffer. Called when the source is exhausted.
    fn flush_interleaved(&mut self) -> Vec<f32> {
        let chunk_samples = GEN_SIZE * MODEL_CHANNELS;
        if !self.input_buffer.is_empty() {
            self.input_buffer.resize(chunk_samples, 0.0);
            let chunk_data: Vec<f32> = self.input_buffer.drain(..chunk_samples).collect();
            self.process_impl(&chunk_data);
        }
        std::mem::take(&mut self.output_buffer)
    }
}

fn should_process<const CHANNELS: usize, const SAMPLE_RATE: u32>(
    enabled: &AtomicBool,
    invalid_config_warned: &AtomicBool,
) -> bool {
    if !enabled.load(Ordering::Relaxed) {
        return false;
    }

    if CHANNELS != MODEL_CHANNELS || SAMPLE_RATE != MODEL_SAMPLE_RATE {
        if !invalid_config_warned.swap(true, Ordering::Relaxed) {
            tracing::warn!(
                "Vocal removal requires {} Hz stereo audio; got {} Hz with {} channels. Passing through.",
                MODEL_SAMPLE_RATE,
                SAMPLE_RATE,
                CHANNELS
            );
        }
        return false;
    }

    true
}

// ── Public structs ────────────────────────────────────────────────────────────

/// Removes vocals from stereo 44 100 Hz f32 audio using RT-DTT + GPU acceleration.
///
/// - **Inference**: Burn Wgpu
/// - **STFT / iSTFT**: gpu-fft (CubeCL / wgpu — GPU accelerated).
/// - **Latency**: ≈ 0.71 s (one `GEN_SIZE = 31 232` sample chunk at 44 100 Hz).
pub struct VocalRemover<Sample, const CHANNELS: usize, const SAMPLE_RATE: u32> {
    enabled: Arc<AtomicBool>,
    invalid_config_warned: AtomicBool,
    state: Mutex<State>,
    _marker: std::marker::PhantomData<Sample>,
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32>
    VocalRemover<Sample, CHANNELS, SAMPLE_RATE>
{
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        #[cfg(feature = "vocal-removal")]
        let state = State {
            separator: RtDttSeparator::new(),
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        };
        #[cfg(not(feature = "vocal-removal"))]
        let state = State {
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        };

        Self {
            enabled,
            invalid_config_warned: AtomicBool::new(false),
            state: Mutex::new(state),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Sample: AudioSample, const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for VocalRemover<Sample, CHANNELS, SAMPLE_RATE>
{
    type Input = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;
    type Output = AudioBuffer<Sample, CHANNELS, SAMPLE_RATE>;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        if !should_process::<CHANNELS, SAMPLE_RATE>(&self.enabled, &self.invalid_config_warned) {
            self.state.lock().unwrap().reset();
            return Some(input);
        }

        let mut st = self.state.lock().unwrap();

        let f32_samples: Vec<f32> = input
            .data()
            .iter()
            .map(|s| s.to_f64_normalized() as f32)
            .collect();
        st.process_interleaved(&f32_samples);

        let out_len = input.data().len();
        let data: Vec<Sample> = st
            .drain_interleaved(out_len)?
            .into_iter()
            .map(|s| Sample::from_f64_normalized(s as f64))
            .collect();
        AudioBuffer::new(data).ok()
    }
}

/// Removes vocals from channel-separated 44 100 Hz stereo audio.
///
/// The RT-DTT model was trained and validated for 44.1 kHz stereo. The synced
/// music pipeline is responsible for resampling to this exact rate before this
/// node and resampling back to the app output rate afterwards.
pub struct DecodedVocalRemover<const CHANNELS: usize, const SAMPLE_RATE: u32> {
    enabled: Arc<AtomicBool>,
    invalid_config_warned: AtomicBool,
    state: Mutex<State>,
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> DecodedVocalRemover<CHANNELS, SAMPLE_RATE> {
    pub fn new(enabled: Arc<AtomicBool>) -> Self {
        debug!(
            "DecodedVocalRemover::new, enabled={}",
            enabled.load(Ordering::Relaxed)
        );
        #[cfg(feature = "vocal-removal")]
        let state = State {
            separator: RtDttSeparator::new(),
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        };
        #[cfg(not(feature = "vocal-removal"))]
        let state = State {
            input_buffer: Vec::new(),
            output_buffer: Vec::new(),
        };

        Self {
            enabled,
            invalid_config_warned: AtomicBool::new(false),
            state: Mutex::new(state),
        }
    }

    pub fn reset(&self) {
        self.state.lock().unwrap().reset();
    }

    /// Flush remaining buffered input (zero-padded) and return whatever audio
    /// the model produces. Call this when the source is exhausted and no more
    /// input will arrive.
    pub fn flush(&self) -> Option<DecodedAudio> {
        if !should_process::<CHANNELS, SAMPLE_RATE>(&self.enabled, &self.invalid_config_warned) {
            return None;
        }
        let flushed = self.state.lock().unwrap().flush_interleaved();
        if flushed.is_empty() {
            return None;
        }
        let num_frames = flushed.len() / CHANNELS;
        let mut channels = vec![Vec::with_capacity(num_frames); CHANNELS];
        for frame in flushed.chunks_exact(CHANNELS) {
            for ch in 0..CHANNELS {
                channels[ch].push(frame[ch]);
            }
        }
        Some(DecodedAudio { channels })
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for DecodedVocalRemover<CHANNELS, SAMPLE_RATE>
{
    type Input = DecodedAudio;
    type Output = DecodedAudio;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        if !should_process::<CHANNELS, SAMPLE_RATE>(&self.enabled, &self.invalid_config_warned) {
            // debug!("DecodedVocalRemover::process should not process");
            self.reset();
            return Some(input);
        }

        let num_frames = input.channels.first().map_or(0, |channel| channel.len());
        // debug!("DecodedVocalRemover::process num_frames={}", num_frames);
        if num_frames == 0 {
            return None;
        }

        let mut interleaved = Vec::with_capacity(num_frames * CHANNELS);
        for frame in 0..num_frames {
            for channel in 0..CHANNELS {
                interleaved.push(input.channels[channel][frame]);
            }
        }

        let mut state = self.state.lock().unwrap();
        state.process_interleaved(&interleaved);
        // debug!("DecodedVocalRemover::process draining interleaved");
        let output = state.drain_interleaved(num_frames * CHANNELS)?;

        let mut channels = (0..CHANNELS)
            .map(|_| Vec::with_capacity(num_frames))
            .collect::<Vec<_>>();
        for frame in output.chunks_exact(CHANNELS) {
            for channel in 0..CHANNELS {
                channels[channel].push(frame[channel]);
            }
        }

        Some(DecodedAudio { channels })
    }
}
