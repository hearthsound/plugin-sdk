//! The component rules: which imports and exports a plugin component may have.
//!
//! A plugin is a WebAssembly component. Its imports must stay inside the set
//! the Hearthsound runtime can satisfy, and its exports must match the layers
//! its manifest declares. The rules are checked here, at install time and by
//! `hsplugin check`, so a plugin that passes packaging is a plugin the server
//! can load.

use serde_json::Value;
use wasmtime::Engine;
use wasmtime::component::Component;
use wasmtime::component::types::ComponentItem;

/// Version of the `hearthsound:plugin` package every plugin targets.
pub(crate) const API_VERSION: &str = "0.5.0";

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

/// Host interface reserved for acquisition plugins that run configured tools.
const HOST_PROCESS: &str = "hearthsound:plugin/host-process";

/// The supported plugin API version, as it appears in a manifest.
const SUPPORTED_API: &str = "hearthsound:plugin@0.5.0";

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
const CAPABILITY_INTERFACES: [&str; 7] = [
    "catalog",
    "metadata",
    "artwork",
    "acquisition",
    "verification",
    "scrobble",
    "discovery",
];

/// The capability a plugin exports alongside `scrobble` or `discovery`; it is
/// not a layer of its own.
const ACCOUNT_LINK_INTERFACE: &str = "account-link";

/// The interface every plugin must export.
const LIFECYCLE_INTERFACE: &str = "lifecycle";

