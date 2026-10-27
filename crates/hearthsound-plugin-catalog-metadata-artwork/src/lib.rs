//! The catalog, metadata and artwork provider world of the
//! `hearthsound:plugin@0.5.0` WIT package.
//!
//! A plugin that searches a catalog, resolves its records, and names the cover
//! images of what it returns implements the guest traits generated in
//! [`catalog_metadata_artwork_provider`] and hands its type to the export
//! macro; plugins depend on `hearthsound-plugin`, which re-exports this module
//! as `hearthsound_plugin::catalog_metadata_artwork_provider` and the macro as
//! `hearthsound_plugin::export_catalog_metadata_artwork_provider!`.
//!
//! The world lives in its own crate on purpose: wit-bindgen exports the CABI
//! macro of an exported interface under a name derived from that interface, and
//! every provider world exports `lifecycle`, so two worlds cannot share a
//! crate.

/// The catalog, metadata and artwork provider world and its export macro.
///
/// The world is the catalog provider world plus `metadata` and `artwork`
/// exports, so the generated module tree contains the guest traits a plugin
/// implements under
/// `exports::hearthsound::plugin::{catalog, metadata, artwork, lifecycle}`,
/// plus the `export_catalog_metadata_artwork_provider` macro that wires a type
/// implementing them into the component's exports.
///
/// The world's `use`d types and host imports are remapped to
/// `hearthsound-plugin-bindings`, so the records in `hearthsound_plugin::types`
/// are the same Rust types the guest traits take and return.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod catalog_metadata_artwork_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        inline: "
            package hearthsound:sdk@0.5.0;
            world catalog-metadata-artwork-provider {
                include hearthsound:plugin/catalog-provider@0.5.0;
                export hearthsound:plugin/metadata@0.5.0;
                export hearthsound:plugin/artwork@0.5.0;
            }
        ",
        world: "hearthsound:sdk/catalog-metadata-artwork-provider",
        with: {
            "hearthsound:plugin/types@0.5.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.5.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.5.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.5.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.5.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_catalog_metadata_artwork_provider",
        default_bindings_module: "hearthsound_plugin::catalog_metadata_artwork_provider",
    });
}
