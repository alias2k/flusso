//! Parse `config.toml` into the core [`Config`](schema_core::Config) model.
//!
//! A config file declares the Postgres source, the sinks documents are written
//! to, and the indexes to build. Parsing happens in two stages:
//!
//! 1. [`ConfigToml`] deserializes the file verbatim, rejecting unknown fields.
//! 2. `TryFrom<ConfigToml>` converts it into [`Config`](schema_core::Config),
//!    resolving [`EnvOrValue`] secrets and validating connection and sink URLs.
//!
//! Any string value may be given literally or as `{ env = "VAR" }`, which reads
//! it from the environment at convert time and keeps credentials out of the file.
//!
//! On top of that, a set of **reserved environment variables** act as a
//! deployment override layer, so the same config file works across environments
//! without edits:
//!
//! - `DATABASE_URL` — the source connection URL.
//! - `<SINK>_OPENSEARCH_URL` / `_USERNAME` / `_PASSWORD` — per-OpenSearch-sink
//!   credentials, where `<SINK>` is the uppercased sink name (so `[sinks.primary]`
//!   reads `PRIMARY_OPENSEARCH_URL`, etc.).
//!
//! A reserved variable, when set, **wins over** a literal written in the file
//! (the override is logged, never silent) and **fills** an omitted value — but
//! an explicit `{ env = "X" }` reference names its own source and is never
//! overridden.
//!
//! The `index` entries are left untouched here — the conversion yields an empty
//! index map, which the `schema` crate's loader fills in by reading each
//! referenced YAML schema. This crate owns only the source and sinks.

mod conversion;
mod entities;
mod env_value;
mod parser;

pub use env_value::{EnvOrValue, EnvOrValueError};
pub use parser::ParseError;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use entities::IndexEntry;
use entities::Sink;
use entities::Source;
use schema_core::common;

#[derive(thiserror::Error, Debug)]
pub enum ConversionError {
    #[error(transparent)]
    EnvVar(#[from] EnvOrValueError),
    #[error("invalid connection URL: {0}")]
    ConnectionUrl(#[from] schema_core::ConnectionUrlError),
    #[error("invalid HTTP URL: {0}")]
    HttpUrl(#[from] schema_core::HttpUrlError),
    #[error("source has no connection_url")]
    MissingConnectionUrl,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigToml {
    pub source: Source,
    #[serde(default)]
    pub sinks: BTreeMap<common::SinkName, Sink>,
    #[serde(default)]
    pub index: Vec<IndexEntry>,
}

/// Converts source and sinks. `indexes` is left empty — the loader populates
/// it by loading each YAML file referenced in `ConfigToml.index`.
impl TryFrom<ConfigToml> for schema_core::Config {
    type Error = ConversionError;

    fn try_from(toml: ConfigToml) -> Result<Self, Self::Error> {
        let source = conversion::convert_source(toml.source)?;
        let sinks = toml
            .sinks
            .into_iter()
            .map(|(name, sink)| conversion::convert_sink(&name, sink).map(|s| (name, s)))
            .collect::<Result<_, _>>()?;

        Ok(schema_core::Config {
            source,
            sinks,
            indexes: BTreeMap::new(),
        })
    }
}
