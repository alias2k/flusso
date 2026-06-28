//! `flusso` — keep a search index in sync with Postgres from a config file.
//!
//! Subcommands (under `commands/`). The first four are local/offline; the last
//! two are thin HTTP clients to a *running* server's private control surface:
//!
//! - [`build`](commands::build) reads a `flusso.toml`, parses and validates every schema it
//!   references, and writes the whole validated configuration to a single
//!   portable binary artifact (`flusso.lock`). No database is needed: the schema
//!   is self-describing, and secrets are kept as references, not baked in.
//! - [`run`](commands::run) streams Postgres changes through the engine to the configured
//!   sink(s). Like `cargo run`, it compiles first: when a `flusso.toml` is present
//!   (the default path, or `--config`) it compiles it afresh, **writes the
//!   `flusso.lock`**, and runs that — so the committed lock stays current with no
//!   extra step. With no config it falls back to loading the existing lock, and
//!   `--locked` runs the lock as-is without recompiling. Connection and
//!   credentials are resolved here, in the running environment. The replication
//!   slot is created automatically if it does not exist. Logs go to stderr.
//! - [`check`](commands::check) validates the config and every schema, prints the fully-typed
//!   mapping (database-free), and — unless `--offline` — also confirms the
//!   declared types and nullability agree with the live database.
//! - [`design`](commands::design) opens a local web UI that introspects the
//!   configured source through the source abstraction, edits the `flusso.toml`
//!   and `*.schema.yml` files in place, and previews the document/mapping.
//! - [`schema_cmd`](commands::schema_cmd) prints an embedded JSON Schema for editor assist — the
//!   `flusso.toml` config schema or the `*.schema.yml` index schema — so a user
//!   can pin the schema that matches their installed version.
//! - [`indexes`](commands::admin::indexes) lists a running server's indexes and
//!   their lifecycle state.
//! - [`reindex`](commands::admin::reindex) triggers a from-scratch rebuild of one
//!   index on a running server (zero read-downtime; reads stay on the old copy
//!   until the rebuild swaps in).

mod backends;
mod commands;
mod http;
mod telemetry;

use clap::{Parser, Subcommand};

use commands::admin::{IndexesArgs, ReindexArgs};
use commands::build::BuildArgs;
use commands::check::CheckArgs;
#[cfg(feature = "design")]
use commands::design::DesignArgs;
use commands::run::RunArgs;
use commands::schema_cmd::SchemaArgs;

/// The default config-file path, by convention `flusso.toml`.
pub(crate) const DEFAULT_CONFIG: &str = "flusso.toml";

/// The default compiled-lock path: written by `build` and by `run` (which
/// recompiles it on start), and loaded by a `run` that has no config to compile.
pub(crate) const DEFAULT_LOCK: &str = "flusso.lock";

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
    /// Open the visual schema designer (a local web UI).
    #[cfg(feature = "design")]
    Design(DesignArgs),
    /// Print an embedded JSON Schema for editor assist.
    Schema(SchemaArgs),
    /// List a running server's indexes and their state.
    Indexes(IndexesArgs),
    /// Trigger a from-scratch rebuild of one index on a running server.
    Reindex(ReindexArgs),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Build(args) => commands::build::execute(args),
        Command::Run(args) => commands::run::execute(args).await,
        Command::Check(args) => commands::check::execute(args).await,
        #[cfg(feature = "design")]
        Command::Design(args) => commands::design::execute(args).await,
        Command::Schema(args) => commands::schema_cmd::execute(args),
        Command::Indexes(args) => commands::admin::indexes(args).await,
        Command::Reindex(args) => commands::admin::reindex(args).await,
    }
}
