//! The mock host that `hsplugin run` and `hsplugin test` execute plugins against.
//!
//! The mock is an independent implementation of the host semantics documented
//! for plugins: a guest sees only the imports declared in the SDK's `wit/`
//! package, HTTP goes through the manifest allowlist, key-value storage honours
//! its quota, files are visible only under the staging root, and every call
//! has a deadline enforced by epoch interruption. It shares no code with the
//! server's plugin host.

pub mod files;
pub mod http;
pub mod kv;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::Value;
use wasmtime::component::{HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use self::bindings::hearthsound::plugin::{host_config, host_files, host_http, host_kv, host_log};

/// Host-side bindings generated from the SDK's WIT package.
///
/// The acquisition world is used because it imports every host interface.
/// Components of the other worlds import a subset of those interfaces, which
/// the linker accepts.
#[allow(missing_docs, unused, clippy::all)]
mod bindings {
    wasmtime::component::bindgen!({
        path: "../../wit",
        world: "hearthsound:plugin/acquisition-provider",
    });
}

/// Length of one epoch tick. The ticker thread advances the engine epoch this
/// often.
pub const EPOCH_TICK: Duration = Duration::from_millis(10);

/// Number of epoch ticks a single call may run before the guest traps: 30 s.
pub const CALL_DEADLINE_TICKS: u64 = 3000;

/// Capacity of the in-memory pipes that capture guest stdout and stderr.
pub const OUTPUT_CAPACITY: usize = 1024 * 1024;

/// Builds the engine used for every mock run, with epoch interruption enabled.
///
/// # Errors
///
/// Returns an error when the engine configuration is rejected.
///
/// # Examples
///
/// ```text
/// let engine = host::engine()?;
/// ```
pub fn engine() -> wasmtime::Result<Engine> {
    let mut config = Config::new();
    config.epoch_interruption(true);
    Engine::new(&config)
}

/// Builds the linker that provides WASI p2 and the Hearthsound host
/// interfaces.
///
/// WASI is linked without preopened directories and without socket access.
///
/// # Errors
///
/// Returns an error when an interface cannot be added to the linker.
pub fn linker(engine: &Engine) -> wasmtime::Result<Linker<HostState>> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    bindings::AcquisitionProvider::add_to_linker::<HostState, HasSelf<HostState>>(
        &mut linker,
        |state| state,
    )?;
    Ok(linker)
}

/// Advances the engine epoch every [`EPOCH_TICK`] until dropped.
///
/// Holding a ticker for the duration of a call makes a guest that never yields
/// trap once it has used [`CALL_DEADLINE_TICKS`] ticks.
#[derive(Debug)]
pub struct EpochTicker {
    stop: Arc<AtomicBool>,
}

impl EpochTicker {
    /// Starts the ticker thread for `engine`.
    ///
    /// # Parameters
    ///
    /// - `engine`: the engine whose epoch is advanced.
    pub fn start(engine: &Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let engine = engine.clone();
        thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                thread::sleep(EPOCH_TICK);
                engine.increment_epoch();
            }
        });
        Self { stop }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// The per-run state that backs the host imports of one guest.
pub struct HostState {
    wasi: WasiCtx,
    table: ResourceTable,
    policy: http::Policy,
    kv: kv::Kv,
    files: files::Files,
    config: Value,
}

impl HostState {
    /// Builds the host state for one run of a plugin.
    ///
    /// # Parameters
    ///
    /// - `manifest`: the parsed plugin manifest; supplies the HTTP allowlist,
    ///   the KV quota and the plugin id used in the user agent.
    /// - `config`: the server-scoped configuration handed to the guest.
    /// - `staging`: the directory exposed as the `staging` file root.
    /// - `stdout`, `stderr`: pipes that capture the guest's standard streams.
    ///
    /// # Errors
    ///
    /// Returns an error when the HTTP policy cannot be built from the
    /// manifest.
    pub fn new(
        manifest: &Value,
        config: Value,
        staging: PathBuf,
        stdout: MemoryOutputPipe,
        stderr: MemoryOutputPipe,
    ) -> Result<Self, String> {
        let id = manifest
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("plugin");
        let policy = http::Policy::from_manifest(manifest, format!("hsplugin-run/{id}"))
            .map_err(|error| format!("{error:#}"))?;
        let wasi = WasiCtxBuilder::new().stdout(stdout).stderr(stderr).build();
        Ok(Self {
            wasi,
            table: ResourceTable::new(),
            policy,
            kv: kv::Kv::new(kv::quota_from_manifest(manifest)),
            files: files::Files::new(staging),
            config,
        })
    }
}

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl host_log::Host for HostState {
    fn log(&mut self, level: host_log::Level, message: String) {
        let label = match level {
            host_log::Level::Trace => "trace",
            host_log::Level::Debug => "debug",
            host_log::Level::Info => "info",
            host_log::Level::Warn => "warn",
            host_log::Level::Error => "error",
        };
        eprintln!("{label}: {message}");
    }
}

impl host_http::Host for HostState {
    fn send(
        &mut self,
        request: host_http::Request,
    ) -> Result<host_http::Response, host_http::HttpError> {
        let request = http::Request {
            method: request.method,
            url: request.url,
            headers: request.headers,
            body: request.body,
            timeout_ms: request.timeout_ms,
        };
        match self.policy.send(request, &self.config) {
            Ok(response) => Ok(host_http::Response {
                status: response.status,
                headers: response.headers,
                body: response.body,
            }),
            Err(failure) => Err(match failure {
                http::Failure::Denied(detail) => host_http::HttpError::Denied(detail),
                http::Failure::Timeout => host_http::HttpError::Timeout,
                http::Failure::BodyTooLarge => host_http::HttpError::BodyTooLarge,
                http::Failure::Network(detail) => host_http::HttpError::Network(detail),
            }),
        }
    }
}

impl host_kv::Host for HostState {
    fn get(&mut self, key: String) -> Option<Vec<u8>> {
        self.kv.get(&key)
    }

    fn set(&mut self, key: String, value: Vec<u8>, ttl_ms: Option<u64>) -> Result<(), String> {
        self.kv.set(&key, value, ttl_ms.map(Duration::from_millis))
    }

    fn delete(&mut self, key: String) {
        self.kv.delete(&key);
    }
}

impl host_config::Host for HostState {
    fn server_config(&mut self) -> String {
        self.config.to_string()
    }
}

impl host_files::Host for HostState {
    fn stat(&mut self, root: String, path: String) -> Option<host_files::FileStat> {
        self.files
            .stat(&root, &path)
            .map(|stat| host_files::FileStat {
                size: stat.size,
                modified_ms: stat.modified_ms,
                is_dir: stat.is_dir,
            })
    }

    fn list_dir(&mut self, root: String, path: String) -> Result<Vec<String>, String> {
        self.files.list_dir(&root, &path)
    }
}

impl bindings::hearthsound::plugin::types::Host for HostState {}
