//! The composition root's [`Backends`]: assembles the Postgres source, the
//! channel stream, the configured sinks, and the source-side helpers from a
//! [`Config`].
//!
//! This is where each port entry's options become a running adapter. The
//! adapter *kinds* are resolved in [`crate::adapters`]; this module builds the
//! instances, resolving connection URLs and credentials in the running
//! environment. The daemon and the engines never see an adapter's name.

use std::io::Write;
use std::sync::Arc;

use anyhow::{Context, ensure};
use async_trait::async_trait;
use config::Config;
use daemon::{Backends, DaemonOptions, SinkParts, SourceParts};
use kernel::{AdapterConfig, IndexName, SinkName};
use sink::{Sink, SinkOptions};
use sink_opensearch::{IndexSettings, OpensearchSink};
use sink_stdout::{StdoutConfig, StdoutSink};
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source::{CaptureProvisioning, Catalog, Diagnostic, DiagnosticKind, Severity, SourceSpec};
use source_postgres::{
    ColumnTypes, PgDocumentBuilder, PostgresConfig, WalChangeCapture, replication_config,
    sql_connection_url,
};
use stream::Stream;
use stream_channel::ChannelStream;

use script::{ScriptedBuilder, Scripts};

use crate::adapters::{self, IndexSinkConfig, SinkConfig};
use crate::commands::print;

/// The one [`Backends`] implementation: the composition root's choice of
/// concrete adapters, looked up by each port entry's `type`.
#[derive(Debug, Default)]
pub(crate) struct FlussoBackends;

#[async_trait]
impl Backends for FlussoBackends {
    fn validate(&self, config: &Config) -> anyhow::Result<()> {
        adapters::validate(config)?;
        compile_scripts(config)?;
        Ok(())
    }

    async fn source(
        &self,
        config: Arc<Config>,
        _options: &DaemonOptions,
    ) -> anyhow::Result<SourceParts> {
        let postgres = adapters::source_config(&config)?;
        let connection_url = resolve_connection_url(&postgres)?;
        let tls = postgres.tls();
        let replication =
            replication_config(&connection_url, &tls, &postgres.slot, &postgres.publication)
                .context("building the replication connection config")?;
        let sql_url = sql_connection_url(&connection_url, &tls)
            .context("applying the source TLS settings to the connection URL")?;

        let spec = source_spec(&config);
        let (documents, column_types) = build_documents(&sql_url, &config).await?;
        let capture: Arc<dyn ChangeCapture> = Arc::new(
            WalChangeCapture::new(replication, sql_url.clone())
                .with_publication_management(spec.all_tables(), postgres.manage_publication)
                .with_pre_image_management(spec.pre_images(), postgres.manage_replica_identity)
                .with_column_types(column_types),
        );

        Ok(SourceParts { capture, documents })
    }

    fn stream(&self, config: &Config, sinks: &[SinkName]) -> anyhow::Result<Arc<dyn Stream>> {
        let channel = adapters::stream_config(config)?;
        Ok(Arc::new(ChannelStream::from_config(
            &channel,
            sinks.iter().cloned(),
        )))
    }

    async fn sinks(&self, config: &Config) -> anyhow::Result<Vec<SinkParts>> {
        build_sinks(config)
    }
}

/// A source capture used only for its read-only provisioning surface
/// (`flusso check`): the slot name is irrelevant there.
pub(crate) fn build_provisioning(
    config: &Config,
    publication: &str,
) -> anyhow::Result<Arc<dyn CaptureProvisioning>> {
    let postgres = adapters::source_config(config)?;
    let connection_url = resolve_connection_url(&postgres)?;
    let tls = postgres.tls();
    let replication = replication_config(&connection_url, &tls, "flusso", publication)
        .context("building the replication connection config")?;
    let sql_url = sql_connection_url(&connection_url, &tls)
        .context("applying the source TLS settings to the connection URL")?;
    let capture = WalChangeCapture::new(replication, sql_url);
    Ok(Arc::new(capture))
}

