//! Compileert ../../proto met protox (pure Rust, geen protoc) en genereert tonic-code.
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../proto");
    let files = [
        "vortex/common/v1/common.proto",
        "vortex/worker/v1/worker.proto",
        "vortex/gui/v1/gui.proto",
        "vortex/admin/v1/admin.proto",
        "vortex/sandbox/v1/sandbox.proto",
    ];
    for f in &files {
        println!("cargo:rerun-if-changed={}", root.join(f).display());
    }
    let fds = protox::compile(files, [&root])?;
    tonic_build::configure().build_server(true).build_client(true).compile_fds(fds)?;
    Ok(())
}
