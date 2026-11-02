//! Build-identiteit (git-commit) voor eval-versies.
fn main() {
    let rev = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "dev".into());
    println!("cargo:rustc-env=VORTEX_BUILD={rev}");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
}
