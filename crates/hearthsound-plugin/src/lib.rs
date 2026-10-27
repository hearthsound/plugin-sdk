//! Rust SDK for building Hearthsound plugins.
//!
//! A Hearthsound plugin is a WebAssembly component that speaks the
//! `hearthsound:plugin@0.5.0` WIT package. This crate re-exports the bindings
//! for that package, generates the catalog world, and provides the host-import
//! wrappers and logging macros a plugin implementation needs. It is the only
//! crate a plugin depends on.
//!
//! # Worlds
//!
//! Every provider world has one module holding the guest traits of the layer it
//! serves and the macro that exports an implementation of them:
//!
//! - [`catalog_provider`] and [`export_catalog_provider!`](crate::export_catalog_provider)
//!   for the `catalog` layer.
//! - [`metadata_provider`] and [`export_metadata_provider!`](crate::export_metadata_provider)
//!   for the `metadata` layer.
//! - [`artwork_provider`] and [`export_artwork_provider!`](crate::export_artwork_provider)
//!   for the `artwork` layer: cover images for albums and portraits for
//!   artists.
//! - [`acquisition_provider`] and [`export_acquisition_provider!`](crate::export_acquisition_provider)
//!   for the `acquisition` layer, the only one that also imports
//!   [`host::files`].
//! - [`process_acquisition_provider`] and
//!   [`export_process_acquisition_provider!`](crate::export_process_acquisition_provider)
//!   for acquisition plugins that run an administrator-configured tool through
//!   [`host::process`].
//! - [`verification_provider`] and
//!   [`export_verification_provider!`](crate::export_verification_provider)
//!   for the `verification` layer: deciding whether a downloaded file is the
//!   recording the server asked for.
//! - [`scrobble_discovery_link_provider`] and
//!   [`export_scrobble_discovery_link_provider!`](crate::export_scrobble_discovery_link_provider)
//!   for plugins that report listening history, suggest music and link a
//!   listener's account on the service.
//! - [`catalog_artwork_provider`] and
//!   [`export_catalog_artwork_provider!`](crate::export_catalog_artwork_provider),
//!   and [`metadata_artwork_provider`] and
//!   [`export_metadata_artwork_provider!`](crate::export_metadata_artwork_provider),
//!   for plugins that serve a second layer alongside `artwork`.
//! - [`catalog_metadata_artwork_provider`] and
//!   [`export_catalog_metadata_artwork_provider!`](crate::export_catalog_metadata_artwork_provider)
//!   for a plugin that covers catalog, metadata and artwork at once, the way a
//!   full music database plugin does.
//!
//! The shared [`types`] module and the [`host`] imports are generated once in
//! `hearthsound-plugin-bindings`, and every world remaps its imports there, so a
//! plugin uses one `types` module and one set of host modules no matter how many
//! layers it implements.
//!
//! # Crate layout
//!
//! wit-bindgen exports the CABI macro of an exported interface under a name
//! derived from that interface, and every provider world exports `lifecycle`, so
//! two worlds cannot be generated in one crate. Each additional world therefore
//! lives in its own crate, and this crate re-exports it:
//!
//! | Crate | Contents |
//! |---|---|
//! | `hearthsound-plugin-bindings` | The `types` interface, the host imports, `host-files` and `host-process` |
//! | `hearthsound-plugin-metadata` | The `metadata` provider world |
//! | `hearthsound-plugin-artwork` | The `artwork` provider world |
//! | `hearthsound-plugin-metadata-artwork` | The `metadata` and `artwork` provider world |
//! | `hearthsound-plugin-catalog-artwork` | The `catalog` and `artwork` provider world |
//! | `hearthsound-plugin-catalog-metadata-artwork` | The `catalog`, `metadata` and `artwork` provider world |
//! | `hearthsound-plugin-acquisition` | The `acquisition` provider world |
//! | `hearthsound-plugin-process-acquisition` | The `acquisition` provider world with the `host-process` import |
//! | `hearthsound-plugin-verification` | The `verification` provider world |
//! | `hearthsound-plugin-scrobble-discovery-account-link` | The `scrobble`, `discovery` and `account-link` provider world |
//! | `hearthsound-plugin` | The `catalog` provider world, host wrappers, config and logging helpers, and the log macros |
//!
//! # Helpers
//!
//! On top of the raw host imports in [`host`], the crate provides [`config`]
//! for typed server configuration, [`http`] for JSON requests, [`kv`] for
//! values that outlive a call, and [`log`] with the [`trace!`] to [`error!`]
//! macros.
//!
//! # Example
//!
//! A catalog plugin implements the [`catalog_provider`] guest trait and hands
//! its type to the export macro:
//!
//! ```ignore
//! struct HelloCatalog;
//!
//! impl hearthsound_plugin::catalog_provider::exports::hearthsound::plugin::catalog::Guest
//!     for HelloCatalog
//! {
//!     fn search(
//!         query: hearthsound_plugin::catalog_provider::exports::hearthsound::plugin::catalog::SearchQuery,
//!     ) -> Result<
//!         hearthsound_plugin::catalog_provider::exports::hearthsound::plugin::catalog::SearchResults,
//!         hearthsound_plugin::types::ProviderError,
//!     > {
//!         let _ = query;
//!         Ok(Default::default())
//!     }
//! }
//!
//! hearthsound_plugin::export_catalog_provider!(HelloCatalog);
//! ```
//!
//! See `examples/hello-catalog` in this repository for a complete plugin.

