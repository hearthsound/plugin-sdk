//! Builds a plugin component with the toolchain its project uses.
//!
//! The project language is read from a marker file beside the manifest:
//! `Cargo.toml` for Rust, `go.mod` for Go, `package.json` for JavaScript,
//! `pyproject.toml` for Python and a `*.csproj` file for C#. The markers are
//! checked in that order. Rust keeps the output cargo writes to its target
//! directory; every other language leaves the component at
//! `build/plugin.wasm`, the path `hsplugin check` and `hsplugin pack` read when
//! no `--wasm` is given.

use std::ffi::OsStr;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::check;
use crate::manifest::Manifest;

/// Directory, relative to the project, that receives non-Rust components.
const BUILD_DIR: &str = "build";

/// Project-relative path of the component written by non-Rust builds.
const COMPONENT_PATH: &str = "build/plugin.wasm";

/// The toolchain that builds a project, chosen from its marker file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toolchain {
    /// Rust: `cargo build --release --target wasm32-wasip2`.
    Cargo,
    /// Go: `go generate` followed by a TinyGo WASI component build.
    Go,
    /// JavaScript: `npm install` followed by the template's `npm run build`.
    Npm,
    /// Python: `componentize-py`.
    Python,
    /// C#: `dotnet publish` of a NativeAOT WebAssembly component.
    DotNet,
}

/// One external command, run in the project directory.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    /// Executable name, resolved through the search path.
    program: &'static str,
    /// Arguments passed to the executable, in order.
    args: Vec<String>,
}

impl Step {
    /// Builds a step from an executable name and its arguments.
    fn new(program: &'static str, args: &[&str]) -> Self {
        Self {
            program,
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
        }
    }
}

impl fmt::Display for Step {
    /// Renders the command as it would be typed at a shell prompt.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

/// Builds the plugin whose manifest is at `manifest_path`.
///
/// The project directory is the manifest's parent. The returned path is the
/// component that `hsplugin check` and `hsplugin pack` would read.
///
/// # Parameters
///
/// - `manifest_path`: path to the plugin's `manifest.json`.
///
/// # Returns
///
/// The path of the built component.
///
/// # Errors
///
/// Returns a message when no marker file matches the project, when a Go or
/// Python build needs a manifest that cannot be read or does not provide
/// exactly one layer with a WIT world, when a build command cannot start or
/// exits unsuccessfully, or when the build finishes without a component.
///
/// # Examples
///
/// ```ignore
/// let wasm = hearthsound_plugin_cli::build::run(Path::new("manifest.json"))?;
/// println!("built {}", wasm.display());
/// ```
pub fn run(manifest_path: &Path) -> Result<PathBuf, String> {
    build_project(manifest_path, None)
}

/// Shared body of [`run`]; `search_path`, when set, replaces the `PATH` seen
/// by every build command.
fn build_project(manifest_path: &Path, search_path: Option<&OsStr>) -> Result<PathBuf, String> {
    let dir = project_dir(manifest_path);
    let toolchain = detect(&dir)?;
    if toolchain == Toolchain::Cargo {
        execute(&dir, &steps(toolchain, ""), search_path)?;
        return check::resolve_wasm(manifest_path);
    }
    let world = match toolchain {
        Toolchain::Go | Toolchain::Python => read_world(manifest_path)?,
        Toolchain::Cargo | Toolchain::Npm | Toolchain::DotNet => "",
    };
    std::fs::create_dir_all(dir.join(BUILD_DIR))
        .map_err(|error| format!("cannot create {}: {error}", dir.join(BUILD_DIR).display()))?;
    execute(&dir, &steps(toolchain, world), search_path)?;
    if toolchain == Toolchain::DotNet {
        publish_component(&dir)?;
    }
    let component = dir.join(COMPONENT_PATH);
    if component.is_file() {
        Ok(component)
    } else {
        Err(format!(
            "build finished without producing {}",
            component.display()
        ))
    }
}

/// Chooses the toolchain for the project in `dir` from its marker files.
///
/// The markers are checked in the order given in the module documentation, so
/// a project with several markers builds with the first one that matches.
///
/// # Errors
///
/// Returns a message when `dir` holds none of the marker files.
fn detect(dir: &Path) -> Result<Toolchain, String> {
    let has = |name: &str| dir.join(name).is_file();
    if has("Cargo.toml") {
        Ok(Toolchain::Cargo)
    } else if has("go.mod") {
        Ok(Toolchain::Go)
    } else if has("package.json") {
        Ok(Toolchain::Npm)
    } else if has("pyproject.toml") {
        Ok(Toolchain::Python)
    } else if find_csproj(dir).is_some() {
        Ok(Toolchain::DotNet)
    } else {
        Err(format!(
            "{}: no Cargo.toml, go.mod, package.json, pyproject.toml or *.csproj to build from",
            dir.display()
        ))
    }
}

/// Returns the first `*.csproj` file directly inside `dir`, in name order.
fn find_csproj(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension() == Some(OsStr::new("csproj")) && path.is_file())
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Returns the directory that holds the manifest, using `.` for a bare
/// file name.
fn project_dir(manifest_path: &Path) -> PathBuf {
    match manifest_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

/// Reads the manifest and returns the WIT world its component targets.
///
/// # Errors
///
/// Returns a message when the manifest cannot be read or parsed, or when
/// [`world_for`] rejects its layers.
fn read_world(manifest_path: &Path) -> Result<&'static str, String> {
    let bytes = std::fs::read(manifest_path)
        .map_err(|error| format!("cannot read {}: {error}", manifest_path.display()))?;
    let manifest = Manifest::parse(&bytes)?;
    world_for(&manifest.provides())
}

/// Returns the WIT world a non-Rust component is built against.
///
/// Each provider layer has a `<layer>-provider` world. A component implements
/// one layer, so the manifest must provide exactly one layer that has a world.
///
/// # Parameters
///
/// - `provides`: the layers listed in the manifest.
///
/// # Returns
///
/// The world name, such as `catalog-provider`.
///
/// # Errors
///
/// Returns a message listing the provided layers when there is not exactly
/// one, or when that layer has no provider world.
fn world_for(provides: &[String]) -> Result<&'static str, String> {
    let layers = provides.join(", ");
    let [layer] = provides else {
        return Err(format!(
            "non-Rust builds need exactly one provided layer; the manifest provides [{layers}]"
        ));
    };
    match layer.as_str() {
        "catalog" => Ok("catalog-provider"),
        "metadata" => Ok("metadata-provider"),
        "artwork" => Ok("artwork-provider"),
        "acquisition" => Ok("acquisition-provider"),
        _ => Err(format!(
            "layer `{layer}` has no provider world to build against; \
             use catalog, metadata, artwork or acquisition"
        )),
    }
}

