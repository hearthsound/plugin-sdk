//! Scrobble suite: the delivery limits the host paces calls with.
//!
//! A plugin that exports `scrobble` must state how many listens one call may
//! carry and how far apart two calls to the service must be, so a host can
//! batch without flooding the service.

use serde_json::Value;

use super::Report;
use super::helpers::expect_ok;
use crate::run::Session;

/// Longest interval a service may ask the host to wait between two calls.
const MAX_INTERVAL_MS: u64 = 60_000;

/// Runs the scrobble checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check("get-limits allows a batch of at least one listen", || {
        let limits = expect_ok(session, "scrobble", "get-limits", None)?;
        match limits.get("max_batch").and_then(Value::as_u64) {
            Some(max) if max >= 1 => Ok(()),
            _ => Err(format!("max_batch is missing or zero: {limits}")),
        }
    });
    report.check("get-limits paces calls within a minute", || {
        let limits = expect_ok(session, "scrobble", "get-limits", None)?;
        match limits.get("min_interval_ms").and_then(Value::as_u64) {
            Some(interval) if interval <= MAX_INTERVAL_MS => Ok(()),
            Some(interval) => Err(format!(
                "min_interval_ms is {interval}, above the {MAX_INTERVAL_MS} ms maximum"
            )),
            None => Err(format!("min_interval_ms is missing: {limits}")),
        }
    });
}
