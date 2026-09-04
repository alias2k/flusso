//! The live operational state a transport reads: one ingest side, one entry per
//! sink, serialized as the `/status` document.
//!
//! Counters are atomics and the small enums sit behind mutexes, so the
//! observer's sync callbacks never block the engines and a reader always gets a
//! consistent-enough snapshot. Per-sink state is what makes a stalled or
//! failing sink visible without stopping the others.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use kernel::{IndexName, SinkName};
use serde::Serialize;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where the deployment as a whole is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Engines are being staged; nothing follows the source yet.
    Starting,
    /// Live, and at least one sink is still seeding an index.
    Backfilling,
    /// Every engine is following its feed.
    Live,
    /// The ingest engine has stopped; the deployment is no longer syncing.
    Stopped,
}

/// Where one sink engine is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SinkPhase {
    /// Not yet staged.
    Starting,
    /// Following its lane while at least one index is being seeded.
    Backfilling,
    /// Following its lane with every index seeded.
    Live,
    /// Stopped on an error; the daemon is restarting it with backoff.
    Failed,
    /// Its lane closed; it will not run again this process.
    Stopped,
}

/// Where one index is, on one sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexState {
    /// Not yet staged.
    Pending,
    /// A snapshot is in flight for it.
    Backfilling,
    /// The sink holds a complete snapshot.
    Seeded,
}

/// The live status of one deployment.
#[derive(Debug)]
pub struct Status {
    started_at: Instant,
    phase: Mutex<Phase>,
    ingest_live: AtomicBool,
    changes_captured: AtomicU64,
    documents_built: AtomicU64,
    slot_lag_bytes: AtomicU64,
    slot_lag_known: AtomicBool,
    errors: AtomicU64,
    last_error: Mutex<Option<String>>,
    sinks: BTreeMap<SinkName, SinkStatus>,
    indexes: Vec<IndexName>,
}

/// The live status of one sink engine.
#[derive(Debug)]
pub struct SinkStatus {
    phase: Mutex<SinkPhase>,
    indexes: Mutex<BTreeMap<IndexName, IndexState>>,
    changes_committed: AtomicU64,
    envelopes_applied: AtomicU64,
    batches: AtomicU64,
    documents_quarantined: AtomicU64,
    last_flush_micros: AtomicU64,
}

impl SinkStatus {
    fn new(indexes: &[IndexName]) -> Self {
        Self {
            phase: Mutex::new(SinkPhase::Starting),
            indexes: Mutex::new(
                indexes
                    .iter()
                    .map(|index| (index.clone(), IndexState::Pending))
                    .collect(),
            ),
            changes_committed: AtomicU64::new(0),
            envelopes_applied: AtomicU64::new(0),
            batches: AtomicU64::new(0),
            documents_quarantined: AtomicU64::new(0),
            last_flush_micros: AtomicU64::new(0),
        }
    }

    fn snapshot(&self, captured: u64) -> SinkSnapshot {
        let committed = self.changes_committed.load(Ordering::Relaxed);
        SinkSnapshot {
            phase: *lock(&self.phase),
            indexes: lock(&self.indexes)
                .iter()
                .map(|(name, state)| (name.as_ref().to_owned(), *state))
                .collect(),
            changes_committed: committed,
            changes_in_flight: captured.saturating_sub(committed),
            envelopes_applied: self.envelopes_applied.load(Ordering::Relaxed),
            batches: self.batches.load(Ordering::Relaxed),
            documents_quarantined: self.documents_quarantined.load(Ordering::Relaxed),
            last_flush_micros: self.last_flush_micros.load(Ordering::Relaxed),
        }
    }
}

impl Status {
    /// A status for `indexes` maintained on `sinks`, everything pending.
    pub fn new(
        indexes: impl IntoIterator<Item = IndexName>,
        sinks: impl IntoIterator<Item = SinkName>,
        now: Instant,
    ) -> Self {
        let indexes: Vec<IndexName> = indexes.into_iter().collect();
        Self {
            started_at: now,
            phase: Mutex::new(Phase::Starting),
            ingest_live: AtomicBool::new(false),
            changes_captured: AtomicU64::new(0),
            documents_built: AtomicU64::new(0),
            slot_lag_bytes: AtomicU64::new(0),
            slot_lag_known: AtomicBool::new(false),
            errors: AtomicU64::new(0),
            last_error: Mutex::new(None),
            sinks: sinks
                .into_iter()
                .map(|sink| (sink, SinkStatus::new(&indexes)))
                .collect(),
            indexes,
        }
    }

