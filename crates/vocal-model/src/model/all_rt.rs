// Generated from ONNX "/Users/ken/Codes/wifi-party/crates/vocal-model/../../assets/all_rt.onnx" by burn-onnx
// Pre-projection LSTM optimization: batches all input projections into one matmul
// and reduces hidden projections from 4 per step to 1 per step.
use burn::nn::InstanceNorm;
use burn::nn::InstanceNormConfig;
use burn::nn::Linear;
use burn::nn::LinearConfig;
use burn::nn::Lstm;
use burn::nn::LstmConfig;
use burn::nn::LstmState;
use burn::nn::PaddingConfig2d;
use burn::nn::conv::Conv2d;
use burn::nn::conv::Conv2dConfig;
use burn::prelude::*;
use burn::tensor::{Bytes, TensorPrimitive};
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;
use burn_wgpu::{CubeBackend, CubeTensor, WgpuDevice, WgpuRuntime};
use cubecl::calculate_cube_count_elemwise;
use cubecl::frontend::{
    CompilationArg, CubeIndexExpand, CubeIndexMutExpand, ExpExpand, TanhExpand,
};
use cubecl::prelude::{
    ABSOLUTE_POS, AddressType, CubeDim, Float, ReadWrite, StorageType, terminate,
};
use cubecl::std::tensor::layout::linear::LinearView;

type WgpuCubeBackend = CubeBackend<WgpuRuntime, f32, i32, u32>;

pub(crate) trait CustomLstmBackend:
    Backend<Device = WgpuDevice, FloatElem = f32, FloatTensorPrimitive = CubeTensor<WgpuRuntime>>
{
}

impl CustomLstmBackend for WgpuCubeBackend {}

fn profile_step<T>(name: &'static str, op: impl FnOnce() -> T) -> T {
    if std::env::var_os("VOCAL_MODEL_PROFILE").is_some() {
        let start = std::time::Instant::now();
        let output = op();
        println!(
            "    model::{name}: {:.1}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
        output
    } else {
        op()
    }
}

mod lstm;
mod submodule1;
mod submodule2;
mod submodule3;
mod submodule4;

use lstm::lstm_preproj;
pub(crate) use lstm::lstm_preproj_equivalence_error;
use submodule1::Submodule1;
use submodule2::Submodule2;
use submodule3::Submodule3;
use submodule4::Submodule4;

#[derive(Module, Debug)]
pub struct Model<B: Backend> {
    submodule1: Submodule1<B>,
    submodule2: Submodule2<B>,
    submodule3: Submodule3<B>,
    submodule4: Submodule4<B>,
    phantom: core::marker::PhantomData<B>,
    #[module(skip)]
    device: B::Device,
}

extern crate std;

impl<B: Backend> Default for Model<B> {
    fn default() -> Self {
        Self::from_file(
            concat!(env!("CARGO_MANIFEST_DIR"), "/src/model/all_rt.bpk"),
            &Default::default(),
        )
    }
}

impl<B: Backend> Model<B> {
    /// Load model weights from a burnpack file.
    pub fn from_file<P: AsRef<std::path::Path>>(file: P, device: &B::Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_file(file);
        model
            .load_from(&mut store)
            .expect("Failed to load burnpack file");
        model
    }

    /// Load model weights from in-memory bytes.
    ///
    /// The bytes must be the contents of a `.bpk` file.
    pub fn from_bytes(bytes: Bytes, device: &B::Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_bytes(Some(bytes));
        model
            .load_from(&mut store)
            .expect("Failed to load burnpack bytes");
        model
    }
}

impl<B: Backend> Model<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let submodule1 = Submodule1::new(device);
        let submodule2 = Submodule2::new(device);
        let submodule3 = Submodule3::new(device);
        let submodule4 = Submodule4::new(device);
        Self {
            submodule1,
            submodule2,
            submodule3,
            submodule4,
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }

    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, input: Tensor<B, 4>) -> Tensor<B, 5>
    where
        B: CustomLstmBackend,
    {
        let add10_out1 = profile_step("submodule1", || self.submodule1.forward(input));
        let add20_out1 = profile_step("submodule2", || self.submodule2.forward(add10_out1.clone()));
        let mul25_out1 = profile_step("submodule3", || {
            self.submodule3.forward(add20_out1, add10_out1)
        });
        let reshape81_out1 = profile_step("submodule4", || self.submodule4.forward(mul25_out1));
        reshape81_out1
    }
}
