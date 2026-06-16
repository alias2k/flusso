//! `flusso schema` — print an embedded JSON Schema for editor assist.
//!
//! Named `schema_cmd` (not `schema`) so it doesn't shadow the `schema` crate it
//! reads the embedded schemas from.

use std::io::Write;

use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub(crate) struct SchemaArgs {
    /// Which schema to print.
    #[arg(value_enum, env = "FLUSSO_SCHEMA")]
    which: SchemaKind,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum SchemaKind {
    /// The `flusso.toml` config schema (JSON Schema).
    Config,
    /// The `*.schema.yml` index schema (JSON Schema, authored as YAML).
    Index,
}

/// Print an embedded JSON Schema to stdout. Needs no config and no database —
/// the schema is compiled into the binary, so it always matches this version.
pub(crate) fn execute(args: SchemaArgs) -> anyhow::Result<()> {
    let body = match args.which {
        SchemaKind::Config => schema::CONFIG_SCHEMA,
        SchemaKind::Index => schema::INDEX_SCHEMA,
    };
    let mut out = std::io::stdout().lock();
    // Normalize to exactly one trailing newline regardless of the file's.
    writeln!(out, "{}", body.trim_end())?;
    Ok(())
}
