//! The composition root's [`Backends`]: assembles the Postgres source, the
//! channel stream, the configured sinks, and the source-side helpers from a
//! [`Config`].
//!
//! This is where each port entry's options become a running adapter. The
//! adapter *kinds* are resolved in [`crate::adapters`]; this module builds the
//! instances, resolving connection URLs and credentials in the running
//! environment. The daemon and the engines never see an adapter's name.

use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use config::Config;
use daemon::{Backends, DaemonOptions, SinkParts, SourceParts};
use kernel::{AdapterConfig, IndexName, SinkName};
use sink::{Sink, SinkOptions};
use sink_opensearch::{IndexSettings, OpensearchSink};
use sink_stdout::{StdoutConfig, StdoutSink};
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source::{CaptureProvisioning, SourceSpec};
use source_postgres::{
    PgDocumentBuilder, PostgresConfig, WalChangeCapture, replication_config, sql_connection_url,
};
use stream::Stream;
use stream_channel::ChannelStream;

use crate::adapters::{self, IndexSinkConfig, SinkConfig};

/// The one [`Backends`] implementation: the composition root's choice of
/// concrete adapters, looked up by each port entry's `type`.
#[derive(Debug, Default)]
pub(crate) struct FlussoBackends;

#[async_trait]
impl Backends for FlussoBackends {
    fn validate(&self, config: &Config) -> anyhow::Result<()> {
        adapters::validate(config)
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
        let capture: Arc<dyn ChangeCapture> = Arc::new(
            WalChangeCapture::new(replication, sql_url.clone())
                .with_publication_management(spec.all_tables(), postgres.manage_publication)
                .with_pre_image_management(spec.pre_images(), postgres.manage_replica_identity),
        );
        let documents = build_documents(&sql_url, &config).await?;

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

async fn build_documents(
    connection_url: &str,
    config: &Config,
) -> anyhow::Result<Arc<dyn DocumentBuilder>> {
    let spec = Arc::new(source_spec(config));
    let builder = PgDocumentBuilder::connect(connection_url, spec)
        .await
        .context("connecting to Postgres")?;
    Ok(Arc::new(builder))
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
