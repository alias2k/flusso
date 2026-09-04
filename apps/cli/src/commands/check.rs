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
use source_postgres::PgDocumentBuilder;

use crate::adapters;
use crate::backends::source_spec;

use crate::DEFAULT_CONFIG;
use crate::commands::print;

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to the configuration file.
    #[arg(short, long, env = "FLUSSO_CONFIG", default_value = DEFAULT_CONFIG)]
    config: std::path::PathBuf,

    /// Validate the files only; do not connect to the database. The mapping is
    /// shown either way; `--offline` skips confirming it against the database
    /// and skips the publication-coverage report.
    #[arg(long, env = "FLUSSO_OFFLINE")]
    offline: bool,

    /// Publication whose coverage to report. Overrides `[source] publication`,
    /// like `flusso run`, so the report reflects the publication a run would use.
    #[arg(long, env = "FLUSSO_PUBLICATION")]
    publication: Option<String>,

    /// Whether `flusso run` would auto-create/extend the publication. Controls
    /// the coverage report's phrasing only (check never mutates). Overrides the
    /// `[source] manage_publication` config option.
    #[arg(long, env = "FLUSSO_MANAGE_PUBLICATION")]
    manage_publication: Option<bool>,

    /// Output format: a human-readable report, or JSON for piping.
    #[arg(long, env = "FLUSSO_FORMAT", value_enum, default_value_t = OutputFormat::Human)]
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
    let mut config = config::load(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    adapters::apply_overrides(
        &mut config,
        &adapters::Overrides {
            publication: args.publication.clone(),
            manage_publication: args.manage_publication,
            ..adapters::Overrides::default()
        },
    );
    // Every entry must instantiate its adapter config, offline or not: a
    // misspelled sink option is a config error, and `check` exists to catch it.
    adapters::validate(&config).with_context(|| format!("validating {}", args.config.display()))?;
    let config = Arc::new(config);

    let mappings = config.resolve_mappings();
    let postgres = adapters::source_config(&config)?;

    let diagnostics = if args.offline {
        None
    } else {
        let (_, sql_url) = crate::backends::source_sql_url(&config)?;
        let spec = Arc::new(source_spec(&config));
        let documents = PgDocumentBuilder::connect(&sql_url, Arc::clone(&spec))
            .await
            .context("connecting to the database")?;
        Some(
            source::validate_indexes(&spec, &documents)
                .await
                .context("validating schemas against the database")?,
        )
    };

    let coverage = if args.offline {
        None
    } else {
        let provisioning = crate::backends::build_provisioning(&config, &postgres.publication)?;
        let required = source_spec(&config).all_tables();
        Some(
            provisioning
                .inspect_coverage(&required)
                .await
                .context("inspecting publication coverage")?,
        )
    };
    let manage = postgres.manage_publication;

    let has_errors = diagnostics
        .as_ref()
        .is_some_and(|ds| ds.iter().any(|d| d.severity == source::Severity::Error));

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
                "coverage": coverage.as_ref().map(|c| serde_json::json!({
                    "satisfied": c.satisfied,
                    "manageable": c.manageable,
                    "will_manage": manage,
                    "present": c.present.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "missing": c.missing.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "blockers": c.blockers,
                    "remediation": c.remediation,
                })),
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
                    if let Some(coverage) = &coverage {
                        print::coverage(&mut out, pen, coverage, manage)?;
                    }
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
