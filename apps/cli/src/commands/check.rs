//! `flusso check` — validate the config and schemas without running the
//! pipeline. Unless `--offline`, it connects and confirms the declared types
//! and nullability agree with the live database, that the source server is
//! one `flusso run` accepts, and that the publication and replica identities
//! cover what the indexes read. An `Error`-level finding fails the check: one
//! that leaves an index not working as its schema says, whatever the cause.
//! `--offline` still reports the findings that need no database. `flusso run`
//! runs the same database validation at start.
//!
//! The human report only checks: a section that passes is one line, a section
//! with a problem prints in full, then warnings, then errors, then a summary
//! line. What the config declares is `flusso show`'s. `--format json` stays
//! complete (config, mappings, every section), since tools read it.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, ensure};
use clap::{Args, ValueEnum};
use config::Config;
use source::{CoverageReport, Diagnostic, PreImageReport, Severity};
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

    /// Validate the files only; do not connect to the database: skips the
    /// database validation, the source server, and the publication and
    /// replica-identity reports.
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

    /// Whether `flusso run` would set `REPLICA IDENTITY FULL` where needed.
    /// Controls the replica-identity report's phrasing only (check never
    /// mutates). Overrides the `[source] manage_replica_identity` config option.
    #[arg(long, env = "FLUSSO_MANAGE_REPLICA_IDENTITY")]
    manage_replica_identity: Option<bool>,

    /// Output format: a human-readable report, or JSON for piping.
    #[arg(long, env = "FLUSSO_FORMAT", value_enum, default_value_t = OutputFormat::Human)]
    format: OutputFormat,

    /// When the human-readable report is coloured: `auto` colours it when
    /// stdout is a terminal and `NO_COLOR` is unset.
    #[arg(long, env = "FLUSSO_COLOR", value_enum, default_value_t = print::ColorChoice::Auto)]
    color: print::ColorChoice,
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
            manage_replica_identity: args.manage_replica_identity,
            ..adapters::Overrides::default()
        },
    );
    // Every entry must instantiate its adapter config, offline or not: a
    // misspelled sink option is a config error, and `check` exists to catch it.
    adapters::validate(&config).with_context(|| format!("validating {}", args.config.display()))?;
    daemon::backoff(&config).with_context(|| format!("validating {}", args.config.display()))?;
    daemon::index_budget_share(&config)
        .with_context(|| format!("validating {}", args.config.display()))?;
    daemon::max_waiting(&config)
        .with_context(|| format!("validating {}", args.config.display()))?;
    daemon::max_pending_documents(&config)
        .with_context(|| format!("validating {}", args.config.display()))?;
    crate::backends::compile_scripts(&config)
        .with_context(|| format!("compiling the scripts of {}", args.config.display()))?;
    let config = Arc::new(config);

    let mappings = config.resolve_mappings();
    let postgres = adapters::source_config(&config)?;

    let diagnostics = if args.offline {
        source::validate_spec(&source_spec(&config))
    } else {
        let (_, sql_url) = crate::backends::source_sql_url(&config)?;
        let spec = Arc::new(source_spec(&config));
        let documents = PgDocumentBuilder::connect(&sql_url, Arc::clone(&spec))
            .await
            .context("connecting to the database")?;
        source::validate_indexes(&spec, &documents)
            .await
            .context("validating schemas against the database")?
    };

    let (coverage, pre_image) = if args.offline {
        (None, None)
    } else {
        let provisioning = crate::backends::build_provisioning(&config, &postgres.publication)?;
        let spec = source_spec(&config);
        let coverage = provisioning
            .inspect_coverage(&spec.all_tables())
            .await
            .context("inspecting publication coverage")?;
        let pre_image = provisioning
            .inspect_pre_image(&spec.pre_images())
            .await
            .context("inspecting replica identities")?;
        (Some(coverage), Some(pre_image))
    };
    let manage = postgres.manage_publication;
    let server = if args.offline {
        None
    } else {
        Some(crate::backends::source_server_problem(&config).await?)
    };
    let refused = server.as_ref().and_then(Option::as_deref);

    let has_errors = diagnostics.iter().any(|d| d.severity == Severity::Error);

    let mut out = std::io::stdout().lock();
    match args.format {
        OutputFormat::Json => {
            let doc = serde_json::json!({
                "config": &*config,
                "mappings": mappings,
                "diagnostics": diagnostics
                    .iter()
                    .map(|d| serde_json::json!({
                        "index": d.index.as_ref(),
                        "field": d.field.as_ref(),
                        "path": d.dotted_path(),
                        "anchor": d.anchor.as_str(),
                        "severity": format!("{:?}", d.severity).to_lowercase(),
                        "kind": d.kind.as_str(),
                        "message": d.message,
                    }))
                    .collect::<Vec<_>>(),
                "server": server.as_ref().map(|problem| serde_json::json!({
                    "ok": problem.is_none(),
                    "problem": problem,
                })),
                "coverage": coverage.as_ref().map(|c| serde_json::json!({
                    "satisfied": c.satisfied,
                    "manageable": c.manageable,
                    "will_manage": manage,
                    "present": c.present.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "missing": c.missing.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "misrouted": c.misrouted.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                    "blockers": c.blockers,
                    "remediation": c.remediation,
                })),
                "pre_image": pre_image.as_ref().map(|p| {
                    let gaps = |gaps: &[source::PreImageGap]| gaps.iter().map(|gap| serde_json::json!({
                        "table": gap.table.to_string(),
                        "missing": gap.missing.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
                        "missing_key": gap.missing_key.iter().map(|c| c.to_string()).collect::<Vec<_>>(),
                        "manageable": gap.manageable,
                        "will_manage": gap.will_manage(postgres.manage_replica_identity),
                        "blockers": gap.blockers,
                        "remediation": gap.remediation,
                        "kept": gap.kept,
                    })).collect::<Vec<_>>();
                    serde_json::json!({
                        "satisfied": p.satisfied(),
                        "complete": p.complete(),
                        "gaps": gaps(&p.gaps),
                        "partial": gaps(&p.partial),
                    })
                }),
            });
            writeln!(out, "{}", serde_json::to_string_pretty(&doc)?)?;
        }
        OutputFormat::Human => {
            let pen = print::Pen::for_stdout(args.color);
            let online = match (&coverage, &pre_image) {
                (Some(coverage), Some(pre_image)) => Some(Online {
                    server: refused,
                    coverage,
                    pre_image,
                    manage_publication: manage,
                    manage_replica_identity: postgres.manage_replica_identity,
                }),
                _ => None,
            };
            human_report(
                &mut out,
                pen,
                &args.config,
                &config,
                &diagnostics,
                online.as_ref(),
            )?;
        }
    }

    if let Some(refusal) = refused {
        anyhow::bail!("flusso run would refuse the source server: {refusal}");
    }
    ensure!(
        !has_errors,
        if args.offline {
            "schema validation failed"
        } else {
            "schema validation failed against the database"
        }
    );
    Ok(())
}

