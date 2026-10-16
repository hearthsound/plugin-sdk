//! Rust SDK for building Hearthsound plugins.
//!
//! A Hearthsound plugin is a WebAssembly component that speaks the
//! `hearthsound:plugin@0.2.0` WIT package. This crate re-exports the bindings
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
//! - [`acquisition_provider`] and [`export_acquisition_provider!`](crate::export_acquisition_provider)
//!   for the `acquisition` layer, the only one that also imports
//!   [`host::files`].
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
//! | `hearthsound-plugin-bindings` | The `types` interface, the host imports and `host-files` |
//! | `hearthsound-plugin-metadata` | The `metadata` provider world |
//! | `hearthsound-plugin-acquisition` | The `acquisition` provider world |
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

/// Shared record types of the `hearthsound:plugin@0.2.0` package.
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
/// configuration and [`files`](crate::host::files) for the file access an
/// acquisition provider needs to report the files it produced.
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
            "hearthsound:plugin/types@0.2.0": hearthsound_plugin_bindings::types,
            "hearthsound:plugin/host-log@0.2.0": hearthsound_plugin_bindings::host::log,
            "hearthsound:plugin/host-http@0.2.0": hearthsound_plugin_bindings::host::http,
            "hearthsound:plugin/host-kv@0.2.0": hearthsound_plugin_bindings::host::kv,
            "hearthsound:plugin/host-config@0.2.0": hearthsound_plugin_bindings::host::config,
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
