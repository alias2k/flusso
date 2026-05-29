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
            .map(|(name, sink)| conversion::convert_sink(sink).map(|s| (name, s)))
            .collect::<Result<_, _>>()?;

        Ok(schema_core::Config {
            source,
            sinks,
            indexes: BTreeMap::new(),
        })
    }
}
