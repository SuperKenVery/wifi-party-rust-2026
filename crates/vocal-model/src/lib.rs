use burn::tensor::{Tensor, TensorData};
use burn_store::{BurnpackStore, ModuleSnapshot};
use burn_wgpu::{Wgpu, WgpuDevice};
use cubecl::wgpu::{WgpuDevice as FftWgpuDevice, WgpuRuntime};
use include_bytes_aligned::include_bytes_aligned;
#[cfg(all(has_vocal_model, any(target_os = "ios", target_os = "macos")))]
use std::sync::OnceLock;
use tracing::debug;

type WgpuModel = Wgpu<f32, i32>;

#[cfg(all(has_vocal_model, any(target_os = "ios", target_os = "macos")))]
static BURN_WGPU_METAL_INIT: OnceLock<()> = OnceLock::new();
#[cfg(all(has_vocal_model, any(target_os = "ios", target_os = "macos")))]
static FFT_WGPU_METAL_INIT: OnceLock<()> = OnceLock::new();

#[cfg(all(has_vocal_model, any(target_os = "ios", target_os = "macos")))]
fn init_burn_wgpu_for_apple(device: &WgpuDevice) {
    BURN_WGPU_METAL_INIT.get_or_init(|| {
        let _ = burn_wgpu::init_setup::<burn_wgpu::graphics::Metal>(
            device,
            burn_wgpu::RuntimeOptions::default(),
        );
    });
}

#[cfg(not(all(has_vocal_model, any(target_os = "ios", target_os = "macos"))))]
fn init_burn_wgpu_for_apple(_device: &WgpuDevice) {}

#[cfg(all(has_vocal_model, any(target_os = "ios", target_os = "macos")))]
fn init_fft_wgpu_for_apple(device: &FftWgpuDevice) {
    FFT_WGPU_METAL_INIT.get_or_init(|| {
        let _ = cubecl::wgpu::init_setup::<cubecl::wgpu::Metal>(
            device,
            cubecl::wgpu::RuntimeOptions::default(),
        );
    });
}

#[cfg(not(all(has_vocal_model, any(target_os = "ios", target_os = "macos"))))]
fn init_fft_wgpu_for_apple(_device: &FftWgpuDevice) {}

#[cfg(has_vocal_model)]
#[allow(dead_code, unused_variables)]
mod all_rt {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/model/all_rt.rs"));
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
    model: Box<all_rt::Model<WgpuModel>>,
    #[cfg(has_vocal_model)]
    device: WgpuDevice,
}

impl RtDttModel {
    pub fn new() -> Option<Self> {
        #[cfg(has_vocal_model)]
        {
            debug!("RtDttModel::new entry");
            let device = WgpuDevice::default();
            init_burn_wgpu_for_apple(&device);
            let aligned_bpk: &'static [u8] = include_bytes_aligned!(
                32,
                concat!(env!("CARGO_MANIFEST_DIR"), "/model/all_rt.bpk")
            );
            debug!("RtDttModel::new creating model struct");
            let mut model = Box::new(all_rt::Model::<WgpuModel>::new(&device));
            debug!("RtDttModel::new loading model weights");
            let mut store = BurnpackStore::from_static(aligned_bpk);
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
            let tensor =
                Tensor::<WgpuModel, 4>::from_data(TensorData::new(input, shape), &self.device);
            let output = self.model.forward(tensor);
            output.into_data().iter::<f32>().collect()
        }

        #[cfg(not(has_vocal_model))]
        {
            let _ = (input, shape);
            unreachable!("RtDttModel::forward called without a generated vocal model")
        }
    }
}

pub struct RtDttSeparator {
    model: Box<RtDttModel>,
    fft_device: FftWgpuDevice,
}

