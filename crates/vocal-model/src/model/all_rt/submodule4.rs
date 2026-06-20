use super::*;

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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant124: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant126: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant128: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant131: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant134: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant136: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant138: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
        let slice15_out1 = conv2d15_out1.slice(s![.., .., 0..-2, ..]);
        let reshape67_out1 = slice15_out1.clone().reshape([0, 8, -1]);
        let instancenormalization25_out1 = self.instancenormalization25.forward(reshape67_out1);
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
        let mul26_out1 = reshape68_out1.mul((constant123_out1).unsqueeze_dims(&[0isize]));
        let constant124_out1 = self.constant124.val();
        let add35_out1 = mul26_out1.add((constant124_out1).unsqueeze_dims(&[0isize]));
        let relu18_out1 = burn::tensor::activation::relu(add35_out1);
        let conv2d16_out1 = self.conv2d16.forward(reshape66_out1);
        let slice16_out1 = conv2d16_out1.slice(s![.., .., 0..-2, ..]);
        let reshape69_out1 = slice16_out1.clone().reshape([0, 8, -1]);
        let instancenormalization26_out1 = self.instancenormalization26.forward(reshape69_out1);
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
        let mul27_out1 = reshape70_out1.mul((constant125_out1).unsqueeze_dims(&[0isize]));
        let constant126_out1 = self.constant126.val();
        let add36_out1 = mul27_out1.add((constant126_out1).unsqueeze_dims(&[0isize]));
        let relu19_out1 = burn::tensor::activation::relu(add36_out1);
        let conv2d17_out1 = self.conv2d17.forward(relu19_out1);
        let slice17_out1 = conv2d17_out1.slice(s![.., .., 0..-2, ..]);
        let reshape71_out1 = slice17_out1.clone().reshape([0, 8, -1]);
        let instancenormalization27_out1 = self.instancenormalization27.forward(reshape71_out1);
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
        let mul28_out1 = reshape72_out1.mul((constant127_out1).unsqueeze_dims(&[0isize]));
        let constant128_out1 = self.constant128.val();
        let add37_out1 = mul28_out1.add((constant128_out1).unsqueeze_dims(&[0isize]));
        let relu20_out1 = burn::tensor::activation::relu(add37_out1);
        let linear11_out1 = self.linear11.forward(relu20_out1.clone());
        let reshape73_out1 = linear11_out1.clone().reshape([0, 8, -1]);
        let instancenormalization28_out1 = self.instancenormalization28.forward(reshape73_out1);
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
        let mul29_out1 = reshape74_out1.mul((constant130_out1).unsqueeze_dims(&[0isize]));
        let constant131_out1 = self.constant131.val();
        let add38_out1 = mul29_out1.add((constant131_out1).unsqueeze_dims(&[0isize]));
        let relu21_out1 = burn::tensor::activation::relu(add38_out1);
        let linear12_out1 = self.linear12.forward(relu21_out1);
        let reshape75_out1 = linear12_out1.clone().reshape([0, 8, -1]);
        let instancenormalization29_out1 = self.instancenormalization29.forward(reshape75_out1);
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
        let mul30_out1 = reshape76_out1.mul((constant133_out1).unsqueeze_dims(&[0isize]));
        let constant134_out1 = self.constant134.val();
        let add39_out1 = mul30_out1.add((constant134_out1).unsqueeze_dims(&[0isize]));
        let relu22_out1 = burn::tensor::activation::relu(add39_out1);
        let add40_out1 = relu20_out1.add(relu22_out1);
        let conv2d18_out1 = self.conv2d18.forward(add40_out1);
        let slice18_out1 = conv2d18_out1.slice(s![.., .., 0..-2, ..]);
        let reshape77_out1 = slice18_out1.clone().reshape([0, 8, -1]);
        let instancenormalization30_out1 = self.instancenormalization30.forward(reshape77_out1);
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
        let mul31_out1 = reshape78_out1.mul((constant135_out1).unsqueeze_dims(&[0isize]));
        let constant136_out1 = self.constant136.val();
        let add41_out1 = mul31_out1.add((constant136_out1).unsqueeze_dims(&[0isize]));
        let relu23_out1 = burn::tensor::activation::relu(add41_out1);
        let conv2d19_out1 = self.conv2d19.forward(relu23_out1);
        let slice19_out1 = conv2d19_out1.slice(s![.., .., 0..-2, ..]);
        let reshape79_out1 = slice19_out1.clone().reshape([0, 8, -1]);
        let instancenormalization31_out1 = self.instancenormalization31.forward(reshape79_out1);
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
        let mul32_out1 = reshape80_out1.mul((constant137_out1).unsqueeze_dims(&[0isize]));
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
