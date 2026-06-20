use super::*;

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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant48: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant50: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant52: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant54: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant57: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant60: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant62: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1, 1].into(),
        );
        let constant64: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([16, 1, 1], (device, burn::tensor::DType::F16))
            },
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
        let instancenormalization1_out1 = self.instancenormalization1.forward(reshape1_out1);
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
        let slice1_out1 = conv2d2_out1.slice(s![.., .., 0..-2, ..]);
        let reshape3_out1 = slice1_out1.clone().reshape([0, 2, -1]);
        let instancenormalization2_out1 = self.instancenormalization2.forward(reshape3_out1);
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
        let slice2_out1 = conv2d3_out1.slice(s![.., .., 0..-2, ..]);
        let reshape5_out1 = slice2_out1.clone().reshape([0, 2, -1]);
        let instancenormalization3_out1 = self.instancenormalization3.forward(reshape5_out1);
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
        let slice3_out1 = conv2d4_out1.slice(s![.., .., 0..-2, ..]);
        let reshape7_out1 = slice3_out1.clone().reshape([0, 2, -1]);
        let instancenormalization4_out1 = self.instancenormalization4.forward(reshape7_out1);
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
        let instancenormalization5_out1 = self.instancenormalization5.forward(reshape9_out1);
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
        let instancenormalization6_out1 = self.instancenormalization6.forward(reshape11_out1);
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
        let slice4_out1 = conv2d5_out1.slice(s![.., .., 0..-2, ..]);
        let reshape13_out1 = slice4_out1.clone().reshape([0, 2, -1]);
        let instancenormalization7_out1 = self.instancenormalization7.forward(reshape13_out1);
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
        let slice5_out1 = conv2d6_out1.slice(s![.., .., 0..-2, ..]);
        let reshape15_out1 = slice5_out1.clone().reshape([0, 2, -1]);
        let instancenormalization8_out1 = self.instancenormalization8.forward(reshape15_out1);
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
