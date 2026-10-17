//! File access behind the mock host's `host-files` import.
//!
//! The mock host grants one root, `staging`, which maps to a scratch
//! directory owned by the run. Paths are relative to a root; a path with a
//! root, a prefix, or a `..` component is refused before any file is touched.
//! Any other root id is refused, because the mock host grants no import roots.

use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

/// The only root id the mock host grants.
pub const STAGING_ROOT: &str = "staging";

/// Metadata reported for one entry, as the WIT `file-stat` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileStat {
    /// Size in bytes.
    pub size: u64,
    /// Last modification time in milliseconds since the Unix epoch.
    pub modified_ms: u64,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

/// The file roots visible to one plugin run.
#[derive(Debug, Clone)]
pub struct Files {
    staging: PathBuf,
}

impl Files {
    /// Creates the file view over a staging directory.
    ///
    /// # Parameters
    ///
    /// - `staging`: the directory that the `staging` root maps to.
    #[must_use]
    pub fn new(staging: PathBuf) -> Self {
        Self { staging }
    }

    /// Returns the metadata of the entry at `path` under `root`, or `None` when
    /// the root is unknown, the path is unsafe, or the entry does not exist.
    #[must_use]
    pub fn stat(&self, root: &str, path: &str) -> Option<FileStat> {
        let target = self.resolve(root, path).ok()?;
        let metadata = std::fs::metadata(target).ok()?;
        let modified_ms = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |elapsed| elapsed.as_millis() as u64);
        Some(FileStat {
            size: metadata.len(),
            modified_ms,
            is_dir: metadata.is_dir(),
        })
    }

    /// Lists the names of the entries in the directory at `path` under `root`,
    /// sorted by name.
    ///
    /// # Errors
    ///
    /// Returns a message when the root is unknown, the path is unsafe, or the
    /// directory cannot be read.
    pub fn list_dir(&self, root: &str, path: &str) -> Result<Vec<String>, String> {
        let target = self.resolve(root, path)?;
        let entries =
            std::fs::read_dir(&target).map_err(|error| format!("cannot read `{path}`: {error}"))?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("cannot read `{path}`: {error}"))?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        names.sort();
        Ok(names)
    }

    /// Maps a root id and a relative path to a location on disk.
    ///
    /// # Errors
    ///
    /// Returns a message when the root is not `staging` or the path is
    /// absolute or contains a component that could leave the root.
    fn resolve(&self, root: &str, path: &str) -> Result<PathBuf, String> {
        if root != STAGING_ROOT {
            return Err(format!("unknown import root `{root}`"));
        }
        let relative = Path::new(path);
        let mut target = self.staging.clone();
        for component in relative.components() {
            match component {
                Component::Normal(part) => target.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(format!("path `{path}` leaves its root"));
                }
            }
        }
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::temp::TempDir;

    fn staged() -> (TempDir, Files) {
        let dir = TempDir::new("hsplugin-files-test").unwrap();
        std::fs::write(dir.path().join("b.txt"), b"hello").unwrap();
        std::fs::write(dir.path().join("a.txt"), b"").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let files = Files::new(dir.path().to_path_buf());
        (dir, files)
    }

    #[test]
    fn stat_reports_size_and_kind() {
        let (_dir, files) = staged();
        let file = files.stat(STAGING_ROOT, "b.txt").unwrap();
        assert_eq!(file.size, 5);
        assert!(!file.is_dir);
        assert!(files.stat(STAGING_ROOT, "sub").unwrap().is_dir);
    }

    #[test]
    fn stat_of_a_missing_entry_is_none() {
        let (_dir, files) = staged();
        assert_eq!(files.stat(STAGING_ROOT, "missing.txt"), None);
    }

    #[test]
    fn list_dir_is_sorted() {
        let (_dir, files) = staged();
        assert_eq!(
            files.list_dir(STAGING_ROOT, "").unwrap(),
            vec!["a.txt", "b.txt", "sub"]
        );
    }

    #[test]
    fn traversal_is_refused() {
        let (_dir, files) = staged();
        assert!(files.stat(STAGING_ROOT, "../escape").is_none());
        assert!(files.list_dir(STAGING_ROOT, "/etc").is_err());
    }

    #[test]
    fn unknown_roots_are_refused() {
        let (_dir, files) = staged();
        assert!(files.stat("library", "a.txt").is_none());
        assert!(files.list_dir("library", "").is_err());
    }
}
