//! The adapter registry: the one place in the workspace that names concrete
//! adapters.
//!
//! Every port entry in a [`Config`] is a `type` plus uninterpreted options.
//! This module maps each known `type` to its adapter's config struct
//! ([`kernel::AdapterConfig`]) and exposes three things built from that:
//!
//! - [`validate`]: every entry instantiates its adapter config, without
//!   connecting. `build`, `check`, `run`, and the designer call it right after
//!   loading, so an unknown type, a misspelled option, or a wrong value fails
//!   before any network call or lock write.
//! - the typed configs the composition root needs to build adapters
//!   ([`source_config`], [`stream_config`], [`sink_config`]).
//! - [`descriptions`]: what each adapter declares about its options, from which
//!   the editor schema, the Reference tables, and the designer forms are
//!   rendered.
//!
//! Adding an adapter is its crate plus one arm in each `match` here.

use std::sync::LazyLock;

use anyhow::{Context, anyhow};
use config::Config;
use kernel::{AdapterConfig, AdapterDescription, Port, PortEntry, SinkName};
use sink_opensearch::OpensearchConfig;
use sink_stdout::StdoutConfig;
use source_postgres::PostgresConfig;
use stream_channel::ChannelConfig;

/// The registered adapters' descriptions, in port order then by kind.
pub(crate) fn descriptions() -> &'static [AdapterDescription] {
    static ALL: LazyLock<Vec<AdapterDescription>> = LazyLock::new(|| {
        vec![
            PostgresConfig::description(),
            ChannelConfig::description(),
            OpensearchConfig::description(),
            StdoutConfig::description(),
        ]
    });
    &ALL
}

/// The description of one registered adapter, if `kind` is known for `port`.
pub(crate) fn description(port: Port, kind: &str) -> Option<&'static AdapterDescription> {
    descriptions()
        .iter()
        .find(|d| d.port == port && d.kind == kind)
}