/// Returns the commands that build `toolchain` in the project, in order.
///
/// `world` is the WIT world for Go and Python and is ignored by the other
/// toolchains.
fn steps(toolchain: Toolchain, world: &str) -> Vec<Step> {
    match toolchain {
        Toolchain::Cargo => vec![Step::new(
            "cargo",
            &["build", "--release", "--target", "wasm32-wasip2"],
        )],
        Toolchain::Go => vec![
            Step::new("go", &["generate", "./..."]),
            Step::new(
                "tinygo",
                &[
                    "build",
                    "-target=wasip2",
                    "--wit-package",
                    "wit/",
                    "--wit-world",
                    world,
                    "-o",
                    COMPONENT_PATH,
                    "./...",
                ],
            ),
        ],
        Toolchain::Npm => vec![
            Step::new("npm", &["install"]),
            Step::new("npm", &["run", "build"]),
        ],
        Toolchain::Python => vec![Step::new(
            "componentize-py",
            &[
                "-d",
                "wit",
                "-w",
                world,
                "componentize",
                "app",
                "-o",
                COMPONENT_PATH,
            ],
        )],
        Toolchain::DotNet => vec![Step::new("dotnet", &["publish", "-c", "Release"])],
    }
}

/// Runs each step in `dir`, stopping at the first failure.
///
/// # Parameters
///
/// - `dir`: working directory of every command.
/// - `steps`: the commands to run, in order.
/// - `search_path`: when set, the `PATH` each command resolves through.
///
/// # Errors
///
/// Returns a message when a command cannot start or exits unsuccessfully.
fn execute(dir: &Path, steps: &[Step], search_path: Option<&OsStr>) -> Result<(), String> {
    for step in steps {
        let mut command = Command::new(step.program);
        command.args(&step.args).current_dir(dir);
        if let Some(path) = search_path {
            command.env("PATH", path);
        }
        let status = command
            .status()
            .map_err(|error| format!("cannot run `{step}`: {error}"))?;
        if !status.success() {
            return Err(format!("`{step}` failed with {status}"));
        }
    }
    Ok(())
}

/// Copies the NativeAOT component that `dotnet publish` wrote to
/// `build/plugin.wasm`.
///
/// The component is the `.wasm` file inside a `publish` directory under
/// `bin/Release`. When several exist, the one named after the project file is
/// used; a single candidate is used whatever its name.
///
/// # Errors
///
/// Returns a message when there is no project file, when no candidate or an
/// ambiguous set of candidates exists, or when the copy fails.
fn publish_component(dir: &Path) -> Result<(), String> {
    let csproj = find_csproj(dir).ok_or_else(|| format!("{}: no *.csproj file", dir.display()))?;
    let stem = csproj.file_stem();
    let mut candidates = Vec::new();
    collect_published(&dir.join("bin").join("Release"), &mut candidates);
    candidates.sort();
    let source = if let Some(named) = candidates.iter().find(|path| path.file_stem() == stem) {
        named.clone()
    } else if let [only] = candidates.as_slice() {
        only.clone()
    } else {
        return Err(format!(
            "expected one published .wasm for {}, found {}",
            csproj.display(),
            candidates.len()
        ));
    };
    let target = dir.join(COMPONENT_PATH);
    std::fs::copy(&source, &target)
        .map(|_| ())
        .map_err(|error| {
            format!(
                "cannot copy {} to {}: {error}",
                source.display(),
                target.display()
            )
        })
}