impl RtDttSeparator {
    pub fn new() -> Option<Box<Self>> {
        let fft_device = FftWgpuDevice::default();
        init_fft_wgpu_for_apple(&fft_device);

        Some(Box::new(Self {
            model: Box::new(RtDttModel::new()?),
            fft_device,
        }))
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
    use std::fs;
    use std::path::Path;
    use std::process::Command;
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

    fn rms(samples: &[f32]) -> f64 {
        (samples
            .iter()
            .map(|sample| {
                let sample = *sample as f64;
                sample * sample
            })
            .sum::<f64>()
            / samples.len().max(1) as f64)
            .sqrt()
    }

    fn trim_interleaved(left: &[f32], right: &[f32]) -> Vec<f32> {
        let mut trimmed = Vec::with_capacity(GEN_SIZE * MODEL_CHANNELS);
        for i in OVERLAP..OVERLAP + GEN_SIZE {
            trimmed.push(left[i]);
            trimmed.push(right[i]);
        }
        trimmed
    }

    fn model_input_from_padded(
        fft_device: &FftWgpuDevice,
        padded: &[f32],
    ) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        let mut left = vec![0.0f32; INF_CHUNK];
        let mut right = vec![0.0f32; INF_CHUNK];
        for i in 0..INF_CHUNK {
            left[i] = padded[i * 2];
            right[i] = padded[i * 2 + 1];
        }

        let stft = stft_gpu(fft_device, &left, &right);
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

        (onnx_in, left, right)
    }

    fn source_waveforms(
        fft_device: &FftWgpuDevice,
        out_vec: &[f32],
        source_idx: usize,
    ) -> Vec<Vec<f32>> {
        let src_stride = 4 * DIM_F * DIM_T;
        let cri_stride = DIM_F * DIM_T;
        let source_stride = source_idx * src_stride;

        let mut source_specs: Vec<(Vec<f32>, Vec<f32>)> = Vec::with_capacity(MODEL_CHANNELS);
        for ch in 0..MODEL_CHANNELS {
            let mut re_bins = vec![0.0f32; N_FREQS * DIM_T];
            let mut im_bins = vec![0.0f32; N_FREQS * DIM_T];
            for freq in 0..DIM_F {
                for t in 0..DIM_T {
                    re_bins[freq * DIM_T + t] =
                        out_vec[source_stride + (ch * 2) * cri_stride + freq * DIM_T + t];
                    im_bins[freq * DIM_T + t] =
                        out_vec[source_stride + (ch * 2 + 1) * cri_stride + freq * DIM_T + t];
                }
            }
            source_specs.push((re_bins, im_bins));
        }

        istft_gpu(fft_device, &source_specs)
    }

    fn read_i16_wav_interleaved(path: &Path) -> Vec<f32> {
        let bytes = fs::read(path).expect("failed to read wav");
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");

        let mut offset = 12usize;
        let mut format_ok = false;
        let mut data = None;
        while offset + 8 <= bytes.len() {
            let id = &bytes[offset..offset + 4];
            let len =
                u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
            let start = offset + 8;
            let end = start + len;
            assert!(end <= bytes.len(), "invalid wav chunk length");

            if id == b"fmt " {
                let audio_format = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
                let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
                let sample_rate =
                    u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
                let bits_per_sample =
                    u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap());
                format_ok = (audio_format == 1 || audio_format == 0xfffe)
                    && channels as usize == MODEL_CHANNELS
                    && sample_rate == MODEL_SAMPLE_RATE
                    && bits_per_sample == 16;
            } else if id == b"data" {
                data = Some((start, end));
                break;
            }

            offset = end + (len & 1);
        }

