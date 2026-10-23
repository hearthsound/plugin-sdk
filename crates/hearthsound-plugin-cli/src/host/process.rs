//! The `host-process` import: the external tools an acquisition plugin runs.
//!
//! The mock host keeps one process table per run. A plugin may spawn a tool from
//! its manifest's `permissions.process.tools`, and the tool's executable comes
//! from the `--tool <name>=<path>` mapping; a tool without a mapping is
//! `not-configured`. Processes run without a shell, with a cleared environment
//! except the allowlisted server variables and the names the plugin supplies,
//! and with a wall-clock limit after which the host kills them. At most four
//! processes run at once, and each stream keeps at most 1 MiB unread.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use super::bindings::hearthsound::plugin::host_process::{
    ProcessError, ProcessHandle, ProcessState, SpawnOptions, StdoutMode,
};

/// Most processes one instance may have running at the same time.
const MAX_LIVE: usize = 4;

/// Most bytes kept unread per stream; bytes beyond the cap are dropped.
const UNREAD_CAP: usize = 1024 * 1024;

/// Longest wall-clock limit a spawn option may ask for, in milliseconds.
const MAX_TIMEOUT_MS: u32 = 3_600_000;

/// How long an exited process stays pollable before its handle is forgotten.
const RETENTION: Duration = Duration::from_secs(600);

/// Server environment variables passed to a tool.
///
/// Every other variable is cleared, and a plugin may only add these names.
const ENV_ALLOWLIST: [&str; 2] = ["PATH", "HOME"];

/// How often the monitor thread reaps the child and checks its deadline.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// What became of a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    /// Still running.
    Running,
    /// Exited with this status code, or -1 when it was killed by a signal.
    Exited(i32),
    /// The host killed it through `kill`.
    Killed,
    /// The wall-clock limit passed and the host killed it.
    TimedOut,
}

impl Status {
    /// Renders the status as the import's process state.
    fn to_wit(self) -> ProcessState {
        match self {
            Self::Running => ProcessState::Running,
            Self::Exited(code) => ProcessState::Exited(code),
            Self::Killed => ProcessState::Killed,
            Self::TimedOut => ProcessState::TimedOut,
        }
    }
}

/// A bounded byte queue that keeps what a process wrote and was not read yet.
#[derive(Debug, Default)]
struct CappedBuffer {
    data: Vec<u8>,
}

impl CappedBuffer {
    /// Appends as much of `bytes` as fits under the unread cap.
    fn push(&mut self, bytes: &[u8]) {
        let room = UNREAD_CAP.saturating_sub(self.data.len());
        self.data.extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    /// Returns everything buffered and empties the queue.
    fn drain(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }
}

/// Everything one running or finished process exposes to the host.
struct Shared {
    /// What became of the process.
    status: Status,
    /// The child while it still needs to be polled or killed.
    child: Option<Child>,
    /// When the process reached a terminal state.
    exited_at: Option<Instant>,
    /// Captured standard output not read yet.
    stdout: CappedBuffer,
    /// Captured standard error not read yet.
    stderr: CappedBuffer,
}

/// The process table of one guest run.
pub struct Processes {
    /// Directory every relative path of a spawn resolves against.
    staging: PathBuf,
    /// Tool names the manifest declares.
    declared: Vec<String>,
    /// Executable paths from the `--tool` mapping.
    tools: HashMap<String, PathBuf>,
    /// Live and recently exited processes, keyed by handle.
    table: HashMap<ProcessHandle, Arc<Mutex<Shared>>>,
    /// Next handle to hand out.
    next: ProcessHandle,
}

impl Processes {
    /// Builds an empty process table for one run.
    ///
    /// # Parameters
    ///
    /// - `staging`: the directory the guest sees as `staging`; relative paths
    ///   resolve against it.
    /// - `declared`: tool names from `permissions.process.tools`.
    /// - `tools`: executable paths from the `--tool` mapping.
    pub fn new(staging: PathBuf, declared: Vec<String>, tools: HashMap<String, PathBuf>) -> Self {
        Self {
            staging,
            declared,
            tools,
            table: HashMap::new(),
            next: 0,
        }
    }

