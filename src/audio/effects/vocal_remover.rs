//! Vocal remover effect using the RT-DTT model via ONNX Runtime (ort) + gpu-fft.
//!
//! ## How it works
//!
//! At **build time** `build.rs` embeds `all_rt.onnx` into the binary
//! (sets `cfg(has_vocal_model)`).  At **run time** `gpu-fft` (CubeCL / wgpu) does
//! the STFT/iSTFT on the GPU, and ONNX Runtime runs the separation network.
//!
//! When the model is not available at build time (no ONNX file found) the node
//! compiles as a pass-through and emits a tracing warning on first use.
//!
//! ## Algorithm  (matches `infer.py` exactly)
//!
//! For each chunk of `GEN_SIZE = 31 232` input samples:
//!
//! 1. Zero-pad OVERLAP=512 on each side → `INF_CHUNK = 32 256` samples.
//! 2. GPU STFT: batch all frames × channels with `gpu_fft::fft_batch`.
//!    Periodic Hann window, `n_fft=1024`, `hop=512`, `center=True`.
//! 3. Pack first `DIM_F=384` bins into an ONNX input `[1,4,384,64]`.
//! 4. Inference with ONNX Runtime → output `[1,4,4,384,64]`.
//! 5. GPU iSTFT: reconstruct the vocal waveform with `gpu_fft::ifft_batch` + OLA.
//! 6. `instrumental = mix − vocals` (source index 3), trim overlap.

#[cfg(has_vocal_model)]
use ort::session::Session;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::audio::AudioSample;
use crate::audio::decoders::DecodedAudio;
use crate::audio::frame::AudioBuffer;
use crate::pipeline::Node;

// ── Model hyper-parameters ────────────────────────────────────────────────────

const N_FFT: usize = 1024;
const HOP_LENGTH: usize = 512;
const DIM_F: usize = 384; // frequency bins used by the model
const DIM_T: usize = 64; // time frames per chunk
const N_FREQS: usize = N_FFT / 2 + 1; // 513 one-sided bins
const OVERLAP: usize = N_FFT / 2; // 512 — center=True padding
/// Total samples per chunk including OVERLAP on each side.
const INF_CHUNK: usize = HOP_LENGTH * (DIM_T - 1); // 32 256
/// Usable audio samples written per chunk.
const GEN_SIZE: usize = INF_CHUNK - 2 * OVERLAP; // 31 232
const N_SOURCES: usize = 4; // bass / drums / other / vocals
const VOCALS_IDX: usize = 3;
const MODEL_SAMPLE_RATE: u32 = 44_100;
const MODEL_CHANNELS: usize = 2;

// ── Hann window ───────────────────────────────────────────────────────────────

/// Periodic Hann window matching `torch.hann_window(N, periodic=True)`.
fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()))
        .collect()
}

// ── STFT (GPU) ────────────────────────────────────────────────────────────────

/// Compute STFT for both channels using batched GPU FFTs.
///
/// Returns `stft[ch * DIM_T + frame] = (real_1024, imag_1024)`.
/// (`gpu_fft::fft` zero-pads to next power of 2; 1024 is already a power of 2.)
fn stft_gpu(left: &[f32], right: &[f32]) -> Vec<(Vec<f32>, Vec<f32>)> {
    debug_assert_eq!(left.len(), INF_CHUNK);
    debug_assert_eq!(right.len(), INF_CHUNK);

    let window = hann_window(N_FFT);

    // Build 2×DIM_T windowed frames (ch0 first, then ch1).
    let mut signals: Vec<Vec<f32>> = Vec::with_capacity(2 * DIM_T);
    for samples in [left, right] {
        for frame_idx in 0..DIM_T {
            // center=True: frame window is aligned so hop 0 starts at -OVERLAP.
            let frame_start = frame_idx as isize * HOP_LENGTH as isize - OVERLAP as isize;
            let mut frame = vec![0.0f32; N_FFT];
            for i in 0..N_FFT {
                let src = frame_start + i as isize;
                if src >= 0 && (src as usize) < INF_CHUNK {
                    frame[i] = samples[src as usize] * window[i];
                }
                // out-of-range → zero (already 0 from vec initialisation)
            }
            signals.push(frame);
        }
    }

    // Batch FFT on GPU — returns one (real, imag) pair per signal, each length 1024.
    gpu_fft::fft_batch(&signals)
}

