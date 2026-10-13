//! Writing a validated plugin to a `.hsplugin` archive.
//!
//! The archive is a zip file holding the manifest, the compiled component, and
//! any of `README.md`, `LICENSE` and `icon.png` that sit next to the manifest.
//! It exists so that a plugin is a single file a user can hand to the server.

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::check::Checked;

/// Files copied into the archive alongside the manifest when present.
const OPTIONAL_FILES: [&str; 3] = ["README.md", "LICENSE", "icon.png"];

/// Writes a validated plugin to `<out_dir>/<id>-<version>.hsplugin`.
///
/// # Parameters
///
/// - `checked`: a plugin that already passed `hsplugin check`.
/// - `out_dir`: directory the archive is written to; created when missing.
///
/// # Returns
///
/// The path of the archive that was written.
///
/// # Errors
///
/// Returns a message when the output directory cannot be created, the archive
/// cannot be written, or the component cannot be read.
pub fn run(checked: &Checked, out_dir: &Path) -> Result<PathBuf, String> {
    let name = format!(
        "{}-{}.hsplugin",
        checked.manifest().id(),
        checked.manifest().version()
    );

    std::fs::create_dir_all(out_dir).map_err(|error| format!("{}: {error}", out_dir.display()))?;
    let path = out_dir.join(&name);

    let file =
        std::fs::File::create(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut archive = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    write_entry(
        &mut archive,
        options,
        "manifest.json",
        checked.manifest_bytes(),
    )?;

    let wasm = std::fs::read(checked.wasm())
        .map_err(|error| format!("{}: {error}", checked.wasm().display()))?;
    write_entry(&mut archive, options, "plugin.wasm", &wasm)?;

    for name in OPTIONAL_FILES {
        let candidate = checked.manifest_dir().join(name);
        if candidate.is_file() {
            let bytes = std::fs::read(&candidate)
                .map_err(|error| format!("{}: {error}", candidate.display()))?;
            write_entry(&mut archive, options, name, &bytes)?;
        }
    }

    archive
        .finish()
        .map_err(|error| format!("{}: {error}", path.display()))?;

    Ok(path)
}

/// Adds one stored entry to the archive.
///
/// # Parameters
///
/// - `archive`: archive being written.
/// - `options`: compression options shared by every entry.
/// - `name`: entry name inside the archive.
/// - `bytes`: entry contents.
///
/// # Errors
///
/// Returns a message when the entry cannot be created or written.
fn write_entry(
    archive: &mut zip::ZipWriter<std::fs::File>,
    options: SimpleFileOptions,
    name: &str,
    bytes: &[u8],
) -> Result<(), String> {
    archive
        .start_file(name, options)
        .map_err(|error| format!("archive entry {name}: {error}"))?;
    archive
        .write_all(bytes)
        .map_err(|error| format!("archive entry {name}: {error}"))?;
    Ok(())
}
