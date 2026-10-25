//! An album-only acquisition fixture for the `hsplugin test` CLI tests.
//!
//! The plugin advertises `album` granularity only, as a release-oriented
//! provider does, and rejects any other search granularity. The acquisition
//! suite reads `get-capabilities` before it searches, so this fixture passes
//! only when the suite asks for the granularity the plugin advertises.

use hearthsound_plugin::acquisition_provider::exports::hearthsound::plugin::acquisition::{
    self, Capabilities, DownloadState, Granularity, Query, SearchState,
};
use hearthsound_plugin::acquisition_provider::exports::hearthsound::plugin::lifecycle;
use hearthsound_plugin::types::ProviderError;

/// The search token this plugin issues.
const SEARCH_TOKEN: &str = "album-search-1";

/// The plugin implementation.
struct AlbumAcquisition;

/// Implements the lifecycle interface.
impl lifecycle::Guest for AlbumAcquisition {
    fn validate_config(_server_config: String) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn health() -> Result<String, ProviderError> {
        Ok("album-acquisition fixture".to_string())
    }
}

/// Implements the acquisition interface.
impl acquisition::Guest for AlbumAcquisition {
    fn get_capabilities() -> Capabilities {
        Capabilities {
            granularities: vec![Granularity::Album],
            progressive: false,
        }
    }

    fn start_search(query: Query) -> Result<String, ProviderError> {
        if query.granularity != Granularity::Album {
            return Err(ProviderError::InvalidConfig(
                "this provider searches for albums only".to_string(),
            ));
        }
        match query.album.as_deref() {
            Some(album) if !album.is_empty() => Ok(SEARCH_TOKEN.to_string()),
            _ => Err(ProviderError::InvalidConfig(
                "an album search names the album".to_string(),
            )),
        }
    }

    fn poll_search(search_token: String) -> Result<SearchState, ProviderError> {
        if search_token == SEARCH_TOKEN {
            Ok(SearchState::Complete(Vec::new()))
        } else {
            Err(ProviderError::NotFound)
        }
    }

    fn cancel_search(_search_token: String) {}

    fn start_download(
        _candidate_token: String,
        _files: Vec<u32>,
    ) -> Result<String, ProviderError> {
        Err(ProviderError::NotFound)
    }

    fn poll_download(_download_token: String) -> Result<DownloadState, ProviderError> {
        Err(ProviderError::NotFound)
    }

    fn cancel_download(_download_token: String) {}
}

hearthsound_plugin::export_acquisition_provider!(AlbumAcquisition);