// ── iSTFT (GPU) ───────────────────────────────────────────────────────────────

/// Compute iSTFT for N_SOURCES × 2 channels using batched GPU IFFTs + CPU OLA.
///
/// `spectrograms[src * 2 + ch]` = flat `[N_FREQS × DIM_T]` (real, imag) pair.
/// Returns `waveforms[src * 2 + ch]` = `INF_CHUNK` samples.
fn istft_gpu(spectrograms: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    let n_items = spectrograms.len(); // N_SOURCES * 2

    // Build full conjugate-symmetric N_FFT-point spectra for each frame.
    // Batch index: item * DIM_T + frame_idx.
    let mut ifft_in: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(n_items * DIM_T);
    for (re_bins, im_bins) in spectrograms {
        for frame_idx in 0..DIM_T {
            let mut re = vec![0.0f32; N_FFT];
            let mut im = vec![0.0f32; N_FFT];

            // One-sided bins (DIM_F = 384 used, DIM_F..N_FREQS zero-padded, N_FREQS..N_FFT mirrored).
            for freq in 0..N_FREQS {
                let r = if freq < DIM_F {
                    re_bins[freq * DIM_T + frame_idx]
                } else {
                    0.0
                };
                let m = if freq < DIM_F {
                    im_bins[freq * DIM_T + frame_idx]
                } else {
                    0.0
                };
                re[freq] = r;
                im[freq] = m;
            }
            // Conjugate mirror: X[N-k] = conj(X[k]) for k = 1..N/2-1.
            for freq in 1..(N_FFT / 2) {
                re[N_FFT - freq] = re[freq];
                im[N_FFT - freq] = -im[freq];
            }

            ifft_in.push((re, im));
        }
    }

    // Batch IFFT on GPU.
    // Output: each element is Vec<f32> of length 2*N_FFT = 2048.
    //   [0..N_FFT]       = real part (already 1/N normalised by gpu-fft)
    //   [N_FFT..2*N_FFT] = imag part (≈ 0 for symmetric spectra)
    let ifft_out = gpu_fft::ifft_batch(&ifft_in);

    // Overlap-add on CPU (sequential by nature).
    let window = hann_window(N_FFT);
    let padded_len = N_FFT + HOP_LENGTH * (DIM_T - 1); // 33 280

    let mut waveforms = Vec::with_capacity(n_items);
    for item_idx in 0..n_items {
        let mut output = vec![0.0f32; padded_len];
        let mut win_sum = vec![0.0f32; padded_len];

        for frame_idx in 0..DIM_T {
            let frame_data = &ifft_out[item_idx * DIM_T + frame_idx];
            // Real part lives in [0..N_FFT].
            let start = frame_idx * HOP_LENGTH;
            for i in 0..N_FFT {
                let w = window[i];
                output[start + i] += frame_data[i] * w;
                win_sum[start + i] += w * w;
            }
        }

        // Normalize OLA by window-squared envelope.
        for i in 0..padded_len {
            if win_sum[i] > 1e-8 {
                output[i] /= win_sum[i];
            }
        }

        // Trim center=True padding: [OVERLAP .. padded_len - OVERLAP] = INF_CHUNK samples.
        waveforms.push(output[OVERLAP..padded_len - OVERLAP].to_vec());
    }

    waveforms
}

// ── Inference ─────────────────────────────────────────────────────────────────

