//! Configuration vocabulary for a run: how changes are grouped into a flush
//! ([`BatchPolicy`]), how item-level sink rejections are handled
//! ([`FailurePolicies`] over [`FailurePolicy`]), and how long a retry waits
//! ([`Backoff`]).
//!
//! ```
//! use std::time::Duration;
//! use engine::Backoff;
//!
//! let backoff = Backoff::new(Duration::from_millis(200), Duration::from_millis(800)).unwrap();
//! let waits: Vec<_> = std::iter::successors(Some(backoff.initial()), |wait| Some(backoff.after(*wait)))
//!     .take(4)
//!     .collect();
//! assert_eq!(waits, [200, 400, 800, 800].map(Duration::from_millis));
//! assert!(Backoff::new(Duration::ZERO, Duration::from_secs(1)).is_err());
//! ```

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

/// How long the engines wait before trying again: a failed engine's restart,
/// a parked lane's probe, a re-staged lane's next staging. The first wait is
/// `initial`, each next one doubles, up to `max`; equal values wait a constant
/// interval. Default: 1 s doubling to 60 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    initial: Duration,
    max: Duration,
}

/// A [`Backoff`] that can't be: a zero first wait (it would spin) or a cap
/// below the first wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BackoffError {
    /// The first wait is zero.
    #[error("the first wait must be at least 1 ms")]
    ZeroInitial,
    /// The cap is below the first wait.
    #[error("the longest wait ({max:?}) is below the first ({initial:?})")]
    MaxBelowInitial {
        /// The first wait.
        initial: Duration,
        /// The longest wait.
        max: Duration,
    },
}

impl Backoff {
    /// Waits from `initial`, doubling up to `max`.
    pub fn new(initial: Duration, max: Duration) -> Result<Self, BackoffError> {
        if initial.is_zero() {
            return Err(BackoffError::ZeroInitial);
        }
        if max < initial {
            return Err(BackoffError::MaxBelowInitial { initial, max });
        }
        Ok(Self { initial, max })
    }

    /// The first wait.
    pub fn initial(&self) -> Duration {
        self.initial
    }

    /// The longest wait.
    pub fn max(&self) -> Duration {
        self.max
    }

    /// The wait after `wait`: doubled, up to the longest.
    pub fn after(&self, wait: Duration) -> Duration {
        wait.saturating_mul(2).min(self.max)
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
        }
    }
}

/// How the engines resolve the [`FailurePolicy`] for a failed document: a
/// global `default` plus per-index overrides, keyed by **logical** index name.
///
/// It governs only *per-document* failures: an item-level rejection (a sink
/// accepted the batch but refused specific documents) and a document that
/// could not be built (a script failed on it, or an `array: true` column held
/// an object). Transport failures, a source
/// decode error, or a flush returning `Err` always stop the run regardless of
/// this.
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
