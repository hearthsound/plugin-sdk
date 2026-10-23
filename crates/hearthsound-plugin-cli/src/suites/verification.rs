//! Verification suite: one fingerprint against one expected track.
//!
//! The suite does not require a particular verdict, because a fixture
//! fingerprint identifies nothing in a real service; it requires a verdict the
//! host can act on and a confidence score in the documented range.

use serde_json::json;

use super::Report;
use super::helpers::expect_ok;
use crate::run::Session;

/// The fingerprint the suite verifies; no real recording carries it.
const FINGERPRINT: &str = "hsplugin-conformance-fingerprint";

/// Duration reported with the fingerprint, in seconds.
const SECONDS: u32 = 120;

/// The verdicts the interface defines.
const VERDICTS: [&str; 3] = ["match", "mismatch", "unknown"];

/// Runs the verification checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check("verify answers a known verdict with a score", || {
        let input = json!({
            "expected": {
                "title": "Hello",
                "artists": ["Hearthsound"],
                "album": null,
                "duration_ms": 180_000,
                "ids": [],
            },
            "print": {"value": FINGERPRINT, "duration_secs": SECONDS},
        });
        let value = expect_ok(session, "verification", "verify", Some(input))?;
        match value.get("verdict").and_then(serde_json::Value::as_str) {
            Some(verdict) if VERDICTS.contains(&verdict) => {}
            _ => return Err(format!("verdict is not one of {VERDICTS:?}: {value}")),
        }
        match value.get("score").and_then(serde_json::Value::as_f64) {
            Some(score) if (0.0..=1.0).contains(&score) => Ok(()),
            _ => Err(format!("score is not a number from 0 to 1: {value}")),
        }
    });
}
