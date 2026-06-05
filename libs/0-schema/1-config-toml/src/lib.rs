//! Parse `flusso.toml` into the core [`Config`](schema_core::Config) model.
//!
//! A config file declares the Postgres source, the sinks documents are written
//! to, and the indexes to build. Parsing happens in two stages:
//!
//! 1. [`ConfigToml`] deserializes the file verbatim, rejecting unknown fields.
//! 2. `TryFrom<ConfigToml>` converts it into [`Config`](schema_core::Config),
//!    mapping each `{ env = "VAR" }` / literal into a deferred
//!    [`Secret`](schema_core::Secret).
//!
//! Secrets are **not** resolved here. Any string value may be given literally or
//! as `{ env = "VAR" }`; conversion carries that choice through unchanged, and
//! the value is read in the environment that runs the pipeline. That is what lets
//! a compiled config travel without baking in its secrets.
//!
//! The reserved deployment-override variables (`DATABASE_URL`,
//! `<SINK>_OPENSEARCH_URL` / `_USERNAME` / `_PASSWORD`) are likewise applied at
//! resolution time — see [`schema_core`]'s `resolve_*` functions — not here.
//!
//! The `index` entries are left untouched here — the conversion yields an empty
//! index map, which the `schema` crate's loader fills in by reading each
//! referenced YAML schema. This crate owns only the source and sinks.

mod conversion;
mod entities;
mod env_value;
mod parser;

pub use env_value::EnvOrValue;
pub use parser::ParseError;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::Infallible;

use entities::IndexEntry;
use entities::Sink;
use entities::Source;
use schema_core::common;

/// Conversion no longer fails: secrets are deferred (not resolved) and URLs are
/// validated at resolution time, so mapping the parsed config into the core
/// model is infallible. The alias keeps the loader's error plumbing stable.
pub type ConversionError = Infallible;

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
///
/// Infallible (secrets are deferred, URLs validated at resolution time), so this
/// is a `From`; the standard blanket impl still gives callers a
/// `TryFrom<ConfigToml>` whose error is [`ConversionError`] (`Infallible`).
impl From<ConfigToml> for schema_core::Config {
    fn from(toml: ConfigToml) -> Self {
        let source = conversion::convert_source(toml.source);
        let sinks = toml
            .sinks
            .into_iter()
            .map(|(name, sink)| (name, conversion::convert_sink(sink)))
            .collect();

        schema_core::Config {
            source,
            sinks,
            indexes: BTreeMap::new(),
        }
    }
}
