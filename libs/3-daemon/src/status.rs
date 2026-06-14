//! The live operational state of a running daemon.
//!
//! [`Status`] is the shared handle the crate's status observer (`StatusObserver`)
//! writes to as the engine emits events, and the HTTP `/status` endpoint reads
//! from. It
//! holds only fast, lock-light state (atomics for counters, short-held mutexes
//! for the phase, the per-index map, and the last error) so updating it never
//! blocks the pipeline's hot path.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use schema_core::IndexName;
use serde::Serialize;

/// Recover a poisoned mutex rather than panicking — a writer that panicked
/// mid-update leaves at worst slightly stale status, never a downed endpoint.
/// (`.lock().unwrap()` is forbidden by the workspace lints anyway.)
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// The pipeline's overall phase, in the order the engine moves through them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Starting up; indexes not yet ensured.
    Starting,
    /// Seeding one or more unseeded indexes from a snapshot.
    Backfilling,
    /// Following live changes.
    Live,
    /// The run has ended (clean stop or error).
    Stopped,
}

/// Where one index is in its lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    /// Not yet known to be seeded this run.
    Pending,
    /// Its initial backfill is in progress.
    Backfilling,
    /// Seeded — either backfilled this run or already seeded on start.
    Seeded,
}

/// Shared, mutable operational state. Cheap to update concurrently from the
/// observer; snapshotted to JSON for the `/status` endpoint.
#[derive(Debug)]
pub struct Status {
    started_at: Instant,
    phase: Mutex<Phase>,
    indexes: Mutex<BTreeMap<IndexName, IndexState>>,
    changes_captured: AtomicU64,
    changes_committed: AtomicU64,
    documents_built: AtomicU64,
    documents_quarantined: AtomicU64,
    batches: AtomicU64,
    last_flush_micros: AtomicU64,
    slot_lag_bytes: AtomicU64,
    slot_lag_known: AtomicBool,
    errors: AtomicU64,
    last_error: Mutex<Option<String>>,
}

impl Status {
    /// A fresh status with every configured index `Pending`. `now` is the start
    /// instant uptime is measured from.
    pub fn new(indexes: impl IntoIterator<Item = IndexName>, now: Instant) -> Self {
        let indexes = indexes
            .into_iter()
            .map(|index| (index, IndexState::Pending))
            .collect();
        Self {
            started_at: now,
            phase: Mutex::new(Phase::Starting),
            indexes: Mutex::new(indexes),
            changes_captured: AtomicU64::new(0),
            changes_committed: AtomicU64::new(0),
            documents_built: AtomicU64::new(0),
            documents_quarantined: AtomicU64::new(0),
            batches: AtomicU64::new(0),
            last_flush_micros: AtomicU64::new(0),
            slot_lag_bytes: AtomicU64::new(0),
            slot_lag_known: AtomicBool::new(false),
            errors: AtomicU64::new(0),
            last_error: Mutex::new(None),
        }
    }

    pub(crate) fn set_phase(&self, phase: Phase) {
        *lock(&self.phase) = phase;
    }

    pub(crate) fn mark_backfilling(&self, indexes: &[IndexName]) {
        let mut map = lock(&self.indexes);
        for index in indexes {
            map.insert(index.clone(), IndexState::Backfilling);
        }
    }

    pub(crate) fn mark_seeded(&self, index: &IndexName) {
        lock(&self.indexes).insert(index.clone(), IndexState::Seeded);
    }

    /// Reaching live capture means every index is seeded by definition, so any
    /// still `Pending` (already seeded before this run, never backfilled here)
    /// is promoted to `Seeded`.
    pub(crate) fn mark_all_seeded(&self) {
        for state in lock(&self.indexes).values_mut() {
            if *state != IndexState::Seeded {
                *state = IndexState::Seeded;
            }
        }
    }

    pub(crate) fn record_capture(&self) {
        self.changes_captured.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_commit(&self, changes: u64, documents: u64, flush_micros: u64) {
        self.changes_committed.fetch_add(changes, Ordering::Relaxed);
        self.documents_built.fetch_add(documents, Ordering::Relaxed);
        self.batches.fetch_add(1, Ordering::Relaxed);
        self.last_flush_micros
            .store(flush_micros, Ordering::Relaxed);
    }

    /// Changes captured but not yet committed — the queue/back-pressure signal.
    /// A cheap two-atomic read, safe to call from a metrics collection thread
    /// (e.g. an observable-gauge callback in the binary).
    pub fn in_flight(&self) -> u64 {
        self.changes_captured
            .load(Ordering::Relaxed)
            .saturating_sub(self.changes_committed.load(Ordering::Relaxed))
    }

    pub(crate) fn record_quarantine(&self) {
        self.documents_quarantined.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_lag(&self, bytes: u64) {
        self.slot_lag_bytes.store(bytes, Ordering::Relaxed);
        self.slot_lag_known.store(true, Ordering::Relaxed);
    }

    pub(crate) fn record_error(&self, error: &str) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        *lock(&self.last_error) = Some(error.to_owned());
    }

    /// A point-in-time, serializable view of the status for the HTTP endpoint.
    pub fn snapshot(&self) -> StatusSnapshot {
        let captured = self.changes_captured.load(Ordering::Relaxed);
        let committed = self.changes_committed.load(Ordering::Relaxed);
        StatusSnapshot {
            phase: *lock(&self.phase),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            indexes: lock(&self.indexes)
                .iter()
                .map(|(name, state)| (name.as_ref().to_owned(), *state))
                .collect(),
            changes_captured: captured,
            changes_committed: committed,
            changes_in_flight: captured.saturating_sub(committed),
            documents_built: self.documents_built.load(Ordering::Relaxed),
            documents_quarantined: self.documents_quarantined.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            last_flush_micros: self.last_flush_micros.load(Ordering::Relaxed),
            slot_lag_bytes: self
                .slot_lag_known
                .load(Ordering::Relaxed)
                .then(|| self.slot_lag_bytes.load(Ordering::Relaxed)),
            errors: self.errors.load(Ordering::Relaxed),
            last_error: lock(&self.last_error).clone(),
        }
    }
}

/// A serializable snapshot of [`Status`], returned as JSON by `/status`.
#[derive(Debug, Clone, Serialize)]
pub struct StatusSnapshot {
    pub phase: Phase,
    pub uptime_seconds: u64,
    pub indexes: BTreeMap<String, IndexState>,
    pub changes_captured: u64,
    pub changes_committed: u64,
    pub changes_in_flight: u64,
    pub documents_built: u64,
    /// Documents the sink rejected and the engine skipped (failure policy
    /// `skip`). A non-zero value means data is being dropped — alert on it.
    pub documents_quarantined: u64,
    pub batches: u64,
    pub last_flush_micros: u64,
    /// `None` until the source first reports lag (e.g. the slot doesn't exist
    /// yet), otherwise bytes the destination trails the source by.
    pub slot_lag_bytes: Option<u64>,
    pub errors: u64,
    pub last_error: Option<String>,
}
