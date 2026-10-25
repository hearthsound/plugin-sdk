//! A process-acquisition fixture for the `hsplugin test` CLI tests.
//!
//! The plugin answers the acquisition conformance suite, and its `start-search`
//! runs the tool its manifest declares, `hello-tool`, before it returns a token.
//! A test that maps the name with `--tool hello-tool=<path>` and then finds the
//! tool's marker file proves the mapping reached the mock host's process table.

use std::thread;
use std::time::{Duration, Instant};

use hearthsound_plugin::host::process::{
    self, ProcessError, ProcessState, SpawnOptions, StdoutMode,
};
use hearthsound_plugin::process_acquisition_provider::exports::hearthsound::plugin::acquisition::{
    self, Capabilities, DownloadState, Granularity, Query, SearchState,
};
use hearthsound_plugin::process_acquisition_provider::exports::hearthsound::plugin::lifecycle;
use hearthsound_plugin::types::ProviderError;

/// The tool the manifest declares and `start-search` runs.
const TOOL: &str = "hello-tool";

/// The search token this plugin issues.
const SEARCH_TOKEN: &str = "hello-search-1";

/// How long `start-search` waits for the tool to exit.
const TOOL_DEADLINE: Duration = Duration::from_secs(10);

/// The pause between polls of a running tool.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// The plugin implementation.
struct ProcessAcquisition;

/// Implements the lifecycle interface.
impl lifecycle::Guest for ProcessAcquisition {
    fn validate_config(_server_config: String) -> Result<(), Vec<String>> {
        Ok(())
    }

    fn health() -> Result<String, ProviderError> {
        Ok("process-acquisition fixture".to_string())
    }
}

/// Implements the acquisition interface.
impl acquisition::Guest for ProcessAcquisition {
    fn get_capabilities() -> Capabilities {
        Capabilities {
            granularities: vec![Granularity::Track],
            progressive: false,
        }
    }

    fn start_search(_query: Query) -> Result<String, ProviderError> {
        run_tool()?;
        Ok(SEARCH_TOKEN.to_string())
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

/// Runs the configured tool and waits for it to exit.
///
/// # Errors
///
/// Returns [`ProviderError::InvalidConfig`] when the tool has no executable
/// configured, and [`ProviderError::Internal`] when the tool cannot be started
/// or does not finish in time.
fn run_tool() -> Result<(), ProviderError> {
    let options = SpawnOptions {
        cwd: None,
        env: Vec::new(),
        stdout: StdoutMode::Capture,
        timeout_ms: 10_000,
    };
    let handle = process::spawn(TOOL, &[], &options).map_err(|error| match error {
        ProcessError::NotConfigured(detail) => ProviderError::InvalidConfig(detail),
        other => ProviderError::Internal(format!("cannot start `{TOOL}`: {other:?}")),
    })?;
    let deadline = Instant::now() + TOOL_DEADLINE;
    loop {
        match process::poll(handle) {
            Ok(ProcessState::Running) => {}
            Ok(_) => return Ok(()),
            Err(error) => {
                return Err(ProviderError::Internal(format!(
                    "cannot poll `{TOOL}`: {error:?}"
                )));
            }
        }
        if Instant::now() >= deadline {
            process::kill(handle);
            return Err(ProviderError::Internal(format!(
                "`{TOOL}` did not finish in time"
            )));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

hearthsound_plugin::export_process_acquisition_provider!(ProcessAcquisition);
