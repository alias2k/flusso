//! The live operational state a transport reads: one ingest side, one entry per
//! sink, serialized as the `/status` document.
//!
//! Counters are atomics and the small enums sit behind mutexes, so the
//! observer's sync callbacks never block the engines and a reader always gets a
//! consistent-enough snapshot. Per-sink state is what makes a stalled or
//! failing sink visible without stopping the others.

use engine::Budget;

use crate::stall::{LaneStall, Stalls};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kernel::{IndexName, SinkName};
use serde::Serialize;
use source::cdc::ChangeCapture;
use source::document::BuildFailure;

use crate::fence::{Fence, FencePositions, SinkPositions};

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where the deployment as a whole is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Phase {
    /// Engines are being staged; nothing follows the source yet.
    Starting,
    /// Live, and at least one sink is still seeding an index.
    Backfilling,
    /// Every engine is following its feed.
    Live,
    /// A stop was requested: nothing new is read, what is in flight lands.
    /// Not ready.
    Stopping,
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
    script_failures: Mutex<BTreeMap<IndexName, ScriptFailures>>,
    sinks: BTreeMap<SinkName, SinkStatus>,
    indexes: Vec<IndexName>,
    /// Where the fence positions come from; `None` reports none.
    fence: Option<Fence>,
    /// Where the visibility wait comes from; `None` reports none.
    source: Option<Arc<dyn ChangeCapture>>,
    budget: Option<Arc<Budget>>,
    /// Where each lane's stall comes from; `None` reports none.
    stalls: Option<Arc<Stalls>>,
    /// The lanes that are not open (parked, blocked, restaging); an open lane
    /// has no entry.
    lane_health: Mutex<BTreeMap<stream::LaneKey, LaneHealthView>>,
}

/// A lane's health in `/status`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneHealthState {
    /// Written as usual.
    #[default]
    Open,
    /// Parked on a refusal: an item was refused under `on_error = "stop"`;
    /// its front item is probed with a backoff.
    Refused,
    /// Parked on a block: the destination refuses writes to the index for now
    /// (a write block, a role without write permission); probed with a
    /// backoff.
    Blocked,
    /// Its target was gone: the index is being staged again.
    Restaging,
}

/// A lane that is not open, as `/status` and the metrics read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneHealthView {
    /// Refused, blocked or restaging (never open: an open lane has no view).
    pub state: LaneHealthState,
    /// When it stopped being written (UTC).
    pub since: chrono::DateTime<chrono::Utc>,
    /// When its front item is next written alone (UTC), while parked.
    pub next_probe: Option<chrono::DateTime<chrono::Utc>>,
    /// Why, in the destination's words (the refused ids for a refusal).
    pub reason: String,
}

impl LaneHealthView {
    /// The view of an engine's `health`, its instants read against now; `None`
    /// for an open lane.
    fn of(health: &engine::LaneHealth) -> Option<Self> {
        let now = tokio::time::Instant::now();
        let wall = chrono::Utc::now();
        let at = |instant: tokio::time::Instant| {
            let ago = chrono::Duration::from_std(now.saturating_duration_since(instant))
                .unwrap_or_default();
            let ahead = chrono::Duration::from_std(instant.saturating_duration_since(now))
                .unwrap_or_default();
            wall - ago + ahead
        };
        match health {
            engine::LaneHealth::Parked {
                reason,
                since,
                next_probe,
            } => Some(Self {
                state: match reason {
                    engine::ParkReason::Blocked { .. } => LaneHealthState::Blocked,
                    _ => LaneHealthState::Refused,
                },
                since: at(*since),
                next_probe: Some(at(*next_probe)),
                reason: reason.to_string(),
            }),
            engine::LaneHealth::Restaging { since, reason } => Some(Self {
                state: LaneHealthState::Restaging,
                since: at(*since),
                next_probe: None,
                reason: reason.clone(),
            }),
            _ => None,
        }
    }