/// The kinds registered for `port`, for error messages.
fn known_kinds(port: Port) -> String {
    descriptions()
        .iter()
        .filter(|d| d.port == port)
        .map(|d| d.kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Instantiate every port entry's adapter config without connecting.
pub(crate) fn validate(config: &Config) -> anyhow::Result<()> {
    source_config(config)?;
    stream_config(config)?;
    for (name, entry) in &config.sinks {
        sink_config(name, entry)?;
    }
    Ok(())
}

/// The source entry as its adapter's typed config.
pub(crate) fn source_config(config: &Config) -> anyhow::Result<PostgresConfig> {
    let entry = &config.source;
    match entry.kind.as_str() {
        PostgresConfig::KIND => PostgresConfig::from_options(entry.options.clone())
            .with_context(|| format!("[source] (type = \"{}\")", entry.kind)),
        other => Err(unknown_kind(Port::Source, other, "[source]")),
    }
}

/// The stream entry as its adapter's typed config.
pub(crate) fn stream_config(config: &Config) -> anyhow::Result<ChannelConfig> {
    let entry = &config.stream;
    match entry.kind.as_str() {
        ChannelConfig::KIND => ChannelConfig::from_options(entry.options.clone())
            .with_context(|| format!("[stream] (type = \"{}\")", entry.kind)),
        other => Err(unknown_kind(Port::Stream, other, "[stream]")),
    }
}

/// One sink entry as its adapter's typed config.
#[derive(Debug, Clone)]
pub(crate) enum SinkConfig {
    Opensearch(OpensearchConfig),
    Stdout(StdoutConfig),
}

/// The sink entry `name` as its adapter's typed config.
pub(crate) fn sink_config(name: &SinkName, entry: &PortEntry) -> anyhow::Result<SinkConfig> {
    let table = format!("[sinks.{name}]");
    match entry.kind.as_str() {
        OpensearchConfig::KIND => OpensearchConfig::from_options(entry.options.clone())
            .map(SinkConfig::Opensearch)
            .with_context(|| format!("{table} (type = \"{}\")", entry.kind)),
        StdoutConfig::KIND => StdoutConfig::from_options(entry.options.clone())
            .map(SinkConfig::Stdout)
            .with_context(|| format!("{table} (type = \"{}\")", entry.kind)),
        other => Err(unknown_kind(Port::Sink, other, &table)),
    }
}

/// Flags and `FLUSSO_*` variables that override one adapter's options: laid
/// over the file's entries before validation, so flag > env > file.
#[derive(Debug, Default, Clone)]
pub(crate) struct Overrides {
    /// `--slot` → `[source] slot` (Postgres).
    pub(crate) slot: Option<String>,
    /// `--publication` → `[source] publication` (Postgres).
    pub(crate) publication: Option<String>,
    /// `--manage-publication` → `[source] manage_publication` (Postgres).
    pub(crate) manage_publication: Option<bool>,
    /// `--pretty` → `pretty = true` on every stdout sink (and the default one).
    pub(crate) pretty: bool,
    /// `--queue-capacity` → `[stream] capacity` (channel).
    pub(crate) queue_capacity: Option<usize>,
}

/// Apply [`Overrides`] to `config`. A deployment with no sink gets the default
/// stdout sink here, named `stdout`, so every run has at least one sink entry.
pub(crate) fn apply_overrides(config: &mut Config, overrides: &Overrides) {
    if config.source.kind == PostgresConfig::KIND {
        if let Some(slot) = &overrides.slot {
            config.source.options.insert("slot", slot.clone());
        }
        if let Some(publication) = &overrides.publication {
            config
                .source
                .options
                .insert("publication", publication.clone());
        }
        if let Some(manage) = overrides.manage_publication {
            config.source.options.insert("manage_publication", manage);
        }
    }
    if config.stream.kind == ChannelConfig::KIND
        && let Some(capacity) = overrides.queue_capacity
    {
        config.stream.options.insert("capacity", capacity);
    }
    if config.sinks.is_empty()
        && let Ok(name) = SinkName::try_new(StdoutConfig::KIND)
    {
        config
            .sinks
            .insert(name, config::SinkEntry::new(StdoutConfig::KIND));
    }
    if overrides.pretty {
        for entry in config.sinks.values_mut() {
            if entry.kind == StdoutConfig::KIND {
                entry.options.insert("pretty", true);
            }
        }
    }
}

fn unknown_kind(port: Port, kind: &str, table: &str) -> anyhow::Error {
    anyhow!(
        "{table}: unknown {port} type \"{kind}\"; known types: {}",
        known_kinds(port)
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn config(toml: &str) -> Config {
        use config::ParseFrom;
        Config::from(config::toml::ConfigToml::try_parse(toml).unwrap())
    }

    #[test]
    fn validate_accepts_every_registered_kind_with_defaults() {
        let cfg = config(
            r#"
            [source]
            type = "postgres"

            [stream]
            type = "channel"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"

            [sinks.audit]
            type = "stdout"
            "#,
        );
        validate(&cfg).unwrap();
        assert_eq!(source_config(&cfg).unwrap().slot, "flusso");
        assert_eq!(stream_config(&cfg).unwrap().capacity, 1024);
    }

    #[test]
    fn validate_names_the_entry_and_the_bad_option() {
        let cfg = config(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"
            batch_sizee = 5
            "#,
        );
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("[sinks.primary]"), "{error}");
        assert!(error.contains("unknown field `batch_sizee`"), "{error}");
    }

    #[test]
    fn validate_rejects_an_unknown_kind_and_lists_the_known_ones() {
        let cfg = config(
            r#"
            [source]
            type = "postgres"

            [sinks.bus]
            type = "kafka"
            "#,
        );
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("unknown sink type \"kafka\""), "{error}");
        assert!(error.contains("opensearch, stdout"), "{error}");
        let cfg = config("[source]\ntype = \"mysql\"\n");
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("unknown source type \"mysql\""), "{error}");
        assert!(error.contains("postgres"), "{error}");
    }

    #[test]
    fn overrides_land_on_their_adapter_and_default_the_sink() {
        let mut cfg = config("[source]\ntype = \"postgres\"\n");
        apply_overrides(
            &mut cfg,
            &Overrides {
                slot: Some("search".into()),
                publication: None,
                manage_publication: Some(false),
                pretty: true,
                queue_capacity: Some(64),
            },
        );
        let postgres = source_config(&cfg).unwrap();
        assert_eq!(postgres.slot, "search");
        assert_eq!(postgres.publication, "flusso");
        assert!(!postgres.manage_publication);
        assert_eq!(stream_config(&cfg).unwrap().capacity, 64);
        let (name, entry) = cfg.sinks.iter().next().unwrap();
        assert_eq!(name.as_ref(), "stdout");
        match sink_config(name, entry).unwrap() {
            SinkConfig::Stdout(stdout) => assert!(stdout.pretty),
            other => panic!("expected the default stdout sink, got {other:?}"),
        }
    }

    #[test]
    fn descriptions_cover_every_port() {
        let ports: Vec<Port> = descriptions().iter().map(|d| d.port).collect();
        assert!(ports.contains(&Port::Source));
        assert!(ports.contains(&Port::Stream));
        assert!(ports.contains(&Port::Sink));
        assert!(description(Port::Sink, "opensearch").is_some());
        assert!(description(Port::Sink, "postgres").is_none());
    }
}
