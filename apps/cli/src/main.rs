//! `flusso` — keep a search index in sync with Postgres from a config file.
//!
//! Four subcommands, one module each:
//!
//! - [`build`] reads a `flusso.toml`, parses and validates every schema it
//!   references, and writes the whole validated configuration to a single
//!   portable binary artifact (`flusso.lock`). No database is needed: the schema
//!   is self-describing, and secrets are kept as references, not baked in.
//! - [`run`] streams Postgres changes through the engine to the configured
//!   sink(s). With no `--config` it loads the compiled artifact; with `--config`
//!   it compiles the source afresh and runs that. Connection and credentials are
//!   resolved here, in the running environment. The replication slot is created
//!   automatically if it does not exist. Logs go to stderr.
//! - [`check`] validates the config and every schema, prints the fully-typed
//!   mapping (database-free), and — unless `--offline` — also confirms the
//!   declared types and nullability agree with the live database.
//! - [`schema_cmd`] prints an embedded JSON Schema for editor assist — the
//!   `flusso.toml` config schema or the `*.schema.yml` index schema — so a user
//!   can pin the schema that matches their installed version.

mod build;
mod check;
mod http;
mod metrics;
mod observer;
mod print;
mod run;
mod schema_cmd;
mod telemetry;

use clap::{Parser, Subcommand};

use build::BuildArgs;
use check::CheckArgs;
use run::RunArgs;
use schema_cmd::SchemaArgs;

/// The default config-file path, by convention `flusso.toml`.
pub(crate) const DEFAULT_CONFIG: &str = "flusso.toml";

/// The default compiled-artifact path, written by `build` and loaded by a
/// bare `run`.
pub(crate) const DEFAULT_ARTIFACT: &str = "flusso.lock";

/// Keep a search index in sync with Postgres, driven by a config file.
#[derive(Debug, Parser)]
#[command(name = "flusso", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Build a config and its schemas into a single portable artifact.
    Build(BuildArgs),
    /// Stream Postgres changes into the configured sink(s).
    Run(RunArgs),
    /// Validate the config and schemas without running the pipeline.
    Check(CheckArgs),
    /// Print an embedded JSON Schema for editor assist.
    Schema(SchemaArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Build(args) => build::execute(args),
        Command::Run(args) => run::execute(args).await,
        Command::Check(args) => check::execute(args).await,
        Command::Schema(args) => schema_cmd::execute(args),
    }
}
