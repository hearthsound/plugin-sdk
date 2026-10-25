//! Runs one exported plugin function against the mock host.
//!
//! A [`Session`] instantiates a checked component once with the mock host
//! linked in. Calls go through the component's exports by name, so the same
//! session serves `hsplugin run` and the conformance suites. Guest standard
//! output is reported as `info:` lines and standard error as `warn:` lines on
//! stderr; the function result is returned as JSON.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use wasmtime::Store;
use wasmtime::component::{Component, Instance, Type, Val};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;

use crate::check::{self, Checked};
use crate::host::{self, CALL_DEADLINE_TICKS, EpochTicker, HostState, OUTPUT_CAPACITY};
use crate::temp::TempDir;
use crate::values::{arguments, val_to_json};

/// Capability names accepted as the first half of a `run` target. `lifecycle`
/// is the base interface every provider world exports.
const CAPABILITIES: [&str; 9] = [
    "catalog",
    "metadata",
    "artwork",
    "acquisition",
    "verification",
    "scrobble",
    "discovery",
    "account-link",
    "lifecycle",
];

/// A loaded component ready for repeated calls.
///
/// A trapped call leaves the guest instance unusable, so callers that need a
/// fresh guest after a failure open a new session.
pub struct Session {
    engine: wasmtime::Engine,
    store: Store<HostState>,
    instance: Instance,
    _staging: TempDir,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    stdout_seen: usize,
    stderr_seen: usize,
    user_config: Option<Value>,
}

impl Session {
    /// Opens a session on a checked component with the given server config.
    ///
    /// The config has the manifest's server-scope schema defaults applied to
    /// every top-level property it leaves out.
    ///
    /// # Parameters
    ///
    /// - `checked`: a component that passed `hsplugin check`.
    /// - `config`: the server-scoped configuration document; `Null` means no
    ///   settings were supplied.
    ///
    /// # Errors
    ///
    /// Returns a message when the engine, the staging directory, the linker or
    /// the instantiation fails.
    pub fn open(checked: &Checked, config: Value) -> Result<Self, String> {
        Self::open_with(checked, config, HashMap::new(), None)
    }

    /// Opens a session with tool mappings for `host-process` and a user config.
    ///
    /// # Parameters
    ///
    /// - `checked`: a component that passed `hsplugin check`.
    /// - `config`: the server-scoped configuration document.
    /// - `tools`: executable paths keyed by the tool name a plugin spawns.
    /// - `user_config`: the document handed to `user-context` arguments as
    ///   their `config` JSON.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Session::open`].
    pub fn open_with(
        checked: &Checked,
        config: Value,
        tools: HashMap<String, PathBuf>,
        user_config: Option<Value>,
    ) -> Result<Self, String> {
        let config = with_defaults(config, checked.manifest().value());
        let engine = host::engine().map_err(|error| format!("{error:#}"))?;
        let component = Component::from_file(&engine, checked.wasm())
            .map_err(|error| format!("cannot load component: {error:#}"))?;
        let staging = TempDir::new("hsplugin-run").map_err(|error| error.to_string())?;
        let stdout = MemoryOutputPipe::new(OUTPUT_CAPACITY);
        let stderr = MemoryOutputPipe::new(OUTPUT_CAPACITY);
        let state = HostState::new(
            checked.manifest().value(),
            config,
            staging.path().to_path_buf(),
            stdout.clone(),
            stderr.clone(),
            tools,
        )?;
        let mut store = Store::new(&engine, state);
        let linker = host::linker(&engine).map_err(|error| format!("{error:#}"))?;
        store.set_epoch_deadline(CALL_DEADLINE_TICKS);
        let ticker = EpochTicker::start(&engine);
        let instantiated = linker.instantiate(&mut store, &component);
        drop(ticker);
        let instance =
            instantiated.map_err(|error| format!("cannot instantiate component: {error:#}"))?;
        Ok(Self {
            engine,
            store,
            instance,
            _staging: staging,
            stdout,
            stderr,
            stdout_seen: 0,
            stderr_seen: 0,
            user_config,
        })
    }

    /// Calls one exported function and returns its result as JSON.
    ///
    /// Provider functions return `{"ok": <value>}` on success and
    /// `{"error": <variant>}` on a provider error; functions with a plain
    /// return type are wrapped as `{"ok": <value>}`.
    ///
    /// # Parameters
    ///
    /// - `capability`: one of `catalog`, `metadata`, `artwork`, `acquisition`,
    ///   `verification`, `scrobble`, `discovery`, `account-link` or
    ///   `lifecycle`.
    /// - `function`: the WIT function name, in kebab-case or snake_case.
    /// - `input`: the JSON arguments. A single-parameter function takes its
    ///   argument directly; a function with several parameters takes an object
    ///   keyed by parameter name. Zero-parameter functions take `None`.
    ///
    /// # Errors
    ///
    /// Returns a message when the capability is unknown, the component does
    /// not export it, the input does not match the signature, or the guest
    /// traps or exceeds its deadline.
    pub fn call(
        &mut self,
        capability: &str,
        function: &str,
        input: Option<Value>,
    ) -> Result<Value, String> {
        let outcome = self.invoke(capability, function, input);
        self.drain_logs();
        outcome
    }

