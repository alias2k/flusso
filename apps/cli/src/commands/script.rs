//! `flusso script run` — the fast loop for writing a script field: build one
//! document, run its index's scripts over it, and show the input, what each
//! script printed, and the finished document (or the failure, with the script
//! line and a caret).
//!
//! The document comes from the live database (`--id`, or the first row when
//! omitted) through the same builder a run uses, or from a JSON file
//! (`--input`) with no database at all. `--watch` re-runs it whenever the
//! config or a schema next to it changes.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{Context, anyhow, bail};
use clap::{Args, Subcommand};
use config::Config;
use kernel::{GenericValue, IndexName};
use script::{Printer, ScriptContext, Scripts};
use source::RowKey;
use source::document::{Document, DocumentBuilder, DocumentId};
use source_postgres::PgDocumentBuilder;

use crate::DEFAULT_CONFIG;
use crate::backends::source_spec;
use crate::commands::print::{self, Pen};

#[derive(Debug, Args)]
pub(crate) struct ScriptArgs {
    #[command(subcommand)]
    command: ScriptCommand,
}

#[derive(Debug, Subcommand)]
enum ScriptCommand {
    /// Run one index's scripts over one document and show the result.
    Run(RunArgs),
}

#[derive(Debug, Args)]
struct RunArgs {
    /// Path to the configuration file.
    #[arg(short, long, env = "FLUSSO_CONFIG", default_value = DEFAULT_CONFIG)]
    config: PathBuf,

    /// The index whose scripts to run.
    #[arg(long, env = "FLUSSO_SCRIPT_INDEX")]
    index: String,

    /// The root row's primary key to build from the database. Without it (and
    /// without `--input`), the first row is used.
    #[arg(long, env = "FLUSSO_SCRIPT_ID", conflicts_with = "input")]
    id: Option<String>,

    /// A JSON document to run the scripts over instead of a database row.
    #[arg(long, env = "FLUSSO_SCRIPT_INPUT")]
    input: Option<PathBuf>,

    /// Show only this field of the finished document (`label`,
    /// `variants[0].price`).
    #[arg(long, env = "FLUSSO_SCRIPT_FIELD")]
    field: Option<String>,

    /// Re-run whenever the config or a schema beside it changes.
    #[arg(long, env = "FLUSSO_SCRIPT_WATCH")]
    watch: bool,
}

pub(crate) async fn execute(args: ScriptArgs) -> anyhow::Result<()> {
    let ScriptCommand::Run(args) = args.command;
    if !args.watch {
        return run_once(&args).await;
    }
    loop {
        if let Err(error) = run_once(&args).await {
            let mut out = std::io::stdout().lock();
            print::failure(&mut out, Pen::detect(), &format!("{error:#}"))?;
        }
        wait_for_change(&args.config).await;
    }
}

async fn run_once(args: &RunArgs) -> anyhow::Result<()> {
    let config = config::load(&args.config)
        .with_context(|| format!("loading config from {}", args.config.display()))?;
    let index = IndexName::try_new(&args.index)
        .map_err(|e| anyhow!("invalid index name `{}`: {e}", args.index))?;
    let spec = source_spec(&config);
    if spec.schema(&index).is_none() {
        bail!("no enabled index `{index}` in {}", args.config.display());
    }
    let printed: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&printed);
    let printer: Printer = Arc::new(move |context: &ScriptContext<'_>, text: &str| {
        if let Ok(mut lines) = sink.lock() {
            lines.push(format!("{}: {text}", context.field));
        }
    });
    let scripts = Scripts::compile_with_printer(&spec, Some(printer))
        .map_err(|errors| anyhow!("{errors}"))?;

    let (label, input) = match &args.input {
        Some(path) => (path.display().to_string(), read_input(path)?),
        None => from_database(&config, &index, args.id.as_deref()).await?,
    };

    let pen = Pen::detect();
    let mut out = std::io::stdout().lock();
    print::heading(&mut out, pen, "input", &label)?;
    writeln!(out, "{}", pretty(&input)?)?;

    let mut finished = input;
    let outcome = scripts.apply(&index, &mut finished);
    let lines = printed.lock().map(|l| l.clone()).unwrap_or_default();
    if !lines.is_empty() {
        print::heading(&mut out, pen, "printed", "")?;
        for line in lines {
            writeln!(out, "{line}")?;
        }
    }
    match outcome {
        Ok(()) => {
            let (title, shown) = match &args.field {
                Some(path) => (
                    format!("output `{path}`"),
                    lookup(&finished, path)
                        .ok_or_else(|| anyhow!("no field `{path}` in the finished document"))?,
                ),
                None => ("output".to_owned(), &finished),
            };
            print::heading(&mut out, pen, &title, "")?;
            writeln!(out, "{}", pretty(shown)?)?;
            Ok(())
        }
        Err(failure) => {
            writeln!(out)?;
            print::failure(&mut out, pen, &failure.to_string())?;
            bail!("a script failed")
        }
    }
}

