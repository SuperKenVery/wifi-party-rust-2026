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

// ─────────────────────────────────────────────────────────────────────────────
// Shared building blocks for the generated submodules.
//
// The ONNX→Burn exporter emits the same handful of patterns hundreds of times:
// zero-initialised constant params, instance-norm configs with one fixed
// epsilon, identically-configured convolutions, and a "reshape → instance-norm →
// reshape back → scale → bias" affine block. These helpers collapse that
// boilerplate while preserving the exact numerics (and, crucially, the struct
// field names, since model weights are loaded from `all_rt.bpk` by field path).
// ─────────────────────────────────────────────────────────────────────────────

use burn::module::Param;
use burn::tensor::activation::relu;

/// Epsilon used by every `InstanceNorm` in the exported graph. This is the exact
/// f64 value the exporter wrote (f32 `1e-5` widened to f64), kept verbatim so the
/// normalisation matches the original bit-for-bit.
const INSTANCE_NORM_EPS: f64 = 0.000009999999747378752;

/// A zero-initialised f16 constant parameter (scale/bias for the affine blocks).
fn const_param<B: Backend, const D: usize>(
    shape: [usize; D],
    device: &B::Device,
) -> Param<Tensor<B, D>> {
    Param::uninitialized(
        burn::module::ParamId::new(),
        move |device, _require_grad| {
            Tensor::<B, D>::zeros(shape, (device, burn::tensor::DType::F16))
        },
        device.clone(),
        false,
        shape.into(),
    )
}

/// An `InstanceNorm` over `channels` groups with the graph-wide epsilon.
fn norm<B: Backend>(channels: usize, device: &B::Device) -> InstanceNorm<B> {
    InstanceNormConfig::new(channels)
        .with_epsilon(INSTANCE_NORM_EPS)
        .init(device)
}

/// A `Conv2d` with the exporter's shared settings (unit stride/dilation, single
/// group, bias enabled); only channel counts, kernel size and padding vary.
fn conv<B: Backend>(
    channels: [usize; 2],
    kernel: [usize; 2],
    padding: PaddingConfig2d,
    device: &B::Device,
) -> Conv2d<B> {
    Conv2dConfig::new(channels, kernel)
        .with_stride([1, 1])
        .with_padding(padding)
        .with_dilation([1, 1])
        .with_groups(1)
        .with_bias(true)
        .init(device)
}

/// The static shape of a tensor as `i64`s, used to restore the rank after the
/// instance-norm reshape collapses everything into one trailing dimension.
fn dims_i64<B: Backend, const D: usize>(tensor: &Tensor<B, D>) -> [i64; D] {
    let dims = tensor.dims();
    let mut out = [0i64; D];
    for i in 0..D {
        out[i] = dims[i] as i64;
    }
    out
}

/// The exporter's normalisation block: reshape into `groups` channels, apply
/// instance norm, reshape back, then a per-channel affine (`scale`/`bias`).
///
/// Equivalent to:
/// `x -> reshape([0, groups, -1]) -> norm -> reshape(orig) -> * scale + bias`.
fn norm_affine<B: Backend, const D: usize, const CD: usize>(
    norm: &InstanceNorm<B>,
    scale: &Param<Tensor<B, CD>>,
    bias: &Param<Tensor<B, CD>>,
    groups: i32,
    x: Tensor<B, D>,
) -> Tensor<B, D> {
    let shape = dims_i64(&x);
    norm.forward(x.reshape([0, groups, -1]))
        .reshape(shape)
        .mul(scale.val().unsqueeze_dims(&[0isize]))
        .add(bias.val().unsqueeze_dims(&[0isize]))
}

/// One band/sequence LSTM stage from `submodule3`: permute into `[seq, batch,
/// features]`, run the optimised LSTM seeded from `init_state`, permute back and
/// project with `linear`. `label` names the stage for `VOCAL_MODEL_PROFILE`.
fn lstm_block<B: CustomLstmBackend>(
    lstm: &Lstm<B>,
    linear: &Linear<B>,
    init_state: Tensor<B, 3>,
    label: &'static str,
    affine: Tensor<B, 3>,
) -> Tensor<B, 3> {
    let input = affine.permute([2, 0, 1]);
    let state = LstmState::new(init_state.clone().squeeze_dim(0), init_state.squeeze_dim(0));
    let output_seq = profile_step(label, || {
        let (output_seq, _) = lstm_preproj(lstm, input, Some(state));
        output_seq
    });
    linear.forward(output_seq.permute([1, 0, 2]))
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
        // Build each submodule directly into the struct literal so they are
        // constructed in place in the return slot rather than as separate stack
        // temporaries that are then moved. Combined with `RtDttModel` holding the
        // model behind a `Box`, this keeps construction off the (iOS-constrained)
        // stack.
        Self {
            submodule1: Submodule1::new(device),
            submodule2: Submodule2::new(device),
            submodule3: Submodule3::new(device),
            submodule4: Submodule4::new(device),
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