    /// The `why` label of `flusso.lane.parked`: `refused`, `blocked` or
    /// `restaging`, the same word `/status` gives.
    pub fn why(&self) -> &'static str {
        match self.state {
            LaneHealthState::Refused => "refused",
            LaneHealthState::Blocked => "blocked",
            LaneHealthState::Restaging => "restaging",
            LaneHealthState::Open => "open",
        }
    }
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
    /// Backfill checkpoints recorded this run, per index.
    checkpoints: Mutex<BTreeMap<IndexName, u64>>,
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
            checkpoints: Mutex::new(BTreeMap::new()),
        }
    }

    fn snapshot(&self, captured: u64, positions: SinkPositions) -> SinkSnapshot {
        let committed = self.changes_committed.load(Ordering::Relaxed);
        let checkpoints = lock(&self.checkpoints).clone();
        SinkSnapshot {
            applied_position: positions.applied,
            searchable_position: positions.searchable,
            lanes: positions
                .indexes
                .into_iter()
                .map(|(index, lane)| {
                    let backfill_checkpoints = checkpoints.get(&index).copied().unwrap_or(0);
                    (
                        index.as_ref().to_owned(),
                        LaneSnapshot {
                            applied_position: lane.applied,
                            searchable_position: lane.searchable,
                            outstanding: lane.outstanding,
                            stall_seconds: None,
                            wal_behind_bytes: None,
                            backfill_checkpoints,
                            health: LaneHealthState::Open,
                            parked_since: None,
                            next_probe: None,
                            reason: None,
                        },
                    )
                })
                .collect(),
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
            script_failures: Mutex::new(BTreeMap::new()),
            sinks: sinks
                .into_iter()
                .map(|sink| (sink, SinkStatus::new(&indexes)))
                .collect(),
            indexes,
            fence: None,
            source: None,
            budget: None,
            stalls: None,
            lane_health: Mutex::new(BTreeMap::new()),
        }
    }

    /// Record `lane`'s health, as the sink engine reported it.
    pub(crate) fn set_lane_health(&self, lane: &stream::LaneKey, health: &engine::LaneHealth) {
        let mut lanes = lock(&self.lane_health);
        match LaneHealthView::of(health) {
            Some(view) => {
                lanes.insert(lane.clone(), view);
            }
            None => {
                lanes.remove(lane);
            }
        }
    }

    /// Every lane that is not open: parked, blocked or being staged again.
    /// What `flusso.lane.parked` and the stop's deadline message read.
    pub fn unhealthy_lanes(&self) -> Vec<(stream::LaneKey, LaneHealthView)> {
        lock(&self.lane_health)
            .iter()
            .map(|(lane, view)| (lane.clone(), view.clone()))
            .collect()
    }

    /// Report each lane's stall and the lane that pins the slot.
    pub fn with_stalls(mut self, stalls: Arc<Stalls>) -> Self {
        self.stalls = Some(stalls);
        self
    }

    /// The last slot lag sampled, if any.
    pub fn slot_lag(&self) -> Option<u64> {
        self.slot_lag_known
            .load(Ordering::Relaxed)
            .then(|| self.slot_lag_bytes.load(Ordering::Relaxed))
    }

    /// Every lane's stall as of the last sample, and the lane pinning the
    /// slot: what the binary's gauges read.
    pub fn stalls(&self) -> (Vec<(stream::LaneKey, LaneStall)>, Option<stream::LaneKey>) {
        self.stalls
            .as_ref()
            .map(|stalls| (stalls.lanes(), stalls.pinned_by()))
            .unwrap_or_default()
    }

    /// Report the memory budget's holding at each snapshot.
    pub fn with_budget(mut self, budget: Arc<Budget>) -> Self {
        self.budget = Some(budget);
        self
    }

    /// Read how long the source has held a change it can't read back yet
    /// ([`ChangeCapture::readable_wait`]) at each snapshot.
    pub fn with_source(mut self, source: Arc<dyn ChangeCapture>) -> Self {
        self.source = Some(source);
        self
    }

    /// How long the source has held a committed change that can't be read back
    /// yet; `None` while nothing waits. Read live, not sampled.
    pub fn visibility_wait(&self) -> Option<Duration> {
        self.source
            .as_ref()
            .and_then(|source| source.readable_wait())
    }

    /// Each `(index, sink)` lane and the items routed to it it hasn't
    /// landed; empty without a fence. Read live, not sampled.
    pub fn lanes_outstanding(&self) -> Vec<(stream::LaneKey, usize)> {
        self.fence
            .as_ref()
            .map(Fence::outstanding)
            .unwrap_or_default()
    }

    /// Report the fence's positions in the snapshot.
    pub fn with_fence(mut self, fence: Fence) -> Self {
        self.fence = Some(fence);
        self
    }

    fn sink(&self, sink: &SinkName) -> Option<&SinkStatus> {
        self.sinks.get(sink)
    }

    /// Set the phase. `stopping` gives way only to `stopped`, so an engine
    /// event during the drain can't make the deployment look ready again.
    pub(crate) fn set_phase(&self, phase: Phase) {
        let mut current = lock(&self.phase);
        if *current != Phase::Stopping || phase == Phase::Stopped {
            *current = phase;
        }
    }

    /// The overall phase: `stopped` is final; otherwise `starting` until the
    /// ingest engine follows the source and every sink engine has staged, then
    /// `backfilling` while any sink still seeds, else `live`. A failed sink
    /// leaves the phase alone (the deployment keeps running) but is not ready.
    fn recompute_phase(&self) {
        let mut phase = lock(&self.phase);
        if matches!(*phase, Phase::Stopping | Phase::Stopped) {
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

    pub(crate) fn walk_recorded(&self, sink: &SinkName, index: &IndexName) {
        if let Some(status) = self.sink(sink) {
            *lock(&status.checkpoints).entry(index.clone()).or_insert(0) += 1;
        }
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

    /// `changes` built into no document `sink` receives: settled for it
    /// without a write.
    pub(crate) fn settle_changes(&self, sink: &SinkName, changes: u64) {
        if let Some(status) = self.sink(sink) {
            status
                .changes_committed
                .fetch_add(changes, Ordering::Relaxed);
        }
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

    pub(crate) fn record_script_failure(
        &self,
        index: &IndexName,
        id: &str,
        failure: &BuildFailure,
    ) {
        let mut failures = lock(&self.script_failures);
        let entry = failures.entry(index.clone()).or_default();
        entry.count += 1;
        entry.last = Some(LastScriptFailure {
            id: id.to_owned(),
            field: failure.field.clone(),
            message: failure.message.clone(),
            line: failure.position.as_ref().map(|at| at.line),
            column: failure.position.as_ref().map(|at| at.column),
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        });
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
        let mut positions: FencePositions = self
            .fence
            .as_ref()
            .map(Fence::positions)
            .unwrap_or_default();
        let sinks: BTreeMap<String, SinkSnapshot> = self
            .sinks
            .iter()
            .map(|(name, status)| {
                let positions = positions.sinks.remove(name).unwrap_or_default();
                let mut snapshot = status.snapshot(captured, positions);
                for (lane, view) in lock(&self.lane_health)
                    .iter()
                    .filter(|(lane, _)| &lane.sink == name)
                {
                    if let Some(entry) = snapshot.lanes.get_mut(lane.index.as_ref()) {
                        entry.health = view.state;
                        entry.parked_since = Some(rfc3339(view.since));
                        entry.next_probe = view.next_probe.map(rfc3339);
                        entry.reason = Some(view.reason.clone());
                    }
                }
                if let Some(stalls) = &self.stalls {
                    for (index, lane) in &mut snapshot.lanes {
                        let Ok(index) = IndexName::try_new(index.as_str()) else {
                            continue;
                        };
                        let stall = stalls.lane(&stream::LaneKey::new(index, name.clone()));
                        lane.stall_seconds = Some(stall.stalled_for.as_secs_f64());
                        lane.wal_behind_bytes = stall.wal_behind;
                    }
                }
                (name.as_ref().to_owned(), snapshot)
            })
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
            visibility_wait_seconds: self.visibility_wait().map(|waited| waited.as_secs_f64()),
            applied_position: positions.applied,
            searchable_position: positions.searchable,
            errors: self.errors.load(Ordering::Relaxed),
            last_error: lock(&self.last_error).clone(),
            script_failures: lock(&self.script_failures)
                .iter()
                .map(|(index, failures)| (index.as_ref().to_owned(), failures.clone()))
                .collect(),
            pinned_by: self
                .stalls
                .as_ref()
                .and_then(|stalls| stalls.pinned_by())
                .map(|lane| LaneId {
                    index: lane.index.as_ref().to_owned(),
                    sink: lane.sink.as_ref().to_owned(),
                }),
            memory: self.budget.as_ref().map(|budget| {
                let held = budget.held();
                MemorySnapshot {
                    limit_bytes: budget.limit(),
                    built_bytes: held.total,
                    backfill_bytes: held.backfill,
                }
            }),
            sinks,
        }
    }
}

/// The memory budget for built documents, in `/status`.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct MemorySnapshot {
    /// `[concurrency] max_built_mib`, in bytes.
    pub limit_bytes: usize,
    /// Bytes of built documents not yet acknowledged by every sink,
    /// reservations of builds in flight included.
    pub built_bytes: usize,
    /// Backfill's share of `built_bytes`; at most half the limit.
    pub backfill_bytes: usize,
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
    /// Seconds the source has held a committed change it can't read back yet
    /// (a transaction not visible: a synchronous standby that does not
    /// answer); every later change waits behind it. `None` while nothing waits.
    pub visibility_wait_seconds: Option<f64>,
    /// The source position every sink has applied, in the source's own text
    /// form: a fetch by id sees every commit at or before it. `None` until the
    /// source follows live changes.
    pub applied_position: Option<String>,
    /// The source position every sink can search: a search sees every commit
    /// at or before it.
    pub searchable_position: Option<String>,
    /// Engine errors, ingest and sink.
    pub errors: u64,
    /// The most recent engine error.
    pub last_error: Option<String>,
    /// Per index, the documents that failed to build (a script failed, or an
    /// `array: true` column held an object), under either policy. Only indexes
    /// with a failure appear.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub script_failures: BTreeMap<String, ScriptFailures>,
    /// The lane that holds the replication slot back the most: the one with
    /// the oldest outstanding change. `None` while no lane has one.
    pub pinned_by: Option<LaneId>,
    /// The memory budget for built documents; `None` when the deployment
    /// counts none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemorySnapshot>,
    pub sinks: BTreeMap<String, SinkSnapshot>,
}

/// Build failures on one index since the daemon started: a script's, or an
/// `array: true` column's.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ScriptFailures {
    /// Documents that failed to build (a `stop` index counts each retry).
    pub count: u64,
    /// The most recent one.
    pub last: Option<LastScriptFailure>,
}

