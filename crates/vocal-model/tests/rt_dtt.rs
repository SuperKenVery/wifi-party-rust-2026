use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use cubecl_fft::wgpu::WgpuDevice as FftWgpuDevice;

use wifi_party_vocal_model::*;

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

fn model_input_from_padded(
    fft_device: &FftWgpuDevice,
    padded: &[f32],
) -> (Vec<f32>, Vec<f32>, Vec<f32>) {
    let (left, right) = deinterleave_stereo(padded);
    let stft = stft_gpu(fft_device, &left, &right);
    let onnx_in = pack_stft_input(&stft);
    (onnx_in, left, right)
}

fn source_waveforms(
    fft_device: &FftWgpuDevice,
    out_vec: &[f32],
    source_idx: usize,
) -> Vec<Vec<f32>> {
    let source_specs = unpack_source_spec(out_vec, source_idx);
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
        let len = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start + len;
        assert!(end <= bytes.len(), "invalid wav chunk length");

        if id == b"fmt " {
            let audio_format = u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap());
            let channels = u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap());
            let sample_rate = u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap());
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

fn read_real_audio_chunks(num_chunks: usize) -> Vec<Vec<f32>> {
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
    let chunk_len = GEN_SIZE * MODEL_CHANNELS;
    let start = start_frame * MODEL_CHANNELS;
    assert!(samples.len() >= start + chunk_len * num_chunks);

    (0..num_chunks)
        .map(|chunk_idx| {
            let chunk_start = start + chunk_idx * chunk_len;
            samples[chunk_start..chunk_start + chunk_len].to_vec()
        })
        .collect()
}

#[cfg(has_vocal_model)]
fn forward_model_direct(input: Vec<f32>) -> Vec<f32> {
    let model = RtDttModel::new().expect("vocal model is not available");
    model.forward(input, [1, 4, DIM_F, DIM_T])
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

#[test]
fn optimized_lstm_matches_burn_lstm() {
    #[cfg(has_vocal_model)]
    {
        let max_error = optimized_lstm_equivalence_error();
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
fn vocal_removal_stays_non_silent_over_consecutive_chunks() {
    #[cfg(has_vocal_model)]
    {
        const CHUNKS: usize = 8;

        println!("loading {CHUNKS} real audio chunks");
        let chunks = read_real_audio_chunks(CHUNKS);
        let Some(separator) = RtDttSeparator::new() else {
            eprintln!("vocal model is not available; skipping consecutive chunk test");
            return;
        };

        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            let output = separator.process_interleaved_chunk(chunk);
            assert_eq!(output.len(), chunk.len());

            let input_rms = rms(chunk);
            let output_rms = rms(&output);
            println!(
                "chunk {} at {:.3}s: input RMS {:.6}, output RMS {:.6}, output/input {:.2}%",
                chunk_idx + 1,
                chunk_idx as f64 * GEN_SIZE as f64 / MODEL_SAMPLE_RATE as f64,
                input_rms,
                output_rms,
                output_rms * 100.0 / input_rms.max(f64::EPSILON),
            );

            assert!(
                output_rms > input_rms * 0.05,
                "chunk {} became silent: input RMS {:.6}, output RMS {:.6}",
                chunk_idx + 1,
                input_rms,
                output_rms
            );
        }
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
