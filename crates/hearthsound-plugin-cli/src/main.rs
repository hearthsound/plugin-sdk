//! `hsplugin`: scaffold, build, validate and package Hearthsound plugin
//! components.
//!
//! `hsplugin new` copies a language template into a new directory with the
//! manifest's identity fields rewritten. `hsplugin build` compiles the plugin
//! with the toolchain its project uses (cargo, TinyGo, npm, componentize-py or
//! `dotnet publish`). `hsplugin check` validates a `manifest.json` against the
//! plugin manifest schema and type-checks the compiled WebAssembly component
//! against the component rules. `hsplugin pack` runs the same checks and then
//! writes a `.hsplugin` archive.
//!
//! `new` prints `created <path>`, `build` prints `built <path>`, `check` prints
//! `ok: ...` and `pack` prints `packed <path>` on stdout; each exits 0 on
//! success. On failure every problem goes to stderr, each prefixed with
//! `error: `, and the exit status is 1.

mod build;
mod check;
mod component;
mod host;
mod manifest;
mod pack;
mod run;
mod scaffold;
mod suites;
mod temp;
mod values;

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

/// Command-line interface of the plugin tool.
#[derive(Debug, Parser)]
#[command(
    name = "hsplugin",
    version,
    about = "Scaffold, build, check and pack Hearthsound plugin components"
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
    /// Compile the plugin with the toolchain its project uses.
    Build(BuildArgs),
    /// Validate a plugin manifest and its WebAssembly component.
    Check(CheckArgs),
    /// Validate a plugin, then write it to a `.hsplugin` archive.
    Pack(PackArgs),
    /// Call one plugin function against the mock host.
    Run(RunArgs),
    /// Check a plugin against a layer's conformance suite.
    Test(TestArgs),
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

/// Arguments of `hsplugin build`.
#[derive(Debug, Args)]
struct BuildArgs {
    /// Path to the plugin's `manifest.json`.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,
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

/// Arguments of `hsplugin run`.
#[derive(Debug, Args)]
struct RunArgs {
    /// The function to run, as `<capability>.<function>`, for example
    /// `catalog.search`.
    target: String,

    /// Path to the plugin's `manifest.json`.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,

    /// Path to the compiled component; defaults to the release build output.
    #[arg(long)]
    wasm: Option<PathBuf>,

    /// JSON file with the server-scoped configuration.
    #[arg(long)]
    config: Option<PathBuf>,

    /// JSON file with the function arguments.
    #[arg(long)]
    input: Option<PathBuf>,
}

/// Arguments of `hsplugin test`.
#[derive(Debug, Args)]
struct TestArgs {
    /// Layer whose conformance suite runs: catalog, metadata, artwork or acquisition.
    layer: String,

    /// Path to the plugin's `manifest.json`.
    #[arg(long, default_value = "manifest.json")]
    manifest: PathBuf,

    /// Path to the compiled component; defaults to the release build output.
    #[arg(long)]
    wasm: Option<PathBuf>,

    /// JSON file with the server-scoped configuration.
    #[arg(long)]
    config: Option<PathBuf>,
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
        Command::Build(args) => match build::run(&args.manifest) {
            Ok(path) => println!("built {}", path.display()),
            Err(error) => fail(&[error]),
        },
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
        Command::Run(args) => match run::run(
            &args.manifest,
            args.wasm.as_deref(),
            args.config.as_deref(),
            &args.target,
            args.input.as_deref(),
        ) {
            Ok(value) => println!("{value}"),
            Err(error) => fail(&[error]),
        },
        Command::Test(args) => match suites::test(
            &args.manifest,
            args.wasm.as_deref(),
            args.config.as_deref(),
            &args.layer,
        ) {
            Ok(report) => {
                for line in report.lines() {
                    println!("{line}");
                }
                if !report.passed() {
                    std::process::exit(1);
                }
            }
            Err(error) => fail(&[error]),
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
