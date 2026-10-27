//! Shared bindings for the `hearthsound:plugin@0.5.0` WIT package.
//!
//! Every provider world imports the same `types` interface and the same host
//! interfaces, and every provider world exports `lifecycle`. This crate
//! generates those interfaces exactly once, and each provider world crate
//! (`hearthsound-plugin-metadata`, `hearthsound-plugin-acquisition` and the
//! catalog world inside `hearthsound-plugin`) remaps its imports here with
//! wit-bindgen's `with` option.
//!
//! Remapping matters because it keeps one Rust type per WIT type: a plugin that
//! hands a [`types::Track`] to the catalog world and a [`types::Album`] to the
//! metadata world uses the same structs, and the host wrappers in
//! [`host`] accept and return those types with no conversion.
//!
//! Provider worlds cannot share a crate: wit-bindgen exports the interface CABI
//! macro of an exported interface under a name derived from that interface
//! (`__export_hearthsound_plugin_lifecycle_0_1_0_cabi`), so two worlds that both
//! export `lifecycle` in one crate collide at compile time. One world per crate
//! is therefore structural, and `hearthsound-plugin` re-exports all of them.

#[allow(missing_docs, unsafe_code, clippy::all)]
mod bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/base",
    });
}

/// Bindings for the `host-files` import, which only acquisition providers use.
///
/// The base world does not import `host-files`, so the interface is generated
/// on its own from a minimal internal world, re-exported as
/// [`host::files`](crate::host::files).
#[allow(missing_docs, unsafe_code, clippy::all)]
mod host_files_bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        inline: "
            package hearthsound:sdk@0.5.0;
            world host-files {
                import hearthsound:plugin/host-files@0.5.0;
            }
        ",
        world: "hearthsound:sdk/host-files",
        generate_all,
    });
}

/// Bindings for the `host-process` import, which only acquisition providers
/// that run tool processes use.
///
/// Like `host-files`, the interface is not part of the base world, so it is
/// generated on its own from a minimal internal world and re-exported as
/// [`host::process`](crate::host::process).
#[allow(missing_docs, unsafe_code, clippy::all)]
mod host_process_bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        inline: "
            package hearthsound:sdk@0.5.0;
            world host-process {
                import hearthsound:plugin/host-process@0.5.0;
            }
        ",
        world: "hearthsound:sdk/host-process",
        generate_all,
    });
}

/// Shared record types of the `hearthsound:plugin@0.5.0` package.
///
/// This re-exports the generated `types` interface, which holds the records
/// every provider interface shares: `IdPair`, `ArtistCredit`, `Image`,
/// `AlbumRef`, `Track`, `Album`, `Artist`, the `ProviderError` variant, the
/// `EntityKind` enum and `UserContext`.
pub use bindings::hearthsound::plugin::types;

/// Host imports available to a plugin.
///
/// Each submodule wraps one host interface: [`log`](crate::host::log) for
/// tracing, [`http`](crate::host::http) for outbound HTTP,
/// [`kv`](crate::host::kv) for key-value storage,
/// [`config`](crate::host::config) for the instance's server-scoped
/// configuration, [`files`](crate::host::files) for the file access an
/// acquisition provider needs to report the files it produced, and
/// [`process`](crate::host::process) for the external tools an acquisition
/// provider runs.
pub mod host {
    pub use crate::bindings::hearthsound::plugin::host_config as config;
    pub use crate::bindings::hearthsound::plugin::host_http as http;
    pub use crate::bindings::hearthsound::plugin::host_kv as kv;
    pub use crate::bindings::hearthsound::plugin::host_log as log;
    pub use crate::host_files_bindings::hearthsound::plugin::host_files as files;
    pub use crate::host_process_bindings::hearthsound::plugin::host_process as process;
}