/// The resolved SQL connection URL (TLS settings applied) for the source, as
/// `check` and the document builder need it.
pub(crate) fn source_sql_url(config: &Config) -> anyhow::Result<(PostgresConfig, String)> {
    let postgres = adapters::source_config(config)?;
    let connection_url = resolve_connection_url(&postgres)?;
    let sql_url = sql_connection_url(&connection_url, &postgres.tls())
        .context("applying the source TLS settings to the connection URL")?;
    Ok((postgres, sql_url))
}

fn resolve_connection_url(postgres: &PostgresConfig) -> anyhow::Result<String> {
    let url = postgres
        .resolve_connection_url()
        .context("resolving the source connection URL")?;
    Ok(url.as_ref().to_owned())
}

/// The document builder, scripts applied, beside its column types, which the
/// capture drops a table's entries from as the table changes.
async fn build_documents(
    connection_url: &str,
    config: &Config,
) -> anyhow::Result<(Arc<dyn DocumentBuilder>, ColumnTypes)> {
    let scripts = compile_scripts(config)?;
    let spec = Arc::new(source_spec(config));
    let builder = PgDocumentBuilder::connect(connection_url, Arc::clone(&spec))
        .await
        .context("connecting to Postgres")?;
    validate_at_start(&spec, &builder, config).await?;
    let column_types = builder.column_types();
    Ok((
        ScriptedBuilder::wrap(Arc::new(builder), scripts),
        column_types,
    ))
}

/// Check the schemas against the database once, before anything is staged,
/// with `flusso check`'s check: print the diagnostics to stderr and refuse to
/// start on one that cannot work as written (see [`stops_the_start`]). Runs on
/// the document builder's own connection, so an unreachable database fails
/// exactly as the connect above does.
async fn validate_at_start(
    spec: &SourceSpec,
    catalog: &dyn Catalog,
    config: &Config,
) -> anyhow::Result<()> {
    let diagnostics = source::validate_indexes(spec, catalog)
        .await
        .context("validating the schemas against the database")?;
    let mut refused = Vec::new();
    for diagnostic in &diagnostics {
        refused.push(match refused_only_by_a_mapping(diagnostic.kind) {
            Some(dropped) => a_sink_refuses(config, &diagnostic.index, dropped)?,
            None => true,
        });
    }
    start_verdict(
        &mut std::io::stderr().lock(),
        print::Pen::detect_stderr(),
        &diagnostics,
        &|at| refused.get(at).copied().unwrap_or(true),
    )
}

/// For a kind whose documents only the sink's mapping refuses, whether
/// `index.mapping.ignore_malformed` lets them in (the value is dropped);
/// `None` for every other kind.
fn refused_only_by_a_mapping(kind: DiagnosticKind) -> Option<bool> {
    match kind {
        DiagnosticKind::DuplicateFieldNameRefused => Some(false),
        DiagnosticKind::TimeOfDayAsDate => Some(true),
        _ => None,
    }
}

