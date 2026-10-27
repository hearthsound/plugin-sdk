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
        stdout.starts_with("ok: com.example.hello-catalog 0.2.0"),
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
        stderr.contains("hearthsound:plugin/metadata@0.5.0"),
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

    let archive_path = out_dir.join("com.example.hello-catalog-0.2.0.hsplugin");
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

/// Writes a small template tree under `CARGO_TARGET_TMPDIR` and returns its path.
///
/// The tree holds a manifest with keys out of canonical order and one source
/// file, and a stale `target` build directory; callers add a `.git` entry when they need one.
fn template_dir(name: &str) -> PathBuf {
    let dir = out_dir(name);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("target/debug")).unwrap();
    std::fs::write(dir.join("target/debug/stale.wasm"), "stale").unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// template\n").unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        r#"{"zeta": 1, "id": "com.example.template", "name": "Template", "version": "0.1.0", "alpha": 2}"#,
    )
    .unwrap();
    dir
}

/// Returns a fresh, absent directory under `CARGO_TARGET_TMPDIR`.
fn out_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Runs `hsplugin new` with `args`.
fn new_plugin(args: &[&str]) -> Output {
    Command::new(HSPLUGIN)
        .arg("new")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn new_copies_template_and_rewrites_manifest() {
    let template = template_dir("new-template");
    std::fs::create_dir_all(template.join(".git")).unwrap();
    let out = out_dir("new-copy");
    let output = new_plugin(&[
        "--lang",
        "rust",
        "--template",
        arg(&template),
        "--out",
        arg(&out),
        "--id",
        "com.example.scratch",
        "--name",
        "Scratch",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.join("src/lib.rs").is_file());
    assert!(!out.join(".git").exists());
    assert!(!out.join("target").exists());

    let text = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest["id"], "com.example.scratch");
    assert_eq!(manifest["name"], "Scratch");
    assert_eq!(manifest["version"], "1.0.0");
    let keys: Vec<&str> = manifest
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, ["zeta", "id", "name", "version", "alpha"]);
}

