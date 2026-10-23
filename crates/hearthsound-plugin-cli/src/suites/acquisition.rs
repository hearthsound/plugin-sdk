//! Acquisition suite: capabilities, search start, polling and unknown tokens.
//!
//! The `search-state` variant reports a search as `running` or `complete` only,
//! so a provider error from `poll-search` is the terminal failure state.
//! Polling passes when the search reaches either terminal state within the
//! deadline.

use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::Report;
use super::helpers::{Reply, call, expect_ok, field};
use crate::run::Session;

/// An identifier that no acquisition plugin should have issued.
const UNKNOWN_TOKEN: &str = "hsplugin-conformance-unknown-token";

/// How long polling may run before the search counts as stuck.
const POLL_DEADLINE: Duration = Duration::from_secs(30);

/// The pause between polls.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Runs the acquisition checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check("get-capabilities lists granularities", || {
        let value = expect_ok(session, "acquisition", "get-capabilities", None)?;
        let granularities = field(&value, "granularities")?;
        if granularities.is_empty() {
            Err("no granularities declared".to_owned())
        } else {
            Ok(())
        }
    });
    let token = report.check("start-search returns a token", || {
        let query = generic_query();
        match expect_ok(session, "acquisition", "start-search", Some(query))? {
            Value::String(token) if !token.is_empty() => Ok(token),
            other => Err(format!("expected a non-empty token, got {other}")),
        }
    });
    report.check("poll-search reaches complete or failed in time", || {
        let token = token
            .as_deref()
            .ok_or_else(|| "start-search returned no token".to_owned())?;
        poll_until_terminal(session, token)
    });
    report.check("poll-search rejects an unknown token", || {
        match call(
            session,
            "acquisition",
            "poll-search",
            Some(json!(UNKNOWN_TOKEN)),
        )? {
            Reply::Err(_) => Ok(()),
            Reply::Ok(state) => Err(format!("answered an unknown token with {state}")),
        }
    });
}

/// Builds a search for one track with no album or year constraints.
fn generic_query() -> Value {
    json!({
        "granularity": "track",
        "album": null,
        "album_artist": null,
        "year": null,
        "tracks": [{
            "title": "Hello",
            "artists": ["Hearthsound"],
            "track_number": null,
            "disc_number": null,
            "duration_ms": null,
            "isrc": null,
            "ids": [],
        }],
        "preferred_formats": [],
    })
}

/// Polls a search until it completes or the provider reports a failure.
///
/// # Errors
///
/// Returns a message when the deadline passes while the search is still
/// running, or when the reply is not a recognised search state.
fn poll_until_terminal(session: &mut Session, token: &str) -> Result<(), String> {
    let deadline = Instant::now() + POLL_DEADLINE;
    loop {
        match call(session, "acquisition", "poll-search", Some(json!(token)))? {
            Reply::Err(_) => return Ok(()),
            Reply::Ok(state) => {
                if state.get("complete").is_some() {
                    return Ok(());
                }
                if state.get("running").is_none() {
                    return Err(format!("unrecognised search state {state}"));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("search was still running at the deadline".to_owned());
        }
        thread::sleep(POLL_INTERVAL);
    }
}
