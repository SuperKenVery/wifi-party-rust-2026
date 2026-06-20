use super::*;

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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant66: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant68: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant70: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant72: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant75: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant78: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant80: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant82: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
        let slice6_out1 = conv2d7_out1.slice(s![.., .., 0..-1, ..]);
        let slice7_out1 = slice6_out1.slice(s![.., .., .., 0..-1]);
        let reshape17_out1 = slice7_out1.clone().reshape([0, 4, -1]);
        let instancenormalization9_out1 = self.instancenormalization9.forward(reshape17_out1);
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
        let slice8_out1 = conv2d8_out1.slice(s![.., .., 0..-2, ..]);
        let reshape19_out1 = slice8_out1.clone().reshape([0, 4, -1]);
        let instancenormalization10_out1 = self.instancenormalization10.forward(reshape19_out1);
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
        let slice9_out1 = conv2d9_out1.slice(s![.., .., 0..-2, ..]);
        let reshape21_out1 = slice9_out1.clone().reshape([0, 4, -1]);
        let instancenormalization11_out1 = self.instancenormalization11.forward(reshape21_out1);
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
        let slice10_out1 = conv2d10_out1.slice(s![.., .., 0..-2, ..]);
        let reshape23_out1 = slice10_out1.clone().reshape([0, 4, -1]);
        let instancenormalization12_out1 = self.instancenormalization12.forward(reshape23_out1);
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
        let instancenormalization13_out1 = self.instancenormalization13.forward(reshape25_out1);
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
        let instancenormalization14_out1 = self.instancenormalization14.forward(reshape27_out1);
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
        let slice11_out1 = conv2d11_out1.slice(s![.., .., 0..-2, ..]);
        let reshape29_out1 = slice11_out1.clone().reshape([0, 4, -1]);
        let instancenormalization15_out1 = self.instancenormalization15.forward(reshape29_out1);
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
        let slice12_out1 = conv2d12_out1.slice(s![.., .., 0..-2, ..]);
        let reshape31_out1 = slice12_out1.clone().reshape([0, 4, -1]);
        let instancenormalization16_out1 = self.instancenormalization16.forward(reshape31_out1);
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
