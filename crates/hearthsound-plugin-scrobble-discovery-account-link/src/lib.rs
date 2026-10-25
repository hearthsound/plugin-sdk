//! The scrobble, discovery and account-link provider world of the
//! `hearthsound:plugin@0.4.0` WIT package.
//!
//! A plugin that reports listening history, suggests music and links a
//! listener's account on the service implements the guest traits generated in
//! [`scrobble_discovery_link_provider`] and hands its type to the export macro;
//! plugins depend on `hearthsound-plugin`, which re-exports this module as
//! `hearthsound_plugin::scrobble_discovery_link_provider` and the macro as
//! `hearthsound_plugin::export_scrobble_discovery_link_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The scrobble, discovery and account-link provider world and its export
/// macro.
///
/// The generated module tree contains the guest traits a plugin implements
/// under `exports::hearthsound::plugin::{scrobble, discovery, account_link,
/// lifecycle}`, plus the `export_scrobble_discovery_link_provider` macro that
/// wires a type implementing them into the component's exports.
///
/// `account-link` is a capability rather than a layer: a manifest only declares
/// the `scrobble` and `discovery` layers it provides.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest traits take and return.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod scrobble_discovery_link_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        inline: "
            package hearthsound:sdk@0.4.0;
            world scrobble-discovery-link-provider {
                include hearthsound:plugin/base@0.4.0;
                export hearthsound:plugin/scrobble@0.4.0;
                export hearthsound:plugin/discovery@0.4.0;
                export hearthsound:plugin/account-link@0.4.0;
            }
        ",
        world: "hearthsound:sdk/scrobble-discovery-link-provider",
        with: {
            "hearthsound:plugin/types@0.4.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.4.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.4.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.4.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.4.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_scrobble_discovery_link_provider",
        default_bindings_module: "hearthsound_plugin::scrobble_discovery_link_provider",
    });
}