async fn from_database(
    config: &Config,
    index: &IndexName,
    id: Option<&str>,
) -> anyhow::Result<(String, GenericValue)> {
    let (_, sql_url) = crate::backends::source_sql_url(config)?;
    let spec = Arc::new(source_spec(config));
    let documents = PgDocumentBuilder::connect(&sql_url, Arc::clone(&spec))
        .await
        .context("connecting to the database")?;
    let Some(id) = id else {
        let body = documents
            .sample_document(index)
            .await?
            .ok_or_else(|| anyhow!("index `{index}` has no rows to build"))?;
        return Ok((format!("{index}, first row"), body));
    };
    let primary_key = spec
        .schema(index)
        .and_then(|schema| schema.primary_key.clone())
        .ok_or_else(|| anyhow!("index `{index}` has no single-column primary key"))?;
    let document = DocumentId {
        index: index.clone(),
        key: RowKey::from_text(primary_key, id),
    };
    match documents.build(&document).await? {
        Document::Upsert { body, .. } => Ok((format!("{index}, id {id}"), body)),
        Document::Delete { .. } | Document::Failed { .. } => {
            bail!("no document `{id}` in `{index}` (the row is missing or soft-deleted)")
        }
    }
}

fn read_input(path: &Path) -> anyhow::Result<GenericValue> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let json: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    Ok(from_json(json))
}

/// Untyped JSON into the document vocabulary: whole numbers as integers,
/// other numbers as doubles, everything else as it reads.
fn from_json(json: serde_json::Value) -> GenericValue {
    match json {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(GenericValue::BigInt)
            .or_else(|| n.as_f64().map(GenericValue::Double))
            .unwrap_or(GenericValue::Null),
        serde_json::Value::String(s) => GenericValue::String(s),
        serde_json::Value::Array(items) => {
            GenericValue::Array(items.into_iter().map(from_json).collect())
        }
        serde_json::Value::Object(map) => GenericValue::Map(
            map.into_iter()
                .map(|(k, v)| (k, from_json(v)))
                .collect::<BTreeMap<_, _>>(),
        ),
    }
}

fn pretty(value: &GenericValue) -> anyhow::Result<String> {
    Ok(serde_json::to_string_pretty(&sink::to_json(value))?)
}

/// The value at a dotted path with `[n]` array positions.
fn lookup<'a>(value: &'a GenericValue, path: &str) -> Option<&'a GenericValue> {
    let mut current = value;
    for segment in path.split('.') {
        let (name, positions) = match segment.find('[') {
            Some(at) => segment.split_at(at),
            None => (segment, ""),
        };
        if !name.is_empty() {
            let GenericValue::Map(map) = current else {
                return None;
            };
            current = map.get(name)?;
        }
        for position in positions.split(['[', ']']).filter(|p| !p.is_empty()) {
            let GenericValue::Array(items) = current else {
                return None;
            };
            current = items.get(position.parse::<usize>().ok()?)?;
        }
    }
    Some(current)
}

/// Block until a `.toml` or `.yml` file under the config's directory changes.
async fn wait_for_change(config: &Path) {
    let dir = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_owned();
    let before = stamp(&dir);
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if stamp(&dir) != before {
            return;
        }
    }
}

/// The newest modification time among the config and schema files under `dir`
/// (two levels deep), with their count, so an added or removed file counts too.
fn stamp(dir: &Path) -> (Option<SystemTime>, usize) {
    fn walk(dir: &Path, depth: u8, newest: &mut Option<SystemTime>, count: &mut usize) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && depth > 0 {
                walk(&path, depth - 1, newest, count);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("toml" | "yml" | "yaml")
            ) {
                *count += 1;
                let modified = entry.metadata().and_then(|m| m.modified()).ok();
                *newest = (*newest).max(modified);
            }
        }
    }
    let mut newest = None;
    let mut count = 0;
    walk(dir, 2, &mut newest, &mut count);
    (newest, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: Vec<(&str, GenericValue)>) -> GenericValue {
        GenericValue::Map(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
    }

    #[test]
    fn lookup_walks_names_and_positions() {
        let doc = map(vec![(
            "variants",
            GenericValue::Array(vec![map(vec![("price", GenericValue::BigInt(3))])]),
        )]);
        assert_eq!(
            lookup(&doc, "variants[0].price"),
            Some(&GenericValue::BigInt(3))
        );
        assert_eq!(lookup(&doc, "variants[1].price"), None);
        assert_eq!(lookup(&doc, "missing"), None);
    }

    #[test]
    fn json_input_keeps_integers_apart_from_numbers() {
        let json = serde_json::json!({"n": 3, "x": 1.5, "s": "a"});
        assert_eq!(
            from_json(json),
            map(vec![
                ("n", GenericValue::BigInt(3)),
                ("s", GenericValue::String("a".to_owned())),
                ("x", GenericValue::Double(1.5)),
            ])
        );
    }
}