/// Shared record types of the `hearthsound:plugin@0.5.0` package.
///
/// This re-exports the generated `types` interface, which holds the records
/// every provider interface shares: `IdPair`, `ArtistCredit`, `Image`,
/// `AlbumRef`, `Track`, `Album`, `Artist`, the `ProviderError` variant, the
/// `EntityKind` enum and `UserContext`.
#[doc(inline)]
pub use hearthsound_plugin_bindings::types;

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
#[doc(inline)]
pub use hearthsound_plugin_bindings::host;

/// The catalog provider world and its export macro.
///
/// The generated module tree contains the guest traits a catalog plugin
/// implements under `exports::hearthsound::plugin::{catalog, lifecycle}`, plus
/// the [`export_catalog_provider!`](crate::export_catalog_provider) macro that
/// wires a type implementing them into the component's exports.
#[allow(missing_docs, unsafe_code, clippy::all)]
pub mod catalog_provider {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/catalog-provider",
        with: {
            "hearthsound:plugin/types@0.5.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.5.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.5.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.5.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.5.0": hearthsound_plugin_bindings::host::config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_catalog_provider",
        default_bindings_module: "hearthsound_plugin::catalog_provider",
    });
}

/// Wires a catalog provider into the component's exports.
///
/// Pass a type that implements the
/// `catalog_provider::exports::hearthsound::plugin::lifecycle::Guest` and
/// `catalog_provider::exports::hearthsound::plugin::catalog::Guest` traits.
/// The macro expands to the `#[unsafe(no_mangle)]` entry points the component
/// model expects.
///
/// # Example
///
/// ```ignore
/// struct MyCatalog;
/// // impl ...::lifecycle::Guest for MyCatalog {}
/// // impl ...::catalog::Guest for MyCatalog {}
///
/// hearthsound_plugin::export_catalog_provider!(MyCatalog);
/// ```
#[doc(inline)]
pub use catalog_provider::export_catalog_provider;

/// The metadata provider world and its export macro.
///
/// The generated module tree contains the guest traits a metadata plugin
/// implements under `exports::hearthsound::plugin::{metadata, lifecycle}`, plus
/// the [`export_metadata_provider!`](crate::export_metadata_provider) macro that
/// wires a type implementing them into the component's exports.
#[doc(inline)]
pub use hearthsound_plugin_metadata::metadata_provider;

/// Wires a metadata provider into the component's exports.
///
/// Pass a type that implements the
/// `metadata_provider::exports::hearthsound::plugin::lifecycle::Guest` and
/// `metadata_provider::exports::hearthsound::plugin::metadata::Guest` traits.
/// The macro expands to the `#[unsafe(no_mangle)]` entry points the component
/// model expects.
///
/// # Example
///
/// ```ignore
/// struct MyMetadata;
/// // impl ...::lifecycle::Guest for MyMetadata {}
/// // impl ...::metadata::Guest for MyMetadata {}
///
/// hearthsound_plugin::export_metadata_provider!(MyMetadata);
/// ```
#[doc(inline)]
pub use metadata_provider::export_metadata_provider;

