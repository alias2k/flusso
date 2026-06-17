//! Configuration vocabulary for a run: how changes are grouped into a flush
//! ([`BatchPolicy`]) and how item-level sink rejections are handled
//! ([`FailurePolicies`] over [`FailurePolicy`]).

use std::collections::HashMap;
use std::time::Duration;

// The policy *vocabulary* (`Stop`/`Skip`) is a config-domain type, so it lives
// in `schema-core` and is re-exported here for engine callers.
pub use schema_core::FailurePolicy;

/// How the worker groups changes into one sink flush.
///
/// Batching trades a little latency for far fewer round-trips: up to
/// `max_changes` changes (or whatever has arrived after `max_delay`, whichever
/// comes first) are buffered and flushed together. `max_changes: 1` reproduces
/// the original flush-per-change behavior.
///
/// Acks respect the batch boundary — see the [module docs](crate). The source
/// ack for a change is confirmed only after the flush that made its documents
/// durable, so at-least-once delivery holds regardless of batch size.
#[derive(Debug, Clone, Copy)]
pub struct BatchPolicy {
    /// Flush once this many changes have accumulated. Clamped to at least 1.
    pub max_changes: usize,
    /// Flush a partial batch this long after its first change, so a trickle of
    /// changes still lands promptly instead of waiting for a full batch.
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
