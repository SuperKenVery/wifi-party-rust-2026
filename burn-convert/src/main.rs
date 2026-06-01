fn main() {
    burn_onnx::ModelGen::new()
        .input("../assets/all_rt.onnx")
        .out_dir("src/model/")
        .run_from_cli();
}
