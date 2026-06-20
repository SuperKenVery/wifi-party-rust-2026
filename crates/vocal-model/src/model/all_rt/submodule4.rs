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
        // Each field is initialised directly in the struct literal (rather than
        // via intermediate `let` bindings) so the large component sub-objects are
        // built in place in the return slot instead of as separate stack temps
        // that are then moved — this keeps the constructor's stack frame small.
        let pad = PaddingConfig2d::Explicit(2, 1, 2, 1);
        Self {
            conv2d15: conv([64, 64], [3, 3], pad.clone(), device),
            instancenormalization25: norm(8, device),
            constant123: const_param([64, 1, 1], device),
            constant124: const_param([64, 1, 1], device),
            conv2d16: conv([64, 64], [3, 3], pad.clone(), device),
            instancenormalization26: norm(8, device),
            constant125: const_param([64, 1, 1], device),
            constant126: const_param([64, 1, 1], device),
            conv2d17: conv([64, 64], [3, 3], pad.clone(), device),
            instancenormalization27: norm(8, device),
            constant127: const_param([64, 1, 1], device),
            constant128: const_param([64, 1, 1], device),
            linear11: LinearConfig::new(384, 24).with_bias(false).init(device),
            instancenormalization28: norm(8, device),
            constant130: const_param([64, 1, 1], device),
            constant131: const_param([64, 1, 1], device),
            linear12: LinearConfig::new(24, 384).with_bias(false).init(device),
            instancenormalization29: norm(8, device),
            constant133: const_param([64, 1, 1], device),
            constant134: const_param([64, 1, 1], device),
            conv2d18: conv([64, 64], [3, 3], pad.clone(), device),
            instancenormalization30: norm(8, device),
            constant135: const_param([64, 1, 1], device),
            constant136: const_param([64, 1, 1], device),
            conv2d19: conv([64, 64], [3, 3], pad, device),
            instancenormalization31: norm(8, device),
            constant137: const_param([64, 1, 1], device),
            constant138: const_param([64, 1, 1], device),
            conv2d20: conv([64, 16], [1, 1], PaddingConfig2d::Valid, device),
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }

    /// Decoder head: two parallel 3×3 stacks (with a 384→24→384 channel
    /// bottleneck) recombine via residual adds, then a 1×1 conv projects to the
    /// 4 sources × 4 complex/real/imag output channels.
    pub fn forward(&self, mul25_out1: Tensor<B, 5>) -> Tensor<B, 5> {
        let reshape66_out1 = mul25_out1.reshape([1, 64, 64, 384]);
        let conv2d15_out1 = self.conv2d15.forward(reshape66_out1.clone());
        let slice15_out1 = conv2d15_out1.slice(s![.., .., 0..-2, ..]);
        let relu18_out1 = relu(norm_affine(
            &self.instancenormalization25,
            &self.constant123,
            &self.constant124,
            8,
            slice15_out1,
        ));

        let conv2d16_out1 = self.conv2d16.forward(reshape66_out1);
        let slice16_out1 = conv2d16_out1.slice(s![.., .., 0..-2, ..]);
        let relu19_out1 = relu(norm_affine(
            &self.instancenormalization26,
            &self.constant125,
            &self.constant126,
            8,
            slice16_out1,
        ));

        let conv2d17_out1 = self.conv2d17.forward(relu19_out1);
        let slice17_out1 = conv2d17_out1.slice(s![.., .., 0..-2, ..]);
        let relu20_out1 = relu(norm_affine(
            &self.instancenormalization27,
            &self.constant127,
            &self.constant128,
            8,
            slice17_out1,
        ));

        // Channel-mixing bottleneck (384→24→384) with a residual back to relu20.
        let linear11_out1 = self.linear11.forward(relu20_out1.clone());
        let relu21_out1 = relu(norm_affine(
            &self.instancenormalization28,
            &self.constant130,
            &self.constant131,
            8,
            linear11_out1,
        ));
        let linear12_out1 = self.linear12.forward(relu21_out1);
        let relu22_out1 = relu(norm_affine(
            &self.instancenormalization29,
            &self.constant133,
            &self.constant134,
            8,
            linear12_out1,
        ));
        let add40_out1 = relu20_out1.add(relu22_out1);

        let conv2d18_out1 = self.conv2d18.forward(add40_out1);
        let slice18_out1 = conv2d18_out1.slice(s![.., .., 0..-2, ..]);
        let relu23_out1 = relu(norm_affine(
            &self.instancenormalization30,
            &self.constant135,
            &self.constant136,
            8,
            slice18_out1,
        ));

        let conv2d19_out1 = self.conv2d19.forward(relu23_out1);
        let slice19_out1 = conv2d19_out1.slice(s![.., .., 0..-2, ..]);
        let relu24_out1 = relu(norm_affine(
            &self.instancenormalization31,
            &self.constant137,
            &self.constant138,
            8,
            slice19_out1,
        ));
        let add43_out1 = relu24_out1.add(relu18_out1);

        let transpose29_out1 = add43_out1.permute([0, 1, 3, 2]);
        let conv2d20_out1 = self.conv2d20.forward(transpose29_out1);
        conv2d20_out1.reshape([1, 4, 4, 384, 64])
    }
}
