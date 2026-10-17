//! `hsplugin new`: scaffold a plugin from a language template.
//!
//! A template is either a local directory, copied as it is, or a git URL,
//! cloned at depth one into a scratch directory. Either way the copy drops
//! `.git` metadata, rewrites the manifest's `id`, `name` and `version`, and
//! refuses a destination that already holds files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::ValueEnum;
use serde_json::Value;

use crate::temp::TempDir;

/// Plugin languages that have a published template.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Lang {
    /// Rust, compiled with cargo for `wasm32-wasip2`.
    Rust,
    /// Go, compiled with TinyGo.
    Go,
    /// JavaScript, componentized with jco.
    Js,
    /// Python, componentized with componentize-py.
    Python,
    /// C#, published as a NativeAOT component.
    Csharp,
}

impl Lang {
    /// Returns the lowercase name used on the command line and in template
    /// repository names.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Js => "js",
            Self::Python => "python",
            Self::Csharp => "csharp",
        }
    }

    /// Returns the URL of the official template repository for this language.
    #[must_use]
    pub fn default_template(self) -> String {
        format!(
            "https://github.com/hearthsound/plugin-template-{}",
            self.name()
        )
    }
}

/// Inputs of [`run`].
#[derive(Debug)]
pub struct NewOptions {
    /// Language whose default template is used when `template` is `None`.
    pub lang: Lang,
    /// Reverse-DNS plugin identifier written into the manifest.
    pub id: String,
    /// Human-readable plugin name written into the manifest.
    pub name: String,
    /// Directory to create; it must not exist or must be empty.
    pub out: PathBuf,
    /// Local directory or git URL to copy from; defaults to the language's
    /// official template.
    pub template: Option<String>,
}

/// Creates a plugin project from a template and returns the new directory.
///
/// The template must contain a `manifest.json`. After copying, the manifest's
/// `id` and `name` are replaced with the options' values and its `version` is
/// reset to `1.0.0`; every other key keeps its position and value. If the
/// copy fails after the destination was created, the destination is removed
/// again.
///
/// # Errors
///
/// Returns a message when the identifier or name is invalid, the destination
/// holds files, the template is missing or has no manifest, `git` cannot clone
/// a URL template, or any file operation fails.
///
/// # Examples
///
/// ```ignore
/// let options = NewOptions {
///     lang: Lang::Rust,
///     id: "com.example.scratch".into(),
///     name: "Scratch".into(),
///     out: "scratch".into(),
///     template: Some("../plugin-template-rust".into()),
/// };
/// let created = run(&options)?;
/// ```
pub fn run(options: &NewOptions) -> Result<PathBuf, String> {
    if !valid_id(&options.id) {
        return Err(format!(
            "{} is not a valid plugin id; use lowercase reverse-DNS segments such as com.example.my-plugin",
            options.id
        ));
    }
    if options.name.trim().is_empty() {
        return Err("the plugin name must not be empty".into());
    }
    ensure_destination_free(&options.out)?;

    let template = options
        .template
        .clone()
        .unwrap_or_else(|| options.lang.default_template());
    let clone = if is_remote(&template) {
        let dir = TempDir::new("hsplugin-template")
            .map_err(|error| format!("cannot create a scratch directory: {error}"))?;
        clone_template(&template, dir.path())?;
        Some(dir)
    } else {
        None
    };
    let source = match &clone {
        Some(dir) => dir.path().to_path_buf(),
        None => PathBuf::from(&template),
    };
    if !source.join("manifest.json").is_file() {
        return Err(format!(
            "template {template} has no manifest.json at its root"
        ));
    }

    let created = !options.out.exists();
    let result = copy_template(&source, &options.out)
        .and_then(|()| rewrite_manifest(&options.out.join("manifest.json"), options));
    if let Err(error) = result {
        if created {
            let _ = fs::remove_dir_all(&options.out);
        }
        return Err(error);
    }
    Ok(options.out.clone())
}

