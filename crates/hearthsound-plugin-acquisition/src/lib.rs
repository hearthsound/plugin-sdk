//! The acquisition provider world of the `hearthsound:plugin@0.3.0` WIT
//! package.
//!
//! An acquisition plugin implements the guest traits generated in
//! [`acquisition_provider`] and hands its type to the export macro; plugins
//! depend on `hearthsound-plugin`, which re-exports this module as
//! `hearthsound_plugin::acquisition_provider` and the macro as
//! `hearthsound_plugin::export_acquisition_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The acquisition provider world and its export macro.
///
/// The generated module tree contains the guest traits an acquisition plugin
/// implements under `exports::hearthsound::plugin::{acquisition, lifecycle}`,
/// plus the `export_acquisition_provider` macro that wires a type implementing
/// them into the component's exports. This is the only world that also imports
/// `host-files`, available to plugins as `hearthsound_plugin::host::files`.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest trait takes and returns.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod acquisition_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/acquisition-provider",
        with: {
            "hearthsound:plugin/types@0.3.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.3.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.3.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.3.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.3.0": hearthsound_plugin_bindings::host::config,
            "hearthsound:plugin/host-files@0.3.0": hearthsound_plugin_bindings::host::files,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_acquisition_provider",
        default_bindings_module: "hearthsound_plugin::acquisition_provider",
    });
}
