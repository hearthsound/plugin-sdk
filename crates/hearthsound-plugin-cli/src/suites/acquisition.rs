//! Acquisition suite: capabilities, search start, polling and unknown tokens.
//!
//! The suite asks `get-capabilities` first and searches with the granularity
//! the plugin advertises, `track` when it offers both, so an album-only
//! provider is exercised with an album query. A plugin that advertises neither
//! fails the first check.
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

/// The album title the suite searches for at album granularity.
const ALBUM_TITLE: &str = "Hello Album";

/// The album artist the suite searches for at album granularity.
const ALBUM_ARTIST: &str = "Hearthsound";

/// How long polling may run before the search counts as stuck.
const POLL_DEADLINE: Duration = Duration::from_secs(30);

/// The pause between polls.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The search granularity the suite uses for the plugin it runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Granularity {
    /// A search for one track.
    Track,
    /// A search for one album.
    Album,
}

impl Granularity {
    /// Returns the granularity's WIT name, as the query record spells it.
    fn as_str(self) -> &'static str {
        match self {
            Self::Track => "track",
            Self::Album => "album",
        }
    }
}

/// Runs the acquisition checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    let granularity = report.check("get-capabilities lists granularities", || {
        let value = expect_ok(session, "acquisition", "get-capabilities", None)?;
        let granularities = field(&value, "granularities")?;
        if granularities.is_empty() {
            return Err("no granularities declared".to_owned());
        }
        search_granularity(granularities)
    });
    let token = report.check("start-search returns a token", || {
        let granularity = granularity.ok_or_else(|| {
            "get-capabilities returned no granularity the suite can search with".to_owned()
        })?;
        let query = query(granularity);
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

/// Picks the granularity to search with from the plugin's advertised list.
///
/// `track` is preferred, because a provider that offers both can answer a
/// single-track search; `album` covers an album-only provider.
///
/// # Errors
///
/// Returns a message naming what the plugin declared when it advertises
/// neither granularity.
fn search_granularity(granularities: &[Value]) -> Result<Granularity, String> {
    let declared: Vec<&str> = granularities.iter().filter_map(Value::as_str).collect();
    if declared.contains(&"track") {
        return Ok(Granularity::Track);
    }
    if declared.contains(&"album") {
        return Ok(Granularity::Album);
    }
    Err(format!(
        "the plugin advertises neither `track` nor `album` granularity; it declares `{}`",
        declared.join("`, `")
    ))
}

/// Builds a search for one track, or for the album that contains it.
fn query(granularity: Granularity) -> Value {
    let track = json!({
        "title": "Hello",
        "artists": ["Hearthsound"],
        "track_number": null,
        "disc_number": null,
        "duration_ms": null,
        "isrc": null,
        "ids": [],
    });
    match granularity {
        Granularity::Track => json!({
            "granularity": granularity.as_str(),
            "album": null,
            "album_artist": null,
            "year": null,
            "tracks": [track],
            "preferred_formats": [],
        }),
        Granularity::Album => json!({
            "granularity": granularity.as_str(),
            "album": ALBUM_TITLE,
            "album_artist": ALBUM_ARTIST,
            "year": null,
            "tracks": [track],
            "preferred_formats": [],
        }),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Track granularity wins when the plugin advertises both.
    #[test]
    fn track_granularity_is_preferred() {
        assert_eq!(
            search_granularity(&[json!("album"), json!("track")]),
            Ok(Granularity::Track)
        );
    }

    /// An album-only provider is searched at album granularity.
    #[test]
    fn album_granularity_is_used_when_track_is_absent() {
        assert_eq!(
            search_granularity(&[json!("album")]),
            Ok(Granularity::Album)
        );
    }

    /// A provider advertising neither granularity fails with a message naming
    /// what it declared.
    #[test]
    fn an_unusable_granularity_list_names_what_was_declared() {
        let error = search_granularity(&[json!("playlist")]).unwrap_err();
        assert!(error.contains("neither `track` nor `album`"), "{error}");
        assert!(error.contains("playlist"), "{error}");
    }
}