    /// Resolves the export, converts the arguments and runs the call.
    fn invoke(
        &mut self,
        capability: &str,
        function: &str,
        input: Option<Value>,
    ) -> Result<Value, String> {
        if !CAPABILITIES.contains(&capability) {
            return Err(format!("unknown capability `{capability}`"));
        }
        let interface = format!("hearthsound:plugin/{capability}@0.4.0");
        let name = function.replace('_', "-");
        let interface_index = self
            .instance
            .get_export_index(&mut self.store, None, &interface)
            .ok_or_else(|| format!("component does not export `{interface}`"))?;
        let function_index = self
            .instance
            .get_export_index(&mut self.store, Some(&interface_index), &name)
            .ok_or_else(|| format!("`{interface}` has no function `{name}`"))?;
        let func = self
            .instance
            .get_func(&mut self.store, function_index)
            .ok_or_else(|| format!("`{name}` is not a function"))?;

        let ty = func.ty(&self.store);
        let params: Vec<(String, Type)> = ty
            .params()
            .map(|(param, ty)| (param.to_owned(), ty))
            .collect();
        let returns_result = matches!(ty.results().next(), Some(Type::Result(_)));
        let args = arguments(&params, input, self.user_config.as_ref())?;
        let mut results = vec![Val::Bool(false); ty.results().len()];

        self.store.set_epoch_deadline(CALL_DEADLINE_TICKS);
        let ticker = EpochTicker::start(&self.engine);
        let outcome = func.call(&mut self.store, &args, &mut results);
        drop(ticker);
        outcome.map_err(|error| format!("guest call failed: {error:#}"))?;

        let value = results.first().map_or(Value::Null, val_to_json);
        Ok(if returns_result {
            value
        } else {
            json!({ "ok": value })
        })
    }

    /// Prints guest output produced since the last drain to stderr.
    fn drain_logs(&mut self) {
        let stdout = self.stdout.contents();
        let fresh = &stdout[self.stdout_seen.min(stdout.len())..];
        for line in String::from_utf8_lossy(fresh).lines() {
            eprintln!("info: {line}");
        }
        self.stdout_seen = stdout.len();

        let stderr = self.stderr.contents();
        let fresh = &stderr[self.stderr_seen.min(stderr.len())..];
        for line in String::from_utf8_lossy(fresh).lines() {
            eprintln!("warn: {line}");
        }
        self.stderr_seen = stderr.len();
    }
}

/// Applies the manifest's server-scope schema defaults to a config document.
///
/// Only top-level properties that the document leaves out are filled, and only
/// when the schema gives them a `default`. A `Null` document becomes an empty
/// object; a non-object document is returned unchanged.
pub(crate) fn with_defaults(config: Value, manifest: &Value) -> Value {
    let mut config = if config.is_null() { json!({}) } else { config };
    let Some(properties) = manifest
        .pointer("/config/server/schema/properties")
        .and_then(Value::as_object)
    else {
        return config;
    };
    let Some(object) = config.as_object_mut() else {
        return config;
    };
    for (key, schema) in properties {
        if let Some(default) = schema.get("default") {
            object.entry(key.clone()).or_insert_with(|| default.clone());
        }
    }
    config
}

/// Runs one function of a plugin and returns its JSON result.
///
/// The manifest and component are checked first, exactly as `hsplugin check`
/// does, so a component that would not load on a server is refused here too.
///
/// # Parameters
///
/// - `manifest_path`: the plugin's `manifest.json`.
/// - `wasm`: an explicit component path; `None` resolves it from the manifest
///   directory.
/// - `config_path`: a JSON file with the server config; `None` means defaults
///   only.
/// - `target`: `<capability>.<function>`, for example `catalog.search`.
/// - `input_path`: a JSON file with the function arguments; `None` for
///   functions without arguments.
/// - `user_config`: the document a `user-context` argument receives as its
///   `config`; `None` leaves that member empty.
/// - `tools`: executable paths keyed by tool name for `host-process`.
///
/// # Errors
///
/// Returns the check errors joined with newlines when the plugin does not pass
/// checking, or a message for an invalid target, unreadable JSON or a failed
/// call.
///
/// # Examples
///
/// ```text
/// let value = run::run(
///     Path::new("manifest.json"),
///     None,
///     None,
///     "catalog.health",
///     None,
///     None,
///     HashMap::new(),
/// )?;
/// assert_eq!(value, serde_json::json!({"ok": "ok"}));
/// ```
pub fn run(
    manifest_path: &Path,
    wasm: Option<&Path>,
    config_path: Option<&Path>,
    target: &str,
    input_path: Option<&Path>,
    user_config: Option<Value>,
    tools: HashMap<String, PathBuf>,
) -> Result<Value, String> {
    let (capability, function) = target
        .split_once('.')
        .ok_or_else(|| format!("target `{target}` must be <capability>.<function>"))?;
    let checked = check::run(manifest_path, wasm).map_err(|errors| errors.join("\n"))?;
    let config = read_json(config_path, "config")?.unwrap_or(Value::Null);
    let input = read_json(input_path, "input")?;
    let mut session = Session::open_with(&checked, config, tools, user_config)?;
    session.call(capability, function, input)
}

/// Reads and parses an optional JSON file.
///
/// # Errors
///
/// Returns a message naming the file when it cannot be read or parsed.
pub(crate) fn read_json(path: Option<&Path>, what: &str) -> Result<Option<Value>, String> {
    let Some(path) = path else {
        return Ok(None);
    };
    let text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {what} file {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("invalid {what} JSON in {}: {error}", path.display()))
}
