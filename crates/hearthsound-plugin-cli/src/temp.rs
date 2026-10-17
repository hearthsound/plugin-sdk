//! Scratch directories that are removed when their guard is dropped.
//!
//! The CLI stages template clones and host-visible file roots in directories
//! under the system temporary directory. [`TempDir`] owns one such directory
//! and removes it, with everything inside, when it goes out of scope.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Distinguishes directories created within the same nanosecond by one process.
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// An empty directory under the system temporary directory.
///
/// The directory is removed recursively on drop; removal errors are ignored
/// because there is nothing useful a command-line tool can do about them.
#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates a new empty directory whose name starts with `prefix`.
    ///
    /// The name combines the prefix, the process id, a timestamp and a sequence
    /// number, so concurrent invocations never collide.
    ///
    /// # Errors
    ///
    /// Returns the I/O error when the directory cannot be created.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let dir = TempDir::new("hsplugin-example")?;
    /// assert!(dir.path().is_dir());
    /// ```
    pub fn new(prefix: &str) -> io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{nanos}-{sequence}",
            std::process::id()
        ));
        std::fs::create_dir(&path)?;
        Ok(Self { path })
    }

    /// Returns the path of the directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