/// Whether some sink of the deployment provably refuses what a mapping
/// refuses on `index`: an OpenSearch sink in a state flusso models in full
/// ([`plain_opensearch`]). A stdout sink takes anything, and a sink in any
/// other state may accept the value or never see it, so neither counts.
fn a_sink_refuses(config: &Config, index: &IndexName, dropped: bool) -> anyhow::Result<bool> {
    for (name, entry) in &config.sinks {
        let SinkConfig::Opensearch(sink) = adapters::sink_config(name, entry)? else {
            continue;
        };
        let mut settings = sink.index_settings.clone();
        if let Some((_, own)) = opensearch_index_settings(config, name)?
            .into_iter()
            .find(|(of, _)| of == index)
        {
            settings = settings.layered(&own);
        }
        if plain_opensearch(&sink, &settings, dropped)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Index settings known to leave a document's values as the mapping reads
/// them: shard counts, and limits that can only refuse more.
const PLAIN_SETTINGS: &[&str] = &[
    "index.number_of_shards",
    "index.number_of_replicas",
    "index.max_result_window",
    "index.mapping.total_fields.limit",
    "index.mapping.nested_fields.limit",
    "index.mapping.nested_objects.limit",
    "index.mapping.depth.limit",
    "index.mapping.field_name_length.limit",
];

/// Whether an OpenSearch sink writes `index` (with the merged `settings`) the
/// plain way: no ingest pipeline, and only settings known to leave the value
/// alone. `index.mapping.ignore_malformed` is plain when off, and when on
/// only for a refusal it doesn't lift (`dropped` false). This is an
/// allow-list: an option or setting not modelled here means no stop.
fn plain_opensearch(
    sink: &sink_opensearch::OpensearchConfig,
    settings: &IndexSettings,
    dropped: bool,
) -> anyhow::Result<bool> {
    // Every option is named, so a new one fails to compile until it is
    // decided here. Only text/keyword leaves read `text_analysis` and
    // `auto_subfields`; the rest concern transport, batching and refresh.
    let sink_opensearch::OpensearchConfig {
        url: _,
        username: _,
        password: _,
        tls_verify: _,
        ca_file: _,
        batch_size: _,
        max_bytes: _,
        timeout_secs: _,
        max_retries: _,
        number_of_shards: _,
        number_of_replicas: _,
        refresh_interval: _,
        text_analysis: _,
        auto_subfields: _,
        index_settings: _,
        pipeline,
    } = sink;
    if pipeline.is_some() {
        return Ok(false);
    }
    let serde_json::Value::Object(entries) =
        serde_json::to_value(settings).context("reading the sink's index settings")?
    else {
        return Ok(false);
    };
    Ok(entries.iter().all(|(key, value)| {
        if key == "index.mapping.ignore_malformed" {
            let on = value == &serde_json::json!(true) || value == &serde_json::json!("true");
            return !(on && dropped);
        }
        PLAIN_SETTINGS.contains(&key.as_str())
    }))
}

/// Print `diagnostics` (nothing when there are none) and fail when any
/// [stops the start](stops_the_start). Every other diagnostic, error or
/// warning, is printed and the start goes on. `refused(i)` says whether a sink
/// refuses what `diagnostics[i]` describes, for a kind only a sink's mapping
/// refuses (see [`a_sink_refuses`]).
fn start_verdict(
    out: &mut impl Write,
    pen: print::Pen,
    diagnostics: &[Diagnostic],
    refused: &dyn Fn(usize) -> bool,
) -> anyhow::Result<()> {
    if diagnostics.is_empty() {
        return Ok(());
    }
    print::diagnostics(out, pen, diagnostics)?;
    writeln!(out)?;
    let blocking = diagnostics
        .iter()
        .enumerate()
        .filter(|(at, d)| stops_the_start(d.kind) && refused(*at))
        .count();
    ensure!(
        blocking == 0,
        "the schemas cannot be built against this database ({blocking} problem(s) above); \
         fix them, confirm with `flusso check`, then start again"
    );
    if diagnostics.iter().any(|d| d.severity == Severity::Error) {
        writeln!(
            out,
            "starting anyway: the errors above do not stop `flusso run` (`flusso check` still fails on them)\n"
        )?;
    }
    Ok(())
}

/// Whether a diagnostic of `kind` means the documents cannot be built as
/// written, so no deployment in that state works: a table or column that does
/// not exist, a `soft_delete` marker that reads no column (it would be dropped,
/// and deleted rows stay indexed), a relation table without a single-column
/// primary key, `transforms` the column's type does not take (every build of
/// the index fails on either), a root `primary_key` other than the table's
/// own key (every document is written as a tombstone), `array: true` over a
/// column that holds no array (a key new in 0.18, so no running deployment has
/// it), a link column Postgres can't compare with its key (every build of
/// the index fails to plan), two namesakes that fail every build or that
/// OpenSearch refuses in every document, or a time of day under a `date`
/// mapping, which OpenSearch refuses whenever it is not null. The last two
/// stop only when an OpenSearch sink writes the index as written
/// ([`a_sink_refuses`]).
/// A type, numeric or nullability disagreement still builds, as it did before
/// `run` validated, so it only prints. So does an `ids` `element_type` that
/// holds no key, or only some: a document whose `ids` array is empty, or
/// holds only keys that fit, indexes as before. So does a root table without a
/// single-column primary key: its live changes are still indexed, so refusing
/// would stop a deployment that runs.
fn stops_the_start(kind: DiagnosticKind) -> bool {
    match kind {
        DiagnosticKind::UnknownTable
        | DiagnosticKind::UnknownColumn
        | DiagnosticKind::UnreadSoftDeleteMarker
        | DiagnosticKind::RelationPrimaryKey
        | DiagnosticKind::TransformNotApplicable
        | DiagnosticKind::RootPrimaryKeyMismatch
        | DiagnosticKind::UnsupportedKeyType
        | DiagnosticKind::NotAnArrayColumn
        | DiagnosticKind::LinkTypeMismatch
        | DiagnosticKind::DuplicateFieldNameFailsToBuild
        | DiagnosticKind::DuplicateFieldNameRefused
        | DiagnosticKind::TimeOfDayAsDate => true,
        DiagnosticKind::TypeMismatch
        | DiagnosticKind::NotNumeric
        | DiagnosticKind::Nullability
        | DiagnosticKind::IgnoredPrimaryKey
        | DiagnosticKind::RootPrimaryKey
        | DiagnosticKind::UndeclaredArrayColumn
        | DiagnosticKind::IdsElementMismatch
        | DiagnosticKind::IdsElementLossy
        | DiagnosticKind::DuplicateFieldName
        | DiagnosticKind::TimestampFormat => false,
    }
}

/// Compile every script field of the enabled indexes, reporting all that fail.
pub(crate) fn compile_scripts(config: &Config) -> anyhow::Result<Scripts> {
    Scripts::compile(&source_spec(config)).map_err(|errors| anyhow::anyhow!("{errors}"))
}

/// The enabled indexes and their schemas: the source-facing subset of a
/// [`Config`].
pub(crate) fn source_spec(config: &Config) -> SourceSpec {
    let indexes = config
        .indexes
        .iter()
        .filter(|(_, index)| index.enabled)
        .map(|(name, index)| (name.clone(), index.schema.clone()))
        .collect();
    SourceSpec::new(indexes)
}

/// The enabled indexes' own `index_settings` for the OpenSearch sink `sink`.
fn opensearch_index_settings(
    config: &Config,
    sink: &SinkName,
) -> anyhow::Result<Vec<(IndexName, IndexSettings)>> {
    let mut settings = Vec::new();
    for (index, entry) in config.indexes.iter().filter(|(_, entry)| entry.enabled) {
        if let Some(options) = entry.sinks.get(sink) {
            let IndexSinkConfig::Opensearch(own) =
                adapters::index_sink_config(config, index, sink, options)?;
            settings.push((index.clone(), own.index_settings));
        }
    }
    Ok(settings)
}

/// Every configured sink as its own engine's parts; with no sink configured,
/// a plain stdout sink so a first run shows its documents.
fn build_sinks(config: &Config) -> anyhow::Result<Vec<SinkParts>> {
    let mut sinks = Vec::with_capacity(config.sinks.len());
    for (name, entry) in &config.sinks {
        let sink: Arc<dyn Sink> = match adapters::sink_config(name, entry)? {
            SinkConfig::Opensearch(os) => Arc::new(
                OpensearchSink::from_config(name, &os)
                    .with_context(|| format!("building OpenSearch sink '{name}'"))?
                    .with_index_prefix(&config.prefix)
                    .with_index_settings(opensearch_index_settings(config, name)?),
            ),
            SinkConfig::Stdout(s) => Arc::new(StdoutSink::from_config(name, &s)),
        };
        sinks.push(SinkParts {
            name: name.clone(),
            sink,
            options: SinkOptions {
                backfill: entry.backfill,
            },
        });
    }
    if sinks.is_empty() {
        let name = SinkName::try_new(StdoutConfig::KIND).context("naming the default sink")?;
        sinks.push(SinkParts {
            sink: Arc::new(StdoutSink::from_config(&name, &StdoutConfig::default())),
            name,
            options: SinkOptions::default(),
        });
    }
    Ok(sinks)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use config::ParseFrom;
    use config::toml::ConfigToml;

    use super::*;

    fn config_with(indexes: &[(&str, bool, i64)]) -> Config {
        let mut raw = String::from(
            r#"
            prefix = "dev_"

            [source]
            type = "postgres"

            [sinks.primary]
            type = "opensearch"
            url = "https://search:9200"
            "#,
        );
        for (name, enabled, limit) in indexes {
            raw.push_str(&format!(
                "\n[[index]]\nname = \"{name}\"\nschema = \"{name}.schema.yml\"\nenabled = {enabled}\n\
                 [index.sinks.primary]\nindex_settings = {{ \"mapping.total_fields.limit\" = {limit} }}\n"
            ));
        }
        let mut draft = ConfigToml::try_parse(&raw).unwrap();
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

    fn diagnostic(kind: DiagnosticKind) -> Diagnostic {
        Diagnostic::new(
            IndexName::try_new("products").unwrap(),
            kernel::FieldName::try_new("title").unwrap(),
            source::Anchor::Field,
            kind,
            format!("a {kind:?} finding"),
        )
    }

    fn verdict(diagnostics: &[Diagnostic]) -> (anyhow::Result<()>, String) {
        let mut out = Vec::new();
        let result = start_verdict(&mut out, print::Pen::detect_stderr(), diagnostics, &|_| {
            true
        });
        (result, String::from_utf8(out).unwrap())
    }

    #[test]
    fn a_clean_validation_starts_and_prints_nothing() {
        let (result, printed) = verdict(&[]);
        assert!(result.is_ok());
        assert_eq!(printed, "");
    }

    fn refuses(kind: DiagnosticKind) {
        let (result, printed) = verdict(&[diagnostic(kind)]);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("1 problem(s)"), "{kind:?}: {error}");
        assert!(
            printed.contains(&format!("a {kind:?} finding")),
            "{printed}"
        );
    }

    fn starts(kind: DiagnosticKind) -> String {
        let (result, printed) = verdict(&[diagnostic(kind)]);
        assert!(result.is_ok(), "{kind:?} should not stop the start");
        assert!(
            printed.contains(&format!("a {kind:?} finding")),
            "{printed}"
        );
        printed
    }

    #[test]
    fn an_unknown_table_refuses_the_start() {
        refuses(DiagnosticKind::UnknownTable);
    }

    #[test]
    fn an_unknown_column_refuses_the_start() {
        refuses(DiagnosticKind::UnknownColumn);
    }

    #[test]
    fn an_unread_soft_delete_marker_refuses_the_start() {
        refuses(DiagnosticKind::UnreadSoftDeleteMarker);
    }

    #[test]
    fn a_relation_table_without_a_primary_key_refuses_the_start() {
        refuses(DiagnosticKind::RelationPrimaryKey);
    }

    #[test]
    fn a_transform_the_column_does_not_take_refuses_the_start() {
        refuses(DiagnosticKind::TransformNotApplicable);
    }

    #[test]
    fn a_root_primary_key_other_than_the_tables_refuses_the_start() {
        refuses(DiagnosticKind::RootPrimaryKeyMismatch);
    }

    #[test]
    fn a_root_key_of_a_type_the_source_cannot_read_back_refuses_the_start() {
        refuses(DiagnosticKind::UnsupportedKeyType);
    }

    #[test]
    fn an_array_flag_over_a_column_without_one_refuses_the_start() {
        refuses(DiagnosticKind::NotAnArrayColumn);
    }

    #[test]
    fn a_link_its_key_cannot_be_compared_with_refuses_the_start() {
        refuses(DiagnosticKind::LinkTypeMismatch);
    }

    #[test]
    fn an_ids_element_type_that_holds_no_key_prints_and_starts() {
        let printed = starts(DiagnosticKind::IdsElementMismatch);
        assert!(printed.contains("[error]"), "{printed}");
        assert!(printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn an_ids_element_type_that_holds_only_some_keys_warns_and_starts() {
        let printed = starts(DiagnosticKind::IdsElementLossy);
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(!printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn an_undeclared_array_column_warns_and_starts() {
        let printed = starts(DiagnosticKind::UndeclaredArrayColumn);
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(!printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_root_table_without_a_single_column_primary_key_prints_and_starts() {
        let printed = starts(DiagnosticKind::RootPrimaryKey);
        assert!(printed.contains("[error]"), "{printed}");
        assert!(printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_type_mismatch_error_prints_and_starts() {
        let printed = starts(DiagnosticKind::TypeMismatch);
        assert!(printed.contains("[error]"), "{printed}");
        assert!(printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_not_numeric_error_prints_and_starts() {
        let printed = starts(DiagnosticKind::NotNumeric);
        assert!(printed.contains("[error]"), "{printed}");
        assert!(printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_nullability_warning_prints_and_starts() {
        let printed = starts(DiagnosticKind::Nullability);
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(!printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn an_ignored_join_primary_key_prints_and_starts() {
        let printed = starts(DiagnosticKind::IgnoredPrimaryKey);
        assert!(printed.contains("[warning]"), "{printed}");
    }

    #[test]
    fn a_duplicate_field_name_prints_and_starts() {
        let printed = starts(DiagnosticKind::DuplicateFieldName);
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(!printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_timestamp_format_prints_and_starts() {
        let printed = starts(DiagnosticKind::TimestampFormat);
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(!printed.contains("starting anyway"), "{printed}");
    }

    #[test]
    fn a_duplicate_that_never_builds_refuses_the_start() {
        refuses(DiagnosticKind::DuplicateFieldNameFailsToBuild);
    }

    #[test]
    fn a_duplicate_opensearch_refuses_in_every_document_refuses_the_start() {
        refuses(DiagnosticKind::DuplicateFieldNameRefused);
    }

    #[test]
    fn a_time_of_day_under_a_date_field_refuses_the_start() {
        refuses(DiagnosticKind::TimeOfDayAsDate);
    }

    #[test]
    fn a_finding_no_sink_refuses_prints_and_starts() {
        let diagnostics = [diagnostic(DiagnosticKind::TimeOfDayAsDate)];
        let mut out = Vec::new();
        let result = start_verdict(&mut out, print::Pen::detect_stderr(), &diagnostics, &|_| {
            false
        });
        assert!(result.is_ok());
        let printed = String::from_utf8(out).unwrap();
        assert!(printed.contains("starting anyway"), "{printed}");
    }

    /// A deployment with `sinks` and the index `products`, whose own sink
    /// tables are `index_sinks`.
    fn deployment(sinks: &str, index_sinks: &str) -> Config {
        let raw = format!("[source]\ntype = \"postgres\"\n{sinks}");
        let mut config = Config::from(ConfigToml::try_parse(&raw).unwrap());
        let entry = format!(
            "[[index]]\nname = \"products\"\nschema = \"products.schema.yml\"\nenabled = true\n{index_sinks}"
        );
        let mut draft =
            ConfigToml::try_parse(&format!("[source]\ntype = \"postgres\"\n{entry}")).unwrap();
        for entry in std::mem::take(&mut draft.index) {
            let schema = serde_json::from_value(serde_json::json!({
                "version": 1, "table": "products", "db_schema": "public", "fields": []
            }))
            .unwrap();
            config.indexes.insert(
                entry.name,
                config::Index {
                    enabled: true,
                    schema,
                    on_error: None,
                    sinks: entry.sinks,
                },
            );
        }
        config
    }

    #[test]
    fn only_an_opensearch_sink_that_maps_the_value_as_written_refuses_it() {
        let products = IndexName::try_new("products").unwrap();
        let opensearch = "[sinks.primary]\ntype = \"opensearch\"\nurl = \"http://search:9200\"\n";
        let refuses =
            |config: &Config, dropped: bool| a_sink_refuses(config, &products, dropped).unwrap();

        let plain = deployment(opensearch, "");
        assert!(refuses(&plain, false) && refuses(&plain, true));

        let stdout = deployment("[sinks.out]\ntype = \"stdout\"\n", "");
        assert!(!refuses(&stdout, false) && !refuses(&stdout, true));
        let none = deployment("", "");
        assert!(!refuses(&none, false) && !refuses(&none, true));

        let pipeline = deployment(&format!("{opensearch}pipeline = \"fix\"\n"), "");
        assert!(!refuses(&pipeline, false) && !refuses(&pipeline, true));

        let lenient = deployment(
            &format!("{opensearch}index_settings = {{ \"mapping.ignore_malformed\" = true }}\n"),
            "",
        );
        assert!(refuses(&lenient, false) && !refuses(&lenient, true));

        let lenient_here = deployment(
            opensearch,
            "[index.sinks.primary]\nindex_settings = { mapping = { ignore_malformed = true } }\n",
        );
        assert!(refuses(&lenient_here, false) && !refuses(&lenient_here, true));

        let one_strict = deployment(
            &format!(
                "{opensearch}index_settings = {{ \"mapping.ignore_malformed\" = true }}\n\
                 [sinks.audit]\ntype = \"opensearch\"\nurl = \"http://audit:9200\"\n"
            ),
            "",
        );
        assert!(refuses(&one_strict, true));
    }

    /// A refusal stops the start only on a sink flusso can prove plain: any
    /// index setting it doesn't model (an ingest pipeline set as an index
    /// setting, `coerce`, anything new) means no stop; the settings it knows
    /// leave the value alone (shard counts, field limits) keep the stop.
    #[test]
    fn a_setting_flusso_does_not_model_means_no_stop() {
        let products = IndexName::try_new("products").unwrap();
        let opensearch = "[sinks.primary]\ntype = \"opensearch\"\nurl = \"http://search:9200\"\n";
        let sink_wide = |settings: &str| {
            deployment(
                &format!("{opensearch}index_settings = {{ {settings} }}\n"),
                "",
            )
        };
        let here = |settings: &str| {
            deployment(
                opensearch,
                &format!("[index.sinks.primary]\nindex_settings = {{ {settings} }}\n"),
            )
        };
        for config in [
            sink_wide("default_pipeline = \"drop-v\""),
            sink_wide("\"index.default_pipeline\" = \"drop-v\""),
            sink_wide("final_pipeline = \"drop-v\""),
            here("default_pipeline = \"drop-v\""),
            here("index = { final_pipeline = \"drop-v\" }"),
            here("mapping = { coerce = false }"),
            here("\"query.default_field\" = \"v\""),
        ] {
            assert!(!a_sink_refuses(&config, &products, false).unwrap());
            assert!(!a_sink_refuses(&config, &products, true).unwrap());
        }
        for config in [
            sink_wide("number_of_shards = 2, \"mapping.total_fields.limit\" = 5000"),
            here("max_result_window = 20000, mapping = { nested_fields = { limit = 100 } }"),
            here("\"mapping.ignore_malformed\" = false"),
        ] {
            assert!(a_sink_refuses(&config, &products, false).unwrap());
            assert!(a_sink_refuses(&config, &products, true).unwrap());
        }
        let other_index = here("default_pipeline = \"drop-v\"");
        let users = IndexName::try_new("users").unwrap();
        assert!(a_sink_refuses(&other_index, &users, true).unwrap());
    }

    #[test]
    fn one_refusing_kind_among_others_refuses_and_prints_them_all() {
        let (result, printed) = verdict(&[
            diagnostic(DiagnosticKind::Nullability),
            diagnostic(DiagnosticKind::TypeMismatch),
            diagnostic(DiagnosticKind::UnknownColumn),
        ]);
        let error = result.unwrap_err().to_string();
        assert!(error.contains("1 problem(s)"), "{error}");
        assert!(printed.contains("[warning]"), "{printed}");
        assert!(printed.contains("a TypeMismatch finding"), "{printed}");
        assert!(printed.contains("a UnknownColumn finding"), "{printed}");
    }

    /// `places(label mood, email email_address)`: an enum type and a domain,
    /// which the catalog reports by their own names.
    struct EnumAndDomain;

    #[async_trait]
    impl Catalog for EnumAndDomain {
        async fn column(
            &self,
            _: &kernel::DatabaseSchema,
            _: &kernel::TableName,
            column: &kernel::ColumnName,
        ) -> source::Result<source::ColumnInfo> {
            let sql_type = if column.as_ref() == "label" {
                "mood"
            } else {
                "email_address"
            };
            Ok(source::ColumnInfo {
                sql_type: sql_type.to_owned(),
                base_type: None,
                nullable: true,
            })
        }

        async fn has_table(
            &self,
            _: &kernel::DatabaseSchema,
            _: &kernel::TableName,
        ) -> source::Result<bool> {
            Ok(true)
        }
    }

    /// The motivating false positive: a `keyword`/`enum` field over a Postgres
    /// enum or domain column is a type error for `flusso check` (no declared
    /// type accepts the type's own name), but the document builds, so `run`
    /// prints it and starts.
    #[test]
    fn a_keyword_or_enum_over_an_enum_or_domain_column_does_not_stop_the_start() {
        let column = |name: &str, ty: &str| {
            serde_json::json!({ "field": name, "source": { "column": {
                "column": name, "ty": ty, "nullable": true,
            } } })
        };
        let schema: kernel::IndexSchema = serde_json::from_value(serde_json::json!({
            "version": 1, "table": "places", "db_schema": "public",
            "fields": [column("label", "enum"), column("email", "keyword")],
        }))
        .unwrap();
        let spec = SourceSpec::new(std::collections::BTreeMap::from([(
            IndexName::try_new("places").unwrap(),
            schema,
        )]));
        let diagnostics =
            futures::executor::block_on(source::validate_indexes(&spec, &EnumAndDomain)).unwrap();
        assert_eq!(
            diagnostics
                .iter()
                .map(|d| (d.kind, d.severity))
                .collect::<Vec<_>>(),
            vec![
                (DiagnosticKind::TypeMismatch, Severity::Error),
                (DiagnosticKind::TypeMismatch, Severity::Error),
            ]
        );
        let (result, printed) = verdict(&diagnostics);
        assert!(result.is_ok(), "{printed}");
        assert!(printed.contains("`mood`"), "{printed}");
        assert!(printed.contains("`email_address`"), "{printed}");
    }

    #[test]
    fn index_settings_skip_disabled_indexes_and_key_by_logical_name() {
        let config = config_with(&[("orders", true, 5000), ("users", false, 9000)]);
        let primary = SinkName::try_new("primary").unwrap();

        let settings = opensearch_index_settings(&config, &primary).unwrap();

        let [(index, orders)] = settings.as_slice() else {
            panic!("expected only the enabled index, got {settings:?}");
        };
        assert_eq!(index.as_ref(), "orders");
        assert_eq!(
            orders.get("index.mapping.total_fields.limit"),
            Some(&serde_json::json!(5000))
        );
    }
}
