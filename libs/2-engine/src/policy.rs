//! Configuration vocabulary for a run: how changes are grouped into a flush
//! ([`BatchPolicy`]) and how item-level sink rejections are handled
//! ([`FailurePolicies`] over [`FailurePolicy`]).

use std::collections::HashMap;
use std::time::Duration;

pub use kernel::FailurePolicy;

/// How the ingest engine groups live changes into one batch.
///
/// A batch takes every change the stream has ready and is committed the
/// moment the stream would block: a lone change on a quiet stream is built at
/// once, and a burst — which keeps the stream ready — fills batches to
/// `max_changes`, trading nothing in latency for far fewer round-trips.
/// `max_changes: 1` reproduces flush-per-change.
///
/// Acks respect the batch boundary — see the [module docs](crate). The source
/// ack for a change is confirmed only after the flush that made its documents
/// durable, so at-least-once delivery holds regardless of batch size.
///
/// `flusso.toml`'s `[batch]` table sets both fields; an omitted field keeps
/// the default here.
#[derive(Debug, Clone, Copy)]
pub struct BatchPolicy {
    /// Commit once this many changes have accumulated. Clamped to at least 1.
    pub max_changes: usize,
    /// The cap on how long a batch stays open after its first change while
    /// changes keep arriving. Also the window the ingest engine holds a
    /// backfill snapshot for straggling requests so a reindex fanned to every
    /// sink is one pass over the table.
    pub max_delay: Duration,
}

impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            max_changes: 256,
            max_delay: Duration::from_millis(50),
        }
    }
}

/// How the engine resolves the [`FailurePolicy`] for a rejected document: a
/// global `default` plus per-index overrides, keyed by **logical** index name.
///
/// The engine governs only *item-level rejections* (a sink accepted the batch
/// but refused specific documents). Transport failures, a source decode error,
/// or a flush returning `Err` always stop the run regardless of this.
#[derive(Debug, Clone, Default)]
pub struct FailurePolicies {
    default: FailurePolicy,
    overrides: HashMap<String, FailurePolicy>,
}

impl FailurePolicies {
    /// A policy set with `default` applied to every index and no overrides.
    pub fn new(default: FailurePolicy) -> Self {
        Self {
            default,
            overrides: HashMap::new(),
        }
    }

    /// Override the policy for one logical index, leaving others on the default.
    pub fn with_override(mut self, index: impl Into<String>, policy: FailurePolicy) -> Self {
        self.overrides.insert(index.into(), policy);
        self
    }

    /// The effective policy for `index` (its override, else the default).
    pub fn resolve(&self, index: &str) -> FailurePolicy {
        self.overrides.get(index).copied().unwrap_or(self.default)
    }
}