/// The most recent build failure on an index.
#[derive(Debug, Clone, Serialize)]
pub struct LastScriptFailure {
    /// The document id.
    pub id: String,
    /// The failing field's path, e.g. `variants[2].grossPrice`.
    pub field: String,
    /// What went wrong, in one line.
    pub message: String,
    /// The line in the field's script, when the failure has a position.
    pub line: Option<usize>,
    /// The column in that line.
    pub column: Option<usize>,
    /// When it happened (RFC 3339, UTC).
    pub at: String,
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
    /// The source position this sink has applied.
    pub applied_position: Option<String>,
    /// The source position this sink can search.
    pub searchable_position: Option<String>,
    /// Each of this sink's `(index, sink)` lanes, by index: how far it has
    /// applied and can search, and what it holds outstanding. Empty when the
    /// status has no fence.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub lanes: BTreeMap<String, LaneSnapshot>,
}

/// One `(index, sink)` lane in `/status`.
#[derive(Debug, Clone, Serialize)]
pub struct LaneSnapshot {
    /// The source position this lane has applied.
    pub applied_position: Option<String>,
    /// The source position this lane can search.
    pub searchable_position: Option<String>,
    /// Items routed to this lane and not landed yet.
    pub outstanding: usize,
    /// How long the lane's oldest outstanding item has stayed its oldest:
    /// the lane has landed nothing for that long. 0 while nothing is
    /// outstanding; absent when the deployment tracks no stalls.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stall_seconds: Option<f64>,
    /// Bytes of WAL the replication slot holds because of this lane; absent
    /// while nothing is outstanding or before the first lag sample.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wal_behind_bytes: Option<u64>,
    /// How many times this run recorded how far the lane's backfill got, so a
    /// restart resumes after it: rising while a long backfill runs. Absent
    /// while 0.
    #[serde(skip_serializing_if = "is_zero")]
    pub backfill_checkpoints: u64,
    /// Whether the lane is written: `open`, `parked` (a refusal under
    /// `on_error = "stop"`), `blocked` (the index refuses writes for now) or
    /// `restaging` (its target was gone).
    pub health: LaneHealthState,
    /// When it stopped being written (RFC 3339, UTC); absent while open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parked_since: Option<String>,
    /// When its front item is next written alone (RFC 3339, UTC); absent
    /// unless parked or blocked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_probe: Option<String>,
    /// Why it is not written, in the destination's words; absent while open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

fn rfc3339(at: chrono::DateTime<chrono::Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// A lane in `/status`'s `pinned_by`.
#[derive(Debug, Clone, Serialize)]
pub struct LaneId {
    pub index: String,
    pub sink: String,
}

fn is_zero(count: &u64) -> bool {
    *count == 0
}
