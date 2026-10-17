//! Catalog suite: the lifecycle checks (`validate-config`, `health`) and the
//! `catalog` interface checks.
//!
//! Inputs are derived from a search for the word `hello`, so the suite works
//! for any catalog plugin that answers that search with at least one result.
//! The unknown-id checks use an id no plugin should recognise.

use serde_json::{Value, json};

use super::Report;
use super::helpers::{
    Reply, call, expect_id, expect_ok, field, first, ids, items, list_or_error, named_error,
};
use crate::run::Session;

/// An id that no catalog plugin should recognise.
const UNKNOWN_ID: &str = "hsplugin-conformance-unknown-id";

/// A well-formed ISRC used for the lookup check.
const ISRC: &str = "USRC17607839";

/// The ids a search returned, grouped by kind.
#[derive(Default)]
struct Found {
    tracks: Vec<String>,
    albums: Vec<String>,
    artists: Vec<String>,
}

/// Runs the catalog and lifecycle checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `config`: the defaulted server configuration the component was opened with.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, config: &Value, report: &mut Report) {
    report.check("validate-config accepts the configuration", || {
        let input = Value::String(config.to_string());
        match call(session, "lifecycle", "validate-config", Some(input))? {
            Reply::Ok(_) => Ok(()),
            Reply::Err(messages) => Err(format!("rejected the configuration: {messages}")),
        }
    });
    report.check("validate-config rejects invalid JSON", || {
        match call(
            session,
            "lifecycle",
            "validate-config",
            Some(json!("{not json")),
        )? {
            Reply::Err(Value::Array(messages)) if !messages.is_empty() => Ok(()),
            Reply::Err(other) => Err(format!("expected a non-empty list, got {other}")),
            Reply::Ok(_) => Err("accepted invalid JSON".to_owned()),
        }
    });
    report.check("health returns a status string", || {
        match expect_ok(session, "lifecycle", "health", None)? {
            Value::String(_) => Ok(()),
            other => Err(format!("expected a string, got {other}")),
        }
    });
    let found = report
        .check("search returns track, album and artist lists", || {
            let value = expect_ok(session, "catalog", "search", Some(search_input(4)))?;
            Ok(Found {
                tracks: ids(field(&value, "tracks")?)?,
                albums: ids(field(&value, "albums")?)?,
                artists: ids(field(&value, "artists")?)?,
            })
        })
        .unwrap_or_default();
    report.check("get-track returns each searched track", || {
        require(&found.tracks, "tracks")?;
        for id in &found.tracks {
            let value = expect_ok(session, "catalog", "get-track", Some(json!(id)))?;
            expect_id(&value, id)?;
        }
        Ok(())
    });
    report.check("get-album returns each searched album", || {
        require(&found.albums, "albums")?;
        for id in &found.albums {
            let value = expect_ok(session, "catalog", "get-album", Some(json!(id)))?;
            expect_id(&value, id)?;
        }
        Ok(())
    });
    report.check("get-artist returns each searched artist", || {
        require(&found.artists, "artists")?;
        for id in &found.artists {
            let value = expect_ok(session, "catalog", "get-artist", Some(json!(id)))?;
            expect_id(&value, id)?;
        }
        Ok(())
    });
    report.check("artist-albums respects limit and offset", || {
        let artist = first(&found.artists, "artists")?;
        let page = expect_ok(
            session,
            "catalog",
            "artist-albums",
            Some(json!({"id": artist, "limit": 1, "offset": 0})),
        )?;
        if items(&page)?.len() > 1 {
            return Err("returned more than the limit of 1".to_owned());
        }
        let beyond = expect_ok(
            session,
            "catalog",
            "artist-albums",
            Some(json!({"id": artist, "limit": 1, "offset": 1_000_000})),
        )?;
        if items(&beyond)?.is_empty() {
            Ok(())
        } else {
            Err("returned albums past the end of the list".to_owned())
        }
    });
    report.check("artist-top-tracks respects limit", || {
        let artist = first(&found.artists, "artists")?;
        let value = expect_ok(
            session,
            "catalog",
            "artist-top-tracks",
            Some(json!({"id": artist, "limit": 1})),
        )?;
        if items(&value)?.len() <= 1 {
            Ok(())
        } else {
            Err("returned more than the limit of 1".to_owned())
        }
    });
    report.check(
        "get-track reports not-found for an unknown id",
        || match call(session, "catalog", "get-track", Some(json!(UNKNOWN_ID)))? {
            Reply::Err(error) => named_error(&error, &["not_found"]),
            Reply::Ok(value) => Err(format!("returned a track for an unknown id: {value}")),
        },
    );
    report.check("lookup-isrc returns a list or not-found", || {
        let reply = call(session, "catalog", "lookup-isrc", Some(json!(ISRC)))?;
        list_or_error(reply, &["not_found"])
    });
    report.check("an empty search returns empty lists", || {
        let value = expect_ok(session, "catalog", "search", Some(search_input(0)))?;
        for name in ["tracks", "albums", "artists"] {
            if !field(&value, name)?.is_empty() {
                return Err(format!("`{name}` is not empty for a zero limit"));
            }
        }
        Ok(())
    });
}

/// Builds the search input for the word `hello` with the given limit.
fn search_input(limit: u32) -> Value {
    json!({"text": "hello", "kinds": [], "limit": limit, "offset": 0})
}

/// Fails when a search found no ids of the given kind.
///
/// # Errors
///
/// Returns a message naming `what` when `found` is empty.
fn require(found: &[String], what: &str) -> Result<(), String> {
    if found.is_empty() {
        Err(format!("search returned no {what}"))
    } else {
        Ok(())
    }
}
