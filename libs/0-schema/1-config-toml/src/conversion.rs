use schema_core::{
    common::{ConnectionUrl, HttpUrl, SourceType},
    OpensearchSink, Sink, Source, StdoutSink,
};

use crate::entities;
use crate::ConversionError;

pub(crate) fn convert_source(source: entities::Source) -> Result<Source, ConversionError> {
    match source {
        entities::Source::Postgres(pg) => {
            let connection_url = match pg.connection_url {
                Some(entities::ConnectionUrl::Url(ev)) => {
                    ConnectionUrl::try_new(ev.resolve()?)?
                }
                Some(entities::ConnectionUrl::Parts {
                    host,
                    port,
                    user,
                    password,
                    database,
                }) => ConnectionUrl::from_parts()
                    .username(user)
                    .host(host)
                    .port(port)
                    .database(database)
                    .maybe_password(password.map(|p| p.resolve()).transpose()?)
                    .call()?,
                None => return Err(ConversionError::MissingConnectionUrl),
            };
            Ok(Source {
                source_type: SourceType::Postgres,
                connection_url,
            })
        }
    }
}

pub(crate) fn convert_sink(sink: entities::Sink) -> Result<Sink, ConversionError> {
    match sink {
        entities::Sink::Opensearch(s) => Ok(Sink::Opensearch(OpensearchSink {
            url: HttpUrl::try_new(s.url.resolve()?)?,
            username: s.username.map(|u| u.resolve()).transpose()?,
            password: s.password.map(|p| p.resolve()).transpose()?,
            tls_verify: s.tls_verify,
            batch_size: s.batch_size,
            max_bytes: s.max_bytes,
            timeout_secs: s.timeout_secs,
            max_retries: s.max_retries,
            pipeline: s.pipeline,
        })),
        entities::Sink::Stdout(s) => Ok(Sink::Stdout(StdoutSink { pretty: s.pretty })),
    }
}
