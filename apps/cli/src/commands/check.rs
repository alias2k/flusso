//! `flusso check` — validate the config and schemas without running the
//! pipeline. Unless `--offline`, it connects and confirms the declared types
//! and nullability agree with the live database, that the source server is
//! one `flusso run` accepts, and that the publication and replica identities
//! cover what the indexes read. An `Error`-level finding fails the check: one
//! that leaves an index not working as its schema says, whatever the cause.
//! `--offline` still reports the findings that need no database. `flusso run`
//! runs the same database validation at start.
//!
//! It also compares the `flusso.lock` with what the config compiles to, as
//! `run` does before rewriting it (see [`crate::lock`]): a stale or unreadable
//! lock is an error, since `run --locked` would run something else. Given no
//! config, it checks the lock itself and says there was nothing to compare.
//! `--fix` (a flag only, no variable) rewrites such a lock as `flusso build`
//! would and reports it rewritten; nothing else is ever written.
//!
//! The report only checks: a section that passes is one line, a section with a
//! problem prints in full, then warnings, then errors, then a summary line.
//! `--format json` is the same report (one `Report` value rendered twice):
//! each section with its status and detail, the diagnostics and the summary.
//! What the config declares, and the mapping, are `flusso show`'s.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use anyhow::Context;
use clap::{Args, ValueEnum};
use config::Config;
use source::{CoverageReport, Diagnostic, PreImageReport};
use source_postgres::PgDocumentBuilder;

use crate::adapters;
use crate::backends::source_spec;

use crate::commands::print;
use crate::lock::{ConfigPlan, LockAt, LockState, plan_config};
use crate::{DEFAULT_CONFIG, DEFAULT_LOCK};

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    /// Path to the configuration file. Defaults to `flusso.toml` when it
    /// exists; otherwise the compiled `--lock` is checked.
    #[arg(short, long, env = "FLUSSO_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// The compiled lock to compare with what the config compiles to (as
    /// `flusso run` resolves it), or to check when there is no config.
    #[arg(long, env = "FLUSSO_LOCK", default_value = DEFAULT_LOCK)]
    lock: std::path::PathBuf,

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

    /// Rewrite the `--lock` from the config when it isn't what the config
    /// compiles to (stale, unreadable, written differently, or missing),
    /// exactly as `flusso build` writes it. Fixes the lock only: it never
    /// changes the database, the schemas or the config, and every other
    /// finding is reported and decides the exit code as without it. No
    /// environment variable sets it.
    #[arg(long)]
    fix: bool,

    /// When the human-readable report is coloured: `auto` colours it when
    /// stdout is a terminal and `NO_COLOR` is unset.
    #[arg(long, env = "FLUSSO_COLOR", value_enum, default_value_t = print::ColorChoice::Auto)]
    color: print::ColorChoice,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// A colored, human-readable report (default).
    Human,
    /// The same report as one JSON document: `config_file`, `lock`, `server`,
    /// `coverage`, `pre_image`, `gaps`, `diagnostics`, `summary`.
    Json,
}

pub(crate) async fn execute(args: CheckArgs) -> anyhow::Result<()> {
    let (mut config, compiled, lock, path) = match plan_config(
        false,
        args.config.as_deref(),
        Path::new(DEFAULT_CONFIG),
        |path| path.exists(),
    ) {
        ConfigPlan::Compile(path) => {
            let compiled = config::compile(&path)
                .with_context(|| format!("loading config from {}", path.display()))?;
            let lock = LockState::of(&compiled, &args.lock);
            (compiled.config.clone(), Some(compiled), lock, path)
        }
        ConfigPlan::Fallback | ConfigPlan::UseLock => {
            let config = config::load_compiled(&args.lock)
                .with_context(|| crate::lock::no_config_headline(&args.lock, " to check"))?;
            (config, None, LockState::NotCompared, args.lock.clone())
        }
        ConfigPlan::Missing(path) => anyhow::bail!("config file {} not found", path.display()),
    };
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
    adapters::validate(&config).with_context(|| format!("validating {}", path.display()))?;
    daemon::backoff(&config).with_context(|| format!("validating {}", path.display()))?;
    daemon::index_budget_share(&config)
        .with_context(|| format!("validating {}", path.display()))?;
    daemon::max_waiting(&config).with_context(|| format!("validating {}", path.display()))?;
    daemon::max_pending_documents(&config)
        .with_context(|| format!("validating {}", path.display()))?;
    crate::backends::compile_scripts(&config)
        .with_context(|| format!("compiling the scripts of {}", path.display()))?;
    let lock = match &compiled {
        Some(compiled) if args.fix => lock.fixed(compiled, &args.lock)?,
        _ => lock,
    };
    let rewritten = matches!(lock, LockState::Rewritten { .. });
    let lock_path = args.lock.clone();
    after_the_fix(
        inspect(args, config, lock, path).await,
        rewritten,
        &lock_path,
    )
}

