//! Discovery suite: artist suggestions for a fixed seed.
//!
//! The seed is a well-known artist with its MusicBrainz id, so a plugin that
//! looks the artist up by id and one that searches by name can both answer.

use serde_json::json;

use super::Report;
use super::helpers::{expect_ok, items};
use crate::run::Session;

/// Name of the artist the suite asks about.
const SEED_NAME: &str = "Radiohead";

/// MusicBrainz id of the artist the suite asks about.
const SEED_MBID: &str = "a74b1b7f-71a5-4011-9441-d0b5e4122711";

/// How many suggestions the suite asks for.
const LIMIT: u32 = 5;

/// Runs the discovery checks in order.
///
/// # Parameters
///
/// - `session`: the open component on the mock host.
/// - `report`: the report the checks are recorded in.
pub(super) fn run(session: &mut Session, report: &mut Report) {
    report.check(
        "similar-artists respects the limit and names every artist",
        || {
            let input = json!({
                "user": null,
                "seed": {
                    "name": SEED_NAME,
                    "ids": [{"namespace": "mb-artist", "value": SEED_MBID}],
                },
                "limit": LIMIT,
            });
            let value = expect_ok(session, "discovery", "similar-artists", Some(input))?;
            let artists = items(&value)?;
            if artists.len() > LIMIT as usize {
                return Err(format!(
                    "returned {} artists for a limit of {LIMIT}",
                    artists.len()
                ));
            }
            for artist in artists {
                match artist.get("name").and_then(serde_json::Value::as_str) {
                    Some(name) if !name.is_empty() => {}
                    _ => return Err(format!("artist without a name: {artist}")),
                }
            }
            Ok(())
        },
    );
}
