#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(2)
        .expect("workspace root above packages/stress-test")
        .to_path_buf()
}

fn musl_binary(target: &str) -> PathBuf {
    workspace_root()
        .join("target")
        .join(target)
        .join("release")
        .join("broccoli-stress-test")
}

fn assert_file_static(binary: &Path) {
    let out = Command::new("file")
        .arg(binary)
        .output()
        .expect("`file` not in PATH");
    assert!(out.status.success(), "`file` failed: {out:?}");
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("statically linked") || s.contains("static-pie linked"),
        "expected statically linked binary, got: {s}"
    );
}

fn assert_ldd_static(binary: &Path) {
    let out = Command::new("ldd")
        .arg(binary)
        .output()
        .expect("`ldd` not in PATH");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("not a dynamic executable") || combined.contains("statically linked"),
        "expected ldd to report static, got: {combined}"
    );
}

fn alpine_help_smoke(binary: &Path, platform: &str) {
    let bin_dir = binary.parent().expect("binary has parent");
    let bin_name = binary.file_name().expect("binary has filename");
    let mount = format!("{}:/work", bin_dir.display());
    let inner = format!("/work/{}", bin_name.to_string_lossy());
    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--platform",
            platform,
            "-v",
            &mount,
            "alpine:3.18",
            &inner,
            "--help",
        ])
        .status()
        .expect("`docker` not in PATH");
    assert!(
        status.success(),
        "alpine smoke test failed for {}",
        binary.display()
    );
}

#[test]
#[ignore = "requires prebuilt musl binary and Docker; opt in with --ignored"]
fn musl_x86_64_binary_is_static() {
    let binary = musl_binary("x86_64-unknown-linux-musl");
    assert!(
        binary.exists(),
        "missing {}; run `just stress-test-linux-x86_64` first",
        binary.display()
    );
    assert_file_static(&binary);
    assert_ldd_static(&binary);
    alpine_help_smoke(&binary, "linux/amd64");
}

#[test]
#[ignore = "requires prebuilt musl binary and Docker; opt in with --ignored"]
fn musl_aarch64_binary_is_static() {
    let binary = musl_binary("aarch64-unknown-linux-musl");
    assert!(
        binary.exists(),
        "missing {}; run `just stress-test-linux-aarch64` first",
        binary.display()
    );
    assert_file_static(&binary);
    assert_ldd_static(&binary);
    alpine_help_smoke(&binary, "linux/arm64");
}