/// What `check` says when the inspection after `--fix` fails: the lock the
/// fix rewrote stays rewritten, and the error says so, since the report that
/// would have is never printed.
fn after_the_fix(result: anyhow::Result<()>, rewritten: bool, lock: &Path) -> anyhow::Result<()> {
    if rewritten {
        result.with_context(|| format!("`check --fix` rewrote {}, then failed", lock.display()))
    } else {
        result
    }
}

/// Everything `check` inspects once the config is loaded, validated and its
/// lock judged (and, with `--fix`, rewritten), then the report.
async fn inspect(
    args: CheckArgs,
    config: Config,
    lock: LockState,
    path: std::path::PathBuf,
) -> anyhow::Result<()> {
    let config = Arc::new(config);

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

    let online = match (&coverage, &pre_image) {
        (Some(coverage), Some(pre_image)) => Some(Online::new(
            &source_spec(&config),
            refused,
            coverage,
            pre_image,
            manage,
            postgres.manage_replica_identity,
        )),
        _ => None,
    };
    let report = Report {
        path: &path,
        config: &config,
        lock: &lock,
        lock_path: &args.lock,
        config_path: args.config.as_deref(),
        fix: args.fix,
        diagnostics: &diagnostics,
        online,
    };

    let mut out = std::io::stdout().lock();
    match args.format {
        OutputFormat::Json => {
            writeln!(out, "{}", serde_json::to_string_pretty(&report.json())?)?;
        }
        OutputFormat::Human => report.human(&mut out, print::Pen::for_stdout(args.color))?,
    }

    match report.failure() {
        Some(failure) => anyhow::bail!("check failed: {failure}"),
        None => Ok(()),
    }
}

/// What an online check found beside the schema diagnostics.
struct Online<'a> {
    /// The refusal `flusso run` would stop with, if any.
    server: Option<&'a str>,
    coverage: &'a CoverageReport,
    pre_image: &'a PreImageReport,
    manage_publication: bool,
    manage_replica_identity: bool,
    /// The gaps in capture `flusso run` won't close: errors.
    gaps: Vec<crate::gaps::Gap>,
}

impl<'a> Online<'a> {
    fn new(
        spec: &source::SourceSpec,
        server: Option<&'a str>,
        coverage: &'a CoverageReport,
        pre_image: &'a PreImageReport,
        manage_publication: bool,
        manage_replica_identity: bool,
    ) -> Self {
        Self {
            gaps: crate::gaps::of(
                spec,
                coverage,
                manage_publication,
                pre_image,
                manage_replica_identity,
            ),
            server,
            coverage,
            pre_image,
            manage_publication,
            manage_replica_identity,
        }
    }
}

/// Everything `check` found, rendered as the human report or as JSON: one
/// value, so the two formats say the same thing. Neither holds the config or
/// the mapping, which are `flusso show`'s.
struct Report<'a> {
    path: &'a Path,
    config: &'a Config,
    /// How the lock stands against the config.
    lock: &'a LockState,
    lock_path: &'a Path,
    /// The `--config` given, which the lock's advice names.
    config_path: Option<&'a Path>,
    /// `--fix` was asked for.
    fix: bool,
    diagnostics: &'a [Diagnostic],
    /// `None` under `--offline`.
    online: Option<Online<'a>>,
}