/// Process one INF_CHUNK stereo chunk through the separation model.
///
/// Returns `GEN_SIZE * 2` interleaved stereo f32 samples (vocal-removed).
#[cfg(has_vocal_model)]
fn infer_chunk(session: &mut Session, chunk: &[f32]) -> Vec<f32> {
    debug_assert_eq!(chunk.len(), INF_CHUNK * 2);

    // 1. De-interleave.
    let mut left = vec![0.0f32; INF_CHUNK];
    let mut right = vec![0.0f32; INF_CHUNK];
    for i in 0..INF_CHUNK {
        left[i] = chunk[i * 2];
        right[i] = chunk[i * 2 + 1];
    }

    // 2. GPU STFT — stft[ch * DIM_T + frame] = (real_1024, imag_1024).
    let stft = stft_gpu(&left, &right);

    // 3. Pack into ONNX input layout: (1, 4, DIM_F, DIM_T)
    //    Channel layout: [ch0_re, ch0_im, ch1_re, ch1_im] (inner-to-outer: t, freq, cri, batch).
    let mut onnx_in = vec![0.0f32; 4 * DIM_F * DIM_T];
    for ch in 0..2usize {
        for frame_idx in 0..DIM_T {
            let (ref re, ref im) = stft[ch * DIM_T + frame_idx];
            for freq in 0..DIM_F {
                onnx_in[(ch * 2) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = re[freq];
                onnx_in[(ch * 2 + 1) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = im[freq];
            }
        }
    }

    // 4. Run ONNX inference.
    //    Input:  [1, 4, DIM_F, DIM_T]
    //    Output: [1, 4, 4, DIM_F, DIM_T]  (batch, source, cri, freq, time)
    let input_array =
        ndarray::Array4::from_shape_vec((1, 4, DIM_F, DIM_T), onnx_in).expect("input shape");
    let input_tensor = ort::value::Tensor::from_array(input_array).expect("create tensor");
    let outputs = session
        .run(ort::inputs!["input" => input_tensor])
        .expect("session run");
    let (_shape, output_slice) = outputs[0]
        .try_extract_tensor::<f32>()
        .expect("extract tensor");
    let out_data: Vec<f32> = output_slice.to_vec();

    // 5. Unpack the vocal source spectrogram.
    //    out_data[src * src_stride + cri * cri_stride + freq * DIM_T + t]
    let src_stride = 4 * DIM_F * DIM_T; // 4 * 384 * 64 = 98 304
    let cri_stride = DIM_F * DIM_T; //     384 * 64 = 24 576

    let vocal_stride = VOCALS_IDX * src_stride;
    let mut vocal_specs: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(2);
    for ch in 0..2usize {
        let mut re_bins = vec![0.0f32; N_FREQS * DIM_T];
        let mut im_bins = vec![0.0f32; N_FREQS * DIM_T];
        for freq in 0..DIM_F {
            for t in 0..DIM_T {
                re_bins[freq * DIM_T + t] =
                    out_data[vocal_stride + (ch * 2) * cri_stride + freq * DIM_T + t];
                im_bins[freq * DIM_T + t] =
                    out_data[vocal_stride + (ch * 2 + 1) * cri_stride + freq * DIM_T + t];
            }
            // Bins DIM_F..N_FREQS remain zero (high-freq padding).
        }
        vocal_specs.push((re_bins, im_bins));
    }

    // 6. GPU iSTFT for vocal channels.
    //    waveforms[ch] = INF_CHUNK samples.
    let waveforms = istft_gpu(&vocal_specs);

    // 7. instrumental = mix − vocals, trim OVERLAP from each end.
    let mut output = vec![0.0f32; GEN_SIZE * 2];
    for i in 0..GEN_SIZE {
        let src_i = OVERLAP + i;
        for ch in 0..2usize {
            let mix_s = chunk[src_i * 2 + ch];
            let voc_s = waveforms[ch][src_i];
            output[i * 2 + ch] = (mix_s - voc_s).clamp(-1.0, 1.0);
        }
    }

    output
}

// ── State ─────────────────────────────────────────────────────────────────────

struct State {
    #[cfg(has_vocal_model)]
    session: Session,
    input_buffer: Vec<f32>,
    output_buffer: Vec<f32>,
}

impl State {
    fn reset(&mut self) {
        self.input_buffer.clear();
        self.output_buffer.clear();
    }

    fn process_interleaved(&mut self, f32_samples: &[f32]) {
        self.input_buffer.extend_from_slice(f32_samples);

        let chunk_samples = GEN_SIZE * MODEL_CHANNELS; // interleaved stereo
        while self.input_buffer.len() >= chunk_samples {
            let chunk_data: Vec<f32> = self.input_buffer.drain(..chunk_samples).collect();

            // Build INF_CHUNK chunk: [ OVERLAP zeros | GEN_SIZE samples | OVERLAP zeros ]
            let mut chunk = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
            let off = OVERLAP * MODEL_CHANNELS;
            chunk[off..off + chunk_samples].copy_from_slice(&chunk_data);

            #[cfg(has_vocal_model)]
            {
                let processed = infer_chunk(&mut self.session, &chunk);
                self.output_buffer.extend_from_slice(&processed);
            }
            #[cfg(not(has_vocal_model))]
            {
                // Pass-through: model not compiled in.
                self.output_buffer.extend_from_slice(&chunk_data);
            }
        }
    }

    fn drain_interleaved(&mut self, out_len: usize) -> Option<Vec<f32>> {
        if self.output_buffer.len() < out_len {
            return None;
        }

        Some(self.output_buffer.drain(..out_len).collect())
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

// ── Public struct ─────────────────────────────────────────────────────────────

/// Removes vocals from stereo 44 100 Hz f32 audio using RT-DTT + GPU acceleration.
///
/// - **Inference**: ONNX Runtime (CoreML on macOS, CPU/GPU elsewhere).
/// - **STFT / iSTFT**: gpu-fft (CubeCL / wgpu — GPU accelerated).
/// - **Latency**: ≈ 0.71 s (one `GEN_SIZE = 31 232` sample chunk at 44 100 Hz).
///
/// # Example
///
/// ```ignore
/// let remover = VocalRemover::new();
/// let pipeline = source.pipe(remover);
/// ```
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
        #[cfg(has_vocal_model)]
        {
            let onnx_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/all_rt.onnx"));
            let session = Session::builder()
                .expect("ort builder")
                .commit_from_memory(onnx_bytes)
                .expect("load onnx model");
            return Self {
                enabled,
                invalid_config_warned: AtomicBool::new(false),
                state: Mutex::new(State {
                    session,
                    input_buffer: Vec::new(),
                    output_buffer: Vec::new(),
                }),
                _marker: std::marker::PhantomData,
            };
        }
        #[cfg(not(has_vocal_model))]
        Self {
            enabled,
            invalid_config_warned: AtomicBool::new(false),
            state: Mutex::new(State {
                input_buffer: Vec::new(),
                output_buffer: Vec::new(),
            }),
            _marker: std::marker::PhantomData,
        }
    }
}

// ── Node impl ─────────────────────────────────────────────────────────────────

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

        // Convert input samples to f32 for processing.
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
        #[cfg(has_vocal_model)]
        {
            let onnx_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/all_rt.onnx"));
            let session = Session::builder()
                .expect("ort builder")
                .commit_from_memory(onnx_bytes)
                .expect("load onnx model");
            return Self {
                enabled,
                invalid_config_warned: AtomicBool::new(false),
                state: Mutex::new(State {
                    session,
                    input_buffer: Vec::new(),
                    output_buffer: Vec::new(),
                }),
            };
        }
        #[cfg(not(has_vocal_model))]
        Self {
            enabled,
            invalid_config_warned: AtomicBool::new(false),
            state: Mutex::new(State {
                input_buffer: Vec::new(),
                output_buffer: Vec::new(),
            }),
        }
    }

    pub fn reset(&self) {
        self.state.lock().unwrap().reset();
    }
}

