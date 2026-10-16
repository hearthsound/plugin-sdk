//! The artwork provider world of the `hearthsound:plugin@0.2.0` WIT package.
//!
//! An artwork plugin implements the guest traits generated in
//! [`artwork_provider`] and hands its type to the export macro; plugins depend
//! on `hearthsound-plugin`, which re-exports this module as
//! `hearthsound_plugin::artwork_provider` and the macro as
//! `hearthsound_plugin::export_artwork_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The artwork provider world and its export macro.
///
/// The generated module tree contains the guest traits an artwork plugin
/// implements under `exports::hearthsound::plugin::{artwork, lifecycle}`, plus
/// the `export_artwork_provider` macro that wires a type implementing them into
/// the component's exports.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest trait takes and returns.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod artwork_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/artwork-provider",
        with: {
            "hearthsound:plugin/types@0.2.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.2.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.2.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.2.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.2.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_artwork_provider",
        default_bindings_module: "hearthsound_plugin::artwork_provider",
    });
}
