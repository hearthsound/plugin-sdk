//! `hsplugin`: validate Hearthsound plugin components.
//!
//! `hsplugin check` validates a `manifest.json` against the plugin manifest
//! schema and type-checks the compiled WebAssembly component against the
//! component rules. It prints one `ok: ...` line to stdout and exits 0 on
//! success; on failure it prints every problem to stderr, each prefixed with
//! `error: `, and exits 1.

mod check;
mod component;
mod manifest;

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

/// Parses the command line and runs the requested subcommand.
fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Check(args) => match check::run(&args.manifest, args.wasm.as_deref()) {
            Ok(checked) => println!("ok: {}", checked.summary()),
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
