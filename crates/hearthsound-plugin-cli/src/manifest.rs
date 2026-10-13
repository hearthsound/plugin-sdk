//! Access to a parsed plugin `manifest.json`.
//!
//! The manifest is kept as a [`serde_json::Value`] because the checks in
//! [`crate::check`] walk it by JSON pointer and report errors against pointers
//! such as `/resources/maxInstances`.

use serde_json::Value;

/// A parsed plugin manifest.
#[derive(Debug)]
pub struct Manifest {
    value: Value,
}

impl Manifest {
    /// Parses the bytes of a `manifest.json`.
    ///
    /// # Errors
    ///
    /// Returns a message naming the file when the bytes are not valid JSON.
    pub fn parse(bytes: &[u8]) -> Result<Manifest, String> {
        let value: Value =
            serde_json::from_slice(bytes).map_err(|e| format!("manifest.json: {e}"))?;
        Ok(Manifest { value })
    }

    /// Returns the raw manifest value.
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the plugin id, or the empty string when it is absent or not a
    /// string.
    pub fn id(&self) -> &str {
        self.value.get("id").and_then(Value::as_str).unwrap_or("")
    }

    /// Returns the plugin version, or the empty string when it is absent or not
    /// a string.
    pub fn version(&self) -> &str {
        self.value
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("")
    }

    /// Returns the provided layers, in manifest order.
    pub fn provides(&self) -> Vec<String> {
        self.value
            .get("provides")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
}
