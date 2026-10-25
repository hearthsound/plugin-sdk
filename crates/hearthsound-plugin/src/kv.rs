//! Typed key-value helpers over the `host-kv` import.
//!
//! [`crate::host::kv`] stores and returns raw bytes; this module stores and
//! returns serde values, so a plugin can keep a cached API response, or any
//! state that must survive an instance being recycled, as a Rust type.
//!
//! The host owns the limits: a value larger than the per-value cap, or a write
//! that would exceed the instance's total quota, fails the write instead of
//! evicting entries. Entries written with a TTL are not returned by reads after
//! it expires.
//!
//! # Example
//!
//! ```ignore
//! use hearthsound_plugin::kv;
//!
//! #[derive(serde::Serialize, serde::Deserialize)]
//! struct CachedRelease {
//!     title: String,
//! }
//!
//! let cached: Option<CachedRelease> = kv::get_json("release:42")?;
//! kv::set_json("release:42", &CachedRelease { title: "Hello".to_string() }, Some(60_000))?;
//! ```

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::host::kv;
use crate::types::ProviderError;

/// Reads `key` and deserializes its JSON value into `T`.
///
/// # Parameters
///
/// - `key`: the key to read.
///
/// # Returns
///
/// `None` when the key is absent or has expired, and when the stored bytes are
/// not JSON for `T`. Treating an unreadable value as a miss keeps a cache entry
/// written by an older version of the plugin from failing a call; a plugin that
/// needs to distinguish the two reads the key through [`crate::host::kv`].
///
/// # Errors
///
/// Returns [`ProviderError::Internal`] when the host cannot read the store at
/// all. That is a storage failure, not a miss, so it is reported instead of
/// being folded into `None`; the reason is the host's safe description of the
/// failure, not the database error.
pub fn get_json<T: DeserializeOwned>(key: &str) -> Result<Option<T>, ProviderError> {
    let bytes = match kv::get(key) {
        Ok(bytes) => bytes,
        Err(reason) => return Err(map_read_error(reason)),
    };
    let Some(bytes) = bytes else {
        return Ok(None);
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

/// Maps a `host-kv` read failure to the provider error a plugin reports.
fn map_read_error(reason: String) -> ProviderError {
    ProviderError::Internal(reason)
}

/// Serializes `value` to JSON and stores it under `key`.
///
/// # Parameters
///
/// - `key`: the key to write.
/// - `value`: the value to serialize and store.
/// - `ttl_ms`: how long the entry stays readable, in milliseconds; `None` keeps
///   it until it is overwritten or deleted.
///
/// # Returns
///
/// `Ok(())` once the host stored the value.
///
/// # Errors
///
/// Returns [`ProviderError::Internal`] when `value` cannot be serialized or the
/// host rejects the write, for example because the value exceeds the per-value
/// cap or the instance's total quota. A plugin that caches best-effort may
/// ignore that error, but the write did not happen.
pub fn set_json<T: Serialize>(
    key: &str,
    value: &T,
    ttl_ms: Option<u64>,
) -> Result<(), ProviderError> {
    let bytes = serde_json::to_vec(value).map_err(|e| ProviderError::Internal(format!("{e}")))?;
    kv::set(key, &bytes, ttl_ms).map_err(ProviderError::Internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read failure becomes the provider error a plugin reports, with the
    /// host's own reason passed through unchanged.
    #[test]
    fn a_read_failure_becomes_an_internal_error() {
        match map_read_error("x".to_string()) {
            ProviderError::Internal(reason) => assert_eq!(reason, "x"),
            other => panic!("expected Internal, got {other:?}"),
        }
    }
}
