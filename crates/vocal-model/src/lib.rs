use burn::tensor::{Tensor, TensorData};
use burn_store::ModuleSnapshot;
use burn_wgpu::{Wgpu, WgpuDevice};
use cubecl::wgpu::{WgpuDevice as FftWgpuDevice, WgpuRuntime};
use include_bytes_aligned::include_bytes_aligned;
use tracing::debug;

#[cfg(has_vocal_model)]
#[allow(dead_code, unused_variables)]
mod all_rt {
    include!(concat!(env!("OUT_DIR"), "/model/all_rt.rs"));
}

pub const HAS_MODEL: bool = cfg!(has_vocal_model);

pub const N_FFT: usize = 1024;
pub const HOP_LENGTH: usize = 512;
pub const DIM_F: usize = 384;
pub const DIM_T: usize = 64;
pub const N_FREQS: usize = N_FFT / 2 + 1;
pub const OVERLAP: usize = N_FFT / 2;
/// Total samples per chunk including overlap on each side.
pub const INF_CHUNK: usize = HOP_LENGTH * (DIM_T - 1);
/// Usable audio samples returned per chunk.
pub const GEN_SIZE: usize = INF_CHUNK - 2 * OVERLAP;
pub const N_SOURCES: usize = 4;
pub const VOCALS_IDX: usize = 3;
pub const MODEL_SAMPLE_RATE: u32 = 44_100;
pub const MODEL_CHANNELS: usize = 2;

pub struct RtDttModel {
    #[cfg(has_vocal_model)]
    model: all_rt::Model<Wgpu>,
    #[cfg(has_vocal_model)]
    device: WgpuDevice,
}

impl RtDttModel {
    pub fn new() -> Option<Self> {
        #[cfg(has_vocal_model)]
        {
            debug!("RtDttModel::new entry");
            let device = WgpuDevice::default();
            let aligned_bpk: &'static [u8] =
                include_bytes_aligned!(32, concat!(env!("OUT_DIR"), "/model/all_rt.bpk"));
            debug!("RtDttModel::new creating model struct");
            let mut model = all_rt::Model::<Wgpu>::new(&device);
            debug!("RtDttModel::new loading model weights");
            let mut store = burn_store::BurnpackStore::from_static(aligned_bpk);
            debug!("RtDttModel::new loading model");
            model
                .load_from(&mut store)
                .expect("Failed to load burnpack weights");
            debug!("RtDttModel::new done loading");
            Some(Self { model, device })
        }

        #[cfg(not(has_vocal_model))]
        {
            None
        }
    }

    pub fn forward(&self, input: Vec<f32>, shape: [usize; 4]) -> Vec<f32> {
        #[cfg(has_vocal_model)]
        {
            let input = Tensor::<Wgpu, 4>::from_data(TensorData::new(input, shape), &self.device);
            let output = self.model.forward(input);
            output.into_data().to_vec().expect("burn tensor to vec")
        }

        #[cfg(not(has_vocal_model))]
        {
            let _ = (input, shape);
            unreachable!("RtDttModel::forward called without a generated vocal model")
        }
    }
}

pub struct RtDttSeparator {
    model: RtDttModel,
    fft_device: FftWgpuDevice,
}

impl RtDttSeparator {
    pub fn new() -> Option<Self> {
        Some(Self {
            model: RtDttModel::new()?,
            fft_device: FftWgpuDevice::default(),
        })
    }

    /// Process one `GEN_SIZE` stereo chunk and return interleaved instrumental samples.
    pub fn process_interleaved_chunk(&self, chunk_data: &[f32]) -> Vec<f32> {
        debug_assert_eq!(chunk_data.len(), GEN_SIZE * MODEL_CHANNELS);

        let mut padded = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
        let offset = OVERLAP * MODEL_CHANNELS;
        padded[offset..offset + chunk_data.len()].copy_from_slice(chunk_data);

        self.process_padded_chunk(&padded)
    }

