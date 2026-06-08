//! `flusso check` — validate the config and schemas without running the
//! pipeline. The mapping is projected from the self-describing schema with no
//! database, so the fully-typed shape is shown either way. Unless `--offline`,
//! the command also connects and confirms the declared types and nullability
//! agree with the live database, reporting any disagreement; an `Error`-level
//! disagreement fails the check.

use std::io::Write;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::{Args, ValueEnum};
use schema::SourceType;
use sources_postgres::PgDocumentBuilder;

use crate::DEFAULT_CONFIG;
use crate::print;

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to the configuration file.
    #[arg(short, long, default_value = DEFAULT_CONFIG)]
    config: std::path::PathBuf,

    /// Validate the files only; do not connect to the database. The mapping is
    /// shown either way; `--offline` skips confirming it against the database.
    #[arg(long)]
    offline: bool,

    /// Output format: a human-readable report, or JSON for piping.
    #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// A colored, human-readable report (default).
    Human,
    /// A single JSON document: `{ "config": …, "mappings": … }`.
    Json,
}

pub(crate) async fn execute(args: CheckArgs) -> anyhow::Result<()> {
    // File-format validation: everything `schema::load` enforces (identifier
    // shapes, join/aggregate arity, declared types, filter value shapes).
    let config = Arc::new(
        schema::load(&args.config)
            .with_context(|| format!("loading config from {}", args.config.display()))?,
    );

    // The mapping is derived from the schema alone — no database needed.
    let mappings = config.resolve_mappings();

    // Source validation (skipped by `--offline`): connect and confirm the
    // declared schema matches the live database, collecting disagreements.
    let diagnostics = if args.offline {
        None
    } else {
        ensure!(
            config.source.source_type == SourceType::Postgres,
            "only postgres sources are supported",
        );
        let connection_url = config
            .source
            .resolve_connection_url()
            .context("resolving the source connection URL")?;
        let documents = PgDocumentBuilder::connect(connection_url.as_ref(), Arc::clone(&config))
            .await
            .context("connecting to the database")?;
        Some(
            sources_core::validate_indexes(&config, &documents)
                .await
                .context("validating schemas against the database")?,
        )
    };

    let has_errors = diagnostics.as_ref().is_some_and(|ds| {
        ds.iter()
            .any(|d| d.severity == sources_core::Severity::Error)
    });

    let mut out = std::io::stdout().lock();
    match args.format {
        OutputFormat::Json => {
            let doc = serde_json::json!({
                "config": &*config,
                "mappings": mappings,
                "diagnostics": diagnostics.as_ref().map(|ds| ds
                    .iter()
                    .map(|d| serde_json::json!({
                        "index": d.index.as_ref(),
                        "field": d.field.as_ref(),
                        "severity": format!("{:?}", d.severity).to_lowercase(),
                        "message": d.message,
                    }))
                    .collect::<Vec<_>>()),
            });
            writeln!(out, "{}", serde_json::to_string_pretty(&doc)?)?;
        }
        OutputFormat::Human => {
            let pen = print::Pen::detect();
            print::success(
                &mut out,
                pen,
                &format!("config valid: {}", args.config.display()),
            )?;
            print::config(&mut out, pen, &config)?;
            print::resolved(&mut out, pen, &mappings)?;
            match &diagnostics {
                None => {
                    print::warning(
                        &mut out,
                        pen,
                        "offline",
                        "skipped database validation — types and nullability not checked",
                    )?;
                }
                Some(diagnostics) => {
                    print::diagnostics(&mut out, pen, diagnostics)?;
                    writeln!(out)?;
                    if has_errors {
                        print::warning(&mut out, pen, "failed", "schema disagrees with database")?;
                    } else {
                        print::success(&mut out, pen, "check passed")?;
                    }
                }
            }
        }
    }

    ensure!(!has_errors, "schema validation failed against the database");
    Ok(())
}