    /// Starts a tool and returns its handle.
    ///
    /// # Parameters
    ///
    /// - `tool`: a name from the manifest, never a path.
    /// - `args`: the argument vector; no shell is involved.
    /// - `options`: working directory, extra environment, stdout sink and
    ///   timeout.
    ///
    /// # Errors
    ///
    /// Returns `denied` for an undeclared tool, a path outside the staging
    /// directory, a non-allowlisted environment name or a fifth live process;
    /// `not-configured` when the tool has no usable executable; and
    /// `spawn-failed` when the operating system refuses to start it.
    pub fn spawn(
        &mut self,
        tool: &str,
        args: &[String],
        options: SpawnOptions,
    ) -> Result<ProcessHandle, ProcessError> {
        if !self.declared.iter().any(|declared| declared == tool) {
            return Err(ProcessError::Denied(format!(
                "`{tool}` is not one of the plugin's process tools"
            )));
        }
        let Some(executable) = self.tools.get(tool) else {
            return Err(ProcessError::NotConfigured(format!(
                "no executable is configured for `{tool}`"
            )));
        };
        if !is_executable(executable) {
            return Err(ProcessError::NotConfigured(format!(
                "`{}` is not an executable file",
                executable.display()
            )));
        }
        let live = self
            .table
            .values()
            .filter(|shared| lock(shared).status == Status::Running)
            .count();
        if live >= MAX_LIVE {
            return Err(ProcessError::Denied("too many processes".into()));
        }

        let mut command = Command::new(executable);
        command.args(args).env_clear().stdin(Stdio::null());
        for name in ENV_ALLOWLIST {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        for (name, value) in options.env {
            if !ENV_ALLOWLIST.contains(&name.as_str()) {
                return Err(ProcessError::Denied(format!(
                    "environment variable `{name}` is not allowlisted"
                )));
            }
            command.env(name, value);
        }
        if let Some(cwd) = &options.cwd {
            command.current_dir(self.inside(cwd).map_err(ProcessError::Denied)?);
        }
        command.stdout(match &options.stdout {
            StdoutMode::Capture => Stdio::piped(),
            StdoutMode::File(relative) => {
                let target = self.inside(relative).map_err(ProcessError::Denied)?;
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)
                        .map_err(|error| ProcessError::Denied(format!("{relative}: {error}")))?;
                }
                let file = std::fs::File::create(&target)
                    .map_err(|error| ProcessError::Denied(format!("{relative}: {error}")))?;
                Stdio::from(file)
            }
        });
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|error| ProcessError::SpawnFailed(format!("{tool}: {error}")))?;
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let shared = Arc::new(Mutex::new(Shared {
            status: Status::Running,
            child: Some(child),
            exited_at: None,
            stdout: CappedBuffer::default(),
            stderr: CappedBuffer::default(),
        }));
        if let Some(stdout) = stdout_pipe {
            spawn_reader(stdout, Arc::clone(&shared), false);
        }
        if let Some(stderr) = stderr_pipe {
            spawn_reader(stderr, Arc::clone(&shared), true);
        }
        let timeout = Duration::from_millis(u64::from(options.timeout_ms.min(MAX_TIMEOUT_MS)));
        spawn_monitor(Arc::clone(&shared), Instant::now() + timeout);

        let handle = self.next;
        self.next = self.next.saturating_add(1);
        self.table.insert(handle, shared);
        Ok(handle)
    }

    /// Returns the state of a process.
    ///
    /// # Errors
    ///
    /// Returns `unknown-handle` for a handle this run never issued, already
    /// released, or that exited more than ten minutes ago.
    pub fn poll(&mut self, handle: ProcessHandle) -> Result<ProcessState, ProcessError> {
        let shared = self.lookup(handle)?;
        Ok(lock(&shared).status.to_wit())
    }

    /// Returns and drains the captured standard output of a process.
    ///
    /// # Errors
    ///
    /// Returns `unknown-handle` like [`Processes::poll`].
    pub fn read_stdout(&mut self, handle: ProcessHandle) -> Result<Vec<u8>, ProcessError> {
        let shared = self.lookup(handle)?;
        Ok(lock(&shared).stdout.drain())
    }

    /// Returns and drains the captured standard error of a process.
    ///
    /// # Errors
    ///
    /// Returns `unknown-handle` like [`Processes::poll`].
    pub fn read_stderr(&mut self, handle: ProcessHandle) -> Result<Vec<u8>, ProcessError> {
        let shared = self.lookup(handle)?;
        Ok(lock(&shared).stderr.drain())
    }

    /// Kills a process that still runs and releases its handle.
    ///
    /// An unknown handle is ignored, because the interface's `kill` returns
    /// nothing.
    pub fn kill(&mut self, handle: ProcessHandle) {
        let Some(shared) = self.table.remove(&handle) else {
            return;
        };
        let mut state = lock(&shared);
        if state.status == Status::Running {
            if let Some(mut child) = state.child.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            state.status = Status::Killed;
            state.exited_at = Some(Instant::now());
        }
    }

    /// Resolves an optional path inside the staging directory.
    ///
    /// # Errors
    ///
    /// Returns a message when the path is absolute or contains a `..`
    /// component, which would escape the staging directory or reach a sibling
    /// of it.
    fn inside(&self, relative: &str) -> Result<PathBuf, String> {
        let path = Path::new(relative);
        if path.is_absolute() {
            return Err(format!("`{relative}` is absolute"));
        }
        for component in path.components() {
            match component {
                Component::ParentDir => {
                    return Err(format!("`{relative}` leaves the staging directory"));
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("`{relative}` is not relative"));
                }
                Component::CurDir | Component::Normal(_) => {}
            }
        }
        Ok(self.staging.join(path))
    }

    /// Looks up a handle, forgetting processes that exited too long ago.
    ///
    /// # Errors
    ///
    /// Returns `unknown-handle` for a handle that is not in the table.
    fn lookup(&mut self, handle: ProcessHandle) -> Result<Arc<Mutex<Shared>>, ProcessError> {
        self.table.retain(|_, shared| match lock(shared).exited_at {
            Some(exited_at) => exited_at.elapsed() < RETENTION,
            None => true,
        });
        self.table
            .get(&handle)
            .cloned()
            .ok_or(ProcessError::UnknownHandle)
    }
}