impl Report<'_> {
    fn summary(&self) -> print::Summary {
        print::Summary::of(
            self.diagnostics,
            self.online.as_ref().map_or(0, |o| o.gaps.len()) + usize::from(self.lock.is_error()),
            self.online.is_none(),
            self.online.as_ref().is_some_and(|o| o.server.is_some()),
        )
    }

    /// The lock this report judged, as its advice names it.
    fn lock_at(&self) -> LockAt<'_> {
        LockAt {
            lock: self.lock_path,
            config: self.config_path,
            offline: self.online.is_none(),
        }
    }

    /// Why the check fails, or `None` when it passed. Decided by the summary,
    /// so the exit code and the report's last line never disagree.
    fn failure(&self) -> Option<String> {
        let summary = self.summary();
        if summary.passed() {
            return None;
        }
        if let Some(refusal) = self.online.as_ref().and_then(|o| o.server) {
            return Some(format!(
                "flusso run would refuse the source server: {refusal}"
            ));
        }
        let gaps = self.online.as_ref().map_or(0, |o| o.gaps.len());
        let lock = usize::from(self.lock.is_error());
        let plural = |n: usize, one: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {one}s")
            }
        };
        let mut failed = Vec::new();
        if summary.errors > gaps + lock {
            failed.push(plural(summary.errors - gaps - lock, "schema error"));
        }
        if gaps > 0 {
            failed.push(format!(
                "{} `flusso run` won't close",
                plural(gaps, "capture gap")
            ));
        }
        if lock > 0 {
            failed.push(format!(
                "{} is {}",
                self.lock_path.display(),
                match self.lock {
                    LockState::Stale { .. } => "stale",
                    _ => "unreadable",
                }
            ));
        }
        let mut failure = failed.join(" and ");
        if summary.offline {
            failure.push_str(" (database checks not run)");
        }
        Some(failure)
    }

    /// The human report: one line per section that passes, a section with a
    /// problem in full (with its SQL), then the schema findings, warnings
    /// above errors, and a summary line. What failed is always on the last
    /// screen.
    fn human(&self, out: &mut impl Write, pen: print::Pen) -> anyhow::Result<()> {
        print::config_line(out, pen, self.path, self.config)?;
        print::lock(out, pen, &self.lock_at(), self.lock)?;
        if self.fix && *self.lock == LockState::NotCompared {
            print::note(
                out,
                pen,
                &format!(
                    "no config to write {} from: --fix wrote nothing",
                    self.lock_path.display()
                ),
            )?;
        }
        match &self.online {
            None => {
                print::offline(out, pen)?;
                print::offline_diagnostics(out, pen, self.diagnostics)?;
                print::lock_error(out, pen, &self.lock_at(), self.lock)?;
            }
            Some(online) => {
                let coverage_fails =
                    print::coverage_status(online.coverage, online.manage_publication)
                        == print::Status::Problem;
                let pre_image_fails =
                    print::pre_image_status(online.pre_image, online.manage_replica_identity)
                        == print::Status::Problem;
                print::server(out, pen, online.server)?;
                if !coverage_fails {
                    print::coverage(out, pen, online.coverage, online.manage_publication)?;
                }
                if !pre_image_fails {
                    print::pre_image(out, pen, online.pre_image, online.manage_replica_identity)?;
                }
                print::diagnostics(out, pen, self.diagnostics)?;
                print::gaps(out, pen, &online.gaps)?;
                print::lock_error(out, pen, &self.lock_at(), self.lock)?;
                if coverage_fails {
                    print::coverage(out, pen, online.coverage, online.manage_publication)?;
                }
                if pre_image_fails {
                    print::pre_image(out, pen, online.pre_image, online.manage_replica_identity)?;
                }
            }
        }
        writeln!(out)?;
        print::summary(out, pen, &self.summary())
    }

    /// The same report as one JSON document: each section with its status and
    /// detail, the diagnostics, and the summary. The sections the database
    /// gives are `null` under `--offline`.
    fn json(&self) -> serde_json::Value {
        let strings = |items: &[source::QualifiedTable]| {
            items.iter().map(ToString::to_string).collect::<Vec<_>>()
        };
        let disabled = self.config.indexes.values().filter(|i| !i.enabled).count();
        let summary = self.summary();
        let online = self.online.as_ref();
        serde_json::json!({
            "config_file": {
                "path": self.path.display().to_string(),
                "status": print::Status::Ok.as_str(),
                "indexes": self.config.indexes.len(),
                "disabled_indexes": disabled,
                "sinks": self.config.sinks.len(),
            },
            "lock": {
                "status": if self.lock.is_error() { print::Status::Problem } else { print::Status::Ok }.as_str(),
                "state": self.lock.as_str(),
                "path": self.lock_path.display().to_string(),
                "rewritten": matches!(self.lock, LockState::Rewritten { .. }),
                "message": self.lock.message(&self.lock_at()),
            },
            "server": online.map(|o| serde_json::json!({
                "status": if o.server.is_none() { print::Status::Ok } else { print::Status::Problem }.as_str(),
                "ok": o.server.is_none(),
                "problem": o.server,
            })),
            "coverage": online.map(|o| {
                let c = o.coverage;
                serde_json::json!({
                    "status": print::coverage_status(c, o.manage_publication).as_str(),
                    "satisfied": c.satisfied,
                    "manageable": c.manageable,
                    "will_manage": o.manage_publication,
                    "present": strings(&c.present),
                    "missing": strings(&c.missing),
                    "misrouted": strings(&c.misrouted),
                    "blockers": c.blockers,
                    "remediation": c.remediation,
                })
            }),
            "pre_image": online.map(|o| {
                let p = o.pre_image;
                let manage = o.manage_replica_identity;
                let gaps = |gaps: &[source::PreImageGap]| gaps.iter().map(|gap| serde_json::json!({
                    "table": gap.table.to_string(),
                    "missing": gap.missing.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "missing_key": gap.missing_key.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    "manageable": gap.manageable,
                    "will_manage": gap.will_manage(manage),
                    "blockers": gap.blockers,
                    "remediation": gap.remediation,
                    "kept": gap.kept,
                })).collect::<Vec<_>>();
                serde_json::json!({
                    "status": print::pre_image_status(p, manage).as_str(),
                    "satisfied": p.satisfied(),
                    "complete": p.complete(),
                    "gaps": gaps(&p.gaps),
                    "partial": gaps(&p.partial),
                })
            }),
            "gaps": online.map(|o| o.gaps.iter().map(|gap| serde_json::json!({
                "kind": gap.kind.as_str(),
                "table": gap.table.to_string(),
                "indexes": gap.indexes.iter().map(ToString::to_string).collect::<Vec<_>>(),
                "message": gap.message,
            })).collect::<Vec<_>>()),
            "diagnostics": self.diagnostics
                .iter()
                .map(|d| serde_json::json!({
                    "index": d.index.as_ref(),
                    "field": d.field.as_ref(),
                    "path": d.dotted_path(),
                    "positions": d.positions,
                    "anchor": d.anchor.as_str(),
                    "severity": format!("{:?}", d.severity).to_lowercase(),
                    "kind": d.kind.as_str(),
                    "message": d.message,
                }))
                .collect::<Vec<_>>(),
            "summary": {
                "passed": summary.passed(),
                "errors": summary.errors,
                "warnings": summary.warnings,
                "offline": summary.offline,
                "database_checked": !summary.offline,
                "server_refused": summary.server_refused,
                "line": summary.line(),
            },
        })
    }
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
        built(&config, diagnostics, online)
            .human(&mut out, pen)
            .unwrap();
        String::from_utf8(out).unwrap()
    }

    fn built<'a>(
        config: &'a Config,
        diagnostics: &'a [Diagnostic],
        online: Option<&Online<'a>>,
    ) -> Report<'a> {
        with_lock(config, diagnostics, online, &LockState::Current)
    }

    fn with_lock<'a>(
        config: &'a Config,
        diagnostics: &'a [Diagnostic],
        online: Option<&Online<'a>>,
        lock: &'a LockState,
    ) -> Report<'a> {
        Report {
            path: Path::new("flusso.toml"),
            config,
            lock,
            lock_path: Path::new("flusso.lock"),
            config_path: None,
            fix: false,
            diagnostics,
            online: online.map(|o| {
                Online::new(
                    &source_spec(config),
                    o.server,
                    o.coverage,
                    o.pre_image,
                    o.manage_publication,
                    o.manage_replica_identity,
                )
            }),
        }
    }

    fn online<'a>(
        coverage: &'a CoverageReport,
        pre_image: &'a PreImageReport,
        server: Option<&'a str>,
    ) -> Online<'a> {
        Online::new(
            &source_spec(&config()),
            server,
            coverage,
            pre_image,
            true,
            true,
        )
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
             ✓ flusso.lock is current\n\
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
        // A gap `run` won't close is an error (issue #485): printed last, after
        // every one-line section, with its SQL, and counted.
        let error = printed
            .find("[error] public.users  `public.users` is not in the publication")
            .unwrap_or_else(|| panic!("no gap line in:\n{printed}"));
        assert!(
            printed.find("✓ replica identity").unwrap() < error,
            "{printed}"
        );
        assert!(
            error < printed.find("\nPublication coverage\n").unwrap(),
            "{printed}"
        );
        assert_eq!(
            printed.trim_end().lines().last(),
            Some("✗ check failed: 1 error, 0 warnings"),
            "{printed}"
        );
    }

    /// A table that only loses the update skip, with management off, prints in
    /// full with its SQL in place, and is no error.
    #[test]
    fn a_partial_replica_identity_set_by_hand_is_no_error() {
        let coverage = satisfied();
        let pre_image = PreImageReport {
            gaps: Vec::new(),
            partial: vec![source::PreImageGap {
                table: table("products"),
                missing: Vec::new(),
                missing_key: Vec::new(),
                manageable: false,
                blockers: vec!["role does not own table public.products".to_owned()],
                remediation: "ALTER TABLE public.products REPLICA IDENTITY FULL;".to_owned(),
                kept: None,
            }],
        };
        let printed = report(
            &[],
            Some(&online(&coverage, &pre_image, None)),
            print::Pen::plain(),
        );
        assert!(printed.contains("\nReplica identity\n"), "{printed}");
        assert!(printed.contains("ALTER TABLE public.products"), "{printed}");
        assert!(!printed.contains("[error]"), "{printed}");
        assert_eq!(
            printed.trim_end().lines().last(),
            Some("✓ check passed"),
            "{printed}"
        );
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

    /// Both formats render the one report: the same sections, the same
    /// findings and the same counts, and no config or mapping in JSON.
    /// The lock's place in the report (issue #485): current, missing or not
    /// compared is one line and no error; stale or unreadable is an error in
    /// the error block at the bottom, counted in the summary and the exit, and
    /// the JSON `lock` section says the same.
    #[test]
    fn the_lock_is_one_line_unless_check_fails_on_it() {
        let config = config();
        let clean = satisfied();
        let pre_image = PreImageReport::default();
        let warning = [finding(source::DiagnosticKind::IgnoredPrimaryKey, "team")];
        let stale = LockState::Stale {
            differs: vec!["the index `products` changed".to_owned()],
            crlf: false,
        };
        let unreadable = LockState::Unreadable {
            reason: "compiled config format version 2 is not supported".to_owned(),
        };
        for (lock, line, error) in [
            (LockState::Current, "✓ flusso.lock is current", false),
            (
                LockState::Missing,
                "! flusso.lock not found; flusso build writes it",
                false,
            ),
            (
                LockState::NotCompared,
                "! no config to compare flusso.lock with: the lock is what was checked",
                false,
            ),
            (
                stale,
                "[error] flusso.lock  `flusso.lock` was compiled from another config than this one (the index `products` changed)",
                true,
            ),
            (
                unreadable,
                "[error] flusso.lock  `flusso.lock` can't be read as a lock",
                true,
            ),
        ] {
            for online in [Some(online(&clean, &pre_image, None)), None] {
                let report = with_lock(&config, &warning, online.as_ref(), &lock);
                let mut out = Vec::new();
                report.human(&mut out, print::Pen::plain()).unwrap();
                let human = String::from_utf8(out).unwrap();
                assert!(
                    human.contains(line),
                    "{lock:?}: {line} missing from\n{human}"
                );
                let json = report.json();
                assert_eq!(json["lock"]["state"], lock.as_str(), "{lock:?}");
                assert_eq!(
                    json["lock"]["status"],
                    if error { "problem" } else { "ok" },
                    "{lock:?}"
                );
                assert_eq!(json["summary"]["errors"], usize::from(error), "{lock:?}");
                assert_eq!(report.failure().is_some(), error, "{lock:?}");
                if error {
                    let at = human.find("[error] flusso.lock").unwrap();
                    assert!(
                        at > human.find("[warning]").unwrap(),
                        "errors last:\n{human}"
                    );
                    let failure = report.failure().unwrap();
                    assert!(failure.contains("flusso.lock is"), "{failure}");
                }
            }
        }
    }

    /// `--fix` rewrote the lock: one passing line saying where, and JSON
    /// keeps the state it found with `rewritten: true` (owner decision on
    /// #486). With no config, `--fix` says it had nothing to write from.
    #[test]
    fn a_rewritten_lock_is_one_line_and_a_lock_alone_is_not_rewritten() {
        let config = config();
        let rewritten = LockState::Rewritten {
            was: Box::new(LockState::Stale {
                differs: Vec::new(),
                crlf: false,
            }),
        };
        let report = with_lock(&config, &[], None, &rewritten);
        let mut out = Vec::new();
        report.human(&mut out, print::Pen::plain()).unwrap();
        let human = String::from_utf8(out).unwrap();
        assert!(human.contains("✓ lock rewritten: flusso.lock"), "{human}");
        let json = report.json();
        assert_eq!(json["lock"]["state"], "lock_stale", "{json}");
        assert_eq!(json["lock"]["rewritten"], true, "{json}");
        assert_eq!(json["lock"]["status"], "ok", "{json}");
        assert_eq!(json["lock"]["path"], "flusso.lock", "{json}");
        assert!(report.failure().is_none());

        let mut alone = with_lock(&config, &[], None, &LockState::NotCompared);
        alone.fix = true;
        let mut out = Vec::new();
        alone.human(&mut out, print::Pen::plain()).unwrap();
        let human = String::from_utf8(out).unwrap();
        assert!(
            human.contains("! no config to write flusso.lock from: --fix wrote nothing"),
            "{human}"
        );
        assert_eq!(alone.json()["lock"]["rewritten"], false);
    }

    /// `--fix` reads no environment variable: a CI job must not rewrite the
    /// lock because of a variable it inherited.
    #[test]
    fn fix_is_a_flag_only_with_no_environment_variable() {
        use clap::Args as _;
        let command = CheckArgs::augment_args(clap::Command::new("check"));
        let fix = command
            .get_arguments()
            .find(|arg| arg.get_id() == "fix")
            .expect("check has --fix");
        assert_eq!(fix.get_env(), None);
        let help = fix.get_help().map(ToString::to_string).unwrap_or_default();
        assert!(help.contains("lock only"), "{help}");
    }

    /// A failure after `--fix` rewrote the lock (an unreachable database,
    /// say) still says the lock was rewritten: the report that would have
    /// said so is never printed.
    #[test]
    fn a_failure_after_the_fix_says_the_lock_was_rewritten() {
        let lock = Path::new("deploy/flusso.lock");
        let failed = || Err(anyhow::anyhow!("connecting to the database"));
        let said = format!("{:#}", after_the_fix(failed(), true, lock).unwrap_err());
        assert_eq!(
            said,
            "`check --fix` rewrote deploy/flusso.lock, then failed: connecting to the database"
        );
        let said = format!("{:#}", after_the_fix(failed(), false, lock).unwrap_err());
        assert_eq!(said, "connecting to the database");
        assert!(after_the_fix(Ok(()), true, lock).is_ok());
    }

    /// `check` fails exactly when its summary says it failed, and the
    /// failure names everything that counts: one judge for the exit code and
    /// the last line.
    #[test]
    fn the_exit_follows_the_summary() {
        let config = config();
        let failing = CoverageReport {
            satisfied: false,
            present: vec![table("products")],
            missing: vec![table("users")],
            misrouted: Vec::new(),
            manageable: false,
            blockers: vec!["role does not own table public.users".to_owned()],
            remediation: Vec::new(),
        };
        let clean = satisfied();
        let pre_image = PreImageReport::default();
        let error = [finding(source::DiagnosticKind::Nullability, "title")];
        let warning = [finding(source::DiagnosticKind::IgnoredPrimaryKey, "team")];
        let cases: Vec<(&str, Option<Online<'_>>, &[Diagnostic], Option<&str>)> = vec![
            ("clean", Some(online(&clean, &pre_image, None)), &[], None),
            (
                "warning",
                Some(online(&clean, &pre_image, None)),
                &warning,
                None,
            ),
            ("offline warning", None, &warning, None),
            (
                "offline error",
                None,
                &error,
                Some("1 schema error (database checks not run)"),
            ),
            (
                "gap",
                Some(online(&failing, &pre_image, None)),
                &warning,
                Some("1 capture gap `flusso run` won't close"),
            ),
            (
                "error and gap",
                Some(online(&failing, &pre_image, None)),
                &error,
                Some("1 schema error and 1 capture gap `flusso run` won't close"),
            ),
            (
                "refused server",
                Some(online(&clean, &pre_image, Some("a standby"))),
                &[],
                Some("flusso run would refuse the source server: a standby"),
            ),
        ];
        for (case, online, diagnostics, failure) in cases {
            let report = built(&config, diagnostics, online.as_ref());
            let found = report.failure();
            assert_eq!(found.is_none(), report.summary().passed(), "{case}");
            if let Some(failure) = failure {
                let found = found.unwrap();
                assert!(found.contains(failure), "{case}: {found}");
            }
        }
    }

    #[test]
    fn both_formats_render_the_same_report() {
        let config = config();
        let clean_coverage = satisfied();
        let failing_coverage = CoverageReport {
            satisfied: false,
            present: vec![table("products")],
            missing: vec![table("users")],
            misrouted: Vec::new(),
            manageable: false,
            blockers: vec!["role does not own table public.users".to_owned()],
            remediation: vec!["ALTER PUBLICATION flusso ADD TABLE public.users;".to_owned()],
        };
        let pre_image = PreImageReport::default();
        let findings = [
            finding(source::DiagnosticKind::UndeclaredArrayColumn, "tags"),
            finding(source::DiagnosticKind::IgnoredPrimaryKey, "team"),
        ];
        let cases: Vec<(&str, Option<Online<'_>>, &[Diagnostic])> = vec![
            (
                "clean",
                Some(online(&clean_coverage, &pre_image, None)),
                &[],
            ),
            (
                "failing section",
                Some(online(&failing_coverage, &pre_image, None)),
                &findings,
            ),
            ("offline", None, &findings),
        ];
        for (case, online, diagnostics) in cases {
            let report = built(&config, diagnostics, online.as_ref());
            let mut out = Vec::new();
            report.human(&mut out, print::Pen::plain()).unwrap();
            let human = String::from_utf8(out).unwrap();
            let json = report.json();
            assert_eq!(
                json["gaps"].is_null(),
                online.is_none(),
                "{case}: offline, `gaps` is not known, as the other database sections"
            );

            assert_eq!(
                json["summary"]["line"].as_str(),
                human.trim_end().lines().last(),
                "{case}"
            );
            let errors = human.matches("[error]").count();
            let warnings = human.matches("[warning]").count();
            assert_eq!(json["summary"]["errors"], errors, "{case}");
            assert_eq!(json["summary"]["warnings"], warnings, "{case}");
            let listed = json["diagnostics"].as_array().unwrap();
            let gaps = json["gaps"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default();
            assert_eq!(listed.len() + gaps.len(), errors + warnings, "{case}");
            for gap in gaps {
                let line = format!("[error] {}  ", gap["table"].as_str().unwrap());
                assert!(
                    human.contains(&line),
                    "{case}: {line} missing from\n{human}"
                );
            }
            for d in listed {
                let line = format!(
                    "[{}] {}.{}",
                    d["severity"].as_str().unwrap(),
                    d["index"].as_str().unwrap(),
                    d["path"].as_str().unwrap()
                );
                assert!(
                    human.contains(&line),
                    "{case}: {line} missing from\n{human}"
                );
            }
            for (key, heading, one_line) in [
                ("coverage", "\nPublication coverage\n", "✓ publication"),
                ("pre_image", "\nReplica identity\n", "✓ replica identity"),
                ("server", "\nSource server\n", "✓ source server"),
            ] {
                match json[key]["status"].as_str() {
                    None => {
                        assert!(json[key].is_null(), "{case}: {key}");
                        assert!(
                            !human.contains(one_line) && !human.contains(heading),
                            "{case}"
                        );
                    }
                    Some("problem") => assert!(human.contains(heading), "{case}: {key}\n{human}"),
                    Some(_) => assert!(human.contains(one_line), "{case}: {key}\n{human}"),
                }
            }
            assert_eq!(
                json["summary"]["offline"],
                human.contains("! offline"),
                "{case}"
            );
            for gone in ["config", "mappings"] {
                assert!(json.get(gone).is_none(), "{case}: {gone} in {json}");
            }
        }
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
