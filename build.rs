/// Build script: converts the RT-DTT ONNX model into Burn Rust code at build time.
///
/// The ONNX model path defaults to the developer location but can be overridden
/// with the `ALL_RT_ONNX_PATH` environment variable at build time:
///
/// ```sh
/// ALL_RT_ONNX_PATH=/path/to/all_rt.onnx cargo build
/// ```
///
/// `burn-onnx` generates into `$OUT_DIR/model/`:
/// - `all_rt.rs`  — Rust source (include!-ed from vocal_remover.rs)
/// - `all_rt.bpk` — BurnPack weight file (loaded at runtime)
///
/// Without the model file, the build succeeds but vocal removal becomes a pass-through.
fn main() {
    let default_path = "assets/all_rt.onnx";
    let model_path = std::env::var("ALL_RT_ONNX_PATH").unwrap_or_else(|_| default_path.to_string());
    let vocal_removal_enabled = std::env::var_os("CARGO_FEATURE_VOCAL_REMOVAL").is_some();

    println!("cargo:rerun-if-env-changed=ALL_RT_ONNX_PATH");
    println!("cargo:rerun-if-changed={model_path}");
    println!("cargo::rustc-check-cfg=cfg(has_vocal_model)");

    if !vocal_removal_enabled {
        println!(
            "cargo:warning=VocalRemover: `vocal-removal` feature disabled. \
             VocalRemover will pass audio through unmodified."
        );
        return;
    }

    if !std::path::Path::new(&model_path).exists() {
        println!(
            "cargo:warning=VocalRemover: ONNX model not found at `{model_path}`. \
             Set ALL_RT_ONNX_PATH at build time to enable vocal removal."
        );
        return;
    }

    // Convert ONNX → Burn. Panics on failure → build error.
    burn_onnx::ModelGen::new()
        .input(&model_path)
        .out_dir("model/")
        .run_from_script();

    println!("cargo:rustc-cfg=has_vocal_model");
}
