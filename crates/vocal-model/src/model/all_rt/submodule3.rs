use super::*;

/// The band/sequence LSTM shared by all six recurrent stages: 16→32 hidden,
/// time-major, no input-forget coupling.
fn band_lstm<B: Backend>(device: &B::Device) -> Lstm<B> {
    LstmConfig::new(16, 32, true)
        .with_batch_first(false)
        .with_input_forget(false)
        .init(device)
}

/// The 32→16 projection applied after each band LSTM.
fn band_linear<B: Backend>(device: &B::Device) -> Linear<B> {
    LinearConfig::new(32, 16).with_bias(true).init(device)
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
        // Fields are initialised directly in the struct literal so each component
        // (notably the six 14 KB LSTMs) is built in place in the return slot
        // instead of as a separate stack temporary that is then moved — keeping
        // the constructor's frame small.
        Self {
            instancenormalization17: norm(1, device),
            constant83: const_param([16, 1], device),
            constant84: const_param([16, 1], device),
            constant240: const_param([1, 768, 32], device),
            lstm1: band_lstm(device),
            linear5: band_linear(device),
            instancenormalization18: norm(1, device),
            constant89: const_param([16, 1], device),
            constant90: const_param([16, 1], device),
            constant252: const_param([1, 128, 32], device),
            lstm2: band_lstm(device),
            linear6: band_linear(device),
            instancenormalization19: norm(1, device),
            constant95: const_param([16, 1], device),
            constant96: const_param([16, 1], device),
            constant264: const_param([1, 768, 32], device),
            lstm3: band_lstm(device),
            linear7: band_linear(device),
            instancenormalization20: norm(1, device),
            constant101: const_param([16, 1], device),
            constant102: const_param([16, 1], device),
            constant276: const_param([1, 128, 32], device),
            lstm4: band_lstm(device),
            linear8: band_linear(device),
            instancenormalization21: norm(1, device),
            constant107: const_param([16, 1], device),
            constant108: const_param([16, 1], device),
            constant288: const_param([1, 768, 32], device),
            lstm5: band_lstm(device),
            linear9: band_linear(device),
            instancenormalization22: norm(1, device),
            constant113: const_param([16, 1], device),
            constant114: const_param([16, 1], device),
            constant300: const_param([1, 128, 32], device),
            lstm6: band_lstm(device),
            linear10: band_linear(device),
            instancenormalization23: norm(4, device),
            constant119: const_param([32, 1, 1], device),
            constant120: const_param([32, 1, 1], device),
            conv2d13: conv([32, 128], [1, 1], PaddingConfig2d::Valid, device),
            conv2d14: conv(
                [128, 64],
                [2, 2],
                PaddingConfig2d::Explicit(1, 1, 1, 1),
                device,
            ),
            instancenormalization24: norm(8, device),
            constant121: const_param([64, 1, 1], device),
            constant122: const_param([64, 1, 1], device),
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }

    /// Six alternating frequency/time-band LSTM stages, each a `norm_affine`
    /// followed by an `lstm_block` and a residual add, then a softmax-attention
    /// head that produces the per-source complex mask applied to `add10_out1`.
    pub fn forward(&self, add20_out1: Tensor<B, 4>, add10_out1: Tensor<B, 4>) -> Tensor<B, 5>
    where
        B: CustomLstmBackend,
    {
        let reshape33_out1 = add20_out1.reshape([2, 16, 64, 384]);
        let transpose2_out1 = reshape33_out1.permute([0, 3, 2, 1]);

        // Band 1 (frequency).
        let reshape34_out1 = transpose2_out1.clone().reshape([768, 64, 16]);
        let transpose3_out1 = reshape34_out1.permute([0, 2, 1]);
        let add21_out1 = norm_affine(
            &self.instancenormalization17,
            &self.constant83,
            &self.constant84,
            1,
            transpose3_out1,
        );
        let linear5_out1 = lstm_block(
            &self.lstm1,
            &self.linear5,
            self.constant240.val(),
            "lstm1",
            add21_out1,
        );
        let reshape37_out1 = linear5_out1.reshape([2, 384, 64, 16]);
        let add22_out1 = reshape37_out1.add(transpose2_out1);
        let transpose6_out1 = add22_out1.permute([0, 2, 1, 3]);

        // Band 2 (time).
        let reshape38_out1 = transpose6_out1.clone().reshape([128, 384, 16]);
        let transpose7_out1 = reshape38_out1.permute([0, 2, 1]);
        let add23_out1 = norm_affine(
            &self.instancenormalization18,
            &self.constant89,
            &self.constant90,
            1,
            transpose7_out1,
        );
        let linear6_out1 = lstm_block(
            &self.lstm2,
            &self.linear6,
            self.constant252.val(),
            "lstm2",
            add23_out1,
        );
        let reshape41_out1 = linear6_out1.reshape([2, 64, 384, 16]);
        let add24_out1 = reshape41_out1.add(transpose6_out1);
        let transpose10_out1 = add24_out1.permute([0, 2, 1, 3]);

        // Band 3 (frequency).
        let reshape42_out1 = transpose10_out1.clone().reshape([768, 64, 16]);
        let transpose11_out1 = reshape42_out1.permute([0, 2, 1]);
        let add25_out1 = norm_affine(
            &self.instancenormalization19,
            &self.constant95,
            &self.constant96,
            1,
            transpose11_out1,
        );
        let linear7_out1 = lstm_block(
            &self.lstm3,
            &self.linear7,
            self.constant264.val(),
            "lstm3",
            add25_out1,
        );
        let reshape45_out1 = linear7_out1.reshape([2, 384, 64, 16]);
        let add26_out1 = reshape45_out1.add(transpose10_out1);
        let transpose14_out1 = add26_out1.permute([0, 2, 1, 3]);

        // Band 4 (time).
        let reshape46_out1 = transpose14_out1.clone().reshape([128, 384, 16]);
        let transpose15_out1 = reshape46_out1.permute([0, 2, 1]);
        let add27_out1 = norm_affine(
            &self.instancenormalization20,
            &self.constant101,
            &self.constant102,
            1,
            transpose15_out1,
        );
        let linear8_out1 = lstm_block(
            &self.lstm4,
            &self.linear8,
            self.constant276.val(),
            "lstm4",
            add27_out1,
        );
        let reshape49_out1 = linear8_out1.reshape([2, 64, 384, 16]);
        let add28_out1 = reshape49_out1.add(transpose14_out1);
        let transpose18_out1 = add28_out1.permute([0, 2, 1, 3]);

        // Band 5 (frequency).
        let reshape50_out1 = transpose18_out1.clone().reshape([768, 64, 16]);
        let transpose19_out1 = reshape50_out1.permute([0, 2, 1]);
        let add29_out1 = norm_affine(
            &self.instancenormalization21,
            &self.constant107,
            &self.constant108,
            1,
            transpose19_out1,
        );
        let linear9_out1 = lstm_block(
            &self.lstm5,
            &self.linear9,
            self.constant288.val(),
            "lstm5",
            add29_out1,
        );
        let reshape53_out1 = linear9_out1.reshape([2, 384, 64, 16]);
        let add30_out1 = reshape53_out1.add(transpose18_out1);
        let transpose22_out1 = add30_out1.permute([0, 2, 1, 3]);

        // Band 6 (time).
        let reshape54_out1 = transpose22_out1.clone().reshape([128, 384, 16]);
        let transpose23_out1 = reshape54_out1.permute([0, 2, 1]);
        let add31_out1 = norm_affine(
            &self.instancenormalization22,
            &self.constant113,
            &self.constant114,
            1,
            transpose23_out1,
        );
        let linear10_out1 = lstm_block(
            &self.lstm6,
            &self.linear10,
            self.constant300.val(),
            "lstm6",
            add31_out1,
        );
        let reshape57_out1 = linear10_out1.reshape([2, 64, 384, 16]);
        let add32_out1 = reshape57_out1.add(transpose22_out1);
        let transpose26_out1 = add32_out1.permute([0, 3, 1, 2]);

        // Softmax-attention head producing the per-source complex masks.
        let reshape58_out1 = transpose26_out1.reshape([1, 32, 64, 384]);
        let add33_out1 = norm_affine(
            &self.instancenormalization23,
            &self.constant119,
            &self.constant120,
            4,
            reshape58_out1,
        );
        let conv2d13_out1 = self.conv2d13.forward(add33_out1);
        let reshape61_out1 = conv2d13_out1.reshape([1, 4, 32, 64, 384]);
        let transpose27_out1 = reshape61_out1.permute([0, 4, 2, 3, 1]);
        let softmax1_out1 = burn::tensor::activation::softmax(transpose27_out1, 4);
        let transpose28_out1 = softmax1_out1.permute([0, 4, 2, 3, 1]);
        let reshape62_out1 = transpose28_out1.reshape([1, 128, 64, 384]);
        let conv2d14_out1 = self.conv2d14.forward(reshape62_out1);
        let slice13_out1 = conv2d14_out1.slice(s![.., .., 0..-1, ..]);
        let slice14_out1 = slice13_out1.slice(s![.., .., .., 0..-1]);
        let add34_out1 = norm_affine(
            &self.instancenormalization24,
            &self.constant121,
            &self.constant122,
            8,
            slice14_out1,
        );
        let relu17_out1 = relu(add34_out1);
        let reshape65_out1 = relu17_out1.reshape([1, 4, 16, 64, 384]);
        let unsqueeze13_out1: Tensor<B, 5> = add10_out1.unsqueeze_dims::<5>(&[1]);
        reshape65_out1.mul(unsqueeze13_out1)
    }
}
