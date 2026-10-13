//! Rust SDK for building Hearthsound plugins.
//!
//! A Hearthsound plugin is a WebAssembly component that speaks the
//! `hearthsound:plugin@0.1.0` WIT package. This crate generates the bindings
//! for that package from the WIT files in this repository, re-exports the
//! shared types, and provides the host-import wrappers and logging macros a
//! plugin implementation needs.
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

#[allow(missing_docs, unsafe_code, clippy::all)]
mod bindings {
    wit_bindgen::generate!({
        path: "../../wit",
        world: "hearthsound:plugin/base",
    });
}

/// Shared record types of the `hearthsound:plugin@0.1.0` package.
///
/// This re-exports the generated `types` interface, which holds the records
/// every provider interface shares: `IdPair`, `ArtistCredit`, `Image`,
/// `AlbumRef`, `Track`, `Album`, `Artist`, the `ProviderError` variant, the
/// `EntityKind` enum and `UserContext`.
pub use bindings::hearthsound::plugin::types;

/// Host imports available to every plugin.
///
/// Each submodule wraps one host interface: [`log`] for tracing, [`http`] for
/// outbound HTTP, [`kv`] for key-value storage and [`config`] for the
/// instance's server-scoped configuration.
pub mod host {
    pub use crate::bindings::hearthsound::plugin::host_config as config;
    pub use crate::bindings::hearthsound::plugin::host_http as http;
    pub use crate::bindings::hearthsound::plugin::host_kv as kv;
    pub use crate::bindings::hearthsound::plugin::host_log as log;
}

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
            "hearthsound:plugin/types@0.1.0": crate::bindings::hearthsound::plugin::types,
            "hearthsound:plugin/host-log@0.1.0": crate::bindings::hearthsound::plugin::host_log,
            "hearthsound:plugin/host-http@0.1.0": crate::bindings::hearthsound::plugin::host_http,
            "hearthsound:plugin/host-kv@0.1.0": crate::bindings::hearthsound::plugin::host_kv,
            "hearthsound:plugin/host-config@0.1.0": crate::bindings::hearthsound::plugin::host_config,
        },
        generate_all,
        pub_export_macro: true,
        export_macro_name: "export_catalog_provider",
        default_bindings_module: "hearthsound_plugin::catalog_provider",
    });
}

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
