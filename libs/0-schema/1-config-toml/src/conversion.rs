use schema_core::{
    OpensearchSink, Sink, Source, StdoutSink, TextAnalysis,
    common::{ConnectionUrl, HttpUrl, SinkName, SourceType},
};

use crate::entities;
use crate::env_value::{resolve_optional, resolve_required};
use crate::{ConversionError, EnvOrValue};

/// Reserved environment variable that supplies / overrides the source
/// connection URL. The source is a singleton, so a single well-known name (the
/// 12-factor convention) is unambiguous.
const SOURCE_URL_VAR: &str = "DATABASE_URL";

pub(crate) fn convert_source(source: entities::Source) -> Result<Source, ConversionError> {
    match source {
        entities::Source::Postgres(pg) => Ok(Source {
            source_type: SourceType::Postgres,
            connection_url: resolve_connection_url(pg.connection_url)?,
        }),
    }
}

/// Resolve the source connection URL, with `DATABASE_URL` as the deployment
/// override. Precedence, highest first:
///
/// 1. An explicit `connection_url = { env = "X" }` names its own source and
///    wins — `DATABASE_URL` is not consulted.
/// 2. `DATABASE_URL`, if set — overriding a literal URL or the assembled
///    connection parts (logged), or filling an omitted `connection_url`.
/// 3. The config's literal URL or connection parts.
fn resolve_connection_url(
    config: Option<entities::ConnectionUrl>,
) -> Result<ConnectionUrl, ConversionError> {
    // An explicit `{ env = "X" }` reference wins and is not overridden.
    if let Some(entities::ConnectionUrl::Url(env @ EnvOrValue::Env { .. })) = config {
        return Ok(ConnectionUrl::try_new(env.resolve()?)?);
    }

    // Otherwise `DATABASE_URL` overrides a configured value or fills an omitted
    // one. A configured value being overridden is logged, never silent.
    if let Ok(url) = std::env::var(SOURCE_URL_VAR) {
        if config.is_some() {
            tracing::warn!(
                var = %SOURCE_URL_VAR,
                "environment variable overrides source connection_url set in config",
            );
        }
        return Ok(ConnectionUrl::try_new(url)?);
    }

    match config {
        Some(entities::ConnectionUrl::Url(ev)) => Ok(ConnectionUrl::try_new(ev.resolve()?)?),
        Some(entities::ConnectionUrl::Parts {
            host,
            port,
            user,
            password,
            database,
        }) => Ok(ConnectionUrl::from_parts()
            .username(user)
            .host(host)
            .port(port)
            .database(database)
            .maybe_password(password.map(|p| p.resolve()).transpose()?)
            .call()?),
        None => Err(ConversionError::MissingConnectionUrl),
    }
}

pub(crate) fn convert_sink(name: &SinkName, sink: entities::Sink) -> Result<Sink, ConversionError> {
    match sink {
        entities::Sink::Opensearch(s) => {
            // Per-sink override vars, namespaced by the sink's (uppercased) name
            // so several OpenSearch sinks never collide: `<NAME>_OPENSEARCH_URL`,
            // `<NAME>_OPENSEARCH_USERNAME`, `<NAME>_OPENSEARCH_PASSWORD`.
            let prefix = name.to_string().to_uppercase();
            let url = resolve_required(s.url, &format!("{prefix}_OPENSEARCH_URL"))?;
            let username = resolve_optional(s.username, &format!("{prefix}_OPENSEARCH_USERNAME"))?;
            let password = resolve_optional(s.password, &format!("{prefix}_OPENSEARCH_PASSWORD"))?;
            Ok(Sink::Opensearch(OpensearchSink {
                url: HttpUrl::try_new(url)?,
                username,
                password,
                tls_verify: s.tls_verify,
                batch_size: s.batch_size,
                max_bytes: s.max_bytes,
                timeout_secs: s.timeout_secs,
                max_retries: s.max_retries,
                pipeline: s.pipeline,
                number_of_shards: s.number_of_shards,
                number_of_replicas: s.number_of_replicas,
                text_analysis: convert_text_analysis(s.text_analysis),
                auto_subfields: s.auto_subfields,
            }))
        }
        entities::Sink::Stdout(s) => Ok(Sink::Stdout(StdoutSink { pretty: s.pretty })),
    }
}

fn convert_text_analysis(value: entities::TextAnalysis) -> TextAnalysis {
    match value {
        entities::TextAnalysis::Builtin => TextAnalysis::Builtin,
        entities::TextAnalysis::Icu => TextAnalysis::Icu,
    }
}
