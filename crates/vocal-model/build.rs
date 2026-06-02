fn main() {
    println!("cargo::rustc-check-cfg=cfg(has_vocal_model)");
    println!("cargo:rerun-if-changed=model/all_rt.rs");
    println!("cargo:rerun-if-changed=model/all_rt.bpk");

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let model_rs = std::path::PathBuf::from(&manifest_dir).join("model/all_rt.rs");
    let model_bpk = std::path::PathBuf::from(&manifest_dir).join("model/all_rt.bpk");

    if model_rs.exists() && model_bpk.exists() {
        println!("cargo:rustc-cfg=has_vocal_model");
    } else {
        println!(
            "cargo:warning=VocalRemover: pre-committed model files not found in crates/vocal-model/model/. \
             VocalRemover will pass audio through unmodified. To regenerate from ONNX, run: \
             ALL_RT_ONNX_PATH=assets/all_rt.onnx cargo run --bin regen-vocal-model"
        );
    }
}
