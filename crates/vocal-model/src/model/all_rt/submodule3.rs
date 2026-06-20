use super::*;

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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant84: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant240: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 768, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant90: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant252: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 128, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant96: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant264: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 768, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant102: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant276: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 128, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant108: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant288: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 768, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant114: burn::module::Param<Tensor<B, 2>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 2>::zeros([16, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [16, 1].into(),
        );
        let constant300: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([1, 128, 32], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [32, 1, 1].into(),
        );
        let constant120: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([32, 1, 1], (device, burn::tensor::DType::F16))
            },
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
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
            device.clone(),
            false,
            [64, 1, 1].into(),
        );
        let constant122: burn::module::Param<Tensor<B, 3>> = burn::module::Param::uninitialized(
            burn::module::ParamId::new(),
            move |device, _require_grad| {
                Tensor::<B, 3>::zeros([64, 1, 1], (device, burn::tensor::DType::F16))
            },
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
    pub fn forward(&self, add20_out1: Tensor<B, 4>, add10_out1: Tensor<B, 4>) -> Tensor<B, 5>
    where
        B: CustomLstmBackend,
    {
        let reshape33_out1 = add20_out1.reshape([2, 16, 64, 384]);
        let transpose2_out1 = reshape33_out1.permute([0, 3, 2, 1]);
        let reshape34_out1 = transpose2_out1.clone().reshape([768, 64, 16]);
        let transpose3_out1 = reshape34_out1.permute([0, 2, 1]);
        let reshape35_out1 = transpose3_out1.clone().reshape([0, 1, -1]);
        let instancenormalization17_out1 = self.instancenormalization17.forward(reshape35_out1);
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
        let instancenormalization18_out1 = self.instancenormalization18.forward(reshape39_out1);
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
        let instancenormalization19_out1 = self.instancenormalization19.forward(reshape43_out1);
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
        let instancenormalization20_out1 = self.instancenormalization20.forward(reshape47_out1);
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
        let mul20_out1 = reshape48_out1.mul((constant101_out1).unsqueeze_dims(&[0isize]));
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
        let instancenormalization21_out1 = self.instancenormalization21.forward(reshape51_out1);
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
        let mul21_out1 = reshape52_out1.mul((constant107_out1).unsqueeze_dims(&[0isize]));
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
        let instancenormalization22_out1 = self.instancenormalization22.forward(reshape55_out1);
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
        let mul22_out1 = reshape56_out1.mul((constant113_out1).unsqueeze_dims(&[0isize]));
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
        let instancenormalization23_out1 = self.instancenormalization23.forward(reshape59_out1);
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
        let mul23_out1 = reshape60_out1.mul((constant119_out1).unsqueeze_dims(&[0isize]));
        let constant120_out1 = self.constant120.val();
        let add33_out1 = mul23_out1.add((constant120_out1).unsqueeze_dims(&[0isize]));
        let conv2d13_out1 = self.conv2d13.forward(add33_out1);
        let reshape61_out1 = conv2d13_out1.reshape([1, 4, 32, 64, 384]);
        let transpose27_out1 = reshape61_out1.permute([0, 4, 2, 3, 1]);
        let softmax1_out1 = burn::tensor::activation::softmax(transpose27_out1, 4);
        let transpose28_out1 = softmax1_out1.permute([0, 4, 2, 3, 1]);
        let reshape62_out1 = transpose28_out1.reshape([1, 128, 64, 384]);
        let conv2d14_out1 = self.conv2d14.forward(reshape62_out1);
        let slice13_out1 = conv2d14_out1.slice(s![.., .., 0..-1, ..]);
        let slice14_out1 = slice13_out1.slice(s![.., .., .., 0..-1]);
        let reshape63_out1 = slice14_out1.clone().reshape([0, 8, -1]);
        let instancenormalization24_out1 = self.instancenormalization24.forward(reshape63_out1);
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
        let mul24_out1 = reshape64_out1.mul((constant121_out1).unsqueeze_dims(&[0isize]));
        let constant122_out1 = self.constant122.val();
        let add34_out1 = mul24_out1.add((constant122_out1).unsqueeze_dims(&[0isize]));
        let relu17_out1 = burn::tensor::activation::relu(add34_out1);
        let reshape65_out1 = relu17_out1.reshape([1, 4, 16, 64, 384]);
        let unsqueeze13_out1: Tensor<B, 5> = add10_out1.unsqueeze_dims::<5>(&[1]);
        let mul25_out1 = reshape65_out1.mul(unsqueeze13_out1);
        mul25_out1
    }
}