/// What an online check found beside the schema diagnostics.
struct Online<'a> {
    /// The refusal `flusso run` would stop with, if any.
    server: Option<&'a str>,
    coverage: &'a CoverageReport,
    pre_image: &'a PreImageReport,
    manage_publication: bool,
    manage_replica_identity: bool,
}

/// The human-readable report: one line per section that passes, a section
/// with a problem in full (with its SQL), then the schema findings, warnings
/// above errors, and a summary line. What failed is always on the last screen.
/// The config and the mapping are `flusso show`'s.
fn human_report(
    out: &mut impl Write,
    pen: print::Pen,
    path: &Path,
    config: &Config,
    diagnostics: &[Diagnostic],
    online: Option<&Online<'_>>,
) -> anyhow::Result<()> {
    print::config_line(out, pen, path, config)?;
    match online {
        None => {
            print::offline(out, pen)?;
            print::offline_diagnostics(out, pen, diagnostics)?;
        }
        Some(online) => {
            print::server(out, pen, online.server)?;
            print::coverage(out, pen, online.coverage, online.manage_publication)?;
            print::pre_image(out, pen, online.pre_image, online.manage_replica_identity)?;
            print::diagnostics(out, pen, diagnostics)?;
        }
    }
    writeln!(out)?;
    print::summary(
        out,
        pen,
        diagnostics,
        online.is_none(),
        online.and_then(|online| online.server),
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use config::ParseFrom;
    use config::toml::ConfigToml;

    use super::*;

    /// A deployment of one stdout sink and the indexes `products` and
    /// `users`, `users` disabled.
    fn config() -> Config {
        let mut draft = ConfigToml::try_parse(
            r#"
            [source]
            type = "postgres"

            [sinks.out]
            type = "stdout"

            [[index]]
            name = "products"
            schema = "products.schema.yml"
            enabled = true

            [[index]]
            name = "users"
            schema = "users.schema.yml"
            enabled = false
            "#,
        )
        .unwrap();
        let entries = std::mem::take(&mut draft.index);
        let mut config = Config::from(draft);
        for entry in entries {
            let schema = serde_json::from_value(serde_json::json!({
                "version": 1, "table": entry.name.as_ref(), "db_schema": "public", "fields": []
            }))
            .unwrap();
            config.indexes.insert(
                entry.name,
                config::Index {
                    enabled: entry.enabled,
                    schema,
                    on_error: None,
                    sinks: entry.sinks,
                },
            );
        }
        config
    }

    fn finding(kind: source::DiagnosticKind, field: &str) -> Diagnostic {
        Diagnostic::new(
            kernel::IndexName::try_new("products").unwrap(),
            kernel::FieldName::try_new(field).unwrap(),
            source::Anchor::Field,
            kind,
            format!("a {kind:?} finding"),
        )
    }

    fn report(diagnostics: &[Diagnostic], online: Option<&Online<'_>>, pen: print::Pen) -> String {
        let config = config();
        let mut out = Vec::new();
        human_report(
            &mut out,
            pen,
            Path::new("flusso.toml"),
            &config,
            diagnostics,
            online,
        )
        .unwrap();
        String::from_utf8(out).unwrap()
    }

    fn online<'a>(
        coverage: &'a CoverageReport,
        pre_image: &'a PreImageReport,
        server: Option<&'a str>,
    ) -> Online<'a> {
        Online {
            server,
            coverage,
            pre_image,
            manage_publication: true,
            manage_replica_identity: true,
        }
    }

    fn table(name: &str) -> source::QualifiedTable {
        source::QualifiedTable::new(
            config::DatabaseSchema::try_new("public").unwrap(),
            config::TableName::try_new(name).unwrap(),
        )
    }

    fn satisfied() -> CoverageReport {
        CoverageReport {
            satisfied: true,
            present: vec![table("products"), table("users")],
            missing: Vec::new(),
            misrouted: Vec::new(),
            manageable: true,
            blockers: Vec::new(),
            remediation: Vec::new(),
        }
    }

    /// Issue #485: a clean check is a handful of lines, one per section.
    #[test]
    fn a_clean_report_is_one_line_per_section() {
        let coverage = satisfied();
        let pre_image = PreImageReport::default();
        let printed = report(
            &[],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        assert_eq!(
            printed,
            "✓ config valid: flusso.toml: 2 indexes (1 disabled), 1 sink\n\
             ✓ source server: the primary, and flusso is not a synchronous standby of it\n\
             ✓ publication covers all 2 tables the indexes read\n\
             ✓ replica identity: every table an index reads logs its whole old row\n\
             \n\
             ✓ check passed\n"
        );
    }

    /// A section with a problem prints in full, its SQL included; the others
    /// stay one line each.
    #[test]
    fn a_failing_section_prints_in_full_and_the_others_in_one_line() {
        let coverage = CoverageReport {
            satisfied: false,
            present: vec![table("products")],
            missing: vec![table("users")],
            misrouted: Vec::new(),
            manageable: false,
            blockers: vec!["role does not own table public.users".to_owned()],
            remediation: vec!["ALTER PUBLICATION flusso ADD TABLE public.users;".to_owned()],
        };
        let pre_image = PreImageReport::default();
        let printed = report(
            &[],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        for line in [
            "\nPublication coverage\n",
            "1 table(s) the indexes read are not yet streamed:",
            "role does not own table public.users",
            "  ALTER PUBLICATION flusso ADD TABLE public.users;\n",
            "✓ source server: the primary, and flusso is not a synchronous standby of it\n",
            "✓ replica identity: every table an index reads logs its whole old row\n",
        ] {
            assert!(printed.contains(line), "{line:?} missing from:\n{printed}");
        }
        assert!(!printed.contains("Indexes"), "no config dump:\n{printed}");
    }

    /// What `flusso run` fixes on its own, or keeps on purpose, is one line:
    /// nothing to do by hand.
    #[test]
    fn what_run_sets_itself_is_one_line() {
        let coverage = CoverageReport {
            satisfied: false,
            present: vec![table("products")],
            missing: vec![table("users")],
            misrouted: Vec::new(),
            manageable: true,
            blockers: Vec::new(),
            remediation: vec!["ALTER PUBLICATION flusso ADD TABLE public.users;".to_owned()],
        };
        let gap = |name: &str, kept: Option<&str>| source::PreImageGap {
            table: table(name),
            missing: Vec::new(),
            missing_key: Vec::new(),
            manageable: true,
            blockers: Vec::new(),
            remediation: format!("ALTER TABLE {name} REPLICA IDENTITY FULL;"),
            kept: kept.map(str::to_owned),
        };
        let pre_image = PreImageReport {
            gaps: Vec::new(),
            partial: vec![gap("teams", None), gap("orders", Some("on an index"))],
        };
        let printed = report(
            &[],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        assert!(
            printed.contains(
                "✓ publication: the next `flusso run` covers the 1 table it misses\n\
                 ✓ replica identity: the next `flusso run` sets REPLICA IDENTITY FULL on 1 table; \
                 1 table kept as is (updates there always rebuild)\n"
            ),
            "{printed}"
        );
        assert!(!printed.contains("ALTER"), "{printed}");
    }

    /// Issue #485: errors are the last thing printed before the summary,
    /// after every other section, and warnings sit just above them, whatever
    /// order validation found them in.
    #[test]
    fn errors_come_last_after_every_other_section() {
        let coverage = satisfied();
        let pre_image = PreImageReport::default();
        let printed = report(
            &[
                finding(source::DiagnosticKind::UndeclaredArrayColumn, "tags"),
                finding(source::DiagnosticKind::IgnoredPrimaryKey, "team"),
                finding(source::DiagnosticKind::Nullability, "name"),
                finding(source::DiagnosticKind::CheckSkipped, "label"),
            ],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        let at = |text: &str| {
            printed
                .find(text)
                .unwrap_or_else(|| panic!("{text} missing from:\n{printed}"))
        };
        let first_error = at("[error]");
        for section in [
            "✓ config valid",
            "✓ source server",
            "✓ publication",
            "✓ replica identity",
            "[warning] products.team",
            "[warning] products.label",
        ] {
            assert!(
                at(section) < first_error,
                "{section:?} after an error:\n{printed}"
            );
        }
        assert!(at("[error] products.tags") < at("[error] products.name"));
        assert!(
            printed.rfind("[warning]").unwrap() < first_error,
            "{printed}"
        );
        let last = printed.trim_end().lines().last().unwrap();
        assert_eq!(last, "✗ check failed: 2 errors, 2 warnings", "{printed}");
    }

    #[test]
    fn a_clean_check_ends_on_its_summary() {
        let coverage = satisfied();
        let pre_image = PreImageReport::default();
        let printed = report(
            &[finding(source::DiagnosticKind::IgnoredPrimaryKey, "team")],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        let last = printed.trim_end().lines().last().unwrap();
        assert_eq!(last, "✓ check passed with 1 warning", "{printed}");
    }

    #[test]
    fn a_refused_server_counts_in_the_summary() {
        let coverage = satisfied();
        let pre_image = PreImageReport::default();
        let printed = report(
            &[],
            Some(&online(
                &coverage,
                &pre_image,
                Some("a synchronous standby"),
            )),
            print::Pen::plain(),
        );
        let last = printed.trim_end().lines().last().unwrap();
        assert_eq!(
            last, "✗ check failed: flusso run would refuse the source server; 0 errors, 0 warnings",
            "{printed}"
        );
        assert!(!printed.contains("check passed"), "{printed}");
    }

    #[test]
    fn an_offline_report_lists_its_errors_after_the_offline_note() {
        let printed = report(
            &[
                finding(
                    source::DiagnosticKind::DuplicateFieldNameFailsToBuild,
                    "tags",
                ),
                finding(source::DiagnosticKind::DuplicateFieldName, "name"),
            ],
            None,
            print::Pen::plain(),
        );
        let offline = printed.find("! offline").unwrap();
        let warning = printed.find("[warning]").unwrap();
        let error = printed.find("[error]").unwrap();
        assert!(offline < warning && warning < error, "{printed}");
        let last = printed.trim_end().lines().last().unwrap();
        assert_eq!(last, "✗ check failed: 1 error, 1 warning", "{printed}");
    }

    /// An offline check that passes doesn't end on an all-clear: the checks
    /// against the database didn't run.
    #[test]
    fn an_offline_pass_says_the_database_was_not_checked() {
        let clean = report(&[], None, print::Pen::plain());
        assert_eq!(
            clean.trim_end().lines().last(),
            Some("✓ offline checks passed; database checks not run"),
            "{clean}"
        );
        let warned = report(
            &[finding(source::DiagnosticKind::DuplicateFieldName, "name")],
            None,
            print::Pen::plain(),
        );
        assert_eq!(
            warned.trim_end().lines().last(),
            Some("✓ offline checks passed with 1 warning; database checks not run"),
            "{warned}"
        );
        assert!(!clean.contains("✓ check passed"), "{clean}");
    }

    #[test]
    fn the_report_is_coloured_only_when_asked() {
        let coverage = satisfied();
        let pre_image = PreImageReport::default();
        let findings = [
            finding(source::DiagnosticKind::Nullability, "name"),
            finding(source::DiagnosticKind::IgnoredPrimaryKey, "team"),
        ];
        let plain = report(
            &findings,
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        assert!(!plain.contains('\x1b'), "{plain}");
        let coloured = report(
            &findings,
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::coloured(),
        );
        for painted in [
            "\x1b[1;31m[error]\x1b[0m",
            "\x1b[1;33m[warning]\x1b[0m",
            "\x1b[1;31m✗ check failed: 1 error, 1 warning\x1b[0m",
            "\x1b[32m✓\x1b[0m",
        ] {
            assert!(
                coloured.contains(painted),
                "{painted:?} missing from:\n{coloured:?}"
            );
        }
    }
}