    /// Process one `INF_CHUNK` stereo chunk that already includes overlap padding.
    pub fn process_padded_chunk(&self, chunk: &[f32]) -> Vec<f32> {
        debug_assert_eq!(chunk.len(), INF_CHUNK * MODEL_CHANNELS);

        // 1. De-interleave.
        let mut left = vec![0.0f32; INF_CHUNK];
        let mut right = vec![0.0f32; INF_CHUNK];
        for i in 0..INF_CHUNK {
            left[i] = chunk[i * 2];
            right[i] = chunk[i * 2 + 1];
        }

        // 2. GPU STFT.
        let stft = stft_gpu(&self.fft_device, &left, &right);

        // 3. Pack into input layout: (1, 4, DIM_F, DIM_T)
        //    Channel layout: [ch0_re, ch0_im, ch1_re, ch1_im]
        let mut onnx_in = vec![0.0f32; 4 * DIM_F * DIM_T];
        for ch in 0..MODEL_CHANNELS {
            for frame_idx in 0..DIM_T {
                let (ref re, ref im) = stft[ch * DIM_T + frame_idx];
                for freq in 0..DIM_F {
                    onnx_in[(ch * 2) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = re[freq];
                    onnx_in[(ch * 2 + 1) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = im[freq];
                }
            }
        }

        // 4. Run Burn inference.
        //    Input:  [1, 4, DIM_F, DIM_T]
        //    Output: [1, 4, 4, DIM_F, DIM_T]  (batch, source, cri, freq, time)
        let out_vec = self.model.forward(onnx_in, [1, 4, DIM_F, DIM_T]);

        // 5. Unpack the vocal source spectrogram.
        let src_stride = 4 * DIM_F * DIM_T;
        let cri_stride = DIM_F * DIM_T;
        let vocal_stride = VOCALS_IDX * src_stride;

        let mut vocal_specs: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(MODEL_CHANNELS);
        for ch in 0..MODEL_CHANNELS {
            let mut re_bins = vec![0.0f32; N_FREQS * DIM_T];
            let mut im_bins = vec![0.0f32; N_FREQS * DIM_T];
            for freq in 0..DIM_F {
                for t in 0..DIM_T {
                    re_bins[freq * DIM_T + t] =
                        out_vec[vocal_stride + (ch * 2) * cri_stride + freq * DIM_T + t];
                    im_bins[freq * DIM_T + t] =
                        out_vec[vocal_stride + (ch * 2 + 1) * cri_stride + freq * DIM_T + t];
                }
            }
            vocal_specs.push((re_bins, im_bins));
        }

        // 6. GPU iSTFT for vocal channels.
        let vocal_waveforms = istft_gpu(&self.fft_device, &vocal_specs);

        // 7. Instrumental = mix - vocals.
        let mut output = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
        for i in 0..INF_CHUNK {
            output[i * 2] = (left[i] - vocal_waveforms[0][i]).clamp(-1.0, 1.0);
            output[i * 2 + 1] = (right[i] - vocal_waveforms[1][i]).clamp(-1.0, 1.0);
        }

        // 8. Trim center=True padding.
        let mut trimmed = Vec::with_capacity(GEN_SIZE * MODEL_CHANNELS);
        for i in OVERLAP..OVERLAP + GEN_SIZE {
            trimmed.push(output[i * 2]);
            trimmed.push(output[i * 2 + 1]);
        }

        trimmed
    }
}

/// Periodic Hann window matching `torch.hann_window(N, periodic=True)`.
fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / size as f32).cos()))
        .collect()
}

/// Compute STFT for both channels using batched GPU FFTs.
fn stft_gpu(fft_device: &FftWgpuDevice, left: &[f32], right: &[f32]) -> Vec<(Vec<f32>, Vec<f32>)> {
    debug_assert_eq!(left.len(), INF_CHUNK);
    debug_assert_eq!(right.len(), INF_CHUNK);

    let window = hann_window(N_FFT);

    let mut signals: Vec<Vec<f32>> = Vec::with_capacity(MODEL_CHANNELS * DIM_T);
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
            }
            signals.push(frame);
        }
    }

    gpu_fft::fft::fft_batch::<WgpuRuntime>(fft_device, &signals)
}

