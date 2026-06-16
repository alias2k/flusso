//! Lifting the parsed `flusso.toml` ([`ConfigToml`]) into the assembled
//! [`Config`].
//!
//! The toml parser ([`schema_config_toml`]) produces neutral entity types that
//! mirror the file; turning those into a `Config` is a composition step, so it
//! lives here next to `Config` rather than in the parser. Secrets are **not**
//! resolved here — a `{ env = "VAR" }` / literal becomes a deferred
//! [`Secret`], read in the environment that runs the
//! pipeline. The `index` entries are left empty; the loader fills them in by
//! reading each referenced YAML schema.

use std::collections::BTreeMap;

use schema_config_toml::{ConfigToml, EnvOrValue, entities};
use schema_core::{
    ConnectionSpec, OpensearchSink, Secret, StdoutSink, TextAnalysis, common::SourceType,
};

use super::{Config, ServerConfig, Sink, Source};

/// Infallible (secrets are deferred, URLs validated at resolution time), so this
/// is a `From`; the blanket impl still gives callers a `TryFrom<ConfigToml>`.
impl From<ConfigToml> for Config {
    fn from(toml: ConfigToml) -> Self {
        let source = convert_source(toml.source);
        let sinks = toml
            .sinks
            .into_iter()
            .map(|(name, sink)| (name, convert_sink(sink)))
            .collect();

        Config {
            source,
            sinks,
            indexes: BTreeMap::new(),
            on_error: toml.on_error,
            server: ServerConfig {
                public_address: toml.server.public_address,
                private_address: toml.server.private_address,
            },
        }
    }
}

fn convert_source(source: entities::Source) -> Source {
    match source {
        entities::Source::Postgres(pg) => Source {
            source_type: SourceType::Postgres,
            connection: pg.connection_url.map(convert_connection_spec),
        },
    }
}

/// Map a parsed connection form into the deferred core [`ConnectionSpec`].
/// Nothing is resolved here — `{ env = "X" }` becomes a [`Secret::Env`] and a
/// literal a [`Secret::Value`], read in the environment that runs the pipeline.
fn convert_connection_spec(url: entities::ConnectionUrl) -> ConnectionSpec {
    match url {
        entities::ConnectionUrl::Url(ev) => ConnectionSpec::Url(to_secret(ev)),
        entities::ConnectionUrl::Parts {
            host,
            port,
            user,
            password,
            database,
        } => ConnectionSpec::Parts {
            host,
            port,
            user,
            password: password.map(to_secret),
            database,
        },
    }
}

fn convert_sink(sink: entities::Sink) -> Sink {
    match sink {
        entities::Sink::Opensearch(s) => Sink::Opensearch(OpensearchSink {
            url: to_secret(s.url),
            username: s.username.map(to_secret),
            password: s.password.map(to_secret),
            tls_verify: s.tls_verify,
            batch_size: s.batch_size,
            max_bytes: s.max_bytes,
            timeout_secs: s.timeout_secs,
            max_retries: s.max_retries,
            pipeline: s.pipeline,
            number_of_shards: s.number_of_shards,
            number_of_replicas: s.number_of_replicas,
            refresh_interval: s.refresh_interval,
            text_analysis: convert_text_analysis(s.text_analysis),
            auto_subfields: s.auto_subfields,
        }),
        entities::Sink::Stdout(s) => Sink::Stdout(StdoutSink { pretty: s.pretty }),
    }
}

/// A parsed `{ env = "X" }` / literal becomes a deferred [`Secret`].
fn to_secret(value: EnvOrValue) -> Secret {
    match value {
        EnvOrValue::Env { env } => Secret::Env(env),
        EnvOrValue::Value(v) => Secret::Value(v),
    }
}

fn convert_text_analysis(value: entities::TextAnalysis) -> TextAnalysis {
    match value {
        entities::TextAnalysis::Builtin => TextAnalysis::Builtin,
        entities::TextAnalysis::Icu => TextAnalysis::Icu,
    }
}
