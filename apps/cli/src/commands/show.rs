//! `flusso show` — what a config declares, with no database: `show config`
//! prints the deployment (source, stream, sinks, indexes; secrets redacted)
//! and `show mapping [index]` every enabled index's typed mapping, or the one
//! named, a disabled one included (said so first).
//!
//! `flusso check` only checks; this is where the detail it used to print
//! lives. The config is read as `check --offline` reads it: `--config` (or
//! `flusso.toml` when it exists), else the compiled `--lock`, then every port
//! entry is validated against its adapter.
//!
//! ```text
//! flusso show config
//! flusso show mapping products --format json
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Args, Subcommand, ValueEnum};
use config::{Config, IndexMapping};

use crate::commands::print;
use crate::{DEFAULT_CONFIG, DEFAULT_LOCK, adapters};

#[derive(Debug, Args)]
pub(crate) struct ShowArgs {
    #[command(subcommand)]
    what: Show,
}

#[derive(Debug, Subcommand)]
enum Show {
    /// The deployment: source, stream, sinks and indexes, secrets redacted.
    Config(ConfigArgs),
    /// The typed mapping of every index, or of the one named.
    Mapping(MappingArgs),
}

#[derive(Debug, Args)]
struct ConfigArgs {
    #[command(flatten)]
    read: Read,
}

#[derive(Debug, Args)]
struct MappingArgs {
    /// The index to show; every index when omitted.
    index: Option<String>,

    #[command(flatten)]
    read: Read,
}

/// Where the config comes from, and how it is printed.
#[derive(Debug, Args)]
struct Read {
    /// Path to the configuration file. Defaults to `flusso.toml` when it
    /// exists; otherwise the compiled `--lock` is read.
    #[arg(short, long, env = "FLUSSO_CONFIG")]
    config: Option<PathBuf>,

    /// Compiled lock to read when there is no config file.
    #[arg(long, env = "FLUSSO_LOCK", default_value = DEFAULT_LOCK)]
    lock: PathBuf,

    /// Output format: a human-readable report, or JSON for piping.
    #[arg(long, env = "FLUSSO_FORMAT", value_enum, default_value_t = Format::Human)]
    format: Format,

    /// When the human-readable output is coloured: `auto` colours it when
    /// stdout is a terminal and `NO_COLOR` is unset.
    #[arg(long, env = "FLUSSO_COLOR", value_enum, default_value_t = print::ColorChoice::Auto)]
    color: print::ColorChoice,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    /// A coloured, human-readable listing (default).
    Human,
    /// A single JSON document.
    Json,
}

pub(crate) fn execute(args: ShowArgs) -> anyhow::Result<()> {
    let mut out = std::io::stdout().lock();
    match args.what {
        Show::Config(args) => {
            let config = read(&args.read)?;
            let pen = print::Pen::for_stdout(args.read.color);
            config_to(&mut out, pen, args.read.format, &config)
        }
        Show::Mapping(args) => {
            let config = read(&args.read)?;
            let pen = print::Pen::for_stdout(args.read.color);
            mapping_to(
                &mut out,
                pen,
                args.read.format,
                &config,
                args.index.as_deref(),
            )
        }
    }
}

/// The config `read` names, validated against the adapters.
fn read(read: &Read) -> anyhow::Result<Config> {
    let default = Path::new(DEFAULT_CONFIG);
    let config = match &read.config {
        Some(path) => load(path)?,
        None if default.exists() => load(default)?,
        None => config::load_compiled(&read.lock).with_context(|| {
            format!(
                "no {DEFAULT_CONFIG} and no compiled lock at {}",
                read.lock.display()
            )
        })?,
    };
    adapters::validate(&config).context("validating the config")?;
    Ok(config)
}

fn load(path: &Path) -> anyhow::Result<Config> {
    config::load(path).with_context(|| format!("loading config from {}", path.display()))
}

fn config_to(
    out: &mut impl Write,
    pen: print::Pen,
    format: Format,
    config: &Config,
) -> anyhow::Result<()> {
    match format {
        Format::Human => print::config(out, pen, config),
        Format::Json => {
            let document = print::redacted_config(config)?;
            writeln!(out, "{}", serde_json::to_string_pretty(&document)?)?;
            Ok(())
        }
    }
}