/// The artwork provider world and its export macro.
///
/// The generated module tree contains the guest traits an artwork plugin
/// implements under `exports::hearthsound::plugin::{artwork, lifecycle}`, plus
/// the [`export_artwork_provider!`](crate::export_artwork_provider) macro that
/// wires a type implementing them into the component's exports. The `artwork`
/// layer answers cover images for albums and portraits for artists.
#[doc(inline)]
pub use hearthsound_plugin_artwork::artwork_provider;

/// Wires an artwork provider into the component's exports.
///
/// Pass a type that implements the
/// `artwork_provider::exports::hearthsound::plugin::lifecycle::Guest` and
/// `artwork_provider::exports::hearthsound::plugin::artwork::Guest` traits. The
/// macro expands to the `#[unsafe(no_mangle)]` entry points the component model
/// expects.
///
/// # Example
///
/// ```ignore
/// struct MyArtwork;
/// // impl ...::lifecycle::Guest for MyArtwork {}
/// // impl ...::artwork::Guest for MyArtwork {}
///
/// hearthsound_plugin::export_artwork_provider!(MyArtwork);
/// ```
#[doc(inline)]
pub use artwork_provider::export_artwork_provider;

/// The metadata and artwork provider world and its export macro.
///
/// The generated module tree contains the guest traits a plugin that serves
/// both layers implements under
/// `exports::hearthsound::plugin::{metadata, artwork, lifecycle}`, plus the
/// [`export_metadata_artwork_provider!`](crate::export_metadata_artwork_provider)
/// macro that wires a type implementing them into the component's exports.
#[doc(inline)]
pub use hearthsound_plugin_metadata_artwork::metadata_artwork_provider;

/// Wires a metadata and artwork provider into the component's exports.
///
/// Pass a type that implements the
/// `metadata_artwork_provider::exports::hearthsound::plugin::lifecycle::Guest`,
/// `...::metadata::Guest` and `...::artwork::Guest` traits. The macro expands to
/// the `#[unsafe(no_mangle)]` entry points the component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyMetadataArtwork;
/// // impl ...::lifecycle::Guest for MyMetadataArtwork {}
/// // impl ...::metadata::Guest for MyMetadataArtwork {}
/// // impl ...::artwork::Guest for MyMetadataArtwork {}
///
/// hearthsound_plugin::export_metadata_artwork_provider!(MyMetadataArtwork);
/// ```
#[doc(inline)]
pub use metadata_artwork_provider::export_metadata_artwork_provider;

/// The catalog and artwork provider world and its export macro.
///
/// The generated module tree contains the guest traits a plugin that serves
/// both layers implements under
/// `exports::hearthsound::plugin::{catalog, artwork, lifecycle}`, plus the
/// [`export_catalog_artwork_provider!`](crate::export_catalog_artwork_provider)
/// macro that wires a type implementing them into the component's exports.
#[doc(inline)]
pub use hearthsound_plugin_catalog_artwork::catalog_artwork_provider;

/// Wires a catalog and artwork provider into the component's exports.
///
/// Pass a type that implements the
/// `catalog_artwork_provider::exports::hearthsound::plugin::lifecycle::Guest`,
/// `...::catalog::Guest` and `...::artwork::Guest` traits. The macro expands to
/// the `#[unsafe(no_mangle)]` entry points the component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyCatalogArtwork;
/// // impl ...::lifecycle::Guest for MyCatalogArtwork {}
/// // impl ...::catalog::Guest for MyCatalogArtwork {}
/// // impl ...::artwork::Guest for MyCatalogArtwork {}
///
/// hearthsound_plugin::export_catalog_artwork_provider!(MyCatalogArtwork);
/// ```
#[doc(inline)]
pub use catalog_artwork_provider::export_catalog_artwork_provider;

/// The catalog, metadata and artwork provider world and its export macro.
///
/// The generated module tree contains the guest traits a plugin that serves all
/// three layers implements under
/// `exports::hearthsound::plugin::{catalog, metadata, artwork, lifecycle}`, plus
/// the
/// [`export_catalog_metadata_artwork_provider!`](crate::export_catalog_metadata_artwork_provider)
/// macro that wires a type implementing them into the component's exports.
#[doc(inline)]
pub use hearthsound_plugin_catalog_metadata_artwork::catalog_metadata_artwork_provider;

