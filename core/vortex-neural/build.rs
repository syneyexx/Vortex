//! ONNX-subset (proto/onnx/onnx-subset.proto) → Rust-types via protox (geen protoc nodig).
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let f = "onnx/onnx-subset.proto";
    println!("cargo:rerun-if-changed={}", root.join(f).display());
    let fds = protox::compile([f], [&root])?;
    prost_build::Config::new().compile_fds(fds)?;
    Ok(())
}
