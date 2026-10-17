//! Metadata suite: recording resolution and artist information.
//!
//! A plugin that does not export the metadata interface fails the first check
//! with the mock host's message about the missing export.

use serde_json::json;

use super::Report;
use super::helpers::{call, expect_ok, list_or_error};
use crate::run::Session;

/// Runs the metadata checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check("resolve-recording returns a list or a known error", || {
        let hint = json!({
            "title": "Hello",
            "artists": ["Hearthsound"],
            "album": null,
            "duration_ms": null,
            "isrc": null,
        });
        let reply = call(session, "metadata", "resolve-recording", Some(hint))?;
        list_or_error(reply, &["not_found", "invalid_config"])
    });
    report.check("get-artist-info returns a result", || {
        let request = json!({"name": "Hearthsound", "ids": []});
        expect_ok(session, "metadata", "get-artist-info", Some(request))?;
        Ok(())
    });
}