    fn sink(&self, sink: &SinkName) -> Option<&SinkStatus> {
        self.sinks.get(sink)
    }

    pub(crate) fn set_phase(&self, phase: Phase) {
        *lock(&self.phase) = phase;
    }

    /// The overall phase: `stopped` is final; otherwise `starting` until the
    /// ingest engine follows the source and every sink engine has staged, then
    /// `backfilling` while any sink still seeds, else `live`. A failed sink
    /// leaves the phase alone (the deployment keeps running) but is not ready.
    fn recompute_phase(&self) {
        let mut phase = lock(&self.phase);
        if *phase == Phase::Stopped {
            return;
        }
        if !self.ingest_live.load(Ordering::Relaxed) {
            *phase = Phase::Starting;
            return;
        }
        let sink_phases: Vec<SinkPhase> = self.sinks.values().map(|s| *lock(&s.phase)).collect();
        if sink_phases.contains(&SinkPhase::Starting) {
            *phase = Phase::Starting;
            return;
        }
        *phase = if sink_phases.contains(&SinkPhase::Backfilling) {
            Phase::Backfilling
        } else {
            Phase::Live
        };
    }

    pub(crate) fn mark_ingest_live(&self) {
        self.ingest_live.store(true, Ordering::Relaxed);
        self.recompute_phase();
    }

    /// The ingest engine stopped on an error and the daemon is restarting it:
    /// not ready until it follows the source again.
    pub(crate) fn mark_ingest_failed(&self) {
        self.ingest_live.store(false, Ordering::Relaxed);
        self.recompute_phase();
    }

    pub(crate) fn mark_sink_started(&self, sink: &SinkName) {
        if let Some(status) = self.sink(sink) {
            let backfilling = lock(&status.indexes)
                .values()
                .any(|s| *s == IndexState::Backfilling);
            *lock(&status.phase) = if backfilling {
                SinkPhase::Backfilling
            } else {
                SinkPhase::Live
            };
            if !backfilling {
                for state in lock(&status.indexes).values_mut() {
                    *state = IndexState::Seeded;
                }
            }
        }
        self.recompute_phase();
    }

    pub(crate) fn mark_backfilling(&self, sink: &SinkName, indexes: &[IndexName]) {
        if let Some(status) = self.sink(sink) {
            let mut map = lock(&status.indexes);
            for index in indexes {
                map.insert(index.clone(), IndexState::Backfilling);
            }
            *lock(&status.phase) = SinkPhase::Backfilling;
        }
        self.recompute_phase();
    }

    pub(crate) fn mark_seeded(&self, sink: &SinkName, index: &IndexName) {
        if let Some(status) = self.sink(sink) {
            let mut map = lock(&status.indexes);
            map.insert(index.clone(), IndexState::Seeded);
            if map.values().all(|s| *s == IndexState::Seeded) {
                *lock(&status.phase) = SinkPhase::Live;
            }
        }
        self.recompute_phase();
    }

    pub(crate) fn mark_sink_failed(&self, sink: &SinkName) {
        if let Some(status) = self.sink(sink) {
            *lock(&status.phase) = SinkPhase::Failed;
        }
    }

    pub(crate) fn mark_sink_stopped(&self, sink: &SinkName) {
        if let Some(status) = self.sink(sink) {
            *lock(&status.phase) = SinkPhase::Stopped;
        }
    }

