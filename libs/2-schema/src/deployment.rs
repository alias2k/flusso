//! The assembled deployment configuration — a **composition** concern.
//!
//! [`Config`] names a source, the sinks, and the indexes to build. It sits
//! *above* the backend crates: sources and sinks depend only on the
//! `schema-core` vocabulary (`IndexSchema`, `IndexMapping`, the newtypes), never
//! on this crate, so a backend can't reach the top-level config. The composition
//! root (CLI/daemon) translates `Config` into the backend-facing subsets each
//! side needs (a source spec, the per-sink configs).
//!
//! These types were lifted out of `schema-core` for exactly that reason —
//! keeping the cross-cutting vocabulary at the bottom layer while the
//! composition lives next to the daemon.

mod conversion;
mod projection;
mod sink;
mod source;

pub use sink::Sink;
pub use source::Source;

use std::collections::BTreeMap;
use std::net::SocketAddr;

use schema_core::{FailurePolicy, IndexSchema, common};
use serde::{Deserialize, Serialize};

/// A whole deployment: where data comes from, where it goes, and what to build.
///
/// Secrets are deferred (a literal or an environment reference, see
/// [`Secret`](schema_core::Secret)), so a serialized `Config` carries only the
/// literals it was given and resolves the rest at runtime. Debug output redacts
/// literal secrets either way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub source: Source,
    pub sinks: BTreeMap<common::SinkName, Sink>,
    pub indexes: BTreeMap<common::IndexName, Index>,
    /// What to do when a sink rejects a document at the item level. The default
    /// for every index; override per index with [`Index::on_error`].
    #[serde(default)]
    pub on_error: FailurePolicy,
    /// Bind addresses for the operational HTTP surfaces. Read by the binary, not
    /// the daemon — transport is the binary's concern. Env/flag overrides win;
    /// see [`ServerConfig`].
    #[serde(default)]
    pub server: ServerConfig,
    /// Literal prefix prepended to every index name flusso owns (indexes,
    /// aliases, and the `flusso_meta` index), so several deployments can share
    /// one OpenSearch cluster without colliding. Empty (the default) means no
    /// prefix. The binary layers the `--index-prefix` flag / `FLUSSO_INDEX_PREFIX`
    /// env var on top (which win); validated at resolution time with
    /// [`validate_index_prefix`](schema_core::validate_index_prefix). The
    /// `flusso-query` client must apply the same prefix at runtime to read back.
    #[serde(default)]
    pub prefix: String,
}

/// Bind addresses for the two operational HTTP surfaces, as configured in
/// `flusso.toml`'s `[server]` table. Parsed and validated at config-read time
/// (see `schema_config_toml`'s `BindAddress`), so these are real socket
/// addresses by the time they reach the binary, which layers `FLUSSO_*` env vars
/// and CLI flags on top (which win).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Public, read-only surface (`/healthz`, `/readyz`, `/status`, `/metrics`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<SocketAddr>,
    /// Private, Basic-auth control surface (`/indexes`, `/reindex`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_address: Option<SocketAddr>,
}

/// One index in a [`Config`], paired with whether it is built on this run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub enabled: bool,
    pub schema: IndexSchema,
    /// Per-index override of [`Config::on_error`]. `None` inherits the global
    /// policy. Lives here (not in [`IndexSchema`]) on purpose: it's operational,
    /// not part of the document shape, so changing it does not alter the index
    /// mapping hash or trigger a reindex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<FailurePolicy>,
}