/// Checks a compiled component against the component rules.
///
/// # Parameters
///
/// - `engine`: engine the component was compiled with.
/// - `component`: the compiled component.
/// - `manifest`: the parsed manifest; its `provides` list and its process tool
///   declarations decide which imports and exports are allowed.
/// - `errors`: collects one message per violation; left untouched when the
///   component is valid.
pub fn check(engine: &Engine, component: &Component, manifest: &Value, errors: &mut Vec<String>) {
    let ty = component.component_type();
    let provides = provides(manifest);
    let tools = process_tools(manifest);

    for (name, ext) in ty.imports(engine) {
        if !matches!(ext.ty, ComponentItem::ComponentInstance(_)) {
            errors.push(format!("component import `{name}` is not an interface"));
            continue;
        }
        let (interface, version) = split_version(name);
        if version == API_VERSION
            && interface == HOST_PROCESS
            && !process_allowed(&provides, &tools)
        {
            errors.push(format!(
                "component imports `{name}` without listing a tool under permissions.process.tools"
            ));
            continue;
        }
        if !import_allowed(interface, version, &provides, &tools) {
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
        let item = interface.strip_prefix("hearthsound:plugin/");
        if item.is_some() && version != API_VERSION {
            errors.push(format!(
                "component export `{name}` is not built for `hearthsound:plugin@{API_VERSION}`"
            ));
            continue;
        }
        match item {
            Some(LIFECYCLE_INTERFACE) => exports_lifecycle = true,
            Some(layer) if CAPABILITY_INTERFACES.contains(&layer) => {
                exported_layers.push(layer.to_string());
            }
            Some(ACCOUNT_LINK_INTERFACE) => {
                if !account_link_allowed(&provides) {
                    errors.push(format!(
                        "component exports `{name}` but the manifest does not declare the `scrobble` or `discovery` layer"
                    ));
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

    for layer in &provides {
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

/// Returns the layers a manifest declares, in manifest order.
fn provides(manifest: &Value) -> Vec<String> {
    manifest
        .get("provides")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the tool names a manifest allows under `permissions.process.tools`.
fn process_tools(manifest: &Value) -> Vec<String> {
    manifest
        .pointer("/permissions/process/tools")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// Returns whether the manifest may export `account-link`.
fn account_link_allowed(provides: &[String]) -> bool {
    provides
        .iter()
        .any(|layer| layer == "scrobble" || layer == "discovery")
}

/// Returns whether the manifest may import `host-process`.
fn process_allowed(provides: &[String], tools: &[String]) -> bool {
    provides.iter().any(|layer| layer == "acquisition") && !tools.is_empty()
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
/// - `tools`: process tool names the manifest declares.
fn import_allowed(interface: &str, version: &str, provides: &[String], tools: &[String]) -> bool {
    if version == API_VERSION
        && (COMMON_HOST_INTERFACES.contains(&interface)
            || (interface == HOST_FILES
                && provides.iter().any(|provided| provided == "acquisition"))
            || (interface == HOST_PROCESS && process_allowed(provides, tools)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Compiles a component that imports and exports one empty instance under
    /// each supplied interface name.
    ///
    /// The component rules read interface names and versions and never the
    /// shape of an interface, so an empty instance is enough to drive them.
    fn component(engine: &Engine, imports: &[&str], exports: &[&str]) -> Component {
        let mut wat = String::from("(component\n  (instance $empty)\n");
        for name in imports {
            wat.push_str(&format!("  (import \"{name}\" (instance))\n"));
        }
        for name in exports {
            wat.push_str(&format!("  (export \"{name}\" (instance $empty))\n"));
        }
        wat.push_str(")\n");
        match Component::new(engine, wat) {
            Ok(component) => component,
            Err(error) => panic!("the test component compiles: {error}"),
        }
    }

    /// Returns a manifest that declares `provides` and `tools`.
    fn manifest(provides: &[&str], tools: &[&str]) -> Value {
        json!({
            "provides": provides,
            "permissions": {
                "process": {"tools": tools},
            },
        })
    }

    /// Returns the errors `check` reports for a component and a manifest.
    fn errors(component: &Component, manifest: &Value) -> Vec<String> {
        let engine = Engine::default();
        let mut errors = Vec::new();
        check(&engine, component, manifest, &mut errors);
        errors
    }

    /// An `artwork` export satisfies a manifest that provides the layer.
    #[test]
    fn an_artwork_export_passes_when_the_manifest_provides_it() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/artwork@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["artwork"], &[]));
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// An `artwork` export without the layer in the manifest is rejected.
    #[test]
    fn an_artwork_export_needs_the_manifest_layer() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/artwork@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&[], &[]));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("does not declare the `artwork` layer")),
            "{errors:?}"
        );
    }

    /// An `artwork` export of another API version is not an interface this
    /// tool checks.
    #[test]
    fn an_artwork_export_of_another_version_is_rejected() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/artwork@0.1.0",
            ],
        );
        let errors = errors(&component, &manifest(&["artwork"], &[]));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("is not built for `hearthsound:plugin@0.5.0`")),
            "{errors:?}"
        );
    }

    /// A component built for the previous API is rejected by name, even when
    /// its manifest declares the current one.
    #[test]
    fn a_component_of_the_previous_api_is_rejected() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.3.0",
                "hearthsound:plugin/catalog@0.3.0",
            ],
        );
        let manifest = json!({
            "api": "hearthsound:plugin@0.5.0",
            "provides": ["catalog"],
        });
        let errors = errors(&component, &manifest);
        assert!(
            errors.iter().any(|error| error
                .contains("component export `hearthsound:plugin/catalog@0.3.0` is not built for `hearthsound:plugin@0.5.0`")),
            "{errors:?}"
        );
    }

    /// `account-link` is allowed next to a `scrobble` layer.
    #[test]
    fn an_account_link_export_passes_with_a_scrobble_layer() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/scrobble@0.5.0",
                "hearthsound:plugin/account-link@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["scrobble"], &[]));
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// `account-link` is allowed next to a `discovery` layer.
    #[test]
    fn an_account_link_export_passes_with_a_discovery_layer() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/discovery@0.5.0",
                "hearthsound:plugin/account-link@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["discovery"], &[]));
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// `account-link` without a layer that uses it is rejected.
    #[test]
    fn an_account_link_export_needs_scrobbling_or_discovery() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &[],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/catalog@0.5.0",
                "hearthsound:plugin/account-link@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["catalog"], &[]));
        assert!(
            errors
                .iter()
                .any(|error| error.contains("does not declare the `scrobble` or `discovery` layer")),
            "{errors:?}"
        );
    }

    /// A tool-driven acquisition plugin may import `host-process`.
    #[test]
    fn a_host_process_import_passes_with_declared_tools() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &["hearthsound:plugin/host-process@0.5.0"],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/acquisition@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["acquisition"], &["yt-dlp"]));
        assert!(errors.is_empty(), "{errors:?}");
    }

    /// A `host-process` import needs a declared process tool.
    #[test]
    fn a_host_process_import_needs_declared_tools() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &["hearthsound:plugin/host-process@0.5.0"],
            &[
                "hearthsound:plugin/lifecycle@0.5.0",
                "hearthsound:plugin/acquisition@0.5.0",
            ],
        );
        let errors = errors(&component, &manifest(&["acquisition"], &[]));
        assert!(
            errors
                .iter()
                .any(|error| error
                    .contains("without listing a tool under permissions.process.tools")),
            "{errors:?}"
        );
    }

    /// A `host-process` import needs the acquisition layer too.
    #[test]
    fn a_host_process_import_needs_the_acquisition_layer() {
        let engine = Engine::default();
        let component = component(
            &engine,
            &["hearthsound:plugin/host-process@0.5.0"],
            &["hearthsound:plugin/lifecycle@0.5.0"],
        );
        let errors = errors(&component, &manifest(&[], &["yt-dlp"]));
        assert!(
            errors
                .iter()
                .any(|error| error
                    .contains("without listing a tool under permissions.process.tools")),
            "{errors:?}"
        );
    }
}
