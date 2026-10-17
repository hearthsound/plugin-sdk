//! Artwork suite: image lookup for each subject kind.
//!
//! Track, album and artist subjects may answer with a list or `not-found`. A
//! playlist subject is outside the artwork contract for this suite and must
//! answer with an empty list.

use serde_json::{Value, json};

use super::Report;
use super::helpers::{call, expect_ok, items, list_or_error};
use crate::run::Session;

/// Runs the artwork checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check("find-artwork returns a list for a track subject", || {
        let reply = call(
            session,
            "artwork",
            "find-artwork",
            Some(subject("track", "Hello")),
        )?;
        list_or_error(reply, &["not_found"])
    });
    report.check("find-artwork returns a list for an album subject", || {
        let reply = call(
            session,
            "artwork",
            "find-artwork",
            Some(subject("album", "Hello")),
        )?;
        list_or_error(reply, &["not_found"])
    });
    report.check("find-artwork returns a list for an artist subject", || {
        let input = subject("artist", "Hearthsound");
        let reply = call(session, "artwork", "find-artwork", Some(input))?;
        list_or_error(reply, &["not_found"])
    });
    report.check("find-artwork returns an empty list for a playlist", || {
        let input = subject("playlist", "Hello");
        let value = expect_ok(session, "artwork", "find-artwork", Some(input))?;
        if items(&value)?.is_empty() {
            Ok(())
        } else {
            Err("returned images for a playlist subject".to_owned())
        }
    });
}

/// Builds an artwork subject of one entity kind with no provider ids.
fn subject(kind: &str, title: &str) -> Value {
    json!({
        "kind": kind,
        "provider_id": null,
        "ids": [],
        "title": title,
        "artists": [],
    })
}