/// Compute iSTFT using batched GPU IFFTs plus CPU overlap-add.
fn istft_gpu(fft_device: &FftWgpuDevice, spectrograms: &[(Vec<f32>, Vec<f32>)]) -> Vec<Vec<f32>> {
    let n_items = spectrograms.len();

    let mut ifft_in: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(n_items * DIM_T);
    for (re_bins, im_bins) in spectrograms {
        for frame_idx in 0..DIM_T {
            let mut re = vec![0.0f32; N_FFT];
            let mut im = vec![0.0f32; N_FFT];

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
            for freq in 1..(N_FFT / 2) {
                re[N_FFT - freq] = re[freq];
                im[N_FFT - freq] = -im[freq];
            }

            ifft_in.push((re, im));
        }
    }

    let ifft_out = gpu_fft::ifft::ifft_batch::<WgpuRuntime>(fft_device, &ifft_in);

    let window = hann_window(N_FFT);
    let padded_len = N_FFT + HOP_LENGTH * (DIM_T - 1);

    let mut waveforms = Vec::with_capacity(n_items);
    for item_idx in 0..n_items {
        let mut output = vec![0.0f32; padded_len];
        let mut win_sum = vec![0.0f32; padded_len];

        for frame_idx in 0..DIM_T {
            let frame_data = &ifft_out[item_idx * DIM_T + frame_idx];
            let start = frame_idx * HOP_LENGTH;
            for i in 0..N_FFT {
                let w = window[i];
                output[start + i] += frame_data[i] * w;
                win_sum[start + i] += w * w;
            }
        }

        for i in 0..padded_len {
            if win_sum[i] > 1e-8 {
                output[i] /= win_sum[i];
            }
        }

        waveforms.push(output[OVERLAP..padded_len - OVERLAP].to_vec());
    }

    waveforms
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    const MEASURED_RUNS: usize = 5;

    fn test_music_chunk() -> Vec<f32> {
        let mut chunk = Vec::with_capacity(GEN_SIZE * MODEL_CHANNELS);
        for sample_idx in 0..GEN_SIZE {
            let t = sample_idx as f32 / MODEL_SAMPLE_RATE as f32;
            let left = (2.0 * std::f32::consts::PI * 220.0 * t).sin() * 0.35
                + (2.0 * std::f32::consts::PI * 880.0 * t).sin() * 0.08;
            let right = (2.0 * std::f32::consts::PI * 330.0 * t).sin() * 0.35
                + (2.0 * std::f32::consts::PI * 660.0 * t).sin() * 0.08;
            chunk.push(left);
            chunk.push(right);
        }
        chunk
    }

    fn duration_secs(duration: Duration) -> f64 {
        duration.as_secs_f64()
    }

    fn print_profile_phase(name: &str, elapsed: Duration, audio_secs: f64) {
        let elapsed_secs = duration_secs(elapsed);
        println!(
            "  {name}: {:.1}ms, {:.2}x real-time",
            elapsed_secs * 1000.0,
            audio_secs / elapsed_secs
        );
    }

    fn profile_one_chunk(separator: &RtDttSeparator, chunk_data: &[f32]) -> Vec<f32> {
        let audio_secs_per_chunk = GEN_SIZE as f64 / MODEL_SAMPLE_RATE as f64;

        let start = Instant::now();
        let mut padded = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
        let offset = OVERLAP * MODEL_CHANNELS;
        padded[offset..offset + chunk_data.len()].copy_from_slice(chunk_data);

        let mut left = vec![0.0f32; INF_CHUNK];
        let mut right = vec![0.0f32; INF_CHUNK];
        for i in 0..INF_CHUNK {
            left[i] = padded[i * 2];
            right[i] = padded[i * 2 + 1];
        }
        print_profile_phase("pad + deinterleave", start.elapsed(), audio_secs_per_chunk);

        let start = Instant::now();
        let stft = stft_gpu(&separator.fft_device, &left, &right);
        print_profile_phase("STFT", start.elapsed(), audio_secs_per_chunk);

        let start = Instant::now();
        let mut onnx_in = vec![0.0f32; 4 * DIM_F * DIM_T];
        for ch in 0..MODEL_CHANNELS {
            for frame_idx in 0..DIM_T {
                let (ref re, ref im) = stft[ch * DIM_T + frame_idx];
                for freq in 0..DIM_F {
                    onnx_in[(ch * 2) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = re[freq];
                    onnx_in[(ch * 2 + 1) * DIM_F * DIM_T + freq * DIM_T + frame_idx] = im[freq];
                }
            }
        }
        print_profile_phase("pack model input", start.elapsed(), audio_secs_per_chunk);

        let start = Instant::now();
        let out_vec = separator.model.forward(onnx_in, [1, 4, DIM_F, DIM_T]);
        print_profile_phase("Burn model forward", start.elapsed(), audio_secs_per_chunk);

        let start = Instant::now();
        let src_stride = 4 * DIM_F * DIM_T;
        let cri_stride = DIM_F * DIM_T;
        let vocal_stride = VOCALS_IDX * src_stride;

        let mut vocal_specs: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(MODEL_CHANNELS);
        for ch in 0..MODEL_CHANNELS {
            let mut re_bins = vec![0.0f32; N_FREQS * DIM_T];
            let mut im_bins = vec![0.0f32; N_FREQS * DIM_T];
            for freq in 0..DIM_F {
                for t in 0..DIM_T {
                    re_bins[freq * DIM_T + t] =
                        out_vec[vocal_stride + (ch * 2) * cri_stride + freq * DIM_T + t];
                    im_bins[freq * DIM_T + t] =
                        out_vec[vocal_stride + (ch * 2 + 1) * cri_stride + freq * DIM_T + t];
                }
            }
            vocal_specs.push((re_bins, im_bins));
        }
        print_profile_phase(
            "unpack vocal spectrogram",
            start.elapsed(),
            audio_secs_per_chunk,
        );

        let start = Instant::now();
        let vocal_waveforms = istft_gpu(&separator.fft_device, &vocal_specs);
        print_profile_phase("iSTFT", start.elapsed(), audio_secs_per_chunk);

        let start = Instant::now();
        let mut output = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
        for i in 0..INF_CHUNK {
            output[i * 2] = (left[i] - vocal_waveforms[0][i]).clamp(-1.0, 1.0);
            output[i * 2 + 1] = (right[i] - vocal_waveforms[1][i]).clamp(-1.0, 1.0);
        }

        let mut trimmed = Vec::with_capacity(GEN_SIZE * MODEL_CHANNELS);
        for i in OVERLAP..OVERLAP + GEN_SIZE {
            trimmed.push(output[i * 2]);
            trimmed.push(output[i * 2 + 1]);
        }
        print_profile_phase("subtract + trim", start.elapsed(), audio_secs_per_chunk);

        trimmed
    }

    #[test]
    fn vocal_removal_inference_realtime_ratio() {
        println!("Creating RtDttSeparator...");
        let init_start = Instant::now();
        let Some(separator) = RtDttSeparator::new() else {
            eprintln!("vocal model is not available; skipping inference speed test");
            return;
        };
        println!(
            "RtDttSeparator init: {:.1}ms",
            duration_secs(init_start.elapsed()) * 1000.0
        );

        let chunk = test_music_chunk();
        let audio_secs_per_chunk = GEN_SIZE as f64 / MODEL_SAMPLE_RATE as f64;

        println!(
            "Measuring vocal remover inference: {GEN_SIZE} frames/chunk, {:.3}s audio/chunk",
            audio_secs_per_chunk
        );
        println!("1.00x real-time means inference takes exactly as long as playback.");

        let warmup_start = Instant::now();
        let warmup_output = separator.process_interleaved_chunk(&chunk);
        let warmup_elapsed = warmup_start.elapsed();
        assert_eq!(warmup_output.len(), chunk.len());
        println!("Warmup: {:.1}ms", duration_secs(warmup_elapsed) * 1000.0);

        println!("Phase breakdown after warmup:");
        let profiled_output = profile_one_chunk(&separator, &chunk);
        assert_eq!(profiled_output.len(), chunk.len());

        let mut timings = Vec::with_capacity(MEASURED_RUNS);
        for run_idx in 0..MEASURED_RUNS {
            let start = Instant::now();
            let output = separator.process_interleaved_chunk(&chunk);
            let elapsed = start.elapsed();
            assert_eq!(output.len(), chunk.len());

            let realtime_ratio = audio_secs_per_chunk / duration_secs(elapsed);
            println!(
                "Run {}: {:.1}ms/chunk, {:.2}x real-time",
                run_idx + 1,
                duration_secs(elapsed) * 1000.0,
                realtime_ratio
            );
            timings.push(elapsed);
        }

        let total_elapsed_secs = timings.iter().copied().map(duration_secs).sum::<f64>();
        let processed_audio_secs = audio_secs_per_chunk * timings.len() as f64;
        let realtime_ratio = processed_audio_secs / total_elapsed_secs;
        let avg_ms = total_elapsed_secs * 1000.0 / timings.len() as f64;
        let min_ms = timings
            .iter()
            .copied()
            .map(duration_secs)
            .fold(f64::INFINITY, f64::min)
            * 1000.0;
        let max_ms = timings
            .iter()
            .copied()
            .map(duration_secs)
            .fold(0.0, f64::max)
            * 1000.0;

        println!(
            "Vocal remover inference speed: {:.2}x real-time; processed {:.3}s audio in {:.3}s; avg {:.1}ms/chunk, min {:.1}ms, max {:.1}ms",
            realtime_ratio, processed_audio_secs, total_elapsed_secs, avg_ms, min_ms, max_ms
        );

        assert!(
            realtime_ratio.is_finite() && realtime_ratio > 0.0,
            "invalid real-time ratio: {realtime_ratio}"
        );
    }
}
