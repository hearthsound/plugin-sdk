//! The catalog and artwork provider world of the `hearthsound:plugin@0.2.0`
//! WIT package.
//!
//! A plugin that both answers catalog searches and finds cover images
//! implements the guest traits generated in [`catalog_artwork_provider`] and
//! hands its type to the export macro; plugins depend on `hearthsound-plugin`,
//! which re-exports this module as
//! `hearthsound_plugin::catalog_artwork_provider` and the macro as
//! `hearthsound_plugin::export_catalog_artwork_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The catalog and artwork provider world and its export macro.
///
/// The world is the catalog provider world plus an `artwork` export, so the
/// generated module tree contains the guest traits a plugin implements under
/// `exports::hearthsound::plugin::{catalog, artwork, lifecycle}`, plus the
/// `export_catalog_artwork_provider` macro that wires a type implementing them
/// into the component's exports.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest traits take and return.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod catalog_artwork_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        inline: "
            package hearthsound:sdk@0.2.0;
            world catalog-artwork-provider {
                include hearthsound:plugin/catalog-provider@0.2.0;
                export hearthsound:plugin/artwork@0.2.0;
            }
        ",
        world: "hearthsound:sdk/catalog-artwork-provider",
        with: {
            "hearthsound:plugin/types@0.2.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.2.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.2.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.2.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.2.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_catalog_artwork_provider",
        default_bindings_module: "hearthsound_plugin::catalog_artwork_provider",
    });
}