/// Wires a catalog, metadata and artwork provider into the component's exports.
///
/// Pass a type that implements the
/// `catalog_metadata_artwork_provider::exports::hearthsound::plugin::lifecycle::Guest`,
/// `...::catalog::Guest`, `...::metadata::Guest` and `...::artwork::Guest`
/// traits. The macro expands to the `#[unsafe(no_mangle)]` entry points the
/// component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyFullProvider;
/// // impl ...::lifecycle::Guest for MyFullProvider {}
/// // impl ...::catalog::Guest for MyFullProvider {}
/// // impl ...::metadata::Guest for MyFullProvider {}
/// // impl ...::artwork::Guest for MyFullProvider {}
///
/// hearthsound_plugin::export_catalog_metadata_artwork_provider!(MyFullProvider);
/// ```
#[doc(inline)]
pub use catalog_metadata_artwork_provider::export_catalog_metadata_artwork_provider;

/// The acquisition provider world and its export macro.
///
/// The generated module tree contains the guest traits an acquisition plugin
/// implements under `exports::hearthsound::plugin::{acquisition, lifecycle}`,
/// plus the [`export_acquisition_provider!`](crate::export_acquisition_provider)
/// macro that wires a type implementing them into the component's exports. This
/// is also the only world that imports `host-files`, available as
/// [`host::files`].
#[doc(inline)]
pub use hearthsound_plugin_acquisition::acquisition_provider;

/// Wires an acquisition provider into the component's exports.
///
/// Pass a type that implements the
/// `acquisition_provider::exports::hearthsound::plugin::lifecycle::Guest` and
/// `acquisition_provider::exports::hearthsound::plugin::acquisition::Guest`
/// traits. The macro expands to the `#[unsafe(no_mangle)]` entry points the
/// component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyAcquisition;
/// // impl ...::lifecycle::Guest for MyAcquisition {}
/// // impl ...::acquisition::Guest for MyAcquisition {}
///
/// hearthsound_plugin::export_acquisition_provider!(MyAcquisition);
/// ```
#[doc(inline)]
pub use acquisition_provider::export_acquisition_provider;

/// The acquisition provider world that also runs external tools.
///
/// The generated module tree contains the guest traits an acquisition plugin
/// implements under `exports::hearthsound::plugin::{acquisition, lifecycle}`,
/// plus the
/// [`export_process_acquisition_provider!`](crate::export_process_acquisition_provider)
/// macro that wires a type implementing them into the component's exports. The
/// world imports `host-files` and `host-process`, available as [`host::files`]
/// and [`host::process`].
#[doc(inline)]
pub use hearthsound_plugin_process_acquisition::process_acquisition_provider;

/// Wires a tool-driven acquisition provider into the component's exports.
///
/// Pass a type that implements the
/// `process_acquisition_provider::exports::hearthsound::plugin::lifecycle::Guest`
/// and `...::acquisition::Guest` traits. The macro expands to the
/// `#[unsafe(no_mangle)]` entry points the component model expects. The
/// manifest must list every tool the plugin spawns under
/// `permissions.process.tools`.
///
/// # Example
///
/// ```ignore
/// struct MyAcquisition;
/// // impl ...::lifecycle::Guest for MyAcquisition {}
/// // impl ...::acquisition::Guest for MyAcquisition {}
///
/// hearthsound_plugin::export_process_acquisition_provider!(MyAcquisition);
/// ```
#[doc(inline)]
pub use process_acquisition_provider::export_process_acquisition_provider;

/// The verification provider world and its export macro.
///
/// The generated module tree contains the guest traits a verification plugin
/// implements under `exports::hearthsound::plugin::{verification, lifecycle}`,
/// plus the [`export_verification_provider!`](crate::export_verification_provider)
/// macro that wires a type implementing them into the component's exports.
#[doc(inline)]
pub use hearthsound_plugin_verification::verification_provider;

/// Wires a verification provider into the component's exports.
///
/// Pass a type that implements the
/// `verification_provider::exports::hearthsound::plugin::lifecycle::Guest` and
/// `verification_provider::exports::hearthsound::plugin::verification::Guest`
/// traits. The macro expands to the `#[unsafe(no_mangle)]` entry points the
/// component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyVerification;
/// // impl ...::lifecycle::Guest for MyVerification {}
/// // impl ...::verification::Guest for MyVerification {}
///
/// hearthsound_plugin::export_verification_provider!(MyVerification);
/// ```
#[doc(inline)]
pub use verification_provider::export_verification_provider;

