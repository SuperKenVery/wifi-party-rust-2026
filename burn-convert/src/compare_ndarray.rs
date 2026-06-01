//! Compare Burn NdArray vs ONNX Runtime outputs, with timing.
pub mod all_rt {
    include!("model/all_rt.rs");
}

use burn::backend::NdArray;
use burn::tensor::Tensor;
use burn::tensor::TensorData;
use std::time::Instant;

type Backend = NdArray;

fn main() {
    let device = Default::default();

    // ── Generate random input ──────────────────────────────────────────
    let mut rng = fastrand::Rng::with_seed(42);
    let batch = 1usize;
    let cri = 4usize;
    let freq = 384usize;
    let time = 64usize;
    let total = batch * cri * freq * time;
    let input_data: Vec<f32> = (0..total).map(|_| rng.f32() * 2.0 - 1.0).collect();

    // ── ONNX Runtime ───────────────────────────────────────────────────
    println!("=== ONNX Runtime (CoreML) ===");
    let onnx_bytes = std::fs::read("../assets/all_rt.onnx").unwrap();
    let mut ort_session = {
        let mut builder = ort::session::Session::builder().unwrap();
        let providers = ort_providers();
        if !providers.is_empty() {
            builder = builder.with_execution_providers(providers).unwrap();
        }
        builder.commit_from_memory(onnx_bytes.as_slice()).unwrap()
    };

    // Warm-up
    for _ in 0..3 {
        let ia =
            ndarray::Array4::from_shape_vec((batch, cri, freq, time), input_data.clone()).unwrap();
        let it = ort::value::Tensor::from_array(ia).unwrap();
        let _ = ort_session.run(ort::inputs!["input" => it]).unwrap();
    }

    let mut ort_times = Vec::new();
    for _ in 0..5 {
        let ia =
            ndarray::Array4::from_shape_vec((batch, cri, freq, time), input_data.clone()).unwrap();
        let it = ort::value::Tensor::from_array(ia).unwrap();
        let t0 = Instant::now();
        let _ = ort_session.run(ort::inputs!["input" => it]).unwrap();
        ort_times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let ort_avg: f64 = ort_times.iter().sum::<f64>() / ort_times.len() as f64;

    // Result
    let ia = ndarray::Array4::from_shape_vec((batch, cri, freq, time), input_data.clone()).unwrap();
    let it = ort::value::Tensor::from_array(ia).unwrap();
    let ort_outputs = ort_session.run(ort::inputs!["input" => it]).unwrap();
    let (_shape, ort_slice) = ort_outputs[0].try_extract_tensor::<f32>().unwrap();
    let ort_result: Vec<f32> = ort_slice.to_vec();

    println!("  Times (ms): {:?}", ort_times);
    println!("  Avg: {:.2} ms", ort_avg);

    // ── Burn NdArray ───────────────────────────────────────────────────
    println!("\n=== Burn (NdArray / CPU) ===");
    let model = all_rt::Model::<Backend>::from_file("src/model/all_rt.bpk", &device);

    // Warm-up
    let warmup = Tensor::<Backend, 4>::from_data(
        TensorData::new(input_data.clone(), [batch, cri, freq, time]),
        &device,
    );
    for i in 0..3 {
        let t0 = Instant::now();
        let _ = model.forward(warmup.clone());
        println!("  warmup {}: {:?}", i + 1, t0.elapsed());
    }

    let mut burn_times = Vec::new();
    for _ in 0..5 {
        let burn_input = Tensor::<Backend, 4>::from_data(
            TensorData::new(input_data.clone(), [batch, cri, freq, time]),
            &device,
        );
        let t0 = Instant::now();
        let burn_output = model.forward(burn_input);
        let _ = burn_output.clone().into_data();
        burn_times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let burn_avg: f64 = burn_times.iter().sum::<f64>() / burn_times.len() as f64;

    // Result
    let burn_input = Tensor::<Backend, 4>::from_data(
        TensorData::new(input_data.clone(), [batch, cri, freq, time]),
        &device,
    );
    let burn_output = model.forward(burn_input);
    let burn_result = burn_output.into_data();
    let burn_vec: Vec<f32> = burn_result.to_vec().unwrap();

    println!("  Times (ms): {:?}", burn_times);
    println!("  Avg: {:.2} ms", burn_avg);

    // ── Summary ────────────────────────────────────────────────────────
    println!("\n=== Summary ===");
    println!("  ORT  (CoreML, ANE): {:.2} ms", ort_avg);
    println!("  Burn (NdArray, CPU): {:.2} ms", burn_avg);
    if burn_avg > 0.0 {
        println!("  Speedup vs ORT: {:.2}x", ort_avg / burn_avg);
    }

    // ── Accuracy ───────────────────────────────────────────────────────
    let n = ort_result.len().min(burn_vec.len());
    let mut max_abs_err: f32 = 0.0;
    let mut sum_abs_err: f64 = 0.0;
    for i in 0..n {
        let abs_err = (ort_result[i] - burn_vec[i]).abs();
        if abs_err > max_abs_err {
            max_abs_err = abs_err;
        }
        sum_abs_err += abs_err as f64;
    }
    let mean_abs_err = sum_abs_err / n as f64;
    println!("\n=== Accuracy ===");
    println!("  Max absolute error: {:.6e}", max_abs_err);
    println!("  Mean absolute error: {:.6e}", mean_abs_err);
    println!(
        "  {}",
        if max_abs_err < 1e-4 && mean_abs_err < 3e-5 {
            "✅ PASS"
        } else {
            "❌ FAIL"
        }
    );
}

#[cfg(target_vendor = "apple")]
fn ort_providers() -> Vec<ort::ep::ExecutionProviderDispatch> {
    use ort::ep;
    vec![
        ep::CoreML::default()
            .with_compute_units(ep::coreml::ComputeUnits::All)
            .with_model_format(ep::coreml::ModelFormat::MLProgram)
            .with_specialization_strategy(ep::coreml::SpecializationStrategy::FastPrediction)
            .build(),
    ]
}

#[cfg(not(target_vendor = "apple"))]
fn ort_providers() -> Vec<ort::ep::ExecutionProviderDispatch> {
    vec![]
}

mod fastrand {
    pub struct Rng(u64);
    impl Rng {
        pub fn with_seed(seed: u64) -> Self {
            Self(seed)
        }
        pub fn f32(&mut self) -> f32 {
            self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z = z ^ (z >> 31);
            (z as u32) as f32 / (u32::MAX as f32)
        }
    }
}
