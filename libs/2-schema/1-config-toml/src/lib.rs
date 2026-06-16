//! Parse `flusso.toml` into neutral [`ConfigToml`] entities.
//!
//! A config file declares the Postgres source, the sinks documents are written
//! to, and the indexes to build. This crate handles only the **parse** stage:
//! [`ConfigToml`] deserializes the file verbatim, rejecting unknown fields, into
//! entity types that mirror the file 1:1 and reference only the `schema-core`
//! vocabulary. Lifting these entities into the assembled `Config` is a
//! composition step that lives in the `schema` crate (`From<ConfigToml>`), so
//! this parser sits at the bottom layer and never depends on `Config`.
//!
//! Secrets are **not** resolved here. Any string value may be given literally or
//! as `{ env = "VAR" }`; the entities carry that choice through unchanged so the
//! value can be read in the environment that runs the pipeline.

pub mod entities;
mod env_value;
mod parser;

pub use env_value::EnvOrValue;
pub use parser::ParseError;

/// The JSON Schema describing a `flusso.toml` config file, embedded from the
/// repo's `schemas/` directory for editor assist and programmatic access. Kept
/// in lockstep with this parser by `schema`'s `schema_drift` test.
pub const CONFIG_SCHEMA: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../schemas/config.schema.json"
));

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
    pub source: Source,
    #[serde(default)]
    pub sinks: BTreeMap<common::SinkName, Sink>,
    #[serde(default)]
    pub index: Vec<IndexEntry>,
    /// Global item-level rejection policy; per-index overrides live on each
    /// [`IndexEntry`]. Defaults to [`FailurePolicy::Stop`](schema_core::FailurePolicy::Stop).
    #[serde(default)]
    pub on_error: schema_core::FailurePolicy,
    /// Bind addresses for the operational HTTP surfaces. The binary layers
    /// `FLUSSO_*` env vars and CLI flags on top (which win); see `CONFIG.md`.
    #[serde(default)]
    pub server: Server,
}
