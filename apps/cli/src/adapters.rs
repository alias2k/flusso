//! The adapter registry: the one place in the workspace that names concrete
//! adapters.
//!
//! Every port entry in a [`Config`] is a `type` plus uninterpreted options.
//! This module maps each known `type` to its adapter's config struct
//! ([`kernel::AdapterConfig`]) and exposes three things built from that:
//!
//! - [`validate`]: every entry instantiates its adapter config, and every
//!   `[index.sinks.<name>]` table its sink's per-index config, without
//!   connecting. `build`, `check`, and `run` call it right after loading, so an
//!   unknown type, a misspelled option, or a wrong value fails before any
//!   network call or lock write. [`validate_draft`] is the same check over an
//!   unsaved `flusso.toml`, for the designer.
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
use kernel::{AdapterConfig, AdapterDescription, IndexName, Options, Port, PortEntry, SinkName};
use sink_opensearch::{OpensearchConfig, OpensearchIndexConfig};
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

/// Instantiate every port entry's adapter config, and every index's per-sink
/// config, without connecting.
pub(crate) fn validate(config: &Config) -> anyhow::Result<()> {
    validate_ports(config)?;
    for (index, entry) in &config.indexes {
        for (sink, options) in &entry.sinks {
            index_sink_config(config, index, sink, options)?;
        }
    }
    Ok(())
}

/// [`validate`] over a parsed but unloaded `flusso.toml`: the per-index sink
/// options come from its `[[index]]` entries, with no schema file read.
#[cfg(feature = "design")]
pub(crate) fn validate_draft(draft: &config::toml::ConfigToml) -> anyhow::Result<()> {
    let config = Config::from(draft.clone());
    validate_ports(&config)?;
    for entry in &draft.index {
        for (sink, options) in &entry.sinks {
            index_sink_config(&config, &entry.name, sink, options)?;
        }
    }
    Ok(())
}

fn validate_ports(config: &Config) -> anyhow::Result<()> {
    source_config(config)?;
    stream_config(config)?;
    for (name, entry) in &config.sinks {
        sink_config(name, entry)?;
    }
    Ok(())
}

/// One index's options for one sink, as that sink's adapter reads them.
#[derive(Debug, Clone)]
pub(crate) enum IndexSinkConfig {
    Opensearch(OpensearchIndexConfig),
}

/// The `[index.sinks.<sink>]` table of `index` as its sink's per-index config.
/// The sink must be declared and its adapter must take per-index options.
pub(crate) fn index_sink_config(
    config: &Config,
    index: &IndexName,
    sink: &SinkName,
    options: &Options,
) -> anyhow::Result<IndexSinkConfig> {
    let table = format!("[[index]] \"{index}\": [index.sinks.{sink}]");
    let Some(entry) = config.sinks.get(sink) else {
        let declared = config
            .sinks
            .keys()
            .map(|name| name.as_ref())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(anyhow!(
            "{table}: no sink is named \"{sink}\"; declared sinks: {declared}"
        ));
    };
    match entry.kind.as_str() {
        OpensearchConfig::KIND => options
            .clone()
            .deserialize_into()
            .map(IndexSinkConfig::Opensearch)
            .with_context(|| format!("{table} (type = \"{}\")", entry.kind)),
        other => Err(anyhow!(
            "{table}: the {other} sink takes no per-index options"
        )),
    }
}

/// How two sinks were found on one cluster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Compared {
    /// Their URLs name it, however spelled (`check --offline`).
    Url,
    /// Both clusters answered the same `cluster_uuid`.
    ClusterUuid,
}

/// What `check` finds of the OpenSearch sinks' clusters (the review of the
/// cut, m-3; of round 10, m-5 and m-6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClusterFinding {
    /// Two sinks write to one cluster: every index, alias and seed marker
    /// flusso owns there is named by the deployment's `prefix` alone, never
    /// by the sink, so both would share them, each taking the other's
    /// records for another process's. An error; `run` refuses it. Told by the
    /// `cluster_uuid` each sink's cluster answers (`GET /`) where the cluster
    /// can be asked (`run`, `check` online), so two URLs reaching one cluster
    /// (two nodes, a hostname and its IP) are found; by the URL alone under
    /// `check --offline`, which asks nothing and so misses them (the review
    /// of round 11, S-4).
    Shared {
        first: SinkName,
        second: SinkName,
        /// The URL both name, or the `cluster_uuid` both answer.
        cluster: String,
        compared: Compared,
    },
    /// A sink's URL can't be resolved here (an unset environment variable),
    /// so it was not compared with the other sinks: `check_skipped`.
    Skipped { sink: SinkName, why: String },
}