fn mapping_to(
    out: &mut impl Write,
    pen: print::Pen,
    format: Format,
    config: &Config,
    index: Option<&str>,
) -> anyhow::Result<()> {
    let mappings = chosen(config, index)?;
    match format {
        Format::Human => {
            if let Some(name) = index
                && config
                    .indexes
                    .iter()
                    .any(|(of, entry)| of.as_ref() == name && !entry.enabled)
            {
                print::note(
                    out,
                    pen,
                    &format!(
                        "index `{name}` is disabled: `flusso run` builds no index for it until it \
                         is enabled"
                    ),
                )?;
            }
            for mapping in &mappings {
                let Some(entry) = config.indexes.get(&mapping.index) else {
                    continue;
                };
                for path in completed_formats(&entry.schema.fields) {
                    print::note(
                        out,
                        pen,
                        &format!(
                            "{}.{path}: flusso appends `{}` to its `format`, so the dates it \
                             writes index; dates still print in the `format`'s first pattern",
                            mapping.index,
                            config::ISO_DATE_PATTERN
                        ),
                    )?;
                }
            }
            print::resolved(out, pen, &mappings)
        }
        Format::Json => {
            writeln!(out, "{}", serde_json::to_string_pretty(&mappings)?)?;
            Ok(())
        }
    }
}

/// The dotted path of every mapped field whose date `format` flusso appends
/// the ISO pattern to ([`config::iso_date_format`]).
fn completed_formats(fields: &[config::Field]) -> Vec<String> {
    let mut paths = Vec::new();
    for field in fields.iter().filter(|field| field.emit) {
        if config::iso_date_format(field).is_some() {
            paths.push(field.field.to_string());
        }
        paths.extend(
            completed_formats(field.children())
                .into_iter()
                .map(|path| format!("{}.{path}", field.field)),
        );
    }
    paths
}

