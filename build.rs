/// Build script: embeds the RT-DTT ONNX model into the binary when available.
///
/// The ONNX model path defaults to the developer location but can be overridden
/// with the `ALL_RT_ONNX_PATH` environment variable at build time:
///
/// ```sh
/// ALL_RT_ONNX_PATH=/path/to/all_rt.onnx cargo build
/// ```
///
/// When the file is found, the script sets `cfg(has_vocal_model)` so VocalRemover
/// compiles with real inference via ONNX Runtime (ort).
/// On any failure (missing file, …) a cargo warning is emitted and VocalRemover
/// falls back to a pass-through node.
fn main() {
    let default_path = "assets/all_rt.onnx";
    let model_path = std::env::var("ALL_RT_ONNX_PATH").unwrap_or_else(|_| default_path.to_string());

    println!("cargo:rerun-if-env-changed=ALL_RT_ONNX_PATH");
    println!("cargo:rerun-if-changed={model_path}");
    println!("cargo::rustc-check-cfg=cfg(has_vocal_model)");

    if !std::path::Path::new(&model_path).exists() {
        println!(
            "cargo:warning=VocalRemover: ONNX model not found at `{model_path}`. \
             Set ALL_RT_ONNX_PATH at build time to enable vocal removal."
        );
        return;
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let dest = std::path::Path::new(&out_dir).join("all_rt.onnx");

    match std::fs::copy(&model_path, &dest) {
        Ok(_) => {
            println!("cargo:rustc-cfg=has_vocal_model");
        }
        Err(e) => {
            println!(
                "cargo:warning=VocalRemover: failed to copy ONNX model to OUT_DIR: {e}. \
                 VocalRemover will pass audio through unmodified."
            );
        }
    }
}
