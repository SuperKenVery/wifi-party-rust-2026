// Generated from ONNX "/Users/ken/Codes/wifi-party/crates/vocal-model/../../assets/all_rt.onnx" by burn-onnx
// Pre-projection LSTM optimization: batches all input projections into one matmul
// and reduces hidden projections from 4 per step to 1 per step.
use burn::prelude::*;
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
use burn::tensor::Bytes;
use burn_store::BurnpackStore;
use burn_store::ModuleSnapshot;

#[cfg(test)]
fn profile_step<T>(name: &'static str, op: impl FnOnce() -> T) -> T {
    if std::env::var_os("VOCAL_MODEL_PROFILE").is_some() {
        let start = std::time::Instant::now();
        let output = op();
        println!("    model::{name}: {:.1}ms", start.elapsed().as_secs_f64() * 1000.0);
        output
    } else {
        op()
    }
}

#[cfg(not(test))]
#[inline(always)]
fn profile_step<T>(_: &'static str, op: impl FnOnce() -> T) -> T {
    op()
}


#[derive(Module, Debug)]
pub struct Submodule1<B: Backend> {
    conv2d1: Conv2d<B>,
    instancenormalization1: InstanceNorm<B>,
    constant47: burn::module::Param<Tensor<B, 3>>,
    constant48: burn::module::Param<Tensor<B, 3>>,
    conv2d2: Conv2d<B>,
    instancenormalization2: InstanceNorm<B>,
    constant49: burn::module::Param<Tensor<B, 3>>,
    constant50: burn::module::Param<Tensor<B, 3>>,
    conv2d3: Conv2d<B>,
    instancenormalization3: InstanceNorm<B>,
    constant51: burn::module::Param<Tensor<B, 3>>,
    constant52: burn::module::Param<Tensor<B, 3>>,
    conv2d4: Conv2d<B>,
    instancenormalization4: InstanceNorm<B>,
    constant53: burn::module::Param<Tensor<B, 3>>,
    constant54: burn::module::Param<Tensor<B, 3>>,
    linear1: Linear<B>,
    instancenormalization5: InstanceNorm<B>,
    constant56: burn::module::Param<Tensor<B, 3>>,
    constant57: burn::module::Param<Tensor<B, 3>>,
    linear2: Linear<B>,
    instancenormalization6: InstanceNorm<B>,
    constant59: burn::module::Param<Tensor<B, 3>>,
    constant60: burn::module::Param<Tensor<B, 3>>,
    conv2d5: Conv2d<B>,
    instancenormalization7: InstanceNorm<B>,
    constant61: burn::module::Param<Tensor<B, 3>>,
    constant62: burn::module::Param<Tensor<B, 3>>,
    conv2d6: Conv2d<B>,
    instancenormalization8: InstanceNorm<B>,
    constant63: burn::module::Param<Tensor<B, 3>>,
    constant64: burn::module::Param<Tensor<B, 3>>,
    phantom: core::marker::PhantomData<B>,
    #[module(skip)]
    device: B::Device,
}
impl<B: Backend> Submodule1<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let conv2d1 = Conv2dConfig::new([4, 16], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization1 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant47: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant48: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let conv2d2 = Conv2dConfig::new([16, 16], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization2 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant49: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant50: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let conv2d3 = Conv2dConfig::new([16, 16], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization3 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant51: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant52: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let conv2d4 = Conv2dConfig::new([16, 16], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization4 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant53: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant54: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let linear1 = LinearConfig::new(384, 24).with_bias(false).init(device);
        let instancenormalization5 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant56: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant57: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let linear2 = LinearConfig::new(24, 384).with_bias(false).init(device);
        let instancenormalization6 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant59: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant60: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let conv2d5 = Conv2dConfig::new([16, 16], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization7 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant61: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant62: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let conv2d6 = Conv2dConfig::new([16, 16], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization8 = InstanceNormConfig::new(2)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant63: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant64: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([16, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        Self {
            conv2d1,
            instancenormalization1,
            constant47,
            constant48,
            conv2d2,
            instancenormalization2,
            constant49,
            constant50,
            conv2d3,
            instancenormalization3,
            constant51,
            constant52,
            conv2d4,
            instancenormalization4,
            constant53,
            constant54,
            linear1,
            instancenormalization5,
            constant56,
            constant57,
            linear2,
            instancenormalization6,
            constant59,
            constant60,
            conv2d5,
            instancenormalization7,
            constant61,
            constant62,
            conv2d6,
            instancenormalization8,
            constant63,
            constant64,
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }
    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, input: Tensor<B, 4>) -> Tensor<B, 4> {
        let conv2d1_out1 = self.conv2d1.forward(input);
        let reshape1_out1 = conv2d1_out1.clone().reshape([0, 2, -1]);
        let instancenormalization1_out1 = self
            .instancenormalization1
            .forward(reshape1_out1);
        let shape1_out1: [i64; 4] = {
            let axes = &conv2d1_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape2_out1 = instancenormalization1_out1.reshape(shape1_out1);
        let constant47_out1 = self.constant47.val();
        let mul1_out1 = reshape2_out1.mul((constant47_out1).unsqueeze_dims(&[0isize]));
        let constant48_out1 = self.constant48.val();
        let add1_out1 = mul1_out1.add((constant48_out1).unsqueeze_dims(&[0isize]));
        let relu1_out1 = burn::tensor::activation::relu(add1_out1);
        let transpose1_out1 = relu1_out1.permute([0, 1, 3, 2]);
        let conv2d2_out1 = self.conv2d2.forward(transpose1_out1.clone());
        let slice1_out1 = conv2d2_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape3_out1 = slice1_out1.clone().reshape([0, 2, -1]);
        let instancenormalization2_out1 = self
            .instancenormalization2
            .forward(reshape3_out1);
        let shape2_out1: [i64; 4] = {
            let axes = &slice1_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape4_out1 = instancenormalization2_out1.reshape(shape2_out1);
        let constant49_out1 = self.constant49.val();
        let mul2_out1 = reshape4_out1.mul((constant49_out1).unsqueeze_dims(&[0isize]));
        let constant50_out1 = self.constant50.val();
        let add2_out1 = mul2_out1.add((constant50_out1).unsqueeze_dims(&[0isize]));
        let relu2_out1 = burn::tensor::activation::relu(add2_out1);
        let conv2d3_out1 = self.conv2d3.forward(transpose1_out1);
        let slice2_out1 = conv2d3_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape5_out1 = slice2_out1.clone().reshape([0, 2, -1]);
        let instancenormalization3_out1 = self
            .instancenormalization3
            .forward(reshape5_out1);
        let shape3_out1: [i64; 4] = {
            let axes = &slice2_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape6_out1 = instancenormalization3_out1.reshape(shape3_out1);
        let constant51_out1 = self.constant51.val();
        let mul3_out1 = reshape6_out1.mul((constant51_out1).unsqueeze_dims(&[0isize]));
        let constant52_out1 = self.constant52.val();
        let add3_out1 = mul3_out1.add((constant52_out1).unsqueeze_dims(&[0isize]));
        let relu3_out1 = burn::tensor::activation::relu(add3_out1);
        let conv2d4_out1 = self.conv2d4.forward(relu3_out1);
        let slice3_out1 = conv2d4_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape7_out1 = slice3_out1.clone().reshape([0, 2, -1]);
        let instancenormalization4_out1 = self
            .instancenormalization4
            .forward(reshape7_out1);
        let shape4_out1: [i64; 4] = {
            let axes = &slice3_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape8_out1 = instancenormalization4_out1.reshape(shape4_out1);
        let constant53_out1 = self.constant53.val();
        let mul4_out1 = reshape8_out1.mul((constant53_out1).unsqueeze_dims(&[0isize]));
        let constant54_out1 = self.constant54.val();
        let add4_out1 = mul4_out1.add((constant54_out1).unsqueeze_dims(&[0isize]));
        let relu4_out1 = burn::tensor::activation::relu(add4_out1);
        let linear1_out1 = self.linear1.forward(relu4_out1.clone());
        let reshape9_out1 = linear1_out1.clone().reshape([0, 2, -1]);
        let instancenormalization5_out1 = self
            .instancenormalization5
            .forward(reshape9_out1);
        let shape5_out1: [i64; 4] = {
            let axes = &linear1_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape10_out1 = instancenormalization5_out1.reshape(shape5_out1);
        let constant56_out1 = self.constant56.val();
        let mul5_out1 = reshape10_out1.mul((constant56_out1).unsqueeze_dims(&[0isize]));
        let constant57_out1 = self.constant57.val();
        let add5_out1 = mul5_out1.add((constant57_out1).unsqueeze_dims(&[0isize]));
        let relu5_out1 = burn::tensor::activation::relu(add5_out1);
        let linear2_out1 = self.linear2.forward(relu5_out1);
        let reshape11_out1 = linear2_out1.clone().reshape([0, 2, -1]);
        let instancenormalization6_out1 = self
            .instancenormalization6
            .forward(reshape11_out1);
        let shape6_out1: [i64; 4] = {
            let axes = &linear2_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape12_out1 = instancenormalization6_out1.reshape(shape6_out1);
        let constant59_out1 = self.constant59.val();
        let mul6_out1 = reshape12_out1.mul((constant59_out1).unsqueeze_dims(&[0isize]));
        let constant60_out1 = self.constant60.val();
        let add6_out1 = mul6_out1.add((constant60_out1).unsqueeze_dims(&[0isize]));
        let relu6_out1 = burn::tensor::activation::relu(add6_out1);
        let add7_out1 = relu4_out1.add(relu6_out1);
        let conv2d5_out1 = self.conv2d5.forward(add7_out1);
        let slice4_out1 = conv2d5_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape13_out1 = slice4_out1.clone().reshape([0, 2, -1]);
        let instancenormalization7_out1 = self
            .instancenormalization7
            .forward(reshape13_out1);
        let shape7_out1: [i64; 4] = {
            let axes = &slice4_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape14_out1 = instancenormalization7_out1.reshape(shape7_out1);
        let constant61_out1 = self.constant61.val();
        let mul7_out1 = reshape14_out1.mul((constant61_out1).unsqueeze_dims(&[0isize]));
        let constant62_out1 = self.constant62.val();
        let add8_out1 = mul7_out1.add((constant62_out1).unsqueeze_dims(&[0isize]));
        let relu7_out1 = burn::tensor::activation::relu(add8_out1);
        let conv2d6_out1 = self.conv2d6.forward(relu7_out1);
        let slice5_out1 = conv2d6_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape15_out1 = slice5_out1.clone().reshape([0, 2, -1]);
        let instancenormalization8_out1 = self
            .instancenormalization8
            .forward(reshape15_out1);
        let shape8_out1: [i64; 4] = {
            let axes = &slice5_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape16_out1 = instancenormalization8_out1.reshape(shape8_out1);
        let constant63_out1 = self.constant63.val();
        let mul8_out1 = reshape16_out1.mul((constant63_out1).unsqueeze_dims(&[0isize]));
        let constant64_out1 = self.constant64.val();
        let add9_out1 = mul8_out1.add((constant64_out1).unsqueeze_dims(&[0isize]));
        let relu8_out1 = burn::tensor::activation::relu(add9_out1);
        let add10_out1 = relu8_out1.add(relu2_out1);
        add10_out1
    }
}
#[derive(Module, Debug)]
pub struct Submodule2<B: Backend> {
    conv2d7: Conv2d<B>,
    instancenormalization9: InstanceNorm<B>,
    constant65: burn::module::Param<Tensor<B, 3>>,
    constant66: burn::module::Param<Tensor<B, 3>>,
    conv2d8: Conv2d<B>,
    instancenormalization10: InstanceNorm<B>,
    constant67: burn::module::Param<Tensor<B, 3>>,
    constant68: burn::module::Param<Tensor<B, 3>>,
    conv2d9: Conv2d<B>,
    instancenormalization11: InstanceNorm<B>,
    constant69: burn::module::Param<Tensor<B, 3>>,
    constant70: burn::module::Param<Tensor<B, 3>>,
    conv2d10: Conv2d<B>,
    instancenormalization12: InstanceNorm<B>,
    constant71: burn::module::Param<Tensor<B, 3>>,
    constant72: burn::module::Param<Tensor<B, 3>>,
    linear3: Linear<B>,
    instancenormalization13: InstanceNorm<B>,
    constant74: burn::module::Param<Tensor<B, 3>>,
    constant75: burn::module::Param<Tensor<B, 3>>,
    linear4: Linear<B>,
    instancenormalization14: InstanceNorm<B>,
    constant77: burn::module::Param<Tensor<B, 3>>,
    constant78: burn::module::Param<Tensor<B, 3>>,
    conv2d11: Conv2d<B>,
    instancenormalization15: InstanceNorm<B>,
    constant79: burn::module::Param<Tensor<B, 3>>,
    constant80: burn::module::Param<Tensor<B, 3>>,
    conv2d12: Conv2d<B>,
    instancenormalization16: InstanceNorm<B>,
    constant81: burn::module::Param<Tensor<B, 3>>,
    constant82: burn::module::Param<Tensor<B, 3>>,
    phantom: core::marker::PhantomData<B>,
    #[module(skip)]
    device: B::Device,
}
impl<B: Backend> Submodule2<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let conv2d7 = Conv2dConfig::new([16, 32], [2, 2])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(1, 1, 1, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization9 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant65: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant66: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d8 = Conv2dConfig::new([32, 32], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization10 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant67: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant68: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d9 = Conv2dConfig::new([32, 32], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization11 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant69: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant70: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d10 = Conv2dConfig::new([32, 32], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization12 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant71: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant72: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let linear3 = LinearConfig::new(384, 24).with_bias(false).init(device);
        let instancenormalization13 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant74: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant75: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let linear4 = LinearConfig::new(24, 384).with_bias(false).init(device);
        let instancenormalization14 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant77: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant78: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d11 = Conv2dConfig::new([32, 32], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization15 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant79: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant80: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d12 = Conv2dConfig::new([32, 32], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization16 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant81: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant82: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        Self {
            conv2d7,
            instancenormalization9,
            constant65,
            constant66,
            conv2d8,
            instancenormalization10,
            constant67,
            constant68,
            conv2d9,
            instancenormalization11,
            constant69,
            constant70,
            conv2d10,
            instancenormalization12,
            constant71,
            constant72,
            linear3,
            instancenormalization13,
            constant74,
            constant75,
            linear4,
            instancenormalization14,
            constant77,
            constant78,
            conv2d11,
            instancenormalization15,
            constant79,
            constant80,
            conv2d12,
            instancenormalization16,
            constant81,
            constant82,
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }
    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, add10_out1: Tensor<B, 4>) -> Tensor<B, 4> {
        let conv2d7_out1 = self.conv2d7.forward(add10_out1);
        let slice6_out1 = conv2d7_out1.slice(s![.., .., 0.. - 1, ..]);
        let slice7_out1 = slice6_out1.slice(s![.., .., .., 0.. - 1]);
        let reshape17_out1 = slice7_out1.clone().reshape([0, 4, -1]);
        let instancenormalization9_out1 = self
            .instancenormalization9
            .forward(reshape17_out1);
        let shape9_out1: [i64; 4] = {
            let axes = &slice7_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape18_out1 = instancenormalization9_out1.reshape(shape9_out1);
        let constant65_out1 = self.constant65.val();
        let mul9_out1 = reshape18_out1.mul((constant65_out1).unsqueeze_dims(&[0isize]));
        let constant66_out1 = self.constant66.val();
        let add11_out1 = mul9_out1.add((constant66_out1).unsqueeze_dims(&[0isize]));
        let relu9_out1 = burn::tensor::activation::relu(add11_out1);
        let conv2d8_out1 = self.conv2d8.forward(relu9_out1.clone());
        let slice8_out1 = conv2d8_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape19_out1 = slice8_out1.clone().reshape([0, 4, -1]);
        let instancenormalization10_out1 = self
            .instancenormalization10
            .forward(reshape19_out1);
        let shape10_out1: [i64; 4] = {
            let axes = &slice8_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape20_out1 = instancenormalization10_out1.reshape(shape10_out1);
        let constant67_out1 = self.constant67.val();
        let mul10_out1 = reshape20_out1.mul((constant67_out1).unsqueeze_dims(&[0isize]));
        let constant68_out1 = self.constant68.val();
        let add12_out1 = mul10_out1.add((constant68_out1).unsqueeze_dims(&[0isize]));
        let relu10_out1 = burn::tensor::activation::relu(add12_out1);
        let conv2d9_out1 = self.conv2d9.forward(relu9_out1);
        let slice9_out1 = conv2d9_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape21_out1 = slice9_out1.clone().reshape([0, 4, -1]);
        let instancenormalization11_out1 = self
            .instancenormalization11
            .forward(reshape21_out1);
        let shape11_out1: [i64; 4] = {
            let axes = &slice9_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape22_out1 = instancenormalization11_out1.reshape(shape11_out1);
        let constant69_out1 = self.constant69.val();
        let mul11_out1 = reshape22_out1.mul((constant69_out1).unsqueeze_dims(&[0isize]));
        let constant70_out1 = self.constant70.val();
        let add13_out1 = mul11_out1.add((constant70_out1).unsqueeze_dims(&[0isize]));
        let relu11_out1 = burn::tensor::activation::relu(add13_out1);
        let conv2d10_out1 = self.conv2d10.forward(relu11_out1);
        let slice10_out1 = conv2d10_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape23_out1 = slice10_out1.clone().reshape([0, 4, -1]);
        let instancenormalization12_out1 = self
            .instancenormalization12
            .forward(reshape23_out1);
        let shape12_out1: [i64; 4] = {
            let axes = &slice10_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape24_out1 = instancenormalization12_out1.reshape(shape12_out1);
        let constant71_out1 = self.constant71.val();
        let mul12_out1 = reshape24_out1.mul((constant71_out1).unsqueeze_dims(&[0isize]));
        let constant72_out1 = self.constant72.val();
        let add14_out1 = mul12_out1.add((constant72_out1).unsqueeze_dims(&[0isize]));
        let relu12_out1 = burn::tensor::activation::relu(add14_out1);
        let linear3_out1 = self.linear3.forward(relu12_out1.clone());
        let reshape25_out1 = linear3_out1.clone().reshape([0, 4, -1]);
        let instancenormalization13_out1 = self
            .instancenormalization13
            .forward(reshape25_out1);
        let shape13_out1: [i64; 4] = {
            let axes = &linear3_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape26_out1 = instancenormalization13_out1.reshape(shape13_out1);
        let constant74_out1 = self.constant74.val();
        let mul13_out1 = reshape26_out1.mul((constant74_out1).unsqueeze_dims(&[0isize]));
        let constant75_out1 = self.constant75.val();
        let add15_out1 = mul13_out1.add((constant75_out1).unsqueeze_dims(&[0isize]));
        let relu13_out1 = burn::tensor::activation::relu(add15_out1);
        let linear4_out1 = self.linear4.forward(relu13_out1);
        let reshape27_out1 = linear4_out1.clone().reshape([0, 4, -1]);
        let instancenormalization14_out1 = self
            .instancenormalization14
            .forward(reshape27_out1);
        let shape14_out1: [i64; 4] = {
            let axes = &linear4_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape28_out1 = instancenormalization14_out1.reshape(shape14_out1);
        let constant77_out1 = self.constant77.val();
        let mul14_out1 = reshape28_out1.mul((constant77_out1).unsqueeze_dims(&[0isize]));
        let constant78_out1 = self.constant78.val();
        let add16_out1 = mul14_out1.add((constant78_out1).unsqueeze_dims(&[0isize]));
        let relu14_out1 = burn::tensor::activation::relu(add16_out1);
        let add17_out1 = relu12_out1.add(relu14_out1);
        let conv2d11_out1 = self.conv2d11.forward(add17_out1);
        let slice11_out1 = conv2d11_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape29_out1 = slice11_out1.clone().reshape([0, 4, -1]);
        let instancenormalization15_out1 = self
            .instancenormalization15
            .forward(reshape29_out1);
        let shape15_out1: [i64; 4] = {
            let axes = &slice11_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape30_out1 = instancenormalization15_out1.reshape(shape15_out1);
        let constant79_out1 = self.constant79.val();
        let mul15_out1 = reshape30_out1.mul((constant79_out1).unsqueeze_dims(&[0isize]));
        let constant80_out1 = self.constant80.val();
        let add18_out1 = mul15_out1.add((constant80_out1).unsqueeze_dims(&[0isize]));
        let relu15_out1 = burn::tensor::activation::relu(add18_out1);
        let conv2d12_out1 = self.conv2d12.forward(relu15_out1);
        let slice12_out1 = conv2d12_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape31_out1 = slice12_out1.clone().reshape([0, 4, -1]);
        let instancenormalization16_out1 = self
            .instancenormalization16
            .forward(reshape31_out1);
        let shape16_out1: [i64; 4] = {
            let axes = &slice12_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape32_out1 = instancenormalization16_out1.reshape(shape16_out1);
        let constant81_out1 = self.constant81.val();
        let mul16_out1 = reshape32_out1.mul((constant81_out1).unsqueeze_dims(&[0isize]));
        let constant82_out1 = self.constant82.val();
        let add19_out1 = mul16_out1.add((constant82_out1).unsqueeze_dims(&[0isize]));
        let relu16_out1 = burn::tensor::activation::relu(add19_out1);
        let add20_out1 = relu16_out1.add(relu10_out1);
        add20_out1
    }
}
#[derive(Module, Debug)]
pub struct Submodule3<B: Backend> {
    instancenormalization17: InstanceNorm<B>,
    constant83: burn::module::Param<Tensor<B, 2>>,
    constant84: burn::module::Param<Tensor<B, 2>>,
    constant240: burn::module::Param<Tensor<B, 3>>,
    lstm1: Lstm<B>,
    linear5: Linear<B>,
    instancenormalization18: InstanceNorm<B>,
    constant89: burn::module::Param<Tensor<B, 2>>,
    constant90: burn::module::Param<Tensor<B, 2>>,
    constant252: burn::module::Param<Tensor<B, 3>>,
    lstm2: Lstm<B>,
    linear6: Linear<B>,
    instancenormalization19: InstanceNorm<B>,
    constant95: burn::module::Param<Tensor<B, 2>>,
    constant96: burn::module::Param<Tensor<B, 2>>,
    constant264: burn::module::Param<Tensor<B, 3>>,
    lstm3: Lstm<B>,
    linear7: Linear<B>,
    instancenormalization20: InstanceNorm<B>,
    constant101: burn::module::Param<Tensor<B, 2>>,
    constant102: burn::module::Param<Tensor<B, 2>>,
    constant276: burn::module::Param<Tensor<B, 3>>,
    lstm4: Lstm<B>,
    linear8: Linear<B>,
    instancenormalization21: InstanceNorm<B>,
    constant107: burn::module::Param<Tensor<B, 2>>,
    constant108: burn::module::Param<Tensor<B, 2>>,
    constant288: burn::module::Param<Tensor<B, 3>>,
    lstm5: Lstm<B>,
    linear9: Linear<B>,
    instancenormalization22: InstanceNorm<B>,
    constant113: burn::module::Param<Tensor<B, 2>>,
    constant114: burn::module::Param<Tensor<B, 2>>,
    constant300: burn::module::Param<Tensor<B, 3>>,
    lstm6: Lstm<B>,
    linear10: Linear<B>,
    instancenormalization23: InstanceNorm<B>,
    constant119: burn::module::Param<Tensor<B, 3>>,
    constant120: burn::module::Param<Tensor<B, 3>>,
    conv2d13: Conv2d<B>,
    conv2d14: Conv2d<B>,
    instancenormalization24: InstanceNorm<B>,
    constant121: burn::module::Param<Tensor<B, 3>>,
    constant122: burn::module::Param<Tensor<B, 3>>,
    phantom: core::marker::PhantomData<B>,
    #[module(skip)]
    device: B::Device,
}
impl<B: Backend> Submodule3<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let instancenormalization17 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant83: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant84: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant240: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 768, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 768, 32].into(),
        );
        let lstm1 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear5 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization18 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant89: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant90: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant252: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 128, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 128, 32].into(),
        );
        let lstm2 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear6 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization19 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant95: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant96: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant264: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 768, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 768, 32].into(),
        );
        let lstm3 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear7 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization20 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant101: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant102: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant276: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 128, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 128, 32].into(),
        );
        let lstm4 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear8 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization21 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant107: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant108: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant288: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 768, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 768, 32].into(),
        );
        let lstm5 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear9 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization22 = InstanceNormConfig::new(1)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant113: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant114: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                2,
            >::zeros([16, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant300: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([1, 128, 32], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [1, 128, 32].into(),
        );
        let lstm6 = LstmConfig::new(16, 32, true)
            .with_batch_first(false)
            .with_input_forget(false)
            .init(device);
        let linear10 = LinearConfig::new(32, 16).with_bias(true).init(device);
        let instancenormalization23 = InstanceNormConfig::new(4)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant119: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant120: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([32, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let conv2d13 = Conv2dConfig::new([32, 128], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let conv2d14 = Conv2dConfig::new([128, 64], [2, 2])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(1, 1, 1, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization24 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant121: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant122: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        Self {
            instancenormalization17,
            constant83,
            constant84,
            constant240,
            lstm1,
            linear5,
            instancenormalization18,
            constant89,
            constant90,
            constant252,
            lstm2,
            linear6,
            instancenormalization19,
            constant95,
            constant96,
            constant264,
            lstm3,
            linear7,
            instancenormalization20,
            constant101,
            constant102,
            constant276,
            lstm4,
            linear8,
            instancenormalization21,
            constant107,
            constant108,
            constant288,
            lstm5,
            linear9,
            instancenormalization22,
            constant113,
            constant114,
            constant300,
            lstm6,
            linear10,
            instancenormalization23,
            constant119,
            constant120,
            conv2d13,
            conv2d14,
            instancenormalization24,
            constant121,
            constant122,
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }
    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(
        &self,
        add20_out1: Tensor<B, 4>,
        add10_out1: Tensor<B, 4>,
    ) -> Tensor<B, 5> {
        let reshape33_out1 = add20_out1.reshape([2, 16, 64, 384]);
        let transpose2_out1 = reshape33_out1.permute([0, 3, 2, 1]);
        let reshape34_out1 = transpose2_out1.clone().reshape([768, 64, 16]);
        let transpose3_out1 = reshape34_out1.permute([0, 2, 1]);
        let reshape35_out1 = transpose3_out1.clone().reshape([0, 1, -1]);
        let instancenormalization17_out1 = self
            .instancenormalization17
            .forward(reshape35_out1);
        let shape17_out1: [i64; 3] = {
            let axes = &transpose3_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape36_out1 = instancenormalization17_out1.reshape(shape17_out1);
        let constant83_out1 = self.constant83.val();
        let mul17_out1 = reshape36_out1.mul((constant83_out1).unsqueeze_dims(&[0isize]));
        let constant84_out1 = self.constant84.val();
        let add21_out1 = mul17_out1.add((constant84_out1).unsqueeze_dims(&[0isize]));
        let transpose4_out1 = add21_out1.permute([2, 0, 1]);
        let constant240_out1 = self.constant240.val();
        let lstm1_out1 = profile_step("lstm1", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm1,
                transpose4_out1,
                Some(LstmState::new(
                    constant240_out1.clone().squeeze_dim(0),
                    constant240_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze1_out1 = lstm1_out1.squeeze_dims::<3>(&[1]);
        let transpose5_out1 = squeeze1_out1.permute([1, 0, 2]);
        let linear5_out1 = self.linear5.forward(transpose5_out1);
        let reshape37_out1 = linear5_out1.reshape([2, 384, 64, 16]);
        let add22_out1 = reshape37_out1.add(transpose2_out1);
        let transpose6_out1 = add22_out1.permute([0, 2, 1, 3]);
        let reshape38_out1 = transpose6_out1.clone().reshape([128, 384, 16]);
        let transpose7_out1 = reshape38_out1.permute([0, 2, 1]);
        let reshape39_out1 = transpose7_out1.clone().reshape([0, 1, -1]);
        let instancenormalization18_out1 = self
            .instancenormalization18
            .forward(reshape39_out1);
        let shape20_out1: [i64; 3] = {
            let axes = &transpose7_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape40_out1 = instancenormalization18_out1.reshape(shape20_out1);
        let constant89_out1 = self.constant89.val();
        let mul18_out1 = reshape40_out1.mul((constant89_out1).unsqueeze_dims(&[0isize]));
        let constant90_out1 = self.constant90.val();
        let add23_out1 = mul18_out1.add((constant90_out1).unsqueeze_dims(&[0isize]));
        let transpose8_out1 = add23_out1.permute([2, 0, 1]);
        let constant252_out1 = self.constant252.val();
        let lstm2_out1 = profile_step("lstm2", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm2,
                transpose8_out1,
                Some(LstmState::new(
                    constant252_out1.clone().squeeze_dim(0),
                    constant252_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze2_out1 = lstm2_out1.squeeze_dims::<3>(&[1]);
        let transpose9_out1 = squeeze2_out1.permute([1, 0, 2]);
        let linear6_out1 = self.linear6.forward(transpose9_out1);
        let reshape41_out1 = linear6_out1.reshape([2, 64, 384, 16]);
        let add24_out1 = reshape41_out1.add(transpose6_out1);
        let transpose10_out1 = add24_out1.permute([0, 2, 1, 3]);
        let reshape42_out1 = transpose10_out1.clone().reshape([768, 64, 16]);
        let transpose11_out1 = reshape42_out1.permute([0, 2, 1]);
        let reshape43_out1 = transpose11_out1.clone().reshape([0, 1, -1]);
        let instancenormalization19_out1 = self
            .instancenormalization19
            .forward(reshape43_out1);
        let shape23_out1: [i64; 3] = {
            let axes = &transpose11_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape44_out1 = instancenormalization19_out1.reshape(shape23_out1);
        let constant95_out1 = self.constant95.val();
        let mul19_out1 = reshape44_out1.mul((constant95_out1).unsqueeze_dims(&[0isize]));
        let constant96_out1 = self.constant96.val();
        let add25_out1 = mul19_out1.add((constant96_out1).unsqueeze_dims(&[0isize]));
        let transpose12_out1 = add25_out1.permute([2, 0, 1]);
        let constant264_out1 = self.constant264.val();
        let lstm3_out1 = profile_step("lstm3", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm3,
                transpose12_out1,
                Some(LstmState::new(
                    constant264_out1.clone().squeeze_dim(0),
                    constant264_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze3_out1 = lstm3_out1.squeeze_dims::<3>(&[1]);
        let transpose13_out1 = squeeze3_out1.permute([1, 0, 2]);
        let linear7_out1 = self.linear7.forward(transpose13_out1);
        let reshape45_out1 = linear7_out1.reshape([2, 384, 64, 16]);
        let add26_out1 = reshape45_out1.add(transpose10_out1);
        let transpose14_out1 = add26_out1.permute([0, 2, 1, 3]);
        let reshape46_out1 = transpose14_out1.clone().reshape([128, 384, 16]);
        let transpose15_out1 = reshape46_out1.permute([0, 2, 1]);
        let reshape47_out1 = transpose15_out1.clone().reshape([0, 1, -1]);
        let instancenormalization20_out1 = self
            .instancenormalization20
            .forward(reshape47_out1);
        let shape26_out1: [i64; 3] = {
            let axes = &transpose15_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape48_out1 = instancenormalization20_out1.reshape(shape26_out1);
        let constant101_out1 = self.constant101.val();
        let mul20_out1 = reshape48_out1
            .mul((constant101_out1).unsqueeze_dims(&[0isize]));
        let constant102_out1 = self.constant102.val();
        let add27_out1 = mul20_out1.add((constant102_out1).unsqueeze_dims(&[0isize]));
        let transpose16_out1 = add27_out1.permute([2, 0, 1]);
        let constant276_out1 = self.constant276.val();
        let lstm4_out1 = profile_step("lstm4", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm4,
                transpose16_out1,
                Some(LstmState::new(
                    constant276_out1.clone().squeeze_dim(0),
                    constant276_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze4_out1 = lstm4_out1.squeeze_dims::<3>(&[1]);
        let transpose17_out1 = squeeze4_out1.permute([1, 0, 2]);
        let linear8_out1 = self.linear8.forward(transpose17_out1);
        let reshape49_out1 = linear8_out1.reshape([2, 64, 384, 16]);
        let add28_out1 = reshape49_out1.add(transpose14_out1);
        let transpose18_out1 = add28_out1.permute([0, 2, 1, 3]);
        let reshape50_out1 = transpose18_out1.clone().reshape([768, 64, 16]);
        let transpose19_out1 = reshape50_out1.permute([0, 2, 1]);
        let reshape51_out1 = transpose19_out1.clone().reshape([0, 1, -1]);
        let instancenormalization21_out1 = self
            .instancenormalization21
            .forward(reshape51_out1);
        let shape29_out1: [i64; 3] = {
            let axes = &transpose19_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape52_out1 = instancenormalization21_out1.reshape(shape29_out1);
        let constant107_out1 = self.constant107.val();
        let mul21_out1 = reshape52_out1
            .mul((constant107_out1).unsqueeze_dims(&[0isize]));
        let constant108_out1 = self.constant108.val();
        let add29_out1 = mul21_out1.add((constant108_out1).unsqueeze_dims(&[0isize]));
        let transpose20_out1 = add29_out1.permute([2, 0, 1]);
        let constant288_out1 = self.constant288.val();
        let lstm5_out1 = profile_step("lstm5", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm5,
                transpose20_out1,
                Some(LstmState::new(
                    constant288_out1.clone().squeeze_dim(0),
                    constant288_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze5_out1 = lstm5_out1.squeeze_dims::<3>(&[1]);
        let transpose21_out1 = squeeze5_out1.permute([1, 0, 2]);
        let linear9_out1 = self.linear9.forward(transpose21_out1);
        let reshape53_out1 = linear9_out1.reshape([2, 384, 64, 16]);
        let add30_out1 = reshape53_out1.add(transpose18_out1);
        let transpose22_out1 = add30_out1.permute([0, 2, 1, 3]);
        let reshape54_out1 = transpose22_out1.clone().reshape([128, 384, 16]);
        let transpose23_out1 = reshape54_out1.permute([0, 2, 1]);
        let reshape55_out1 = transpose23_out1.clone().reshape([0, 1, -1]);
        let instancenormalization22_out1 = self
            .instancenormalization22
            .forward(reshape55_out1);
        let shape32_out1: [i64; 3] = {
            let axes = &transpose23_out1.dims()[0..3];
            let mut output = [0i64; 3];
            for i in 0..3 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape56_out1 = instancenormalization22_out1.reshape(shape32_out1);
        let constant113_out1 = self.constant113.val();
        let mul22_out1 = reshape56_out1
            .mul((constant113_out1).unsqueeze_dims(&[0isize]));
        let constant114_out1 = self.constant114.val();
        let add31_out1 = mul22_out1.add((constant114_out1).unsqueeze_dims(&[0isize]));
        let transpose24_out1 = add31_out1.permute([2, 0, 1]);
        let constant300_out1 = self.constant300.val();
        let lstm6_out1 = profile_step("lstm6", || {
            let (output_seq, _) = lstm_preproj(
                &self.lstm6,
                transpose24_out1,
                Some(LstmState::new(
                    constant300_out1.clone().squeeze_dim(0),
                    constant300_out1.squeeze_dim(0),
                )),
            );
            output_seq.unsqueeze_dims::<4>(&[1])
        });
        let squeeze6_out1 = lstm6_out1.squeeze_dims::<3>(&[1]);
        let transpose25_out1 = squeeze6_out1.permute([1, 0, 2]);
        let linear10_out1 = self.linear10.forward(transpose25_out1);
        let reshape57_out1 = linear10_out1.reshape([2, 64, 384, 16]);
        let add32_out1 = reshape57_out1.add(transpose22_out1);
        let transpose26_out1 = add32_out1.permute([0, 3, 1, 2]);
        let reshape58_out1 = transpose26_out1.reshape([1, 32, 64, 384]);
        let reshape59_out1 = reshape58_out1.clone().reshape([0, 4, -1]);
        let instancenormalization23_out1 = self
            .instancenormalization23
            .forward(reshape59_out1);
        let shape35_out1: [i64; 4] = {
            let axes = &reshape58_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape60_out1 = instancenormalization23_out1.reshape(shape35_out1);
        let constant119_out1 = self.constant119.val();
        let mul23_out1 = reshape60_out1
            .mul((constant119_out1).unsqueeze_dims(&[0isize]));
        let constant120_out1 = self.constant120.val();
        let add33_out1 = mul23_out1.add((constant120_out1).unsqueeze_dims(&[0isize]));
        let conv2d13_out1 = self.conv2d13.forward(add33_out1);
        let reshape61_out1 = conv2d13_out1.reshape([1, 4, 32, 64, 384]);
        let transpose27_out1 = reshape61_out1.permute([0, 4, 2, 3, 1]);
        let softmax1_out1 = burn::tensor::activation::softmax(transpose27_out1, 4);
        let transpose28_out1 = softmax1_out1.permute([0, 4, 2, 3, 1]);
        let reshape62_out1 = transpose28_out1.reshape([1, 128, 64, 384]);
        let conv2d14_out1 = self.conv2d14.forward(reshape62_out1);
        let slice13_out1 = conv2d14_out1.slice(s![.., .., 0.. - 1, ..]);
        let slice14_out1 = slice13_out1.slice(s![.., .., .., 0.. - 1]);
        let reshape63_out1 = slice14_out1.clone().reshape([0, 8, -1]);
        let instancenormalization24_out1 = self
            .instancenormalization24
            .forward(reshape63_out1);
        let shape36_out1: [i64; 4] = {
            let axes = &slice14_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape64_out1 = instancenormalization24_out1.reshape(shape36_out1);
        let constant121_out1 = self.constant121.val();
        let mul24_out1 = reshape64_out1
            .mul((constant121_out1).unsqueeze_dims(&[0isize]));
        let constant122_out1 = self.constant122.val();
        let add34_out1 = mul24_out1.add((constant122_out1).unsqueeze_dims(&[0isize]));
        let relu17_out1 = burn::tensor::activation::relu(add34_out1);
        let reshape65_out1 = relu17_out1.reshape([1, 4, 16, 64, 384]);
        let unsqueeze13_out1: Tensor<B, 5> = add10_out1.unsqueeze_dims::<5>(&[1]);
        let mul25_out1 = reshape65_out1.mul(unsqueeze13_out1);
        mul25_out1
    }
}
#[derive(Module, Debug)]
pub struct Submodule4<B: Backend> {
    conv2d15: Conv2d<B>,
    instancenormalization25: InstanceNorm<B>,
    constant123: burn::module::Param<Tensor<B, 3>>,
    constant124: burn::module::Param<Tensor<B, 3>>,
    conv2d16: Conv2d<B>,
    instancenormalization26: InstanceNorm<B>,
    constant125: burn::module::Param<Tensor<B, 3>>,
    constant126: burn::module::Param<Tensor<B, 3>>,
    conv2d17: Conv2d<B>,
    instancenormalization27: InstanceNorm<B>,
    constant127: burn::module::Param<Tensor<B, 3>>,
    constant128: burn::module::Param<Tensor<B, 3>>,
    linear11: Linear<B>,
    instancenormalization28: InstanceNorm<B>,
    constant130: burn::module::Param<Tensor<B, 3>>,
    constant131: burn::module::Param<Tensor<B, 3>>,
    linear12: Linear<B>,
    instancenormalization29: InstanceNorm<B>,
    constant133: burn::module::Param<Tensor<B, 3>>,
    constant134: burn::module::Param<Tensor<B, 3>>,
    conv2d18: Conv2d<B>,
    instancenormalization30: InstanceNorm<B>,
    constant135: burn::module::Param<Tensor<B, 3>>,
    constant136: burn::module::Param<Tensor<B, 3>>,
    conv2d19: Conv2d<B>,
    instancenormalization31: InstanceNorm<B>,
    constant137: burn::module::Param<Tensor<B, 3>>,
    constant138: burn::module::Param<Tensor<B, 3>>,
    conv2d20: Conv2d<B>,
    phantom: core::marker::PhantomData<B>,
    #[module(skip)]
    device: B::Device,
}
impl<B: Backend> Submodule4<B> {
    #[allow(unused_variables)]
    pub fn new(device: &B::Device) -> Self {
        let conv2d15 = Conv2dConfig::new([64, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization25 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant123: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant124: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let conv2d16 = Conv2dConfig::new([64, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization26 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant125: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant126: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let conv2d17 = Conv2dConfig::new([64, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization27 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant127: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant128: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let linear11 = LinearConfig::new(384, 24).with_bias(false).init(device);
        let instancenormalization28 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant130: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant131: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let linear12 = LinearConfig::new(24, 384).with_bias(false).init(device);
        let instancenormalization29 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant133: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant134: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let conv2d18 = Conv2dConfig::new([64, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization30 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant135: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant136: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let conv2d19 = Conv2dConfig::new([64, 64], [3, 3])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Explicit(2, 1, 2, 1))
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        let instancenormalization31 = InstanceNormConfig::new(8)
            .with_epsilon(0.000009999999747378752f64)
            .init(device);
        let constant137: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant138: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| Tensor::<
                B,
                3,
            >::zeros([64, 1, 1], (device, burn::tensor::DType::F16)),
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let conv2d20 = Conv2dConfig::new([64, 16], [1, 1])
            .with_stride([1, 1])
            .with_padding(PaddingConfig2d::Valid)
            .with_dilation([1, 1])
            .with_groups(1)
            .with_bias(true)
            .init(device);
        Self {
            conv2d15,
            instancenormalization25,
            constant123,
            constant124,
            conv2d16,
            instancenormalization26,
            constant125,
            constant126,
            conv2d17,
            instancenormalization27,
            constant127,
            constant128,
            linear11,
            instancenormalization28,
            constant130,
            constant131,
            linear12,
            instancenormalization29,
            constant133,
            constant134,
            conv2d18,
            instancenormalization30,
            constant135,
            constant136,
            conv2d19,
            instancenormalization31,
            constant137,
            constant138,
            conv2d20,
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }
    #[allow(clippy::let_and_return, clippy::approx_constant)]
    pub fn forward(&self, mul25_out1: Tensor<B, 5>) -> Tensor<B, 5> {
        let reshape66_out1 = mul25_out1.reshape([1, 64, 64, 384]);
        let conv2d15_out1 = self.conv2d15.forward(reshape66_out1.clone());
        let slice15_out1 = conv2d15_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape67_out1 = slice15_out1.clone().reshape([0, 8, -1]);
        let instancenormalization25_out1 = self
            .instancenormalization25
            .forward(reshape67_out1);
        let shape37_out1: [i64; 4] = {
            let axes = &slice15_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape68_out1 = instancenormalization25_out1.reshape(shape37_out1);
        let constant123_out1 = self.constant123.val();
        let mul26_out1 = reshape68_out1
            .mul((constant123_out1).unsqueeze_dims(&[0isize]));
        let constant124_out1 = self.constant124.val();
        let add35_out1 = mul26_out1.add((constant124_out1).unsqueeze_dims(&[0isize]));
        let relu18_out1 = burn::tensor::activation::relu(add35_out1);
        let conv2d16_out1 = self.conv2d16.forward(reshape66_out1);
        let slice16_out1 = conv2d16_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape69_out1 = slice16_out1.clone().reshape([0, 8, -1]);
        let instancenormalization26_out1 = self
            .instancenormalization26
            .forward(reshape69_out1);
        let shape38_out1: [i64; 4] = {
            let axes = &slice16_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape70_out1 = instancenormalization26_out1.reshape(shape38_out1);
        let constant125_out1 = self.constant125.val();
        let mul27_out1 = reshape70_out1
            .mul((constant125_out1).unsqueeze_dims(&[0isize]));
        let constant126_out1 = self.constant126.val();
        let add36_out1 = mul27_out1.add((constant126_out1).unsqueeze_dims(&[0isize]));
        let relu19_out1 = burn::tensor::activation::relu(add36_out1);
        let conv2d17_out1 = self.conv2d17.forward(relu19_out1);
        let slice17_out1 = conv2d17_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape71_out1 = slice17_out1.clone().reshape([0, 8, -1]);
        let instancenormalization27_out1 = self
            .instancenormalization27
            .forward(reshape71_out1);
        let shape39_out1: [i64; 4] = {
            let axes = &slice17_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape72_out1 = instancenormalization27_out1.reshape(shape39_out1);
        let constant127_out1 = self.constant127.val();
        let mul28_out1 = reshape72_out1
            .mul((constant127_out1).unsqueeze_dims(&[0isize]));
        let constant128_out1 = self.constant128.val();
        let add37_out1 = mul28_out1.add((constant128_out1).unsqueeze_dims(&[0isize]));
        let relu20_out1 = burn::tensor::activation::relu(add37_out1);
        let linear11_out1 = self.linear11.forward(relu20_out1.clone());
        let reshape73_out1 = linear11_out1.clone().reshape([0, 8, -1]);
        let instancenormalization28_out1 = self
            .instancenormalization28
            .forward(reshape73_out1);
        let shape40_out1: [i64; 4] = {
            let axes = &linear11_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape74_out1 = instancenormalization28_out1.reshape(shape40_out1);
        let constant130_out1 = self.constant130.val();
        let mul29_out1 = reshape74_out1
            .mul((constant130_out1).unsqueeze_dims(&[0isize]));
        let constant131_out1 = self.constant131.val();
        let add38_out1 = mul29_out1.add((constant131_out1).unsqueeze_dims(&[0isize]));
        let relu21_out1 = burn::tensor::activation::relu(add38_out1);
        let linear12_out1 = self.linear12.forward(relu21_out1);
        let reshape75_out1 = linear12_out1.clone().reshape([0, 8, -1]);
        let instancenormalization29_out1 = self
            .instancenormalization29
            .forward(reshape75_out1);
        let shape41_out1: [i64; 4] = {
            let axes = &linear12_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape76_out1 = instancenormalization29_out1.reshape(shape41_out1);
        let constant133_out1 = self.constant133.val();
        let mul30_out1 = reshape76_out1
            .mul((constant133_out1).unsqueeze_dims(&[0isize]));
        let constant134_out1 = self.constant134.val();
        let add39_out1 = mul30_out1.add((constant134_out1).unsqueeze_dims(&[0isize]));
        let relu22_out1 = burn::tensor::activation::relu(add39_out1);
        let add40_out1 = relu20_out1.add(relu22_out1);
        let conv2d18_out1 = self.conv2d18.forward(add40_out1);
        let slice18_out1 = conv2d18_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape77_out1 = slice18_out1.clone().reshape([0, 8, -1]);
        let instancenormalization30_out1 = self
            .instancenormalization30
            .forward(reshape77_out1);
        let shape42_out1: [i64; 4] = {
            let axes = &slice18_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape78_out1 = instancenormalization30_out1.reshape(shape42_out1);
        let constant135_out1 = self.constant135.val();
        let mul31_out1 = reshape78_out1
            .mul((constant135_out1).unsqueeze_dims(&[0isize]));
        let constant136_out1 = self.constant136.val();
        let add41_out1 = mul31_out1.add((constant136_out1).unsqueeze_dims(&[0isize]));
        let relu23_out1 = burn::tensor::activation::relu(add41_out1);
        let conv2d19_out1 = self.conv2d19.forward(relu23_out1);
        let slice19_out1 = conv2d19_out1.slice(s![.., .., 0.. - 2, ..]);
        let reshape79_out1 = slice19_out1.clone().reshape([0, 8, -1]);
        let instancenormalization31_out1 = self
            .instancenormalization31
            .forward(reshape79_out1);
        let shape43_out1: [i64; 4] = {
            let axes = &slice19_out1.dims()[0..4];
            let mut output = [0i64; 4];
            for i in 0..4 {
                output[i] = axes[i] as i64;
            }
            output
        };
        let reshape80_out1 = instancenormalization31_out1.reshape(shape43_out1);
        let constant137_out1 = self.constant137.val();
        let mul32_out1 = reshape80_out1
            .mul((constant137_out1).unsqueeze_dims(&[0isize]));
        let constant138_out1 = self.constant138.val();
        let add42_out1 = mul32_out1.add((constant138_out1).unsqueeze_dims(&[0isize]));
        let relu24_out1 = burn::tensor::activation::relu(add42_out1);
        let add43_out1 = relu24_out1.add(relu18_out1);
        let transpose29_out1 = add43_out1.permute([0, 1, 3, 2]);
        let conv2d20_out1 = self.conv2d20.forward(transpose29_out1);
        let reshape81_out1 = conv2d20_out1.reshape([1, 4, 4, 384, 64]);
        reshape81_out1
    }
}

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
            "/Users/ken/Codes/wifi-party/target/aarch64-apple-darwin/desktop-dev/build/wifi-party-vocal-model-34d8209dcf82dfb6/out/model/all_rt.bpk",
            &Default::default(),
        )
    }
}

impl<B: Backend> Model<B> {
    /// Load model weights from a burnpack file.
    pub fn from_file<P: AsRef<std::path::Path>>(file: P, device: &B::Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_file(file);
        model.load_from(&mut store).expect("Failed to load burnpack file");
        model
    }

    /// Load model weights from in-memory bytes.
    ///
    /// The bytes must be the contents of a `.bpk` file.
    pub fn from_bytes(bytes: Bytes, device: &B::Device) -> Self {
        let mut model = Self::new(device);
        let mut store = BurnpackStore::from_bytes(Some(bytes));
        model.load_from(&mut store).expect("Failed to load burnpack bytes");
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
    pub fn forward(&self, input: Tensor<B, 4>) -> Tensor<B, 5> {
        let add10_out1 = profile_step("submodule1", || self.submodule1.forward(input));
        let add20_out1 =
            profile_step("submodule2", || self.submodule2.forward(add10_out1.clone()));
        let mul25_out1 =
            profile_step("submodule3", || self.submodule3.forward(add20_out1, add10_out1));
        let reshape81_out1 = profile_step("submodule4", || self.submodule4.forward(mul25_out1));
        reshape81_out1
    }
}

/// Optimized LSTM forward pass.
///
/// Reduces GPU dispatches from ~11/step to ~3/step by:
/// 1. Pre-projecting all input steps in ONE batched matmul (amortised over seq_len)
/// 2. Concatenating all 4 gate hidden weights so hidden projection is ONE matmul/step
///
/// Input/output convention: `batch_first = false`, i.e. `[seq, batch, features]`.
fn lstm_preproj<B: Backend>(
    lstm: &burn::nn::Lstm<B>,
    input: Tensor<B, 3>,             // [seq, batch, input_size]
    state: Option<LstmState<B, 2>>,  // h/c each [batch, hidden]
) -> (Tensor<B, 3>, LstmState<B, 2>) {
    let [seq, batch, input_size] = input.dims();
    let hidden = lstm.d_hidden;
    let device = input.device();

    // ── 1. Pre-project all seq steps in one GEMM ──────────────────────────────
    // LinearLayout::Row stores weight as [d_input, d_output].
    // Concat along dim=1: [d_input, 4*d_output] = [input_size, 4*hidden]
    let w_x = Tensor::cat(
        vec![
            lstm.input_gate.input_transform.weight.val(),
            lstm.forget_gate.input_transform.weight.val(),
            lstm.cell_gate.input_transform.weight.val(),
            lstm.output_gate.input_transform.weight.val(),
        ],
        1,
    ); // [input_size, 4*hidden]
    // Flatten seq*batch; reshape handles non-contiguous inputs (implicit contiguous copy)
    let input_flat = input.reshape([seq * batch, input_size]); // [seq*batch, input_size]
    // [seq*batch, input_size] @ [input_size, 4*hidden]  →  [seq*batch, 4*hidden]
    let mut x_proj_flat = input_flat.matmul(w_x);
    // Add input biases (all 4 gates combined)
    if let Some(b) = lstm.input_gate.input_transform.bias.as_ref() {
        let b_x = Tensor::cat(
            vec![
                b.val(),
                lstm.forget_gate.input_transform.bias.as_ref().unwrap().val(),
                lstm.cell_gate.input_transform.bias.as_ref().unwrap().val(),
                lstm.output_gate.input_transform.bias.as_ref().unwrap().val(),
            ],
            0,
        ); // [4*hidden]
        x_proj_flat = x_proj_flat + b_x.unsqueeze_dims::<2>(&[0]); // broadcast over batch
    }
    let x_proj = x_proj_flat.reshape([seq, batch, 4 * hidden]); // [seq, batch, 4*hidden]

    // ── 2. Concat hidden weights (computed once, shared across all steps) ──────
    // [d_input=hidden, 4*d_output=4*hidden] after dim=1 concat
    let w_h = Tensor::cat(
        vec![
            lstm.input_gate.hidden_transform.weight.val(),
            lstm.forget_gate.hidden_transform.weight.val(),
            lstm.cell_gate.hidden_transform.weight.val(),
            lstm.output_gate.hidden_transform.weight.val(),
        ],
        1,
    ); // [hidden, 4*hidden]

    let b_h = lstm.input_gate.hidden_transform.bias.as_ref().map(|_| {
        Tensor::cat(
            vec![
                lstm.input_gate.hidden_transform.bias.as_ref().unwrap().val(),
                lstm.forget_gate.hidden_transform.bias.as_ref().unwrap().val(),
                lstm.cell_gate.hidden_transform.bias.as_ref().unwrap().val(),
                lstm.output_gate.hidden_transform.bias.as_ref().unwrap().val(),
            ],
            0,
        ) // [4*hidden]
    });

    // ── 3. Initialise h, c ────────────────────────────────────────────────────
    let (mut h, mut c) = match state {
        Some(s) => (s.hidden, s.cell),
        None => (
            Tensor::zeros([batch, hidden], &device),
            Tensor::zeros([batch, hidden], &device),
        ),
    };

    // ── 4. Sequential recurrence over time steps ──────────────────────────────
    let mut output_steps = Vec::with_capacity(seq);

    for t in 0..seq {
        // Pre-projected input for step t: [batch, 4*hidden]
        let x_t = x_proj.clone().narrow(0, t, 1).squeeze_dims::<2>(&[0]);

        // Hidden projection: ONE matmul → [batch, 4*hidden]
        let mut gates = x_t + h.clone().matmul(w_h.clone());
        if let Some(ref b) = b_h {
            gates = gates + b.clone().unsqueeze_dims::<2>(&[0]);
        }

        // The generated model uses the default LSTM activations: sigmoid for
        // input/forget/output gates and tanh for cell/hidden state. Applying
        // each activation to the packed gate tensor trades a little extra math
        // for fewer tiny GPU dispatches in the recurrent loop.
        let gates_sigmoid = burn::tensor::activation::sigmoid(gates.clone());
        let gates_tanh = gates.tanh();
        let i = gates_sigmoid.clone().narrow(1, 0, hidden);
        let f = gates_sigmoid.clone().narrow(1, hidden, hidden);
        let g = gates_tanh.narrow(1, 2 * hidden, hidden);
        let o = gates_sigmoid.narrow(1, 3 * hidden, hidden);

        c = f * c + i * g;
        if let Some(clip) = lstm.clip {
            c = c.clamp(-clip as f32, clip as f32);
        }
        h = o * c.clone().tanh();

        output_steps.push(h.clone().unsqueeze_dims::<3>(&[0]));
    }

    let output = Tensor::cat(output_steps, 0);

    (output, LstmState::new(c, h))
}

#[cfg(test)]
pub(super) fn lstm_preproj_equivalence_error<B: Backend>(device: &B::Device) -> f32 {
    let seq = 5;
    let batch = 3;
    let input_size = 16;
    let hidden = 32;

    let lstm = LstmConfig::new(input_size, hidden, true)
        .with_batch_first(false)
        .with_input_forget(false)
        .init(device);
    let input = Tensor::<B, 3>::ones([seq, batch, input_size], device);
    let cell = Tensor::<B, 2>::ones([batch, hidden], device) * 0.125;
    let hidden_state = Tensor::<B, 2>::ones([batch, hidden], device) * -0.25;

    let (expected_output, expected_state) = lstm.forward(
        input.clone(),
        Some(LstmState::new(cell.clone(), hidden_state.clone())),
    );
    let (actual_output, actual_state) = lstm_preproj(
        &lstm,
        input,
        Some(LstmState::new(cell, hidden_state)),
    );

    fn max_abs_diff<B: Backend, const D: usize>(lhs: Tensor<B, D>, rhs: Tensor<B, D>) -> f32 {
        let lhs = lhs.into_data().iter::<f32>().collect::<Vec<_>>();
        let rhs = rhs.into_data().iter::<f32>().collect::<Vec<_>>();
        lhs.iter()
            .zip(rhs.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max)
    }

    max_abs_diff(expected_output, actual_output)
        .max(max_abs_diff(expected_state.cell, actual_state.cell))
        .max(max_abs_diff(expected_state.hidden, actual_state.hidden))
}
