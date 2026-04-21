/// Build script: converts the RT-DTT ONNX model to burn-native Rust code.
///
/// The ONNX model path defaults to the developer location but can be overridden
/// with the `ALL_RT_ONNX_PATH` environment variable at build time:
///
/// ```sh
/// ALL_RT_ONNX_PATH=/path/to/all_rt.onnx cargo build
/// ```
///
/// When the file is found **and** burn-onnx can convert it, the script sets
/// `cfg(has_vocal_model)` so VocalRemover compiles with real GPU inference.
/// On any failure (missing file, unsupported opset, …) a cargo warning is
/// emitted and VocalRemover falls back to a pass-through node.
fn main() {
    let default_path = "/Users/ken/Codes/tmp/REALTIME_DTT/all_rt.onnx";
    let model_path = std::env::var("ALL_RT_ONNX_PATH")
        .unwrap_or_else(|_| default_path.to_string());

    println!("cargo:rerun-if-env-changed=ALL_RT_ONNX_PATH");
    println!("cargo:rerun-if-changed={model_path}");
    // Declare the cfg flag as expected so rustc doesn't warn about it.
    println!("cargo::rustc-check-cfg=cfg(has_vocal_model)");

    if !std::path::Path::new(&model_path).exists() {
        println!(
            "cargo:warning=VocalRemover: ONNX model not found at `{model_path}`. \
             Set ALL_RT_ONNX_PATH at build time to enable GPU vocal removal."
        );
        return;
    }

    // Wrap the conversion in catch_unwind so an unsupported opset or missing
    // op doesn't abort the entire build — VocalRemover simply falls back to
    // pass-through if conversion fails.
    let result = std::panic::catch_unwind(|| {
        burn_onnx::ModelGen::new()
            .input(&model_path)
            // Embed weights in the binary — no external .bpk file needed at runtime.
            .load_strategy(burn_onnx::LoadStrategy::Embedded)
            .out_dir("vocal_model/")
            .run_from_script();
    });

    match result {
        Ok(()) => {
            println!("cargo:rustc-cfg=has_vocal_model");
        }
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("unknown error");
            println!(
                "cargo:warning=VocalRemover: ONNX conversion failed: {msg}. \
                 VocalRemover will pass audio through unmodified."
            );
        }
    }
}
