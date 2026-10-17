//! Conformance suites that `hsplugin test` runs against a built component.
//!
//! A suite is a numbered list of checks for one hosted layer. Each check calls
//! the component's exports through a [`Session`] on the mock host, so a plugin
//! passes only when its functions answer the way the layer's contract says.
//! The lifecycle checks (`validate-config`, `health`) run with the catalog
//! suite. A check that needs the output of an earlier check reports the missing
//! input as its own failure.

mod acquisition;
mod artwork;
mod catalog;
mod helpers;
mod metadata;

use std::path::Path;

use serde_json::Value;

use crate::check;
use crate::run::{Session, read_json, with_defaults};

/// The layers that have a conformance suite.
const LAYERS: [&str; 4] = ["catalog", "metadata", "artwork", "acquisition"];

/// The outcome of every check in one suite run, in the order they ran.
#[derive(Debug, Default)]
pub struct Report {
    entries: Vec<Entry>,
}

/// One recorded check: its name and, when it failed, why.
#[derive(Debug)]
struct Entry {
    name: &'static str,
    failure: Option<String>,
}

impl Report {
    /// Runs one check and records its outcome.
    ///
    /// Checks are numbered in the order they are run, starting at 1. The
    /// check's value is returned when it passes, so later checks can use it.
    ///
    /// # Parameters
    ///
    /// - `name`: the check's description, printed in the report.
    /// - `run`: the check; an `Err` is its failure detail.
    ///
    /// # Returns
    ///
    /// The value of a passing check, or `None` when it failed.
    pub fn check<T>(
        &mut self,
        name: &'static str,
        run: impl FnOnce() -> Result<T, String>,
    ) -> Option<T> {
        match run() {
            Ok(value) => {
                self.entries.push(Entry {
                    name,
                    failure: None,
                });
                Some(value)
            }
            Err(detail) => {
                self.entries.push(Entry {
                    name,
                    failure: Some(detail),
                });
                None
            }
        }
    }

    /// Returns whether every recorded check passed.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.entries.iter().all(|entry| entry.failure.is_none())
    }

    /// Renders one line per check: `ok <n>: <check>` for a pass, or
    /// `error <n>: <check>: <detail>` for a failure.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let number = index + 1;
                match &entry.failure {
                    None => format!("ok {number}: {}", entry.name),
                    Some(detail) => format!("error {number}: {}: {detail}", entry.name),
                }
            })
            .collect()
    }
}

/// Runs the conformance suite for one layer against a plugin.
///
/// The plugin is checked first, the configuration is read and given the
/// manifest's server-scope defaults, and the component is opened on the mock
/// host. A failing check is part of the returned [`Report`], not an error.
///
/// # Parameters
///
/// - `manifest_path`: path to the plugin's `manifest.json`.
/// - `wasm`: explicit component path, or `None` to find the build output.
/// - `config_path`: optional JSON file with the server-scoped configuration.
/// - `layer`: one of `catalog`, `metadata`, `artwork` or `acquisition`.
///
/// # Errors
///
/// Returns a message when the layer has no suite, the plugin fails the checks
/// of `hsplugin check`, the configuration cannot be read, or the component
/// cannot be opened.
pub fn test(
    manifest_path: &Path,
    wasm: Option<&Path>,
    config_path: Option<&Path>,
    layer: &str,
) -> Result<Report, String> {
    if !LAYERS.contains(&layer) {
        return Err(format!(
            "no conformance suite for `{layer}`; choose one of {}",
            LAYERS.join(", ")
        ));
    }
    let checked = check::run(manifest_path, wasm).map_err(|errors| errors.join("\n"))?;
    let config = with_defaults(
        read_json(config_path, "config")?.unwrap_or(Value::Null),
        checked.manifest().value(),
    );
    let mut session = Session::open(&checked, config.clone())?;
    let mut report = Report::default();
    match layer {
        "catalog" => catalog::run(&mut session, &config, &mut report),
        "metadata" => metadata::run(&mut session, &mut report),
        "artwork" => artwork::run(&mut session, &mut report),
        _ => acquisition::run(&mut session, &mut report),
    }
    Ok(report)
}
