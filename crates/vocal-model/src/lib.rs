use burn::tensor::{Tensor, TensorData};
use burn_store::{BurnpackStore, ModuleSnapshot};
use burn_wgpu::{CubeBackend, WgpuDevice};
use cubecl_fft::wgpu::{WgpuDevice as FftWgpuDevice, WgpuRuntime};
use include_bytes_aligned::include_bytes_aligned;
use tracing::debug;

type WgpuModel = CubeBackend<burn_wgpu::WgpuRuntime, f32, i32, u32>;

#[cfg(has_vocal_model)]
#[allow(dead_code, unused_variables)]
mod model;

#[cfg(has_vocal_model)]
use model::all_rt;

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
    model: all_rt::Model<WgpuModel>,
    #[cfg(has_vocal_model)]
    device: WgpuDevice,
}

impl RtDttModel {
    pub fn new() -> Option<Self> {
        #[cfg(has_vocal_model)]
        {
            debug!("RtDttModel::new entry");
            let device = WgpuDevice::default();
            let aligned_bpk: &'static [u8] = include_bytes_aligned!(
                32,
                concat!(env!("CARGO_MANIFEST_DIR"), "/src/model/all_rt.bpk")
            );
            debug!("RtDttModel::new creating model struct");
            let mut model = all_rt::Model::<WgpuModel>::new(&device);
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

#[cfg(has_vocal_model)]
pub fn optimized_lstm_equivalence_error() -> f32 {
    let device = WgpuDevice::default();
    all_rt::lstm_preproj_equivalence_error::<WgpuModel>(&device)
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
pub fn stft_gpu(
    fft_device: &FftWgpuDevice,
    left: &[f32],
    right: &[f32],
) -> Vec<(Vec<f32>, Vec<f32>)> {
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
pub fn istft_gpu(
    fft_device: &FftWgpuDevice,
    spectrograms: &[(Vec<f32>, Vec<f32>)],
) -> Vec<Vec<f32>> {
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
