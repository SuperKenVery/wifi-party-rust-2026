fn main() {
    let model_path = std::env::var("ALL_RT_ONNX_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("../../assets/all_rt.onnx")
        });

    println!("cargo:rerun-if-env-changed=ALL_RT_ONNX_PATH");
    println!("cargo:rerun-if-changed={}", model_path.display());
    println!("cargo::rustc-check-cfg=cfg(has_vocal_model)");

    if !model_path.exists() {
        println!(
            "cargo:warning=VocalRemover: ONNX model not found at `{}`. \
             VocalRemover will pass audio through unmodified.",
            model_path.display()
        );
        return;
    }

    burn_onnx::ModelGen::new()
        .input(model_path.to_string_lossy().as_ref())
        .out_dir("model/")
        .run_from_script();

    println!("cargo:rustc-cfg=has_vocal_model");
}