impl ClusterFinding {
    /// Whether the finding fails `check`.
    pub(crate) fn is_error(&self) -> bool {
        matches!(self, Self::Shared { .. })
    }

    /// The finding's kind, as the JSON report names it.
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Shared { .. } => "shared_cluster",
            Self::Skipped { .. } => "check_skipped",
        }
    }

    /// The sinks it is about.
    pub(crate) fn sinks(&self) -> Vec<&SinkName> {
        match self {
            Self::Shared { first, second, .. } => vec![first, second],
            Self::Skipped { sink, .. } => vec![sink],
        }
    }

    /// What was found, and what to do.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Shared {
                first,
                second,
                cluster,
                compared: Compared::Url,
            } => shared_message(first, second, &format!("both URLs name {cluster}")),
            Self::Shared {
                first,
                second,
                cluster,
                compared: Compared::ClusterUuid,
            } => shared_message(
                first,
                second,
                &format!("both answer cluster_uuid {cluster}"),
            ),
            Self::Skipped { sink, why } => format!(
                "[sinks.{sink}] was not compared with the other OpenSearch sinks for a shared \
                 cluster ({why}); `flusso run` checks it where it can reach the cluster"
            ),
        }
    }
}

/// The finding of `first` and `second` on one cluster, found as `how` says.
fn shared_message(first: &SinkName, second: &SinkName, how: &str) -> String {
    format!(
        "[sinks.{first}] and [sinks.{second}] write to the same OpenSearch cluster \
                 ({how}): every index, alias and seed marker flusso keeps there is named by \
                 the deployment's `prefix`, never by the sink, so the two would share them. Give \
                 each sink its own cluster, or run each in its own deployment with its own \
                 `prefix`"
    )
}

/// The OpenSearch sinks' clusters, compared: each pair on one cluster, and
/// each sink whose URL can't be resolved here. Nothing with fewer than two
/// OpenSearch sinks. A sink on another cluster (a backup, an audit copy) is
/// no finding.
pub(crate) fn cluster_findings(config: &Config) -> Vec<ClusterFinding> {
    let opensearch: Vec<(&SinkName, Box<OpensearchConfig>)> = config
        .sinks
        .iter()
        .filter_map(|(name, entry)| match sink_config(name, entry) {
            Ok(SinkConfig::Opensearch(sink)) => Some((name, sink)),
            _ => None,
        })
        .collect();
    if opensearch.len() < 2 {
        return Vec::new();
    }
    let mut findings = Vec::new();
    let mut seen: std::collections::BTreeMap<String, &SinkName> = std::collections::BTreeMap::new();
    for (name, sink) in &opensearch {
        let url = match sink.resolve_url(name) {
            Ok(url) => url,
            Err(error) => {
                findings.push(ClusterFinding::Skipped {
                    sink: (*name).clone(),
                    why: error.to_string(),
                });
                continue;
            }
        };
        let Some(cluster) = cluster_of(url.as_ref()) else {
            findings.push(ClusterFinding::Skipped {
                sink: (*name).clone(),
                why: "not a URL this check reads".to_owned(),
            });
            continue;
        };
        if let Some(first) = seen.insert(cluster.clone(), name) {
            findings.push(ClusterFinding::Shared {
                first: first.clone(),
                second: (*name).clone(),
                cluster,
                compared: Compared::Url,
            });
        }
    }
    findings
}

