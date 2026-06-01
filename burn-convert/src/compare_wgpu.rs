//! Compare Burn WGPU with-fusion vs no-fusion (same binary, same run).
use burn::tensor::Tensor;
use burn::tensor::TensorData;
use burn_wgpu::{CubeBackend, Wgpu, WgpuDevice};
use cubecl::wgpu::WgpuRuntime;
use std::time::Instant;

pub mod all_rt {
    include!("model/all_rt.rs");
}

fn main() {
    let device = WgpuDevice::default();
    println!("Device: {:?}", device);

    let mut rng = fastrand::Rng::with_seed(42);
    let s = [1usize, 4, 384, 64];
    let total: usize = s.iter().product();
    let input_data: Vec<f32> = (0..total).map(|_| rng.f32() * 2.0 - 1.0).collect();
    let td = TensorData::new(input_data, s);

    {
        type Bw = Wgpu; // Fusion<CubeBackend>
        println!("\n=== WITH fusion (Fusion<CubeBackend>) ===");
        time_it::<Bw>(&device, &td);
    }
    {
        type Bn = CubeBackend<WgpuRuntime, f32, i32, u32>;
        println!("\n=== WITHOUT fusion (raw CubeBackend) ===");
        time_it::<Bn>(&device, &td);
    }
}

fn time_it<B: burn::tensor::backend::Backend>(device: &B::Device, td: &TensorData) {
    let model = all_rt::Model::<B>::from_file("src/model/all_rt.bpk", device);

    // warmup
    let warmup = Tensor::<B, 4>::from_data(td.clone(), device);
    for i in 0..5 {
        let t0 = Instant::now();
        let out = model.forward(warmup.clone());
        drop(out.into_data());
        println!("  warmup {i}: {:?}", t0.elapsed());
    }
    // timed
    let mut times = Vec::new();
    for _ in 0..5 {
        let input = Tensor::<B, 4>::from_data(td.clone(), device);
        let t0 = Instant::now();
        let out = model.forward(input);
        drop(out.into_data());
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let avg: f64 = times.iter().sum::<f64>() / times.len() as f64;
    let best = times.iter().copied().fold(f64::INFINITY, f64::min);
    println!("  Times (ms): {times:?}");
    println!("  Avg: {avg:.2} ms | Best: {best:.2} ms");
}

mod fastrand {
    /* unchanged */
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