/// Collects every `.wasm` file whose parent directory is named `publish`
/// beneath `dir`. A missing directory contributes nothing.
fn collect_published(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_published(&path, found);
        } else if path.extension() == Some(OsStr::new("wasm"))
            && path.parent().and_then(Path::file_name) == Some(OsStr::new("publish"))
        {
            found.push(path);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::temp::TempDir;

    /// Creates an empty temporary project holding the files named in `files`.
    fn project(files: &[&str]) -> TempDir {
        let dir = TempDir::new("hsplugin-build").unwrap();
        for file in files {
            std::fs::write(dir.path().join(file), "").unwrap();
        }
        dir
    }

    #[test]
    fn each_marker_selects_its_toolchain() {
        let cases = [
            ("Cargo.toml", Toolchain::Cargo),
            ("go.mod", Toolchain::Go),
            ("package.json", Toolchain::Npm),
            ("pyproject.toml", Toolchain::Python),
            ("plugin.csproj", Toolchain::DotNet),
        ];
        for (marker, expected) in cases {
            let dir = project(&[marker]);
            assert_eq!(detect(dir.path()).unwrap(), expected, "marker {marker}");
        }
    }

    #[test]
    fn the_first_marker_in_order_wins() {
        let dir = project(&["go.mod", "Cargo.toml", "package.json"]);
        assert_eq!(detect(dir.path()).unwrap(), Toolchain::Cargo);
    }

    #[test]
    fn a_project_without_a_marker_is_refused() {
        let dir = project(&["manifest.json"]);
        let error = detect(dir.path()).unwrap_err();
        assert!(error.contains("no Cargo.toml"), "{error}");
    }

    #[test]
    fn the_world_follows_the_single_provided_layer() {
        let world = |layers: &[&str]| {
            let layers: Vec<String> = layers.iter().map(|layer| (*layer).to_owned()).collect();
            world_for(&layers)
        };
        assert_eq!(world(&["catalog"]).unwrap(), "catalog-provider");
        assert_eq!(world(&["acquisition"]).unwrap(), "acquisition-provider");
        assert!(world(&["catalog", "metadata"]).is_err());
        assert!(world(&["verification"]).is_err());
    }

    #[test]
    fn each_toolchain_runs_its_documented_commands() {
        let rendered = |toolchain, world| {
            steps(toolchain, world)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rendered(Toolchain::Cargo, ""),
            ["cargo build --release --target wasm32-wasip2"]
        );
        assert_eq!(
            rendered(Toolchain::Go, "catalog-provider"),
            [
                "go generate ./...",
                "tinygo build -target=wasip2 --wit-package wit/ --wit-world catalog-provider -o build/plugin.wasm ./...",
            ]
        );
        assert_eq!(
            rendered(Toolchain::Npm, ""),
            ["npm install", "npm run build"]
        );
        assert_eq!(
            rendered(Toolchain::Python, "catalog-provider"),
            ["componentize-py -d wit -w catalog-provider componentize app -o build/plugin.wasm"]
        );
        assert_eq!(
            rendered(Toolchain::DotNet, ""),
            ["dotnet publish -c Release"]
        );
    }

    /// Writes an executable shell script named `name` into `bin`.
    #[cfg(unix)]
    fn stub(bin: &Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(bin).unwrap();
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Returns a search path that tries `bin` first and then the inherited path.
    #[cfg(unix)]
    fn search_path_with(bin: &Path) -> std::ffi::OsString {
        let inherited = std::env::var_os("PATH").unwrap_or_default();
        let mut path = bin.as_os_str().to_owned();
        path.push(":");
        path.push(inherited);
        path
    }

    #[cfg(unix)]
    #[test]
    fn an_npm_project_runs_install_then_build_and_keeps_the_component() {
        let dir = project(&["package.json"]);
        let bin = dir.path().join("stub-bin");
        stub(
            &bin,
            "npm",
            "mkdir -p build\necho \"$*\" >> build/commands.log\n\
             if [ \"$1\" = run ]; then printf wasm > build/plugin.wasm; fi",
        );
        let manifest = dir.path().join("manifest.json");
        let path = build_project(&manifest, Some(&search_path_with(&bin))).unwrap();
        assert_eq!(path, dir.path().join("build/plugin.wasm"));
        let log = std::fs::read_to_string(dir.path().join("build/commands.log")).unwrap();
        assert_eq!(log, "install\nrun build\n");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_build_command_is_reported_with_its_status() {
        let dir = project(&["package.json"]);
        let bin = dir.path().join("stub-bin");
        stub(&bin, "npm", "exit 3");
        let manifest = dir.path().join("manifest.json");
        let error = build_project(&manifest, Some(&search_path_with(&bin))).unwrap_err();
        assert!(error.contains("`npm install` failed"), "{error}");
    }
}