    pub(crate) fn record_capture(&self) {
        self.changes_captured.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_build(&self, documents: u64) {
        self.documents_built.fetch_add(documents, Ordering::Relaxed);
    }

    pub(crate) fn record_commit(
        &self,
        sink: &SinkName,
        changes: u64,
        envelopes: u64,
        flush_micros: u64,
    ) {
        if let Some(status) = self.sink(sink) {
            status
                .changes_committed
                .fetch_add(changes, Ordering::Relaxed);
            status
                .envelopes_applied
                .fetch_add(envelopes, Ordering::Relaxed);
            status.batches.fetch_add(1, Ordering::Relaxed);
            status
                .last_flush_micros
                .store(flush_micros, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_quarantine(&self, sink: &SinkName) {
        if let Some(status) = self.sink(sink) {
            status.documents_quarantined.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(crate) fn record_lag(&self, bytes: u64) {
        self.slot_lag_bytes.store(bytes, Ordering::Relaxed);
        self.slot_lag_known.store(true, Ordering::Relaxed);
    }

    pub(crate) fn record_error(&self, error: &str) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        *lock(&self.last_error) = Some(error.to_owned());
    }

    /// Changes captured but not yet committed by the slowest sink.
    pub fn in_flight(&self) -> u64 {
        let captured = self.changes_captured.load(Ordering::Relaxed);
        let slowest = self
            .sinks
            .values()
            .map(|s| s.changes_committed.load(Ordering::Relaxed))
            .min()
            .unwrap_or(captured);
        captured.saturating_sub(slowest)
    }

    /// Changes captured but not yet committed by `sink`.
    pub fn in_flight_for(&self, sink: &SinkName) -> u64 {
        let captured = self.changes_captured.load(Ordering::Relaxed);
        self.sink(sink).map_or(0, |s| {
            captured.saturating_sub(s.changes_committed.load(Ordering::Relaxed))
        })
    }

    /// The sinks this status tracks.
    pub fn sinks(&self) -> impl Iterator<Item = &SinkName> {
        self.sinks.keys()
    }

    /// Whether every engine is ready: the ingest engine is live and every sink
    /// engine is live or backfilling.
    pub fn is_ready(&self) -> bool {
        let phase = *lock(&self.phase);
        matches!(phase, Phase::Live | Phase::Backfilling)
            && self
                .sinks
                .values()
                .all(|s| matches!(*lock(&s.phase), SinkPhase::Live | SinkPhase::Backfilling))
    }

    /// A point-in-time copy for serialization.
    pub fn snapshot(&self) -> StatusSnapshot {
        let captured = self.changes_captured.load(Ordering::Relaxed);
        let sinks: BTreeMap<String, SinkSnapshot> = self
            .sinks
            .iter()
            .map(|(name, status)| (name.as_ref().to_owned(), status.snapshot(captured)))
            .collect();
        let indexes = self
            .indexes
            .iter()
            .map(|index| {
                let states: Vec<IndexState> = self
                    .sinks
                    .values()
                    .filter_map(|s| lock(&s.indexes).get(index).copied())
                    .collect();
                let state = if states.contains(&IndexState::Backfilling) {
                    IndexState::Backfilling
                } else if !states.is_empty() && states.iter().all(|s| *s == IndexState::Seeded) {
                    IndexState::Seeded
                } else {
                    IndexState::Pending
                };
                (index.as_ref().to_owned(), state)
            })
            .collect();
        StatusSnapshot {
            phase: *lock(&self.phase),
            uptime_seconds: self.started_at.elapsed().as_secs(),
            indexes,
            changes_captured: captured,
            changes_in_flight: self.in_flight(),
            documents_built: self.documents_built.load(Ordering::Relaxed),
            slot_lag_bytes: self
                .slot_lag_known
                .load(Ordering::Relaxed)
                .then(|| self.slot_lag_bytes.load(Ordering::Relaxed)),
            errors: self.errors.load(Ordering::Relaxed),
            last_error: lock(&self.last_error).clone(),
            sinks,
        }
    }
}

/// The `/status` document.
#[derive(Debug, Clone, Serialize)]
pub struct StatusSnapshot {
    /// Where the deployment as a whole is.
    pub phase: Phase,
    /// Seconds since the daemon started.
    pub uptime_seconds: u64,
    /// Each index's state across sinks: backfilling if any sink is seeding it,
    /// seeded when every sink holds it.
    pub indexes: BTreeMap<String, IndexState>,
    /// Changes the ingest engine pulled from the source.
    pub changes_captured: u64,
    /// Captured but not yet committed by the slowest sink.
    /// Captured minus committed by the slowest sink (per sink: by that sink).
    pub changes_in_flight: u64,
    /// Documents the ingest engine built, once for every sink.
    pub documents_built: u64,
    /// Bytes the confirmed position trails the source head by; `None` until sampled.
    pub slot_lag_bytes: Option<u64>,
    /// Engine errors, ingest and sink.
    pub errors: u64,
    /// The most recent engine error.
    pub last_error: Option<String>,
    pub sinks: BTreeMap<String, SinkSnapshot>,
}

/// One sink's slice of the `/status` document.
#[derive(Debug, Clone, Serialize)]
pub struct SinkSnapshot {
    /// Where this sink engine is.
    pub phase: SinkPhase,
    pub indexes: BTreeMap<String, IndexState>,
    /// Changes whose batch this sink flushed and acknowledged.
    pub changes_committed: u64,
    /// Captured minus committed by the slowest sink (per sink: by that sink).
    pub changes_in_flight: u64,
    /// Documents written to this sink.
    pub envelopes_applied: u64,
    /// Batches this sink flushed.
    pub batches: u64,
    /// Documents this sink rejected and its engine skipped.
    pub documents_quarantined: u64,
    /// Duration of the most recent flush, in microseconds.
    pub last_flush_micros: u64,
}
