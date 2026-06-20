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
        // Fields are initialised directly in the struct literal so each component
        // is built in place in the return slot instead of as a separate stack
        // temporary that is then moved — keeping the constructor's frame small.
        let pad = PaddingConfig2d::Explicit(2, 1, 2, 1);
        Self {
            conv2d7: conv([16, 32], [2, 2], PaddingConfig2d::Explicit(1, 1, 1, 1), device),
            instancenormalization9: norm(4, device),
            constant65: const_param([32, 1, 1], device),
            constant66: const_param([32, 1, 1], device),
            conv2d8: conv([32, 32], [3, 3], pad.clone(), device),
            instancenormalization10: norm(4, device),
            constant67: const_param([32, 1, 1], device),
            constant68: const_param([32, 1, 1], device),
            conv2d9: conv([32, 32], [3, 3], pad.clone(), device),
            instancenormalization11: norm(4, device),
            constant69: const_param([32, 1, 1], device),
            constant70: const_param([32, 1, 1], device),
            conv2d10: conv([32, 32], [3, 3], pad.clone(), device),
            instancenormalization12: norm(4, device),
            constant71: const_param([32, 1, 1], device),
            constant72: const_param([32, 1, 1], device),
            linear3: LinearConfig::new(384, 24).with_bias(false).init(device),
            instancenormalization13: norm(4, device),
            constant74: const_param([32, 1, 1], device),
            constant75: const_param([32, 1, 1], device),
            linear4: LinearConfig::new(24, 384).with_bias(false).init(device),
            instancenormalization14: norm(4, device),
            constant77: const_param([32, 1, 1], device),
            constant78: const_param([32, 1, 1], device),
            conv2d11: conv([32, 32], [3, 3], pad.clone(), device),
            instancenormalization15: norm(4, device),
            constant79: const_param([32, 1, 1], device),
            constant80: const_param([32, 1, 1], device),
            conv2d12: conv([32, 32], [3, 3], pad, device),
            instancenormalization16: norm(4, device),
            constant81: const_param([32, 1, 1], device),
            constant82: const_param([32, 1, 1], device),
            phantom: core::marker::PhantomData,
            device: device.clone(),
        }
    }

    /// Downsampling stage: a 2×2 conv halves the grid, then two parallel 3×3
    /// stacks (with a 384→24→384 channel bottleneck) recombine via residual adds.
    pub fn forward(&self, add10_out1: Tensor<B, 4>) -> Tensor<B, 4> {
        let conv2d7_out1 = self.conv2d7.forward(add10_out1);
        let slice6_out1 = conv2d7_out1.slice(s![.., .., 0..-1, ..]);
        let slice7_out1 = slice6_out1.slice(s![.., .., .., 0..-1]);
        let relu9_out1 = relu(norm_affine(
            &self.instancenormalization9,
            &self.constant65,
            &self.constant66,
            4,
            slice7_out1,
        ));

        let conv2d8_out1 = self.conv2d8.forward(relu9_out1.clone());
        let slice8_out1 = conv2d8_out1.slice(s![.., .., 0..-2, ..]);
        let relu10_out1 = relu(norm_affine(
            &self.instancenormalization10,
            &self.constant67,
            &self.constant68,
            4,
            slice8_out1,
        ));

        let conv2d9_out1 = self.conv2d9.forward(relu9_out1);
        let slice9_out1 = conv2d9_out1.slice(s![.., .., 0..-2, ..]);
        let relu11_out1 = relu(norm_affine(
            &self.instancenormalization11,
            &self.constant69,
            &self.constant70,
            4,
            slice9_out1,
        ));

        let conv2d10_out1 = self.conv2d10.forward(relu11_out1);
        let slice10_out1 = conv2d10_out1.slice(s![.., .., 0..-2, ..]);
        let relu12_out1 = relu(norm_affine(
            &self.instancenormalization12,
            &self.constant71,
            &self.constant72,
            4,
            slice10_out1,
        ));

        // Channel-mixing bottleneck (384→24→384) with a residual back to relu12.
        let linear3_out1 = self.linear3.forward(relu12_out1.clone());
        let relu13_out1 = relu(norm_affine(
            &self.instancenormalization13,
            &self.constant74,
            &self.constant75,
            4,
            linear3_out1,
        ));
        let linear4_out1 = self.linear4.forward(relu13_out1);
        let relu14_out1 = relu(norm_affine(
            &self.instancenormalization14,
            &self.constant77,
            &self.constant78,
            4,
            linear4_out1,
        ));
        let add17_out1 = relu12_out1.add(relu14_out1);

        let conv2d11_out1 = self.conv2d11.forward(add17_out1);
        let slice11_out1 = conv2d11_out1.slice(s![.., .., 0..-2, ..]);
        let relu15_out1 = relu(norm_affine(
            &self.instancenormalization15,
            &self.constant79,
            &self.constant80,
            4,
            slice11_out1,
        ));

        let conv2d12_out1 = self.conv2d12.forward(relu15_out1);
        let slice12_out1 = conv2d12_out1.slice(s![.., .., 0..-2, ..]);
        let relu16_out1 = relu(norm_affine(
            &self.instancenormalization16,
            &self.constant81,
            &self.constant82,
            4,
            slice12_out1,
        ));

        relu16_out1.add(relu10_out1)
    }
}
