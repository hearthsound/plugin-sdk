//! `hsplugin`: validate and package Hearthsound plugin components.
//!
//! `hsplugin check` validates a `manifest.json` against the plugin manifest
//! schema and type-checks the compiled WebAssembly component against the
//! component rules. `hsplugin pack` runs the same checks and then writes a
//! `.hsplugin` archive.
//!
//! Both subcommands print one `ok: ...` or `packed ...` line to stdout and exit
//! 0 on success; on failure they print every problem to stderr, each prefixed
//! with `error: `, and exit 1.

mod check;
mod component;
mod manifest;
mod pack;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line interface of the plugin tool.
#[derive(Debug, Parser)]
#[command(
    name = "hsplugin",
    version,
    about = "Check and pack Hearthsound plugin components"
)]
struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Command,
}

/// The available subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a plugin manifest and its WebAssembly component.
    Check(CheckArgs),
    /// Validate a plugin, then write it to a `.hsplugin` archive.
    Pack(PackArgs),
}

/// Arguments of `hsplugin check`.
#[derive(Debug, Args)]
struct CheckArgs {
    /// Path to the plugin's `manifest.json`.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,

    /// Path to the compiled component; defaults to the release build output.
    #[arg(long)]
    wasm: Option<PathBuf>,
}

/// Arguments of `hsplugin pack`.
#[derive(Debug, Args)]
struct PackArgs {
    /// Path to the plugin's `manifest.json`.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,

    /// Path to the compiled component; defaults to the release build output.
    #[arg(long)]
    wasm: Option<PathBuf>,

    /// Directory the archive is written to.
    #[arg(long, default_value = ".")]
    out_dir: PathBuf,
}

/// Parses the command line and runs the requested subcommand.
fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => match check::run(&args.manifest, args.wasm.as_deref()) {
            Ok(checked) => println!("ok: {}", checked.summary()),
            Err(errors) => fail(&errors),
        },
        Command::Pack(args) => match check::run(&args.manifest, args.wasm.as_deref()) {
            Ok(checked) => match pack::run(&checked, &args.out_dir) {
                Ok(path) => println!("packed {}", path.display()),
                Err(error) => fail(&[error]),
            },
            Err(errors) => fail(&errors),
        },
    }
}

/// Prints every error to stderr and exits with status 1.
fn fail(errors: &[String]) -> ! {
    for error in errors {
        eprintln!("error: {error}");
    }
    std::process::exit(1);
}
