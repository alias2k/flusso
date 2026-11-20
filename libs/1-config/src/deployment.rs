//! The assembled deployment configuration — a **composition** concern.
//!
//! [`Config`] names one source, one stream, the sinks, and the indexes to
//! build. Each port is a [`PortEntry`]: the adapter kind plus its options,
//! **uninterpreted** here. The adapters depend only on the kernel vocabulary
//! and never on this crate, and this crate names no adapter: the composition
//! root (the CLI) looks each kind up in its adapter registry and hands the
//! options to that adapter's config type, which validates them strictly.
//!
//! These types were lifted out of `kernel` for exactly that reason —
//! keeping the cross-cutting vocabulary at the bottom layer while the
//! composition lives next to the daemon.

mod conversion;
mod projection;

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::num::{NonZeroU64, NonZeroUsize};

use kernel::{FailurePolicy, IndexSchema, Options, PortEntry, common};
use serde::{Deserialize, Serialize};

use crate::toml::SinkEntry;

/// The stream adapter used when `[stream]` is omitted: the in-process channel.
pub const DEFAULT_STREAM_KIND: &str = "channel";

/// A whole deployment: where data comes from, where it goes, and what to build.
///
/// Secrets are deferred (a literal or an environment reference, see
/// [`Secret`](kernel::Secret)), so a serialized `Config` carries only the
/// literals it was given and resolves the rest at runtime. Debug output redacts
/// literal secrets either way.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The source port entry (`[source]`): its adapter kind and options.
    pub source: PortEntry,
    /// The stream port entry (`[stream]`); [`DEFAULT_STREAM_KIND`] with no
    /// options when the file omits it.
    #[serde(default = "default_stream")]
    pub stream: PortEntry,
    /// The sink entries (`[sinks.<name>]`), by name: each adapter's port entry
    /// plus the universal sink keys. Empty means the composition root's default
    /// sink (stdout).
    #[serde(default)]
    pub sinks: BTreeMap<common::SinkName, SinkEntry>,
    #[serde(default)]
    pub indexes: BTreeMap<common::IndexName, Index>,
    /// What to do when a sink rejects a document at the item level. The default
    /// for every index; override per index with [`Index::on_error`].
    #[serde(default)]
    pub on_error: FailurePolicy,
    /// How the ingest engine groups live changes into a batch (`[batch]`). An
    /// unset field keeps the engine's default; the binary layers the
    /// `--batch-*` flags / `FLUSSO_BATCH_*` env vars on top (which win).
    #[serde(default, skip_serializing_if = "BatchConfig::is_empty")]
    pub batch: BatchConfig,
    /// How much the pipeline does at once (`[concurrency]`). Never written
    /// into the lock, and read back from one as unset: it belongs to the
    /// machine that runs flusso, which sets it with `--writes` /
    /// `FLUSSO_WRITES` and `--builds` / `FLUSSO_BUILDS` (which win over the
    /// file).
    #[serde(skip)]
    pub concurrency: ConcurrencyConfig,
    /// How long flusso waits before trying again (`[backoff]`). Never written
    /// into the lock, and read back from one as unset: it belongs to the
    /// machine that runs flusso, which sets it with `--backoff-initial-ms` /
    /// `FLUSSO_BACKOFF_INITIAL_MS` and `--backoff-max-ms` /
    /// `FLUSSO_BACKOFF_MAX_MS` (which win over the file).
    #[serde(skip)]
    pub backoff: BackoffConfig,
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
    /// [`validate_index_prefix`](kernel::validate_index_prefix). The
    /// `flusso-query` client must apply the same prefix at runtime to read back.
    #[serde(default)]
    pub prefix: String,
}

fn default_stream() -> PortEntry {
    PortEntry::new(DEFAULT_STREAM_KIND)
}

/// Bind addresses for the two operational HTTP surfaces, as configured in
/// `flusso.toml`'s `[server]` table. Parsed and validated at config-read time
/// (see the toml module's `BindAddress`), so these are real socket
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

/// The live-change batching knobs from `flusso.toml`'s `[batch]` table. Every
/// field is optional so the engine's defaults have one home (its
/// `BatchPolicy`) and an unset key serializes to nothing — a lock compiled
/// from a config without `[batch]` is byte-identical to before the table
/// existed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchConfig {
    /// Commit a batch once this many changes have accumulated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_changes: Option<NonZeroUsize>,
    /// The cap on how long a batch stays open while changes keep arriving, in
    /// milliseconds; also the backfill request-coalescing window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_delay_ms: Option<u64>,
}

impl BatchConfig {
    /// True when no key is set.
    pub fn is_empty(&self) -> bool {
        self.max_changes.is_none() && self.max_delay_ms.is_none()
    }
}

/// The `[concurrency]` knobs from `flusso.toml`. An unset key keeps the
/// daemon's default.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ConcurrencyConfig {
    /// Lane items each sink writes at once.
    pub writes: Option<NonZeroUsize>,
    /// Document builds the ingest engine runs at once.
    pub builds: Option<NonZeroUsize>,
    /// MiB of built documents held before the ingest engine waits.
    pub max_built_mib: Option<NonZeroUsize>,
    /// The share of `max_built_mib` one index's live documents may hold with two or more
    /// indexes, with where it was set (the file, then `--index-budget-share`
    /// / `FLUSSO_INDEX_BUDGET_SHARE` laid over it by the binary).
    pub index_budget_share: Option<ShareValue>,
}

/// The `[backoff]` knobs, in milliseconds, each with where it was set: the
/// file, then `--backoff-*` / `FLUSSO_BACKOFF_*` laid over it by the binary.
/// An unset key keeps the daemon's default. The daemon resolves and checks
/// the pair (`daemon::backoff`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BackoffConfig {
    /// The first wait.
    pub initial_ms: Option<BackoffValue>,
    /// The longest wait.
    pub max_ms: Option<BackoffValue>,
}

/// One `[backoff]` value and where it was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackoffValue {
    /// Milliseconds.
    pub ms: NonZeroU64,
    /// Where it was set.
    pub origin: Origin,
}

impl BackoffValue {
    /// `ms`, set in `flusso.toml`.
    pub fn from_file(ms: NonZeroU64) -> Self {
        Self {
            ms,
            origin: Origin::File,
        }
    }
}

/// Where a setting of the machine that runs flusso (`[backoff]`,
/// `[concurrency] index_budget_share`) was set, so a refusal can say how to
/// change it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// `flusso.toml`.
    File,
    /// The command line.
    Flag,
    /// The environment.
    Env,
}

/// `[concurrency] index_budget_share` and where it was set. Taken as it is
/// written: the daemon's judge (`daemon::index_budget_share`) refuses a value
/// outside (0, 1], naming this origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareValue {
    /// The fraction of `max_built_mib` one index's live documents may hold.
    pub share: f64,
    /// Where it was set.
    pub origin: Origin,
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
    /// Per-sink options for this index, keyed by sink name, uninterpreted
    /// here: the composition root validates each against its sink's adapter.
    /// Operational like `on_error`, so outside [`IndexSchema`] and the hash.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sinks: BTreeMap<common::SinkName, Options>,
}
