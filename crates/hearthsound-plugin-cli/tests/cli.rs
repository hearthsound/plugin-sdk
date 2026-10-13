//! End-to-end tests for the `hsplugin` binary.
//!
//! The tests build the example plugin once per test binary into
//! `CARGO_TARGET_TMPDIR`, then run the real binary against it and against
//! deliberately broken copies of its manifest.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::LazyLock;

/// The `hsplugin` binary under test.
const HSPLUGIN: &str = env!("CARGO_BIN_EXE_hsplugin");

/// Returns the plugin-sdk repository root.
fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

/// Returns the cargo binary to drive sub-builds with.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

/// Builds the example plugin once and returns the path of its component.
fn example_wasm() -> &'static Path {
    static WASM: LazyLock<PathBuf> = LazyLock::new(|| {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello");
        let status = Command::new(cargo())
            .arg("build")
            .arg("--release")
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--manifest-path")
            .arg(repo_root().join("examples/hello-catalog/Cargo.toml"))
            .arg("--target-dir")
            .arg(&target)
            .status()
            .expect("cargo build runs");
        assert!(status.success(), "building the example plugin failed");
        target.join("wasm32-wasip2/release/hello_catalog.wasm")
    });
    &WASM
}

/// Runs `hsplugin` with `args`.
fn hsplugin(args: &[&str]) -> Output {
    Command::new(HSPLUGIN)
        .args(args)
        .output()
        .expect("hsplugin runs")
}

/// Writes a mutated copy of the example manifest under `CARGO_TARGET_TMPDIR`.
fn manifest_copy(name: &str, mutate: impl FnOnce(String) -> String) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let text =
        std::fs::read_to_string(repo_root().join("examples/hello-catalog/manifest.json")).unwrap();
    let path = dir.join("manifest.json");
    std::fs::write(&path, mutate(text)).unwrap();
    path
}

/// Returns an argument string for a path.
fn arg(path: &Path) -> &str {
    path.to_str().unwrap()
}

#[test]
fn example_manifest_is_valid() {
    let wasm = example_wasm();
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let output = hsplugin(&["check", "--manifest", arg(&manifest), "--wasm", arg(wasm)]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "hsplugin failed: {stderr}");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.starts_with("ok: com.example.hello-catalog 0.1.0"),
        "unexpected stdout: {stdout}"
    );
}

#[test]
fn mismatched_layer_is_rejected() {
    let wasm = example_wasm();
    let manifest = manifest_copy("metadata-layer", |text| {
        text.replace(
            "\"provides\": [\"catalog\"]",
            "\"provides\": [\"metadata\"]",
        )
    });
    let output = hsplugin(&["check", "--manifest", arg(&manifest), "--wasm", arg(wasm)]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "hsplugin unexpectedly succeeded");
    assert!(
        stderr.contains("hearthsound:plugin/metadata@0.1.0"),
        "unexpected stderr: {stderr}"
    );
}

#[test]
fn invalid_id_is_rejected() {
    let wasm = example_wasm();
    let manifest = manifest_copy("bad-id", |text| {
        text.replace(
            "\"id\": \"com.example.hello-catalog\"",
            "\"id\": \"Bad Id\"",
        )
    });
    let output = hsplugin(&["check", "--manifest", arg(&manifest), "--wasm", arg(wasm)]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "hsplugin unexpectedly succeeded");
    assert!(stderr.contains("/id"), "unexpected stderr: {stderr}");
}

#[test]
fn pack_writes_archive() {
    let wasm = example_wasm();
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pack-out");
    let output = hsplugin(&[
        "pack",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(wasm),
        "--out-dir",
        arg(&out_dir),
    ]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "hsplugin pack failed: {stderr}");

    let archive_path = out_dir.join("com.example.hello-catalog-0.1.0.hsplugin");
    assert!(
        archive_path.is_file(),
        "archive missing at {}",
        archive_path.display()
    );

    let file = std::fs::File::open(&archive_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();

    let mut names = Vec::new();
    for index in 0..archive.len() {
        names.push(archive.by_index(index).unwrap().name().unwrap().to_string());
    }
    assert!(
        names.iter().any(|name| name == "manifest.json"),
        "{names:?}"
    );
    assert!(names.iter().any(|name| name == "plugin.wasm"), "{names:?}");

    let mut packed_wasm = Vec::new();
    std::io::Read::read_to_end(
        &mut archive.by_name("plugin.wasm").unwrap(),
        &mut packed_wasm,
    )
    .unwrap();
    assert_eq!(packed_wasm, std::fs::read(wasm).unwrap());

    let mut packed_manifest = Vec::new();
    std::io::Read::read_to_end(
        &mut archive.by_name("manifest.json").unwrap(),
        &mut packed_manifest,
    )
    .unwrap();
    assert_eq!(packed_manifest, std::fs::read(&manifest).unwrap());
}