/// The scrobble, discovery and account-link provider world.
///
/// The generated module tree contains the guest traits a plugin implements
/// under `exports::hearthsound::plugin::{scrobble, discovery, account_link,
/// lifecycle}`, plus the
/// [`export_scrobble_discovery_link_provider!`](crate::export_scrobble_discovery_link_provider)
/// macro that wires a type implementing them into the component's exports.
/// `account-link` is a capability, not a layer: the manifest only declares the
/// `scrobble` and `discovery` layers the plugin provides.
#[doc(inline)]
pub use hearthsound_plugin_scrobble_discovery_account_link::scrobble_discovery_link_provider;

/// Wires a scrobbling, discovery and account-link provider into the
/// component's exports.
///
/// Pass a type that implements the
/// `scrobble_discovery_link_provider::exports::hearthsound::plugin::lifecycle::Guest`,
/// `...::scrobble::Guest`, `...::discovery::Guest` and
/// `...::account_link::Guest` traits. The macro expands to the
/// `#[unsafe(no_mangle)]` entry points the component model expects.
///
/// # Example
///
/// ```ignore
/// struct MyScrobbler;
/// // impl ...::lifecycle::Guest for MyScrobbler {}
/// // impl ...::scrobble::Guest for MyScrobbler {}
/// // impl ...::discovery::Guest for MyScrobbler {}
/// // impl ...::account_link::Guest for MyScrobbler {}
///
/// hearthsound_plugin::export_scrobble_discovery_link_provider!(MyScrobbler);
/// ```
#[doc(inline)]
pub use scrobble_discovery_link_provider::export_scrobble_discovery_link_provider;

/// Typed HTTP helpers over the `host-http` import.
pub mod http;

/// Typed key-value helpers over the `host-kv` import.
pub mod kv;

/// Server-scoped configuration helpers.
pub mod config {
    use crate::types;

    /// Deserializes the instance's server-scoped configuration.
    ///
    /// Reads the JSON string returned by the `host-config` import and
    /// deserializes it into `T`.
    ///
    /// # Errors
    ///
    /// Returns [`types::ProviderError::InvalidConfig`] when the host returns a
    /// string that does not deserialize into `T`.
    pub fn server_config<T: serde::de::DeserializeOwned>() -> Result<T, types::ProviderError> {
        let raw = crate::host::config::server_config();
        serde_json::from_str(&raw)
            .map_err(|e| types::ProviderError::InvalidConfig(format!("server config: {e}")))
    }
}

/// Logging helpers.
pub mod log {
    use crate::host;

    /// Writes one log line to the host.
    ///
    /// The host prefixes the message with `target` so that a plugin's own
    /// module names show up in the server log.
    ///
    /// # Parameters
    ///
    /// - `level`: severity of the line.
    /// - `target`: prefix for the message, usually the plugin's module path.
    /// - `message`: the text to log.
    pub fn write(level: host::log::Level, target: &str, message: &str) {
        host::log::log(level, &format!("{target}: {message}"));
    }
}

/// Logs a message at the `trace` level.
///
/// Arguments are formatted with [`std::format!`] and tagged with the calling
/// module path.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)*) => {
        $crate::log::write(
            $crate::host::log::Level::Trace,
            module_path!(),
            &::std::format!($($arg)*),
        )
    };
}

/// Logs a message at the `debug` level.
///
/// Arguments are formatted with [`std::format!`] and tagged with the calling
/// module path.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::log::write(
            $crate::host::log::Level::Debug,
            module_path!(),
            &::std::format!($($arg)*),
        )
    };
}

/// Logs a message at the `info` level.
///
/// Arguments are formatted with [`std::format!`] and tagged with the calling
/// module path.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::log::write(
            $crate::host::log::Level::Info,
            module_path!(),
            &::std::format!($($arg)*),
        )
    };
}

/// Logs a message at the `warn` level.
///
/// Arguments are formatted with [`std::format!`] and tagged with the calling
/// module path.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::log::write(
            $crate::host::log::Level::Warn,
            module_path!(),
            &::std::format!($($arg)*),
        )
    };
}

/// Logs a message at the `error` level.
///
/// Arguments are formatted with [`std::format!`] and tagged with the calling
/// module path.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::log::write(
            $crate::host::log::Level::Error,
            module_path!(),
            &::std::format!($($arg)*),
        )
    };
}