/// Every enabled index's mapping, or the one of `index`, disabled or not; an
/// unknown name lists the indexes the config declares.
fn chosen(config: &Config, index: Option<&str>) -> anyhow::Result<Vec<IndexMapping>> {
    let Some(name) = index else {
        return Ok(config.resolve_mappings());
    };
    if let Some((of, entry)) = config.indexes.iter().find(|(of, _)| of.as_ref() == name) {
        return Ok(vec![entry.schema.resolve(of.clone())]);
    }
    let known: Vec<String> = config
        .indexes
        .iter()
        .map(|(of, entry)| {
            if entry.enabled {
                of.to_string()
            } else {
                format!("{of} (disabled)")
            }
        })
        .collect();
    anyhow::bail!(
        "no index `{name}` in the config; the indexes are: {}",
        if known.is_empty() {
            "(none)".to_owned()
        } else {
            known.join(", ")
        }
    )
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use config::ParseFrom;
    use config::toml::ConfigToml;

    use super::*;

    /// `products` and `users`, and `archive` disabled, read by a Postgres
    /// source whose URL holds a
    /// password, written to an OpenSearch sink whose password is literal and
    /// whose URL names a variable.
    fn config() -> Config {
        let mut draft = ConfigToml::try_parse(
            r#"
            [source]
            type = "postgres"
            connection_url = "postgres://flusso:hunter2@db:5432/app"

            [sinks.primary]
            type = "opensearch"
            url = { env = "SEARCH_URL" }
            username = "admin"
            password = "s3cret"

            [[index]]
            name = "products"
            schema = "products.schema.yml"
            enabled = true

            [[index]]
            name = "users"
            schema = "users.schema.yml"
            enabled = true

            [[index]]
            name = "archive"
            schema = "archive.schema.yml"
            enabled = false
            "#,
        )
        .unwrap();
        let entries = std::mem::take(&mut draft.index);
        let mut config = Config::from(draft);
        for entry in entries {
            let schema = serde_json::from_value(serde_json::json!({
                "version": 1, "table": entry.name.as_ref(), "db_schema": "public",
                "primary_key": "id",
                "fields": [
                    { "field": "id", "source": { "column": {
                        "column": "id", "ty": "integer", "nullable": false } } },
                    { "field": "title", "source": { "column": {
                        "column": "title", "ty": "keyword", "nullable": true } } },
                ],
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

    fn printed(write: impl FnOnce(&mut Vec<u8>) -> anyhow::Result<()>) -> String {
        let mut out = Vec::new();
        write(&mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn show_config_redacts_secrets() {
        let config = config();
        let human = printed(|out| config_to(out, print::Pen::plain(), Format::Human, &config));
        assert!(human.contains("connection_url=***"), "{human}");
        assert!(human.contains("password=***"), "{human}");
        assert!(human.contains("url=${SEARCH_URL}"), "{human}");
        assert!(human.contains("products"), "{human}");
        let json = printed(|out| config_to(out, print::Pen::plain(), Format::Json, &config));
        for leaked in ["hunter2", "s3cret"] {
            assert!(!human.contains(leaked), "{human}");
            assert!(!json.contains(leaked), "{json}");
        }
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(document["source"]["connection_url"], "***");
        assert_eq!(document["sinks"]["primary"]["password"], "***");
        assert_eq!(document["sinks"]["primary"]["url"]["env"], "SEARCH_URL");
        assert_eq!(document["sinks"]["primary"]["type"], "opensearch");
    }

    /// A literal secret is masked whole, whatever it holds: a password in a
    /// URL's query string, a userinfo with no password, a token.
    #[test]
    fn show_config_redacts_a_password_in_the_query_string() {
        let draft = ConfigToml::try_parse(
            r#"
            [source]
            type = "postgres"
            connection_url = "postgres://db:5432/app?user=flusso&password=hunter2"

            [sinks.primary]
            type = "opensearch"
            url = "https://os:9200/?token=abc123"

            [sinks.keyed]
            type = "opensearch"
            url = "https://apikey0451@os:9200"
            "#,
        )
        .unwrap();
        let config = Config::from(draft);
        for format in [Format::Human, Format::Json] {
            let out = printed(|out| config_to(out, print::Pen::plain(), format, &config));
            for leaked in ["hunter2", "abc123", "apikey0451"] {
                assert!(!out.contains(leaked), "{leaked} in {format:?}:\n{out}");
            }
        }
        let json = printed(|out| config_to(out, print::Pen::plain(), Format::Json, &config));
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(document["source"]["connection_url"], "***");
        assert_eq!(document["sinks"]["primary"]["url"], "***");
        assert_eq!(document["sinks"]["keyed"]["url"], "***");
    }

    #[test]
    fn show_mapping_prints_every_index() {
        let config = config();
        let human =
            printed(|out| mapping_to(out, print::Pen::plain(), Format::Human, &config, None));
        assert!(human.contains("Index  products"), "{human}");
        assert!(human.contains("Index  users"), "{human}");
        assert!(human.contains("title"), "{human}");
        let json = printed(|out| mapping_to(out, print::Pen::plain(), Format::Json, &config, None));
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(document.as_array().unwrap().len(), 2, "{json}");
    }

    #[test]
    fn show_mapping_prints_the_index_named() {
        let config = config();
        let human = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Human,
                &config,
                Some("users"),
            )
        });
        assert!(human.contains("Index  users"), "{human}");
        assert!(!human.contains("products"), "{human}");
        let json = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Json,
                &config,
                Some("users"),
            )
        });
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(document[0]["index"], "users", "{json}");
    }

    #[test]
    fn show_mapping_of_an_unknown_index_lists_the_known() {
        let config = config();
        let error = mapping_to(
            &mut Vec::new(),
            print::Pen::plain(),
            Format::Human,
            &config,
            Some("orders"),
        )
        .unwrap_err()
        .to_string();
        assert_eq!(
            error,
            "no index `orders` in the config; the indexes are: archive (disabled), products, users"
        );
    }

    /// A date `format` flusso appends `strict_date_optional_time` to gets a
    /// one-line note saying so and why; JSON carries the mapping alone, with
    /// the format as the index gets it.
    #[test]
    fn show_mapping_notes_a_date_format_flusso_completed() {
        let mut config = config();
        let products = config
            .indexes
            .get_mut(&config::IndexName::try_new("products").unwrap())
            .unwrap();
        products.schema.fields.push(
            serde_json::from_value(serde_json::json!({
                "field": "placed",
                "options": { "format": { "String": "yyyy-MM-dd HH:mm:ss" } },
                "source": { "column": {
                    "column": "placed_at", "ty": "timestamp", "nullable": true } },
            }))
            .unwrap(),
        );
        let human = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Human,
                &config,
                Some("products"),
            )
        });
        assert!(
            human.contains(
                "! products.placed: flusso appends `strict_date_optional_time` to its `format`, \
                 so the dates it writes index; dates still print in the `format`'s first pattern"
            ),
            "{human}"
        );
        let json = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Json,
                &config,
                Some("products"),
            )
        });
        assert!(
            json.contains("yyyy-MM-dd HH:mm:ss||strict_date_optional_time"),
            "{json}"
        );
        assert!(!json.contains("flusso appends"), "{json}");
        let users = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Human,
                &config,
                Some("users"),
            )
        });
        assert!(!users.contains("appends"), "{users}");
    }

    /// A disabled index is named as disabled, and its mapping still shows.
    #[test]
    fn show_mapping_of_a_disabled_index_says_it_is_disabled() {
        let config = config();
        let all = printed(|out| mapping_to(out, print::Pen::plain(), Format::Human, &config, None));
        assert!(
            !all.contains("archive"),
            "every index means every enabled one:\n{all}"
        );
        let human = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Human,
                &config,
                Some("archive"),
            )
        });
        assert!(
            human.contains(
                "index `archive` is disabled: `flusso run` builds no index for it until it is enabled"
            ),
            "{human}"
        );
        assert!(human.contains("Index  archive"), "{human}");
        let json = printed(|out| {
            mapping_to(
                out,
                print::Pen::plain(),
                Format::Json,
                &config,
                Some("archive"),
            )
        });
        let document: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(document[0]["index"], "archive", "{json}");
    }
}