/// [`cluster_findings`], then each OpenSearch sink's cluster asked its
/// `cluster_uuid` (`GET /`): two answering the same are on one cluster,
/// however their URLs differ; one that can't be asked is not compared
/// (`check_skipped`, naming why). Online `check` and `run` (the review of
/// round 11, S-4).
pub(crate) async fn cluster_findings_online(config: &Config) -> Vec<ClusterFinding> {
    let mut findings = cluster_findings(config);
    let opensearch: Vec<(&SinkName, Box<OpensearchConfig>)> = config
        .sinks
        .iter()
        .filter_map(|(name, entry)| match sink_config(name, entry) {
            Ok(SinkConfig::Opensearch(sink)) => Some((name, sink)),
            _ => None,
        })
        .collect();
    if opensearch.len() < 2 {
        return findings;
    }
    let mut answers: Vec<(&SinkName, String)> = Vec::new();
    for (name, sink) in &opensearch {
        let skipped = |findings: &[ClusterFinding]| {
            findings.iter().any(
                |finding| matches!(finding, ClusterFinding::Skipped { sink, .. } if sink == *name),
            )
        };
        if skipped(&findings) {
            continue;
        }
        let asked = match sink_opensearch::OpensearchSink::from_config(name, sink) {
            Ok(built) => built
                .cluster_uuid()
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        match asked {
            Ok(uuid) => answers.push((name, uuid)),
            Err(why) => findings.push(ClusterFinding::Skipped {
                sink: (*name).clone(),
                why: format!("its cluster could not be asked its cluster_uuid: {why}"),
            }),
        }
    }
    let answered: Vec<(&SinkName, &str)> = answers
        .iter()
        .map(|(name, uuid)| (*name, uuid.as_str()))
        .collect();
    let by_uuid = shared_by_uuid(&answered, &findings);
    findings.extend(by_uuid);
    findings
}

/// The pairs of `answers` (each sink and the `cluster_uuid` its cluster
/// answered) on one cluster, but for those `found` names already.
pub(crate) fn shared_by_uuid(
    answers: &[(&SinkName, &str)],
    found: &[ClusterFinding],
) -> Vec<ClusterFinding> {
    let named = |a: &SinkName, b: &SinkName| {
        found.iter().any(|finding| {
            matches!(finding, ClusterFinding::Shared { first, second, .. }
                if (first == a && second == b) || (first == b && second == a))
        })
    };
    let mut seen: std::collections::BTreeMap<&str, &SinkName> = std::collections::BTreeMap::new();
    let mut shared = Vec::new();
    for (name, uuid) in answers {
        match seen.get(uuid) {
            Some(first) if !named(first, name) => shared.push(ClusterFinding::Shared {
                first: (*first).clone(),
                second: (*name).clone(),
                cluster: (*uuid).to_owned(),
                compared: Compared::ClusterUuid,
            }),
            Some(_) => {}
            None => {
                seen.insert(uuid, name);
            }
        }
    }
    shared
}

/// Refuse two OpenSearch sinks on one cluster ([`ClusterFinding::Shared`]),
/// as `run` does before anything starts, by URL and by the `cluster_uuid`
/// each cluster answers. A sink whose cluster can't be asked is not
/// refused, only logged: its own start fails on it as before (the review of
/// round 11, S-4).
pub(crate) async fn distinct_clusters_online(config: &Config) -> anyhow::Result<()> {
    let findings = cluster_findings_online(config).await;
    if let Some(shared) = findings.iter().find(|finding| finding.is_error()) {
        return Err(anyhow!(shared.message()));
    }
    for skipped in &findings {
        tracing::warn!("{}", skipped.message());
    }
    Ok(())
}

/// The cluster `url` names, however it is spelled: scheme and host in lower
/// case, the port with its default filled in, the path without a trailing
/// slash.
fn cluster_of(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    let port = url.port_or_known_default()?;
    let path = url.path().trim_end_matches('/');
    Some(format!("{}://{host}:{port}{path}", url.scheme()))
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
    Opensearch(Box<OpensearchConfig>),
    Stdout(StdoutConfig),
}

/// The sink entry `name` as its adapter's typed config.
pub(crate) fn sink_config(name: &SinkName, entry: &PortEntry) -> anyhow::Result<SinkConfig> {
    let table = format!("[sinks.{name}]");
    match entry.kind.as_str() {
        OpensearchConfig::KIND => OpensearchConfig::from_options(entry.options.clone())
            .map(|config| SinkConfig::Opensearch(Box::new(config)))
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
    /// `--manage-replica-identity` → `[source] manage_replica_identity` (Postgres).
    pub(crate) manage_replica_identity: Option<bool>,
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
        if let Some(manage) = overrides.manage_replica_identity {
            config
                .source
                .options
                .insert("manage_replica_identity", manage);
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
        assert_eq!(stream_config(&cfg).unwrap().capacity, None);
    }

    fn two_opensearch_sinks(primary: &str, audit: &str) -> Config {
        config(&format!(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "{primary}"

            [sinks.audit]
            type = "opensearch"
            url = "{audit}"
            "#
        ))
    }

    /// The review of the cut, m-3: two OpenSearch sinks on one cluster
    /// share every index flusso owns there; refused, naming both sinks,
    /// however the same URL is spelled.
    #[test]
    fn two_opensearch_sinks_on_one_cluster_are_refused() {
        for (primary, audit) in [
            ("https://search:9200", "https://search:9200"),
            ("https://search:9200/", "https://SEARCH:9200"),
            ("https://search", "https://search:443/"),
        ] {
            let error = cluster_findings(&two_opensearch_sinks(primary, audit))
                .iter()
                .find(|finding| finding.is_error())
                .map(ClusterFinding::message)
                .unwrap_or_default();
            assert!(
                error.contains("[sinks.audit]") && error.contains("[sinks.primary]"),
                "{primary} and {audit}: {error}"
            );
        }
        let one_and_stdout = config(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"

            [sinks.audit]
            type = "stdout"
            "#,
        );
        assert!(cluster_findings(&one_and_stdout).is_empty());
    }

    /// The URL comparison, which `check --offline` makes: URLs that differ
    /// pass it, whether they name two clusters (a backup, an audit copy: the
    /// owner's decision on the review of round 10) or two ways to one (two
    /// nodes, a proxy's paths), which only asking the cluster tells apart
    /// (the review of round 11, S-4).
    #[test]
    fn urls_that_differ_pass_the_url_comparison() {
        for (primary, audit) in [
            ("https://search:9200", "https://backup:9200"),
            ("https://search:9200", "https://search:9201"),
            ("http://search:9200", "https://search:9200"),
            ("https://proxy/a", "https://proxy/b"),
        ] {
            let config = two_opensearch_sinks(primary, audit);
            assert_eq!(cluster_findings(&config), [], "{primary} and {audit}");
        }
    }

    /// The review of round 11, S-4: sinks are on one cluster when they answer
    /// the same `cluster_uuid`, however their URLs differ; a sink on another
    /// cluster (a backup) is accepted. Each pair on one cluster is named once.
    #[test]
    fn sinks_answering_one_cluster_uuid_share_a_cluster() {
        let name = |name: &str| SinkName::try_new(name).unwrap();
        let (primary, audit, backup) = (name("primary"), name("audit"), name("backup"));
        let answers = [(&primary, "c1"), (&backup, "c2"), (&audit, "c1")];
        assert_eq!(
            shared_by_uuid(&answers, &[]),
            [ClusterFinding::Shared {
                first: primary.clone(),
                second: audit.clone(),
                cluster: "c1".to_owned(),
                compared: Compared::ClusterUuid,
            }]
        );
        let by_url = ClusterFinding::Shared {
            first: primary.clone(),
            second: audit.clone(),
            cluster: "https://search:9200".to_owned(),
            compared: Compared::Url,
        };
        assert_eq!(
            shared_by_uuid(&answers, std::slice::from_ref(&by_url)),
            [],
            "named once, by its URL"
        );
        assert_eq!(
            shared_by_uuid(&[(&primary, "c1"), (&backup, "c2")], &[]),
            []
        );
    }

    /// An online check asks each sink's cluster: one that can't be asked is
    /// `check_skipped`, naming why; `run` leaves it to the sink's own start.
    #[tokio::test]
    async fn a_cluster_that_cannot_be_asked_is_a_skipped_check() {
        let config = two_opensearch_sinks("http://127.0.0.1:1", "http://127.0.0.1:2");
        let findings = cluster_findings_online(&config).await;
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings.iter().all(|finding| matches!(finding,
                ClusterFinding::Skipped { why, .. } if why.contains("could not be asked"))),
            "{findings:?}"
        );
        distinct_clusters_online(&config).await.unwrap();
    }

    /// The review of round 10, m-5: a sink whose URL names an environment
    /// variable that isn't set is not compared, and says so; `run` leaves it
    /// to the sink's own start.
    #[test]
    fn a_sink_url_that_cannot_be_resolved_is_a_skipped_check() {
        let unresolved = config(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"

            [sinks.audit]
            type = "opensearch"
            url = { env = "FLUSSO_TEST_UNSET_AUDIT_URL" }
            "#,
        );
        let findings = cluster_findings(&unresolved);
        assert!(
            matches!(
                findings.as_slice(),
                [ClusterFinding::Skipped { sink, why }]
                    if sink.as_ref() == "audit" && why.contains("FLUSSO_TEST_UNSET_AUDIT_URL")
            ),
            "{findings:?}"
        );
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

    const INDEXED: &str = r#"
        [source]
        type = "postgres"

        [sinks.primary]
        type = "opensearch"
        url = "https://search:9200"

        [sinks.audit]
        type = "stdout"
    "#;

    fn with_index_sinks(sinks: &str) -> Config {
        use config::ParseFrom;
        let raw = format!(
            "{INDEXED}\n[[index]]\nname = \"orders\"\nschema = \"orders.schema.yml\"\nenabled = true\n{sinks}"
        );
        let mut parsed = config::toml::ConfigToml::try_parse(&raw).unwrap();
        let entry = parsed.index.remove(0);
        let mut cfg = Config::from(parsed);
        cfg.indexes.insert(
            entry.name,
            config::Index {
                enabled: true,
                schema: serde_json::from_value(serde_json::json!({
                    "version": 1, "table": "orders", "db_schema": "public", "fields": []
                }))
                .unwrap(),
                on_error: None,
                sinks: entry.sinks,
            },
        );
        cfg
    }

    #[test]
    fn validate_accepts_index_settings_for_an_opensearch_sink() {
        let cfg = with_index_sinks(
            "[index.sinks.primary]\nindex_settings = { \"mapping.total_fields.limit\" = 5000 }",
        );
        validate(&cfg).unwrap();
    }

    #[test]
    fn validate_rejects_per_index_options_for_an_undeclared_sink() {
        let cfg = with_index_sinks("[index.sinks.primry]\nindex_settings = { x = 1 }");
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("[index.sinks.primry]"), "{error}");
        assert!(error.contains("no sink is named \"primry\""), "{error}");
        assert!(error.contains("audit, primary"), "{error}");
    }

    #[test]
    fn validate_rejects_per_index_options_for_a_sink_that_takes_none() {
        let cfg = with_index_sinks("[index.sinks.audit]\npretty = true");
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(
            error.contains("stdout sink takes no per-index options"),
            "{error}"
        );
    }

    #[test]
    fn validate_rejects_an_unknown_per_index_option_and_an_owned_setting() {
        let cfg = with_index_sinks("[index.sinks.primary]\nindex_setings = { x = 1 }");
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("[[index]] \"orders\""), "{error}");
        assert!(error.contains("unknown field `index_setings`"), "{error}");
        let cfg = with_index_sinks(
            "[index.sinks.primary]\nindex_settings = { refresh_interval = \"1s\" }",
        );
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("owned by flusso"), "{error}");
    }

    #[test]
    fn validate_rejects_an_owned_sink_wide_setting() {
        let cfg = config(
            r#"
            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"
            index_settings = { "analysis.analyzer.x.type" = "standard" }
            "#,
        );
        let error = format!("{:#}", validate(&cfg).unwrap_err());
        assert!(error.contains("[sinks.primary]"), "{error}");
        assert!(error.contains("index.analysis"), "{error}");
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
                manage_replica_identity: Some(false),
                pretty: true,
                queue_capacity: Some(64),
            },
        );
        let postgres = source_config(&cfg).unwrap();
        assert_eq!(postgres.slot, "search");
        assert_eq!(postgres.publication, "flusso");
        assert!(!postgres.manage_publication);
        assert!(!postgres.manage_replica_identity);
        assert_eq!(stream_config(&cfg).unwrap().capacity, Some(64));
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