/// Reports whether `template` names a git URL rather than a local directory.
fn is_remote(template: &str) -> bool {
    ["https://", "http://", "git@", "ssh://", "file://"]
        .iter()
        .any(|prefix| template.starts_with(prefix))
}

/// Checks that `dest` either does not exist or is an empty directory.
///
/// # Errors
///
/// Returns a message when `dest` holds entries or cannot be read as a
/// directory.
fn ensure_destination_free(dest: &Path) -> Result<(), String> {
    match fs::read_dir(dest) {
        Ok(mut entries) => match entries.next() {
            None => Ok(()),
            Some(_) => Err(format!("destination {} is not empty", dest.display())),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "cannot use destination {} as a directory: {error}",
            dest.display()
        )),
    }
}

/// Clones `url` into `dir` at depth one.
///
/// # Errors
///
/// Returns a message when `git` cannot be started or the clone fails; the
/// message carries git's own error output.
fn clone_template(url: &str, dir: &Path) -> Result<(), String> {
    let output = Command::new("git")
        .args(["clone", "--depth", "1", "--quiet", url])
        .arg(dir)
        .output()
        .map_err(|error| format!("cannot run git to clone {url}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git clone {url} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

/// Copies the template tree at `from` into `to`, creating `to` if needed.
///
/// # Errors
///
/// Returns a message naming the failing path when a read or write fails.
fn copy_template(from: &Path, to: &Path) -> Result<(), String> {
    copy_entries(from, to, true)
        .map_err(|error| format!("cannot copy template into {}: {error}", to.display()))
}

/// Copies the entries of `from` into `to`; `root` marks the template root,
/// the only level where `target` is skipped.
fn copy_entries(from: &Path, to: &Path, root: bool) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || (root && name == "target") {
            continue;
        }
        let source = entry.path();
        let target = to.join(&name);
        let metadata = fs::metadata(&source)?;
        if metadata.is_dir() {
            copy_entries(&source, &target, false)?;
        } else if metadata.is_file() {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

/// Rewrites the copied manifest's identity fields in place.
///
/// # Errors
///
/// Returns a message when the manifest cannot be read, is not a JSON object,
/// or cannot be written back.
fn rewrite_manifest(path: &Path, options: &NewOptions) -> Result<(), String> {
    let bytes =
        fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let mut manifest: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("template manifest is not valid JSON: {error}"))?;
    let object = manifest
        .as_object_mut()
        .ok_or_else(|| "template manifest must be a JSON object".to_owned())?;
    object.insert("id".into(), Value::String(options.id.clone()));
    object.insert("name".into(), Value::String(options.name.clone()));
    object.insert("version".into(), Value::String("1.0.0".into()));
    let mut text = serde_json::to_string_pretty(&manifest)
        .map_err(|error| format!("cannot serialize the manifest: {error}"))?;
    text.push('\n');
    fs::write(path, text).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Checks an identifier against the manifest schema pattern
/// `^[a-z0-9]+(-[a-z0-9]+)*(\.[a-z0-9]+(-[a-z0-9]+)*)+$` with a 128-byte cap.
///
/// # Examples
///
/// ```ignore
/// assert!(valid_id("com.example.my-plugin"));
/// assert!(!valid_id("single"));
/// ```
#[must_use]
pub fn valid_id(id: &str) -> bool {
    if id.len() > 128 {
        return false;
    }
    let segments: Vec<&str> = id.split('.').collect();
    segments.len() >= 2 && segments.iter().all(|segment| valid_segment(segment))
}

/// Checks one dot-separated segment: lowercase alphanumerics with single
/// interior hyphens.
fn valid_segment(segment: &str) -> bool {
    !segment.is_empty()
        && !segment.starts_with('-')
        && !segment.ends_with('-')
        && !segment.contains("--")
        && segment
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}
