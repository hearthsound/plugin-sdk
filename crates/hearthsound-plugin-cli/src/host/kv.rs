//! In-memory key-value store behind the mock host's `host-kv` import.
//!
//! The store enforces the same caps the server applies: a value may be at
//! most 256 KiB, and the namespace may hold at most the manifest's
//! `permissions.kv.maxBytes` budget (1 MiB when the manifest sets none). The
//! budget counts key and value bytes together. Entries written with a
//! time-to-live disappear once it elapses.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;

/// Largest value a plugin may store under one key.
pub const MAX_VALUE_BYTES: usize = 256 * 1024;

/// Namespace budget used when the manifest does not set `permissions.kv.maxBytes`.
pub const DEFAULT_QUOTA_BYTES: u64 = 1024 * 1024;

/// One stored value and its optional expiry.
#[derive(Debug, Clone)]
struct Entry {
    value: Vec<u8>,
    expires_at: Option<Instant>,
}

/// A plugin's key-value namespace.
///
/// # Examples
///
/// ```text
/// use hearthsound_plugin_cli::host::kv::Kv;
///
/// let mut kv = Kv::new(1024);
/// kv.set("greeting", b"hello".to_vec(), None).unwrap();
/// assert_eq!(kv.get("greeting"), Some(b"hello".to_vec()));
/// kv.delete("greeting");
/// assert_eq!(kv.get("greeting"), None);
/// ```
#[derive(Debug)]
pub struct Kv {
    entries: HashMap<String, Entry>,
    quota: u64,
    used: u64,
}

impl Kv {
    /// Creates an empty namespace with the given byte budget.
    #[must_use]
    pub fn new(quota: u64) -> Self {
        Self {
            entries: HashMap::new(),
            quota,
            used: 0,
        }
    }

    /// Returns the value stored under `key`, or `None` when the key is absent
    /// or its time-to-live has elapsed.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<Vec<u8>> {
        let entry = self.entries.get(key)?;
        match entry.expires_at {
            Some(deadline) if Instant::now() >= deadline => None,
            _ => Some(entry.value.clone()),
        }
    }

    /// Stores `value` under `key`, replacing any previous value.
    ///
    /// `ttl` sets an expiry measured from now; `None` keeps the value until it
    /// is deleted.
    ///
    /// # Parameters
    ///
    /// - `key`: the entry name.
    /// - `value`: the bytes to store.
    /// - `ttl`: optional time-to-live.
    ///
    /// # Errors
    ///
    /// Returns a message when the value exceeds [`MAX_VALUE_BYTES`] or when
    /// the namespace would exceed its budget. The stored state is unchanged on
    /// error.
    pub fn set(&mut self, key: &str, value: Vec<u8>, ttl: Option<Duration>) -> Result<(), String> {
        if value.len() > MAX_VALUE_BYTES {
            return Err(format!(
                "value is {} bytes; the limit is {MAX_VALUE_BYTES}",
                value.len()
            ));
        }
        let previous = self
            .entries
            .get(key)
            .map_or(0, |entry| footprint(key, &entry.value));
        let next = self.used - previous + footprint(key, &value);
        if next > self.quota {
            return Err(format!(
                "kv quota exceeded: {next} of {} bytes would be used",
                self.quota
            ));
        }
        self.used = next;
        let expires_at = ttl.map(|ttl| Instant::now() + ttl);
        self.entries
            .insert(key.to_owned(), Entry { value, expires_at });
        Ok(())
    }

    /// Removes `key` if present. Deleting an absent key does nothing.
    pub fn delete(&mut self, key: &str) {
        if let Some(entry) = self.entries.remove(key) {
            self.used -= footprint(key, &entry.value);
        }
    }

    /// Returns the bytes currently charged against the budget.
    #[cfg(test)]
    #[must_use]
    pub fn used(&self) -> u64 {
        self.used
    }
}

/// Bytes one entry charges against the budget: its key and its value.
fn footprint(key: &str, value: &[u8]) -> u64 {
    (key.len() + value.len()) as u64
}

/// Reads the namespace budget from a manifest, falling back to
/// [`DEFAULT_QUOTA_BYTES`].
///
/// # Parameters
///
/// - `manifest`: the parsed `manifest.json` value.
#[must_use]
pub fn quota_from_manifest(manifest: &Value) -> u64 {
    manifest
        .pointer("/permissions/kv/maxBytes")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_QUOTA_BYTES)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn set_then_get_round_trips() {
        let mut kv = Kv::new(1024);
        kv.set("a", vec![1, 2, 3], None).unwrap();
        assert_eq!(kv.get("a"), Some(vec![1, 2, 3]));
        assert_eq!(kv.used(), 4);
    }

    #[test]
    fn overwrite_releases_the_previous_footprint() {
        let mut kv = Kv::new(1024);
        kv.set("a", vec![0; 100], None).unwrap();
        kv.set("a", vec![0; 10], None).unwrap();
        assert_eq!(kv.used(), 11);
    }

    #[test]
    fn value_over_the_cap_is_rejected_and_nothing_is_stored() {
        let mut kv = Kv::new(u64::MAX);
        let error = kv
            .set("big", vec![0; MAX_VALUE_BYTES + 1], None)
            .unwrap_err();
        assert!(error.contains("limit"));
        assert_eq!(kv.get("big"), None);
        assert_eq!(kv.used(), 0);
    }

    #[test]
    fn quota_rejects_a_write_that_does_not_fit() {
        let mut kv = Kv::new(10);
        kv.set("a", vec![0; 5], None).unwrap();
        assert!(kv.set("b", vec![0; 5], None).is_err());
        assert_eq!(kv.get("b"), None);
        assert_eq!(kv.used(), 6);
    }

    #[test]
    fn delete_frees_budget_and_absent_keys_are_ignored() {
        let mut kv = Kv::new(64);
        kv.set("a", vec![0; 8], None).unwrap();
        kv.delete("a");
        kv.delete("a");
        assert_eq!(kv.used(), 0);
    }

    #[test]
    fn expired_entries_are_invisible() {
        let mut kv = Kv::new(64);
        kv.set("a", vec![1], Some(Duration::ZERO)).unwrap();
        assert_eq!(kv.get("a"), None);
    }

    #[test]
    fn manifest_quota_overrides_the_default() {
        assert_eq!(quota_from_manifest(&json!({})), DEFAULT_QUOTA_BYTES);
        let manifest = json!({ "permissions": { "kv": { "maxBytes": 4096 } } });
        assert_eq!(quota_from_manifest(&manifest), 4096);
    }
}
