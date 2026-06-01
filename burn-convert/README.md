# burn-convert

此仓库用来调研burn-onnx能否胜任网络推理工作。包含 ：

- 格式转换。看看能不能转，缺不缺算子。
- 运行对比。用ort和burn分别运行，对比结果和精度，确保推理正确。
- 速度对比。用ort+coreml和burn分别运行，发现还是coreml快。

## Motivation

ort还要自己编译onnxruntime才能启用QNN/XNNPACK，在安卓上甚至无法用vulkan加速，比较麻烦，因此决定用更统一的burn。
