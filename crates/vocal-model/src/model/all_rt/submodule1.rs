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
        // Fields are initialised directly in the struct literal so each component
        // is built in place in the return slot instead of as a separate stack
        // temporary that is then moved — keeping the constructor's frame small.
        let pad = PaddingConfig2d::Explicit(2, 1, 2, 1);
        Self {
            conv2d1: conv([4, 16], [1, 1], PaddingConfig2d::Valid, device),
            instancenormalization1: norm(2, device),
            constant47: const_param([16, 1, 1], device),
            constant48: const_param([16, 1, 1], device),
            conv2d2: conv([16, 16], [3, 3], pad.clone(), device),
            instancenormalization2: norm(2, device),
            constant49: const_param([16, 1, 1], device),
            constant50: const_param([16, 1, 1], device),
            conv2d3: conv([16, 16], [3, 3], pad.clone(), device),
            instancenormalization3: norm(2, device),
            constant51: const_param([16, 1, 1], device),
            constant52: const_param([16, 1, 1], device),
            conv2d4: conv([16, 16], [3, 3], pad.clone(), device),
            instancenormalization4: norm(2, device),
            constant53: const_param([16, 1, 1], device),
            constant54: const_param([16, 1, 1], device),
            linear1: LinearConfig::new(384, 24).with_bias(false).init(device),
            instancenormalization5: norm(2, device),
            constant56: const_param([16, 1, 1], device),
            constant57: const_param([16, 1, 1], device),
            linear2: LinearConfig::new(24, 384).with_bias(false).init(device),
            instancenormalization6: norm(2, device),
            constant59: const_param([16, 1, 1], device),
            constant60: const_param([16, 1, 1], device),
            conv2d5: conv([16, 16], [3, 3], pad.clone(), device),
            instancenormalization7: norm(2, device),
            constant61: const_param([16, 1, 1], device),
            constant62: const_param([16, 1, 1], device),
            conv2d6: conv([16, 16], [3, 3], pad, device),
            instancenormalization8: norm(2, device),
            constant63: const_param([16, 1, 1], device),
            constant64: const_param([16, 1, 1], device),
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }

    /// Two parallel 3×3 stacks over the input feature map fused by a residual add.
    /// Each conv/linear is followed by the exporter's `norm_affine` + ReLU block.
    pub fn forward(&self, input: Tensor<B, 4>) -> Tensor<B, 4> {
        let conv2d1_out1 = self.conv2d1.forward(input);
        let relu1_out1 = relu(norm_affine(
            &self.instancenormalization1,
            &self.constant47,
            &self.constant48,
            2,
            conv2d1_out1,
        ));
        let transpose1_out1 = relu1_out1.permute([0, 1, 3, 2]);

        let conv2d2_out1 = self.conv2d2.forward(transpose1_out1.clone());
        let slice1_out1 = conv2d2_out1.slice(s![.., .., 0..-2, ..]);
        let relu2_out1 = relu(norm_affine(
            &self.instancenormalization2,
            &self.constant49,
            &self.constant50,
            2,
            slice1_out1,
        ));

        let conv2d3_out1 = self.conv2d3.forward(transpose1_out1);
        let slice2_out1 = conv2d3_out1.slice(s![.., .., 0..-2, ..]);
        let relu3_out1 = relu(norm_affine(
            &self.instancenormalization3,
            &self.constant51,
            &self.constant52,
            2,
            slice2_out1,
        ));

        let conv2d4_out1 = self.conv2d4.forward(relu3_out1);
        let slice3_out1 = conv2d4_out1.slice(s![.., .., 0..-2, ..]);
        let relu4_out1 = relu(norm_affine(
            &self.instancenormalization4,
            &self.constant53,
            &self.constant54,
            2,
            slice3_out1,
        ));

        // Channel-mixing bottleneck (384→24→384) with a residual back to relu4.
        let linear1_out1 = self.linear1.forward(relu4_out1.clone());
        let relu5_out1 = relu(norm_affine(
            &self.instancenormalization5,
            &self.constant56,
            &self.constant57,
            2,
            linear1_out1,
        ));
        let linear2_out1 = self.linear2.forward(relu5_out1);
        let relu6_out1 = relu(norm_affine(
            &self.instancenormalization6,
            &self.constant59,
            &self.constant60,
            2,
            linear2_out1,
        ));
        let add7_out1 = relu4_out1.add(relu6_out1);

        let conv2d5_out1 = self.conv2d5.forward(add7_out1);
        let slice4_out1 = conv2d5_out1.slice(s![.., .., 0..-2, ..]);
        let relu7_out1 = relu(norm_affine(
            &self.instancenormalization7,
            &self.constant61,
            &self.constant62,
            2,
            slice4_out1,
        ));

        let conv2d6_out1 = self.conv2d6.forward(relu7_out1);
        let slice5_out1 = conv2d6_out1.slice(s![.., .., 0..-2, ..]);
        let relu8_out1 = relu(norm_affine(
            &self.instancenormalization8,
            &self.constant63,
            &self.constant64,
            2,
            slice5_out1,
        ));

        relu8_out1.add(relu2_out1)
    }
}
