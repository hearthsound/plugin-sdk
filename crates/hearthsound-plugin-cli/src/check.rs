//! Shared validation used by `hsplugin check`.
//!
//! Validation runs in five passes: the manifest schema, the plugin API
//! version, the nested configuration schemas, compilation of the component, and
//! the component rules. Every pass appends to the same error list so a broken
//! plugin reports all of its problems at once.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::component;
use crate::manifest::Manifest;

/// The manifest schema, embedded at build time.
const MANIFEST_SCHEMA: &str = include_str!("../../../schema/manifest.schema.json");

/// A manifest and component that passed every check.
#[derive(Debug)]
pub struct Checked {
    manifest: Manifest,
}

impl Checked {
    /// Returns the one-line summary printed on success.
    ///
    /// The summary is `<id> <version> (provides <layers>)`.
    pub fn summary(&self) -> String {
        format!(
            "{} {} (provides {})",
            self.manifest.id(),
            self.manifest.version(),
            self.manifest.provides().join(", ")
        )
    }
}

/// Validates a manifest and its component.
///
/// # Parameters
///
/// - `manifest_path`: path to the plugin's `manifest.json`.
/// - `wasm`: path to the component; when absent it is derived from the
///   manifest's directory and the cargo release output.
///
/// # Errors
///
/// Returns every problem found, as display-ready strings without the `error: `
/// prefix.
pub fn run(manifest_path: &Path, wasm: Option<&Path>) -> Result<Checked, Vec<String>> {
    let mut errors = Vec::new();

    let bytes = match std::fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => return Err(vec![format!("{}: {error}", manifest_path.display())]),
    };

    let manifest = match Manifest::parse(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => return Err(vec![error]),
    };

    check_schema(manifest.value(), &mut errors);
    check_api(manifest.value(), &mut errors);
    check_config_schemas(manifest.value(), &mut errors);

    let wasm_path = match wasm {
        Some(path) => path.to_path_buf(),
        None => match resolve_wasm(manifest_path) {
            Ok(path) => path,
            Err(error) => {
                errors.push(error);
                return Err(errors);
            }
        },
    };

    let engine = wasmtime::Engine::default();
    match wasmtime::component::Component::from_file(&engine, &wasm_path) {
        Ok(compiled) => {
            component::check(&engine, &compiled, &manifest.provides(), &mut errors);
        }
        Err(error) => errors.push(format!("{}: {error:#}", wasm_path.display())),
    }

    if errors.is_empty() {
        Ok(Checked { manifest })
    } else {
        Err(errors)
    }
}

/// Validates the manifest against the embedded draft-07 schema.
fn check_schema(instance: &Value, errors: &mut Vec<String>) {
    let schema: Value = match serde_json::from_str(MANIFEST_SCHEMA) {
        Ok(schema) => schema,
        Err(error) => {
            errors.push(format!("embedded manifest schema is not JSON: {error}"));
            return;
        }
    };
    let validator = match jsonschema::draft7::new(&schema) {
        Ok(validator) => validator,
        Err(error) => {
            errors.push(format!("embedded manifest schema is invalid: {error}"));
            return;
        }
    };
    for error in validator.iter_errors(instance) {
        errors.push(format!(
            "manifest {}: {error}",
            pointer(&error.instance_path().to_string())
        ));
    }
}

/// Checks the plugin API version against the one this tool supports.
fn check_api(instance: &Value, errors: &mut Vec<String>) {
    if let Some(api) = instance.get("api").and_then(Value::as_str)
        && api != component::supported_api()
    {
        errors.push(format!(
            "manifest /api: unsupported api `{api}`, expected `{}`",
            component::supported_api()
        ));
    }
}

/// Compiles every non-null configuration schema as a draft-07 schema.
fn check_config_schemas(instance: &Value, errors: &mut Vec<String>) {
    for section in ["server", "user"] {
        let Some(entry) = instance
            .get("config")
            .and_then(|config| config.get(section))
        else {
            continue;
        };
        if entry.is_null() {
            continue;
        }
        let Some(schema) = entry.get("schema") else {
            continue;
        };
        if let Err(error) = jsonschema::draft7::new(schema) {
            errors.push(format!("manifest /config/{section}/schema: {error}"));
        }
    }
}

/// Derives the component path from the manifest's directory.
///
/// The component is expected at
/// `<manifest dir>/<target dir>/wasm32-wasip2/release/<package name>.wasm`,
/// where `<target dir>` is `CARGO_TARGET_DIR` when set and `target` otherwise,
/// and the package name comes from the `package.name` of the `Cargo.toml` next
/// to the manifest.
///
/// # Errors
///
/// Returns a message when the `Cargo.toml` cannot be read or parsed, when it
/// has no `package.name`, or when the component file does not exist.
fn resolve_wasm(manifest_path: &Path) -> Result<PathBuf, String> {
    let manifest_dir = manifest_path.parent().unwrap_or(Path::new("."));
    let cargo_toml = manifest_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&cargo_toml)
        .map_err(|error| format!("{}: {error}", cargo_toml.display()))?;
    let value: toml::Value =
        toml::from_str(&text).map_err(|error| format!("{}: {error}", cargo_toml.display()))?;
    let name = value
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("{}: no package.name", cargo_toml.display()))?;

    let target_dir = match std::env::var_os("CARGO_TARGET_DIR") {
        Some(dir) => {
            let dir = PathBuf::from(dir);
            if dir.is_absolute() {
                dir
            } else {
                manifest_dir.join(dir)
            }
        }
        None => manifest_dir.join("target"),
    };
    let file = format!("{}.wasm", name.replace('-', "_"));
    let path = target_dir.join("wasm32-wasip2").join("release").join(file);

    if path.is_file() {
        Ok(path)
    } else {
        Err(format!(
            "component not found at {}; build it with cargo build --release --target wasm32-wasip2 or pass --wasm",
            path.display()
        ))
    }
}

/// Renders an empty JSON pointer as `/`.
fn pointer(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}
