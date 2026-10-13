//! The component rules: which imports and exports a plugin component may have.
//!
//! A plugin is a WebAssembly component. Its imports must stay inside the set
//! the Hearthsound runtime can satisfy, and its exports must match the layers
//! its manifest declares. The rules are checked here, at install time and by
//! `hsplugin check`, so a plugin that passes packaging is a plugin the server
//! can load.

use wasmtime::Engine;
use wasmtime::component::Component;
use wasmtime::component::types::ComponentItem;

/// Version of the `hearthsound:plugin` package every plugin targets.
const API_VERSION: &str = "0.1.0";

/// Host interfaces every plugin may import.
const COMMON_HOST_INTERFACES: [&str; 5] = [
    "hearthsound:plugin/types",
    "hearthsound:plugin/host-log",
    "hearthsound:plugin/host-http",
    "hearthsound:plugin/host-kv",
    "hearthsound:plugin/host-config",
];

/// Host interface reserved for acquisition plugins.
const HOST_FILES: &str = "hearthsound:plugin/host-files";

/// The supported plugin API version, as it appears in a manifest.
const SUPPORTED_API: &str = "hearthsound:plugin@0.1.0";

/// WASI packages plugins may import, provided by `wasmtime-wasi`.
const WASI_PACKAGES: [&str; 6] = [
    "wasi:io",
    "wasi:clocks",
    "wasi:random",
    "wasi:cli",
    "wasi:filesystem",
    "wasi:sockets",
];

/// Provider interfaces a component may export, mapped to manifest layers.
const CAPABILITY_INTERFACES: [&str; 3] = ["catalog", "metadata", "acquisition"];

/// The interface every plugin must export.
const LIFECYCLE_INTERFACE: &str = "lifecycle";

/// Checks a compiled component against the component rules.
///
/// # Parameters
///
/// - `engine`: engine the component was compiled with.
/// - `component`: the compiled component.
/// - `provides`: the layers the manifest declares.
/// - `errors`: collects one message per violation; left untouched when the
///   component is valid.
pub fn check(
    engine: &Engine,
    component: &Component,
    provides: &[String],
    errors: &mut Vec<String>,
) {
    let ty = component.component_type();

    for (name, ext) in ty.imports(engine) {
        if !matches!(ext.ty, ComponentItem::ComponentInstance(_)) {
            errors.push(format!("component import `{name}` is not an interface"));
            continue;
        }
        let (interface, version) = split_version(name);
        if !import_allowed(interface, version, provides) {
            errors.push(format!("component imports unsupported interface `{name}`"));
        }
    }

    let mut exports_lifecycle = false;
    let mut exported_layers: Vec<String> = Vec::new();
    for (name, ext) in ty.exports(engine) {
        if !matches!(ext.ty, ComponentItem::ComponentInstance(_)) {
            errors.push(format!("component export `{name}` is not an interface"));
            continue;
        }
        let (interface, version) = split_version(name);
        let layer = interface.strip_prefix("hearthsound:plugin/");
        match layer {
            Some(lifecycle)
                if version == API_VERSION
                    && (lifecycle == LIFECYCLE_INTERFACE
                        || CAPABILITY_INTERFACES.contains(&lifecycle)) =>
            {
                if lifecycle == LIFECYCLE_INTERFACE {
                    exports_lifecycle = true;
                } else {
                    exported_layers.push(lifecycle.to_string());
                }
            }
            _ => errors.push(format!(
                "component export `{name}` is not a supported hearthsound:plugin interface"
            )),
        }
    }

    if !exports_lifecycle {
        errors.push(format!(
            "component does not export `hearthsound:plugin/{LIFECYCLE_INTERFACE}@{API_VERSION}`"
        ));
    }

    for layer in provides {
        if !exported_layers.iter().any(|exported| exported == layer) {
            errors.push(format!(
                "component does not export `hearthsound:plugin/{layer}@{API_VERSION}` for the `{layer}` layer it provides"
            ));
        }
    }

    for layer in &exported_layers {
        if !provides.iter().any(|provided| provided == layer) {
            errors.push(format!(
                "component exports `hearthsound:plugin/{layer}@{API_VERSION}` but the manifest does not declare the `{layer}` layer"
            ));
        }
    }
}

/// Splits an interface name into its interface id and version.
///
/// The version is everything after the last `@`, and the empty string when the
/// name carries no version.
fn split_version(name: &str) -> (&str, &str) {
    match name.rsplit_once('@') {
        Some((interface, version)) => (interface, version),
        None => (name, ""),
    }
}

/// Returns whether an import is inside the allowed set.
///
/// # Parameters
///
/// - `interface`: interface id, without its version.
/// - `version`: interface version.
/// - `provides`: layers the manifest declares.
fn import_allowed(interface: &str, version: &str, provides: &[String]) -> bool {
    if version == API_VERSION
        && (COMMON_HOST_INTERFACES.contains(&interface)
            || (interface == HOST_FILES
                && provides.iter().any(|provided| provided == "acquisition")))
    {
        return true;
    }
    if let Some((package, _)) = interface.split_once('/')
        && WASI_PACKAGES.contains(&package)
        && version.starts_with("0.2.")
    {
        return true;
    }
    false
}

/// The plugin API version supported by this tool.
pub fn supported_api() -> &'static str {
    SUPPORTED_API
}