impl<const CHANNELS: usize, const SAMPLE_RATE: u32> Node
    for DecodedVocalRemover<CHANNELS, SAMPLE_RATE>
{
    type Input = DecodedAudio;
    type Output = DecodedAudio;

    fn process(&self, input: Self::Input) -> Option<Self::Output> {
        if !should_process::<CHANNELS, SAMPLE_RATE>(&self.enabled, &self.invalid_config_warned) {
            self.reset();
            return Some(input);
        }

        let num_frames = input.channels.first().map_or(0, |channel| channel.len());
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::time::Instant;

    use rubato::{FftFixedIn, Resampler};
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions};
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    use super::*;

    const INPUT_PATH: &str = "/Users/ken/Projects/apple-music-downloader/AM-DL downloads/Swedish House Mafia/Don't You Worry Child (feat. John Martin) - EP/01. Don't You Worry Child (Radio Edit) [feat. John Martin].m4a";
    const OUTPUT_PATH: &str = "/tmp/dont_you_worry_child_instrumental.wav";
    const TARGET_SR: u32 = 44100;

    /// Decode the m4a and return (sample_rate, interleaved_stereo_f32).
    fn decode_m4a(path: &str) -> (u32, Vec<f32>, Vec<f32>) {
        let data = std::fs::read(path).expect("audio file not found");
        let mss = MediaSourceStream::new(Box::new(Cursor::new(data)), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("m4a");

        let probed = symphonia::default::get_probe()
            .format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            )
            .expect("probe failed");
        let mut format = probed.format;

        let track = format
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .expect("no audio track");
        let src_rate = track.codec_params.sample_rate.expect("unknown sample rate");
        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let mut decoder = symphonia::default::get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .expect("codec not supported");

        let mut left: Vec<f32> = Vec::new();
        let mut right: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) if p.track_id() == track_id => p,
                Ok(_) => continue,
                Err(_) => break,
            };
            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(_) => continue,
            };
            match decoded {
                AudioBufferRef::F32(buf) => {
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        left.push(buf.chan(0)[f]);
                        right.push(buf.chan(1 % ch)[f]);
                    }
                }
                AudioBufferRef::S16(buf) => {
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        left.push(buf.chan(0)[f] as f32 / 32768.0);
                        right.push(buf.chan(1 % ch)[f] as f32 / 32768.0);
                    }
                }
                AudioBufferRef::S32(buf) => {
                    let ch = buf.spec().channels.count();
                    for f in 0..buf.frames() {
                        left.push(buf.chan(0)[f] as f32 / 2147483648.0);
                        right.push(buf.chan(1 % ch)[f] as f32 / 2147483648.0);
                    }
                }
                _ => {}
            }
        }

        (src_rate, left, right)
    }

    /// Resample per-channel PCM to TARGET_SR using rubato.
    fn resample(left: Vec<f32>, right: Vec<f32>, src_rate: u32) -> (Vec<f32>, Vec<f32>) {
        if src_rate == TARGET_SR {
            return (left, right);
        }
        let chunk = 1024usize;
        let mut resampler =
            FftFixedIn::<f32>::new(src_rate as usize, TARGET_SR as usize, chunk, 1, 2)
                .expect("resampler init failed");

        let mut out_l: Vec<f32> = Vec::new();
        let mut out_r: Vec<f32> = Vec::new();
        let n = left.len();
        let mut pos = 0usize;

        while pos + chunk <= n {
            let result = resampler
                .process(&[&left[pos..pos + chunk], &right[pos..pos + chunk]], None)
                .expect("resample failed");
            out_l.extend_from_slice(&result[0]);
            out_r.extend_from_slice(&result[1]);
            pos += chunk;
        }

        (out_l, out_r)
    }

    /// Diagnose inference speed with ort.
    #[cfg(has_vocal_model)]
    #[test]
    fn infer_chunk_timing() {
        let onnx_bytes = include_bytes!(concat!(env!("OUT_DIR"), "/all_rt.onnx"));
        let mut session = Session::builder()
            .expect("ort builder")
            .commit_from_memory(onnx_bytes)
            .expect("load onnx model");

        let dummy_chunk = vec![0.0f32; super::INF_CHUNK * 2];

        println!("Warming up (ORT session)...");
        let t_warmup = Instant::now();
        let _ = super::infer_chunk(&mut session, &dummy_chunk);
        println!("Warmup: {:.1}ms", t_warmup.elapsed().as_secs_f64() * 1000.0);

        for i in 0..3 {
            let t0 = Instant::now();
            let _ = super::infer_chunk(&mut session, &dummy_chunk);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            println!(
                "Run {i}: {ms:.1}ms / chunk ({:.1}s audio / {:.2}x RT)",
                super::GEN_SIZE as f64 / 44100.0,
                (super::GEN_SIZE as f64 / 44100.0) / t0.elapsed().as_secs_f64()
            );
        }
    }

    #[test]
    fn vocal_removal_speed_test() {
        let (src_rate, left, right) = decode_m4a(INPUT_PATH);
        let total_input_samples = left.len();
        println!(
            "Decoded {} samples at {}Hz ({:.1}s)",
            total_input_samples,
            src_rate,
            total_input_samples as f64 / src_rate as f64
        );

        let (left, right) = resample(left, right, src_rate);
        let total_samples = left.len();
        let audio_duration_secs = total_samples as f64 / TARGET_SR as f64;
        println!(
            "Resampled to {} samples at {}Hz ({:.1}s)",
            total_samples, TARGET_SR, audio_duration_secs
        );

        // Interleave stereo.
        let mut interleaved: Vec<f32> = Vec::with_capacity(total_samples * 2);
        for i in 0..total_samples {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }

        let remover = VocalRemover::<f32, 2, 44100>::new(Arc::new(AtomicBool::new(true)));
        let mut output_samples: Vec<f32> = Vec::new();

        // Feed in chunks matching GEN_SIZE so the node emits output promptly.
        let feed_chunk = GEN_SIZE * 2; // interleaved stereo frames

        // Warmup: trigger any first-run overhead before timing.
        {
            let warmup_buf = AudioBuffer::new(interleaved[..feed_chunk].to_vec())
                .expect("AudioBuffer creation failed");
            let _ = remover.process(warmup_buf);
        }

        let start = Instant::now();
        let mut i = 0;
        while i + feed_chunk <= interleaved.len() {
            let buf = AudioBuffer::new(interleaved[i..i + feed_chunk].to_vec())
                .expect("AudioBuffer creation failed");
            if let Some(out) = remover.process(buf) {
                output_samples.extend_from_slice(out.data());
            }
            i += feed_chunk;
        }

        let elapsed = start.elapsed().as_secs_f64();
        let rt_ratio = audio_duration_secs / elapsed;
        println!(
            "Processed {:.1}s of audio in {:.2}s → {:.2}x real-time",
            audio_duration_secs, elapsed, rt_ratio
        );

        // Write WAV output.
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: TARGET_SR,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(OUTPUT_PATH, spec).expect("wav create failed");
        for &s in &output_samples {
            writer.write_sample(s).expect("wav write failed");
        }
        writer.finalize().expect("wav finalize failed");
        println!("Wrote instrumental to {OUTPUT_PATH}");

        assert!(
            rt_ratio > 0.1,
            "Processing was unreasonably slow: {rt_ratio:.3}x real-time"
        );
    }
}