        assert!(format_ok, "expected 44.1 kHz stereo 16-bit PCM WAV");
        let (start, end) = data.expect("wav data chunk missing");
        bytes[start..end]
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()) as f32 / i16::MAX as f32)
            .collect()
    }

    fn read_real_audio_chunk() -> Vec<f32> {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let src = manifest_dir.join("../../assets/read_you.m4a");
        let wav = std::env::temp_dir().join("wifi_party_read_you_44100_s16.wav");
        let status = Command::new("/usr/bin/afconvert")
            .args(["-f", "WAVE", "-d", "LEI16@44100", "-c", "2"])
            .arg(&src)
            .arg(&wav)
            .status()
            .expect("failed to run afconvert");
        assert!(status.success(), "afconvert failed");

        let samples = read_i16_wav_interleaved(&wav);
        let start_frame = 60 * MODEL_SAMPLE_RATE as usize;
        let start = start_frame * MODEL_CHANNELS;
        let len = GEN_SIZE * MODEL_CHANNELS;
        assert!(samples.len() >= start + len);
        samples[start..start + len].to_vec()
    }

    #[cfg(has_vocal_model)]
    fn forward_model_direct(input: Vec<f32>) -> Vec<f32> {
        let device = WgpuDevice::default();
        let aligned_bpk: &'static [u8] =
            include_bytes_aligned!(32, concat!(env!("CARGO_MANIFEST_DIR"), "/model/all_rt.bpk"));
        let mut model = all_rt::Model::<WgpuModel>::new(&device);
        let mut store = BurnpackStore::from_static(aligned_bpk);
        model
            .load_from(&mut store)
            .expect("Failed to load burnpack weights");
        let tensor = Tensor::<WgpuModel, 4>::from_data(
            TensorData::new(input, [1, 4, DIM_F, DIM_T]),
            &device,
        );
        model.forward(tensor).into_data().iter::<f32>().collect()
    }

    fn print_source_diagnostics(
        label: &str,
        fft_device: &FftWgpuDevice,
        out_vec: &[f32],
        mix_left: &[f32],
        mix_right: &[f32],
    ) -> Vec<f32> {
        let mix = trim_interleaved(mix_left, mix_right);
        let mut sum_left = vec![0.0f32; INF_CHUNK];
        let mut sum_right = vec![0.0f32; INF_CHUNK];
        let mut vocals = Vec::new();

        println!("{label}: mix RMS {:.6}", rms(&mix));
        for source_idx in 0..N_SOURCES {
            let waveforms = source_waveforms(fft_device, out_vec, source_idx);
            for i in 0..INF_CHUNK {
                sum_left[i] += waveforms[0][i];
                sum_right[i] += waveforms[1][i];
            }

            let stem = trim_interleaved(&waveforms[0], &waveforms[1]);
            if source_idx == VOCALS_IDX {
                vocals = stem.clone();
            }
            println!(
                "{label}: source {source_idx} RMS {:.6}, source/mix {:.2}%",
                rms(&stem),
                rms(&stem) * 100.0 / rms(&mix).max(f64::EPSILON),
            );
        }

        let summed = trim_interleaved(&sum_left, &sum_right);
        let sum_error = summed
            .iter()
            .zip(mix.iter())
            .map(|(actual, expected)| actual - expected)
            .collect::<Vec<_>>();
        let instrumental = mix
            .iter()
            .zip(vocals.iter())
            .map(|(sample, vocal)| sample - vocal)
            .collect::<Vec<_>>();
        let changed = mix
            .iter()
            .zip(instrumental.iter())
            .map(|(input, output)| input - output)
            .collect::<Vec<_>>();
        println!(
            "{label}: sum-stems err RMS {:.6}; vocal diff RMS {:.6}; instrumental RMS {:.6}",
            rms(&sum_error),
            rms(&changed),
            rms(&instrumental),
        );

        vocals
    }

    fn print_spectrogram_diagnostics(label: &str, out_vec: &[f32]) {
        let src_stride = 4 * DIM_F * DIM_T;
        println!("{label}: output tensor RMS {:.6}", rms(out_vec));
        for source_idx in 0..N_SOURCES {
            let start = source_idx * src_stride;
            let end = start + src_stride;
            println!(
                "{label}: source {source_idx} spectrogram RMS {:.6}",
                rms(&out_vec[start..end])
            );
        }
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
    fn optimized_lstm_matches_burn_lstm() {
        #[cfg(has_vocal_model)]
        {
            let device = WgpuDevice::default();
            let max_error = all_rt::lstm_preproj_equivalence_error::<WgpuModel>(&device);
            assert!(
                max_error < 1e-2,
                "optimized LSTM diverges from Burn LSTM; max abs error {max_error}"
            );
        }
    }

    #[test]
    fn stft_istft_roundtrip_preserves_audio() {
        let fft_device = FftWgpuDevice::default();
        let chunk = test_music_chunk();
        let mut padded = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
        let offset = OVERLAP * MODEL_CHANNELS;
        padded[offset..offset + chunk.len()].copy_from_slice(&chunk);

        let mut left = vec![0.0f32; INF_CHUNK];
        let mut right = vec![0.0f32; INF_CHUNK];
        for i in 0..INF_CHUNK {
            left[i] = padded[i * 2];
            right[i] = padded[i * 2 + 1];
        }

        let stft = stft_gpu(&fft_device, &left, &right);
        let mut specs = Vec::with_capacity(MODEL_CHANNELS);
        for ch in 0..MODEL_CHANNELS {
            let mut re_bins = vec![0.0f32; N_FREQS * DIM_T];
            let mut im_bins = vec![0.0f32; N_FREQS * DIM_T];
            for frame_idx in 0..DIM_T {
                let (re, im) = &stft[ch * DIM_T + frame_idx];
                for freq in 0..N_FREQS {
                    re_bins[freq * DIM_T + frame_idx] = re[freq];
                    im_bins[freq * DIM_T + frame_idx] = im[freq];
                }
            }
            specs.push((re_bins, im_bins));
        }
        let reconstructed = istft_gpu(&fft_device, &specs);

        let left_error = reconstructed[0]
            .iter()
            .zip(left.iter())
            .map(|(actual, expected)| actual - expected)
            .collect::<Vec<_>>();
        let right_error = reconstructed[1]
            .iter()
            .zip(right.iter())
            .map(|(actual, expected)| actual - expected)
            .collect::<Vec<_>>();

        println!(
            "STFT/iSTFT RMS: left in {:.6}, out {:.6}, err {:.6}; right in {:.6}, out {:.6}, err {:.6}",
            rms(&left),
            rms(&reconstructed[0]),
            rms(&left_error),
            rms(&right),
            rms(&reconstructed[1]),
            rms(&right_error),
        );

        assert!(rms(&left_error) < 1e-3, "left roundtrip error too high");
        assert!(rms(&right_error) < 1e-3, "right roundtrip error too high");
    }

    #[test]
    fn real_audio_vocal_removal_diagnostics() {
        #[cfg(has_vocal_model)]
        {
            println!("loading real audio chunk");
            let chunk = read_real_audio_chunk();
            let mut padded = vec![0.0f32; INF_CHUNK * MODEL_CHANNELS];
            let offset = OVERLAP * MODEL_CHANNELS;
            padded[offset..offset + chunk.len()].copy_from_slice(&chunk);

            let fft_device = FftWgpuDevice::default();
            println!("building model input");
            let (onnx_in, left, right) = model_input_from_padded(&fft_device, &padded);

            println!("running model forward");
            let f32_out = forward_model_direct(onnx_in);
            println!("f32 model forward done");
            print_spectrogram_diagnostics("f32", &f32_out);
            let vocals = print_source_diagnostics("f32", &fft_device, &f32_out, &left, &right);
            let mix = trim_interleaved(&left, &right);
            let instrumental = mix
                .iter()
                .zip(vocals.iter())
                .map(|(sample, vocal)| sample - vocal)
                .collect::<Vec<_>>();
            assert!(
                rms(&vocals) > rms(&mix) * 0.25,
                "vocal stem is too small; mix RMS {:.6}, vocal RMS {:.6}",
                rms(&mix),
                rms(&vocals),
            );
            assert!(
                rms(&instrumental) < rms(&mix) * 0.8,
                "instrumental output is too close to input; mix RMS {:.6}, instrumental RMS {:.6}",
                rms(&mix),
                rms(&instrumental),
            );
        }
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