impl Drop for Processes {
    fn drop(&mut self) {
        for shared in self.table.values() {
            let mut state = lock(shared);
            if state.status == Status::Running {
                if let Some(mut child) = state.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                state.status = Status::Killed;
                state.exited_at = Some(Instant::now());
            }
        }
    }
}

/// Locks a process entry, recovering the data when another thread panicked
/// while holding the lock.
fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    match shared.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Returns whether `path` is a file the current user can execute.
fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Reads `reader` into one of the process's buffers until it closes.
///
/// Runs on its own thread so a verbose tool never blocks the guest.
fn spawn_reader(mut reader: impl Read + Send + 'static, shared: Arc<Mutex<Shared>>, stderr: bool) {
    thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    let mut state = lock(&shared);
                    if stderr {
                        state.stderr.push(&buffer[..count]);
                    } else {
                        state.stdout.push(&buffer[..count]);
                    }
                }
            }
        }
    });
}

/// Watches a process until it exits or its deadline passes.
///
/// The monitor reaps the child, records its exit status and kills it when the
/// wall-clock limit passes.
fn spawn_monitor(shared: Arc<Mutex<Shared>>, deadline: Instant) {
    thread::spawn(move || {
        loop {
            let mut state = lock(&shared);
            if state.status != Status::Running {
                break;
            }
            let exited = match state.child.as_mut() {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => Some(status.code().unwrap_or(-1)),
                    Ok(None) => None,
                    Err(_) => Some(-1),
                },
                None => None,
            };
            if let Some(code) = exited {
                state.status = Status::Exited(code);
                state.child = None;
                state.exited_at = Some(Instant::now());
                break;
            }
            if Instant::now() >= deadline {
                if let Some(mut child) = state.child.take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                state.status = Status::TimedOut;
                state.exited_at = Some(Instant::now());
                break;
            }
            drop(state);
            thread::sleep(POLL_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a table whose `yt-dlp` tool is `/bin/sh`.
    fn table() -> (crate::temp::TempDir, Processes) {
        let staging = match crate::temp::TempDir::new("hsplugin-process") {
            Ok(dir) => dir,
            Err(error) => panic!("scratch directory: {error}"),
        };
        let mut tools = HashMap::new();
        tools.insert("yt-dlp".to_string(), PathBuf::from("/bin/sh"));
        let processes = Processes::new(
            staging.path().to_path_buf(),
            vec!["yt-dlp".to_string()],
            tools,
        );
        (staging, processes)
    }

    /// Builds spawn options that capture output with a one-minute deadline.
    fn options(timeout_ms: u32) -> SpawnOptions {
        SpawnOptions {
            cwd: None,
            env: Vec::new(),
            stdout: StdoutMode::Capture,
            timeout_ms,
        }
    }

    /// Waits for a process to reach a terminal state.
    fn wait(processes: &mut Processes, handle: ProcessHandle) -> ProcessState {
        for _ in 0..500 {
            match processes.poll(handle) {
                Ok(ProcessState::Running) => std::thread::sleep(POLL_INTERVAL),
                Ok(state) => return state,
                Err(_) => return ProcessState::Killed,
            }
        }
        panic!("the process did not finish");
    }

    /// A tool the manifest does not declare cannot be spawned.
    #[test]
    fn an_undeclared_tool_is_denied() {
        let (_staging, mut processes) = table();
        match processes.spawn("ffmpeg", &[], options(1_000)) {
            Err(ProcessError::Denied(detail)) => assert!(detail.contains("ffmpeg"), "{detail}"),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    /// A declared tool without a mapping is not configured.
    #[test]
    fn an_unmapped_tool_is_not_configured() {
        let mut processes = Processes::new(
            std::env::temp_dir(),
            vec!["yt-dlp".to_string()],
            HashMap::new(),
        );
        match processes.spawn("yt-dlp", &[], options(1_000)) {
            Err(ProcessError::NotConfigured(detail)) => {
                assert!(detail.contains("yt-dlp"), "{detail}");
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }

    /// A path that leaves the staging directory is refused.
    #[test]
    fn paths_outside_staging_are_denied() {
        let (_staging, mut processes) = table();
        let mut parent = options(1_000);
        parent.cwd = Some("../elsewhere".to_string());
        match processes.spawn("yt-dlp", &[], parent) {
            Err(ProcessError::Denied(detail)) => assert!(detail.contains("leaves"), "{detail}"),
            other => panic!("unexpected result: {other:?}"),
        }
        let mut absolute = options(1_000);
        absolute.stdout = StdoutMode::File("/tmp/output".to_string());
        match processes.spawn("yt-dlp", &[], absolute) {
            Err(ProcessError::Denied(detail)) => assert!(detail.contains("absolute"), "{detail}"),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    /// Only allowlisted environment names may be added.
    #[test]
    fn environment_names_must_be_allowlisted() {
        let (_staging, mut processes) = table();
        let mut env = options(1_000);
        env.env = vec![("SECRET".to_string(), "value".to_string())];
        match processes.spawn("yt-dlp", &[], env) {
            Err(ProcessError::Denied(detail)) => assert!(detail.contains("SECRET"), "{detail}"),
            other => panic!("unexpected result: {other:?}"),
        }
    }

    /// Captured output is returned and drained.
    #[test]
    fn captured_output_is_read_once() {
        let (_staging, mut processes) = table();
        let args = vec!["-c".to_string(), "printf hello".to_string()];
        let handle = match processes.spawn("yt-dlp", &args, options(10_000)) {
            Ok(handle) => handle,
            Err(error) => panic!("spawn failed: {error:?}"),
        };
        match wait(&mut processes, handle) {
            ProcessState::Exited(0) => {}
            other => panic!("unexpected state: {other:?}"),
        }
        match processes.read_stdout(handle) {
            Ok(bytes) => assert_eq!(bytes, b"hello"),
            Err(error) => panic!("read failed: {error:?}"),
        }
        match processes.read_stdout(handle) {
            Ok(bytes) => assert!(bytes.is_empty(), "{bytes:?}"),
            Err(error) => panic!("read failed: {error:?}"),
        }
    }

    /// Output written to a file lands in the staging directory.
    #[test]
    fn file_output_lands_in_staging() {
        let (_staging, mut processes) = table();
        let mut to_file = options(10_000);
        to_file.stdout = StdoutMode::File("out/result.txt".to_string());
        let args = vec!["-c".to_string(), "printf hello".to_string()];
        let handle = match processes.spawn("yt-dlp", &args, to_file) {
            Ok(handle) => handle,
            Err(error) => panic!("spawn failed: {error:?}"),
        };
        match wait(&mut processes, handle) {
            ProcessState::Exited(0) => {}
            other => panic!("unexpected state: {other:?}"),
        }
        let written = processes.staging.join("out/result.txt");
        match std::fs::read(&written) {
            Ok(bytes) => assert_eq!(bytes, b"hello"),
            Err(error) => panic!("{}: {error}", written.display()),
        }
    }

    /// A process that outlives its deadline is killed and reported.
    #[test]
    fn a_process_past_its_deadline_times_out() {
        let (_staging, mut processes) = table();
        let args = vec!["-c".to_string(), "exec sleep 5".to_string()];
        let handle = match processes.spawn("yt-dlp", &args, options(200)) {
            Ok(handle) => handle,
            Err(error) => panic!("spawn failed: {error:?}"),
        };
        match wait(&mut processes, handle) {
            ProcessState::TimedOut => {}
            other => panic!("unexpected state: {other:?}"),
        }
    }

    /// `kill` releases the handle, after which polling answers unknown.
    #[test]
    fn kill_releases_the_handle() {
        let (_staging, mut processes) = table();
        let args = vec!["-c".to_string(), "exec sleep 5".to_string()];
        let handle = match processes.spawn("yt-dlp", &args, options(60_000)) {
            Ok(handle) => handle,
            Err(error) => panic!("spawn failed: {error:?}"),
        };
        processes.kill(handle);
        match processes.poll(handle) {
            Err(ProcessError::UnknownHandle) => {}
            other => panic!("unexpected result: {other:?}"),
        }
    }

    /// Handles this run never issued are unknown.
    #[test]
    fn an_unknown_handle_is_rejected() {
        let (_staging, mut processes) = table();
        match processes.poll(99) {
            Err(ProcessError::UnknownHandle) => {}
            other => panic!("unexpected result: {other:?}"),
        }
    }
}
