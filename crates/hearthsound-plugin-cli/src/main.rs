//! `hsplugin`: scaffold, validate and package Hearthsound plugin components.
//!
//! `hsplugin new` copies a language template into a new directory with the
//! manifest's identity fields rewritten. `hsplugin check` validates a
//! `manifest.json` against the plugin manifest schema and type-checks the
//! compiled WebAssembly component against the component rules. `hsplugin pack`
//! runs the same checks and then writes a `.hsplugin` archive.
//!
//! `new` prints `created <path>`, `check` prints `ok: ...` and `pack` prints
//! `packed <path>` on stdout; each exits 0 on success. On failure every
//! problem goes to stderr, each prefixed with `error: `, and the exit status
//! is 1.

mod check;
mod component;
mod manifest;
mod pack;
mod scaffold;
mod temp;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line interface of the plugin tool.
#[derive(Debug, Parser)]
#[command(
    name = "hsplugin",
    version,
    about = "Scaffold, check and pack Hearthsound plugin components"
)]
struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Command,
}

/// The available subcommands.
#[derive(Debug, Subcommand)]
enum Command {
    /// Create a plugin project from a language template.
    New(NewArgs),
    /// Validate a plugin manifest and its WebAssembly component.
    Check(CheckArgs),
    /// Validate a plugin, then write it to a `.hsplugin` archive.
    Pack(PackArgs),
}

/// Arguments of `hsplugin new`.
#[derive(Debug, Args)]
struct NewArgs {
    /// Language of the template to copy.
    #[arg(long, value_enum)]
    lang: scaffold::Lang,

    /// Reverse-DNS plugin identifier written into the new manifest.
    #[arg(long, default_value = "com.example.my-plugin")]
    id: String,

    /// Human-readable plugin name written into the new manifest.
    #[arg(long, default_value = "My plugin")]
    name: String,

    /// Directory to create; defaults to a directory named after the id.
    #[arg(long)]
    out: Option<PathBuf>,

    /// Local directory or git URL to copy from; defaults to the official
    /// template for the language.
    #[arg(long)]
    template: Option<String>,
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
        Command::New(args) => {
            let out = args.out.unwrap_or_else(|| PathBuf::from(&args.id));
            let options = scaffold::NewOptions {
                lang: args.lang,
                id: args.id,
                name: args.name,
                out,
                template: args.template,
            };
            match scaffold::run(&options) {
                Ok(path) => println!("created {}", path.display()),
                Err(error) => fail(&[error]),
            }
        }
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
