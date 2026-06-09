use schema_core::{
    ConnectionSpec, OpensearchSink, Secret, Sink, Source, StdoutSink, TextAnalysis,
    common::SourceType,
};

use crate::EnvOrValue;
use crate::entities;

pub(crate) fn convert_source(source: entities::Source) -> Source {
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

pub(crate) fn convert_sink(sink: entities::Sink) -> Sink {
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
