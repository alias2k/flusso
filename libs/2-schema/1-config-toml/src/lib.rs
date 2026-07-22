#![doc = include_str!("../README.md")]

pub mod entities;
mod env_value;
mod parser;

pub use env_value::EnvOrValue;
pub use parser::ParseError;

/// The JSON Schema describing a `flusso.toml` config file, embedded from this
/// crate's `schemas/` directory for editor assist and programmatic access (both
/// re-exported from `schema` and emitted by `flusso schema config`). Kept in
/// lockstep with this parser by `schema`'s `schema_drift` test.
pub const CONFIG_SCHEMA: &str = include_str!("../config.schema.json");

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use entities::IndexEntry;
use entities::Server;
use entities::Sink;
use entities::Source;
use schema_core::common;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigToml {
    // Field order is the `flusso.toml` serialization order: the `toml` writer
    // floats the scalar globals (prefix, on_error) to the top, then emits tables
    // in declaration order — source, server, sinks, indexes.
    pub source: Source,
    /// Literal prefix prepended to every index name flusso owns, so several
    /// deployments can share one OpenSearch cluster without colliding. The
    /// `--index-prefix` flag / `FLUSSO_INDEX_PREFIX` env var override it at
    /// runtime (which win); see the [configuration
    /// guide](https://alias2k.github.io/flusso/guides/configuration.html). Empty
    /// (the default) means no prefix.
    #[serde(default)]
    pub prefix: String,
    /// Global item-level rejection policy; per-index overrides live on each
    /// [`IndexEntry`]. Defaults to [`FailurePolicy::Stop`](schema_core::FailurePolicy::Stop).
    #[serde(default)]
    pub on_error: schema_core::FailurePolicy,
    /// Bind addresses for the operational HTTP surfaces. The binary layers
    /// `FLUSSO_*` env vars and CLI flags on top (which win); see the
    /// [configuration guide](https://alias2k.github.io/flusso/guides/configuration.html).
    /// Omitted from serialized output when no address is set.
    #[serde(default, skip_serializing_if = "Server::is_empty")]
    pub server: Server,
    #[serde(default)]
    pub sinks: BTreeMap<common::SinkName, Sink>,
    #[serde(default)]
    pub index: Vec<IndexEntry>,
}