#[test]
fn new_clones_git_template_url() {
    let template = template_dir("new-git-template");
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .current_dir(&template)
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(args)
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?} failed");
    };
    git(&["init", "-q"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "template"]);

    let out = out_dir("new-git-copy");
    let url = format!("file://{}", arg(&template));
    let output = new_plugin(&[
        "--lang",
        "go",
        "--template",
        &url,
        "--out",
        arg(&out),
        "--id",
        "com.example.cloned",
        "--name",
        "Cloned",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(out.join("src/lib.rs").is_file());
    assert!(!out.join(".git").exists());
    let text = std::fs::read_to_string(out.join("manifest.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest["id"], "com.example.cloned");
    assert_eq!(manifest["version"], "1.0.0");
}

#[test]
fn new_refuses_a_non_empty_destination() {
    let template = template_dir("new-busy-template");
    let out = out_dir("new-busy");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "mine").unwrap();
    let output = new_plugin(&[
        "--lang",
        "rust",
        "--template",
        arg(&template),
        "--out",
        arg(&out),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not empty"));
    assert_eq!(
        std::fs::read_to_string(out.join("keep.txt")).unwrap(),
        "mine"
    );
    assert!(!out.join("manifest.json").exists());
}

#[test]
fn new_refuses_an_invalid_id() {
    let template = template_dir("new-badid-template");
    let out = out_dir("new-badid");
    let output = new_plugin(&[
        "--lang",
        "rust",
        "--template",
        arg(&template),
        "--out",
        arg(&out),
        "--id",
        "Bad_ID",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("not a valid plugin id"));
    assert!(!out.exists());
}

#[test]
fn new_leaves_no_destination_when_the_template_has_no_manifest() {
    let template = out_dir("new-empty-template");
    std::fs::create_dir_all(&template).unwrap();
    std::fs::write(template.join("README.md"), "no manifest here\n").unwrap();
    let out = out_dir("new-no-manifest");
    let output = new_plugin(&[
        "--lang",
        "js",
        "--template",
        arg(&template),
        "--out",
        arg(&out),
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("manifest.json"));
    assert!(!out.exists());
}

#[test]
fn build_compiles_a_rust_project() {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello");
    let output = Command::new(HSPLUGIN)
        .arg("build")
        .arg("--manifest")
        .arg(repo_root().join("examples/hello-catalog/manifest.json"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("hsplugin runs");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("built {}", example_wasm().display())
    );
}

#[test]
fn build_refuses_a_project_without_a_marker() {
    let dir = out_dir("build-no-marker");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("manifest.json"), "{}").unwrap();
    let output = hsplugin(&["build", "--manifest", arg(&dir.join("manifest.json"))]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no Cargo.toml"));
}

/// Writes `text` to a scratch file whose name is derived from `label` and the
/// text, so parallel tests never share a file, and returns its path.
fn scratch_json(label: &str, text: &str) -> PathBuf {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("run");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{label}-{:x}.json", hasher.finish()));
    std::fs::write(&path, text).unwrap();
    path
}

/// Runs `hsplugin run <target>` against the example plugin, with optional
/// config and input JSON, and returns the parsed JSON printed on stdout.
fn run_example(target: &str, config: Option<&str>, input: Option<&str>) -> serde_json::Value {
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let mut args: Vec<String> = vec![
        "run".into(),
        target.into(),
        "--manifest".into(),
        arg(&manifest).into(),
        "--wasm".into(),
        arg(example_wasm()).into(),
    ];
    if let Some(text) = config {
        args.push("--config".into());
        args.push(arg(&scratch_json("config", text)).into());
    }
    if let Some(text) = input {
        args.push("--input".into());
        args.push(arg(&scratch_json("input", text)).into());
    }
    let output = Command::new(HSPLUGIN)
        .args(&args)
        .output()
        .expect("hsplugin runs");
    assert!(
        output.status.success(),
        "hsplugin run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn run_health_reports_a_string() {
    let value = run_example("lifecycle.health", None, None);
    assert!(value["ok"].is_string(), "unexpected output: {value}");
}

#[test]
fn run_validate_config_accepts_a_valid_document() {
    let value = run_example(
        "lifecycle.validate-config",
        None,
        Some(r#""{\"greeting\":\"Hi\"}""#),
    );
    assert!(value.get("ok").is_some(), "unexpected output: {value}");
    assert!(value.get("error").is_none(), "unexpected output: {value}");
}

#[test]
fn run_validate_config_rejects_invalid_json() {
    let value = run_example("lifecycle.validate-config", None, Some(r#""not json""#));
    let errors = value["error"].as_array().expect("error list");
    assert!(!errors.is_empty(), "unexpected output: {value}");
}

#[test]
fn run_search_round_trips_the_configured_greeting() {
    let value = run_example(
        "catalog.search",
        Some(r#"{"greeting":"Hi"}"#),
        Some(r#"{"text":"world","kinds":["track"],"limit":5,"offset":0}"#),
    );
    let track = &value["ok"]["tracks"][0];
    assert_eq!(track["id"], "hello-track-1", "unexpected output: {value}");
    assert_eq!(track["title"], "Hi, world", "unexpected output: {value}");
}

#[test]
fn run_get_track_returns_the_requested_id() {
    let value = run_example("catalog.get-track", None, Some(r#""hello-track-1""#));
    assert_eq!(
        value["ok"]["id"], "hello-track-1",
        "unexpected output: {value}"
    );
}

#[test]
fn run_unknown_track_yields_a_not_found_error() {
    let value = run_example("catalog.get-track", None, Some(r#""missing""#));
    assert_eq!(value, serde_json::json!({"error": "not_found"}));
}

#[test]
fn test_catalog_suite_passes_on_the_example() {
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let wasm = example_wasm();
    let output = hsplugin(&[
        "test",
        "catalog",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(wasm),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "unexpected failure: {stdout}");
    assert!(stdout.contains("ok 1: validate-config accepts the configuration"));
    assert!(stdout.contains("ok 12: an empty search returns empty lists"));
    assert!(
        !stdout.contains("error "),
        "unexpected error line: {stdout}"
    );
}

#[test]
fn test_suite_reports_numbered_failures_and_exits_one() {
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let wasm = example_wasm();
    let output = hsplugin(&[
        "test",
        "metadata",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(wasm),
    ]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout.contains("error 1: resolve-recording"),
        "missing numbered failure: {stdout}"
    );
}

#[test]
fn test_unknown_layer_is_rejected() {
    let manifest = repo_root().join("examples/hello-catalog/manifest.json");
    let output = hsplugin(&["test", "bogus", "--manifest", arg(&manifest)]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr.contains("no conformance suite for `bogus`"),
        "unexpected stderr: {stderr}"
    );
}

/// Returns the path of the process-acquisition fixture's manifest.
fn acquisition_fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/process-acquisition/manifest.json")
}

/// Builds the process-acquisition fixture once and returns its component.
fn acquisition_fixture_wasm() -> &'static Path {
    static WASM: LazyLock<PathBuf> = LazyLock::new(|| {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("process-acquisition");
        let status = Command::new(cargo())
            .arg("build")
            .arg("--release")
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--manifest-path")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/process-acquisition/Cargo.toml"),
            )
            .arg("--target-dir")
            .arg(&target)
            .status()
            .expect("cargo build runs");
        assert!(status.success(), "building the fixture plugin failed");
        target.join("wasm32-wasip2/release/process_acquisition_fixture.wasm")
    });
    &WASM
}

/// Writes an executable shell script that records that it ran and returns its
/// path.
///
/// The script writes the marker file named `marker`, so a test proves the mock
/// host started the tool rather than only accepting the mapping.
#[cfg(unix)]
fn marker_tool(marker: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let script = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello-tool.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf ran > {}\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(unix)]
#[test]
fn test_acquisition_suite_runs_a_mapped_tool() {
    let marker = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hello-tool-ran");
    let _ = std::fs::remove_file(&marker);
    let tool = marker_tool(&marker);
    let manifest = acquisition_fixture_manifest();
    let output = hsplugin(&[
        "test",
        "acquisition",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(acquisition_fixture_wasm()),
        "--tool",
        &format!("hello-tool={}", arg(&tool)),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "unexpected failure: {stdout}");
    assert!(
        stdout.contains("ok 2: start-search returns a token"),
        "unexpected stdout: {stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "ran",
        "the mapped tool did not run"
    );
}

#[cfg(unix)]
#[test]
fn test_acquisition_suite_without_a_tool_mapping_fails_the_start_check() {
    let manifest = acquisition_fixture_manifest();
    let output = hsplugin(&[
        "test",
        "acquisition",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(acquisition_fixture_wasm()),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stdout.contains("error 2: start-search returns a token: ")
            && stdout.contains("provider error invalid_config"),
        "unexpected stdout: {stdout}"
    );
}

/// Returns the path of the album-only fixture's manifest.
fn album_fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/album-acquisition/manifest.json")
}

/// Builds the album-only fixture once and returns its component.
fn album_fixture_wasm() -> &'static Path {
    static WASM: LazyLock<PathBuf> = LazyLock::new(|| {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("album-acquisition");
        let status = Command::new(cargo())
            .arg("build")
            .arg("--release")
            .arg("--target")
            .arg("wasm32-wasip2")
            .arg("--manifest-path")
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/album-acquisition/Cargo.toml"),
            )
            .arg("--target-dir")
            .arg(&target)
            .status()
            .expect("cargo build runs");
        assert!(status.success(), "building the album fixture failed");
        target.join("wasm32-wasip2/release/album_acquisition_fixture.wasm")
    });
    &WASM
}

#[test]
fn test_acquisition_suite_searches_an_album_only_plugin() {
    let manifest = album_fixture_manifest();
    let output = hsplugin(&[
        "test",
        "acquisition",
        "--manifest",
        arg(&manifest),
        "--wasm",
        arg(album_fixture_wasm()),
    ]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "unexpected failure: {stdout}");
    assert!(
        stdout.contains("ok 1: get-capabilities lists granularities"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        stdout.contains("ok 2: start-search returns a token"),
        "unexpected stdout: {stdout}"
    );
    assert!(
        !stdout.contains("error "),
        "unexpected error line: {stdout}"
    );
}
