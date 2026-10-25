//! The verification provider world of the `hearthsound:plugin@0.4.0` WIT
//! package.
//!
//! A verification plugin decides whether a downloaded file is the recording the
//! server asked for. It implements the guest traits generated in
//! [`verification_provider`] and hands its type to the export macro; plugins
//! depend on `hearthsound-plugin`, which re-exports this module as
//! `hearthsound_plugin::verification_provider` and the macro as
//! `hearthsound_plugin::export_verification_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The verification provider world and its export macro.
///
/// The generated module tree contains the guest traits a verification plugin
/// implements under `exports::hearthsound::plugin::{verification, lifecycle}`,
/// plus the `export_verification_provider` macro that wires a type implementing
/// them into the component's exports.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest trait takes and returns.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod verification_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/verification-provider",
        with: {
            "hearthsound:plugin/types@0.4.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.4.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.4.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.4.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.4.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_verification_provider",
        default_bindings_module: "hearthsound_plugin::verification_provider",
    });
}
