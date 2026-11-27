//! The one way the sink engine makes a request that may erase, or loosen, a
//! record another process may rely on (ADR 0012 stage 10, O29 as the owner
//! narrowed it on 2026-11-26): this process destroys or rewrites no such
//! record unless it holds the slot.
//!
//! Each gated method checks [`Rereads::may_erase_records`] immediately before
//! its **sink call**, after every await that precedes it in its caller, so a
//! stream that ended while the caller read, ensured or waited is seen (the
//! sixth re-check, B-1). The check is per sink call, not per HTTP request:
//! inside the call, OpenSearch's `mark_seeded` refreshes the whole new
//! generation (seconds on a large index), swaps the aliases and drops other
//! generations before it writes the marker and deletes the record;
//! `begin_rebuild` and `demote` delete generations before their conditional
//! writes, `detach` after its record. What stays open, for an owner lease
//! to close (#490): a sink call already begun when the stream ends, and the
//! time before this process notices Postgres dropped its walsender (the
//! slot still reads as held then).
//!
//! The engine holds its sink only as [`Plain`], whose `Sink` this module
//! alone can reach: it exposes the requests that erase nothing, and makes
//! [`Erasing`]. A call of an erasing request anywhere else in the engine
//! doesn't compile, however it is spelled (the seventh re-check, S-1). The
//! wall covers the engine crate: the daemon moves `SinkParts.sink` into
//! `SinkEngine::new` and the CLI keeps no clone of it.
//!
//! Every request that writes a record, a gap or the marker also tells
//! [`KnownRecords`] what it left (`known`): the gate keeps this process from
//! erasing another's record, and `known` tells it when another process
//! changed one, which stops the deployment (the cut of 2026-11-27).
//!
//! Every request of the [`Sink`] trait that writes a record, the seed marker
//! or a generation, from the OpenSearch adapter's requests, and why it is
//! gated or not (`gated`: refused here with no slot held):
//!
//! | Request (OpenSearch requests) | Without the slot |
//! | --- | --- |
//! | `mark_seeded` (aliases swapped, every other generation deleted, the marker written whole, a finished rebuild's record deleted) | gated: the marker erases any record and the gaps |
//! | `forget_detachment` (`DELETE detach:…` on the version read): the failed-detach clear, a reattach | gated; a reattach also writes its gap first, which stays and is reused when asked again |
//! | `forget_detachment` of a rebuild a newer seeded generation finished (`forget_finished`) | safe: the record is obsolete, and the delete is conditional on the version read, so any write since fails it |
//! | `reindex` (the marker moved to a new generation) | gated: the reindex waits, its lane restaging |
//! | `reindex` at a `Fresh` source's staging (`reindex_at_a_fresh_start`) | safe: it runs before `prepare` creates the slot (#120), so no slot exists for another process to hold |
//! | `begin_rebuild` (every generation but the serving one deleted, the marker moved, the record rewritten) | gated: the rebuild is owed |
//! | `demote` (every generation but the serving one deleted with its checkpoint, the marker and the record rewritten) | gated (the sixth re-check, S-2): what it deletes may be the rebuild the holder walks. A staging with no slot takes the lane as detached, its decision left to the first read while the slot is held, which demotes (`LaneState.undecided`); the read again runs only then |
//! | `detach` (the marker moved, the record written, every generation but the serving one deleted), staging's `DetachAgain` | gated, as `demote`: a refused lane stays as it was; a staging with no slot takes it as detached and undecided, as for `demote` |
//! | `ensure_index` (the marker written whole, the generation created) | safe: the marker is written whole only when no marker names an existing generation. Nothing anyone holds is there: no rows, so neither a checkpoint nor a gap means anything; `detaching_from` matters only with no record, and losing it leaves an unseeded generation to backfill while reads stay on the serving one. As on `main` |
//! | `ensure_index` deleting an unseeded generation whose `index.uuid` isn't the one recorded | safe: such a generation was recreated by a write under auto-create, not built by any process; a process writing to it restages. As on `main` |
//! | `detachment`, the read, putting a half-moved marker back (`PUT` on the marker read) | safe: only where the marker carries `detaching_from` and no record stands, on the version read. A detach is gated, so one between its two writes is the holder's (or one in flight as its stream ended: #490). Its record landing after the put back still says detached, and the next read detaches again (`DetachAgain`) |
//! | `record_gaps` (`_update` of the marker on the version read) | safe: a gap only grows; an unknown end is settled only while the slot is held (`known_gaps`) |
//! | `record_walked`, `forget_walked` (`_update` of `walked`) | safe: a checkpoint claims only this process's own pages, read only where no record stands against the index; forgetting one only makes a walk start earlier |
//! | `write`, `write_parts` (bulk) | plain writes of what the lanes hold: as on `main` |
//!
//! ```text
//! caller: read records, ensure, write a gap …   (awaits; the stream may end)
//! Erasing::forget_detachment(..)                 may_erase_records()? then send
//!   held     -> the request is sent
//!   not held -> NotHeld: the caller leaves the record and retries later
//! ```

use std::sync::Arc;

use kernel::{Envelope, IndexMapping, IndexName};
use sink::{
    CaughtUp, DecideGaps, Detach, DetachRecord, Detachment, Ensured, Gap, MarkerAt, RecordVersion,
    Recorded, Seeding, Sink, WriteReport,
};

use super::known::{KnownRecords, Wrote};
use crate::Rereads;

/// The sink as the engine holds it: only the requests that erase or loosen
/// no record. The erasing ones exist only on [`Erasing`], made from this,
/// and the `Sink` inside is reachable from this module alone: a call that
/// skips the slot's check doesn't compile (the seventh re-check, S-1).
/// Every request that writes a record or the marker also tells
/// [`KnownRecords`] what it left, so this process's own writes are never
/// taken for another's (`known`).
#[derive(Debug, Clone)]
pub(super) struct Plain(Arc<dyn Sink>, Arc<KnownRecords>);

impl Plain {
    pub(super) fn new(sink: Arc<dyn Sink>, known: Arc<KnownRecords>) -> Self {
        Self(sink, known)
    }

    /// The erasing requests, each checked against `rereads`' slot right
    /// before its sink call; `mappings` name each index's records for the
    /// read back after a write.
    pub(super) fn erasing<'a>(
        &'a self,
        rereads: &'a Rereads,
        mappings: &'a [IndexMapping],
    ) -> Erasing<'a> {
        Erasing {
            sink: self.0.as_ref(),
            rereads,
            known: &self.1,
            mappings,
        }
    }

    /// Ensure `mapping`'s index; one created writes the marker whole. A
    /// failed ensure may have written it.
    pub(super) async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<Ensured> {
        let ensured = self.0.ensure_index(mapping).await;
        match &ensured {
            Ok(ensured) if ensured.created => self.1.wrote(&mapping.index, &Wrote::Marker, None),
            Ok(_) => {}
            Err(_) => self.1.unsure(&mapping.index, &Wrote::Marker, None),
        }
        ensured
    }

    pub(super) async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        caught_up: CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        self.0.write_parts(parts, caught_up).await
    }

    pub(super) fn write_limit(&self) -> Option<usize> {
        self.0.write_limit()
    }

    pub(super) fn ordered(&self) -> bool {
        self.0.ordered()
    }

    pub(super) async fn seeding(&self, index: &IndexName) -> sink::Result<Seeding> {
        self.0.seeding(index).await
    }

    pub(super) async fn is_rebuilding(&self, index: &IndexName) -> sink::Result<bool> {
        self.0.is_rebuilding(index).await
    }

    /// The records' read; it may put a half-moved marker back, on the
    /// version read (the table above: safe).
    pub(super) async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        self.0.detachment(mapping).await
    }

    /// The gaps kept with the marker as `decide` says. A failed write may
    /// have landed the list it was deciding.
    pub(super) async fn record_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &DecideGaps<'_>,
    ) -> sink::Result<Vec<Gap>> {
        let decided = std::sync::Mutex::new(None);
        let deciding = |kept: &[Gap]| {
            let new = decide(kept);
            *decided
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = new.clone();
            new
        };
        let gaps = self.0.record_gaps(mapping, &deciding).await;
        match &gaps {
            Ok(gaps) => self
                .1
                .wrote(&mapping.index, &Wrote::Gaps(Some(gaps.clone())), None),
            Err(_) => {
                let decided = decided
                    .into_inner()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                self.1.unsure(&mapping.index, &Wrote::Gaps(decided), None);
            }
        }
        gaps
    }

    pub(super) async fn record_walked(&self, index: &IndexName, mark: &str) -> sink::Result<()> {
        self.0.record_walked(index, mark).await
    }

    pub(super) async fn forget_walked(&self, index: &IndexName) -> sink::Result<()> {
        self.0.forget_walked(index).await
    }
}

/// This process may not erase or loosen a record now: it holds no slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotHeld;

impl std::fmt::Display for NotHeld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "this process doesn't hold the slot (its stream ended or its opening failed), so the \
             record may be another process's",
        )
    }
}

/// The sink's requests that may erase or loosen a record, each checked
/// against the slot immediately before it is sent.
#[derive(Clone, Copy)]
pub(super) struct Erasing<'a> {
    sink: &'a dyn Sink,
    rereads: &'a Rereads,
    known: &'a KnownRecords,
    mappings: &'a [IndexMapping],
}

impl std::fmt::Debug for Erasing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Erasing").finish_non_exhaustive()
    }
}

impl Erasing<'_> {
    /// `result`, of an own write that leaves `wrote` on `mapping`'s index
    /// when it returns `Ok`, told to [`KnownRecords`]: what it leaves, or,
    /// when it failed, every state it may have left. A write of the record,
    /// or one that failed, is then read back while the slot is still held,
    /// which settles it; a marker write that returned `Ok` keeps the
    /// record's version and needs no read.
    async fn told<T>(
        &self,
        mapping: &IndexMapping,
        wrote: Wrote,
        result: sink::Result<T>,
    ) -> sink::Result<T> {
        let at = Some(self.rereads.read_begins());
        match &result {
            Ok(_) => self.known.wrote(&mapping.index, &wrote, at),
            Err(_) => self.known.unsure(&mapping.index, &wrote, at),
        }
        if result.is_err() || !matches!(wrote, Wrote::Marker) {
            self.read_back(mapping).await;
        }
        result
    }

    /// Right after an own write, the records read back while the slot is
    /// still held. No other process writes while this one holds the slot,
    /// so what the read finds is this process's own write: it settles a
    /// write whose outcome was unknown, and learns the record's version, so
    /// an identical record another process writes later is told apart. A
    /// stream that ended meanwhile, or a failed read, leaves what was known
    /// (a call in flight as the stream ends: #490). The records read, when
    /// held throughout.
    async fn read_back(&self, mapping: &IndexMapping) -> Option<Detachment> {
        let at = self.rereads.read_begins();
        self.held().ok()?;
        let found = self.sink.detachment(mapping).await.ok()?;
        let held_throughout = self.rereads.held_or_alone() && self.rereads.read_begins() == at;
        if !held_throughout {
            return None;
        }
        let _ = self.known.judge(&mapping.index, &found, at, true);
        Some(found)
    }

    fn held(&self) -> Result<(), NotHeld> {
        if self.rereads.may_erase_records() {
            Ok(())
        } else {
            Err(NotHeld)
        }
    }

    /// Seed `index`: its marker is written whole, erasing any gap; a
    /// finished rebuild's record is deleted best effort. Read back while
    /// held, a record the delete left is deleted again here, as a staging
    /// would (the review of the cut, B-1).
    pub(super) async fn mark_seeded(&self, index: &IndexName) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        let seeded = self.sink.mark_seeded(index).await;
        let Some(mapping) = self.mappings.iter().find(|mapping| &mapping.index == index) else {
            return Ok(seeded);
        };
        let at = Some(self.rereads.read_begins());
        match &seeded {
            Ok(()) => self.known.wrote(index, &Wrote::Seed, at),
            Err(_) => self.known.unsure(index, &Wrote::Seed, at),
        }
        if let Some(found) = self.read_back(mapping).await
            && seeded.is_ok()
            && let (
                Some(DetachRecord::Rebuilding { live_only: false }),
                Some(MarkerAt::Newer { seeded: true }),
                Some(version),
            ) = (&found.record, found.marker, &found.version)
        {
            tracing::warn!(%index, "the seed left the finished rebuild's detach record (its delete failed); deleting it now");
            let _ = self.forget_finished(mapping, version).await;
        }
        Ok(seeded)
    }

    /// Delete the detach record read at `version`.
    pub(super) async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        let forgotten = self.sink.forget_detachment(mapping, version).await;
        Ok(self.told(mapping, Wrote::Forget, forgotten).await)
    }

    /// Delete the record of a rebuild a newer seeded generation finished.
    /// Ungated: the record is obsolete, and the delete is conditional on the
    /// version read, so any write since fails it.
    pub(super) async fn forget_finished(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        let forgotten = self.sink.forget_detachment(mapping, version).await;
        self.told(mapping, Wrote::Forget, forgotten).await
    }

    /// Move `mapping`'s index to a new generation; its marker is written
    /// whole, for the ensure that follows to create its generation.
    pub(super) async fn reindex(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        let reindexed = self.sink.reindex(mapping).await;
        Ok(self.told(mapping, Wrote::Marker, reindexed).await)
    }

    /// [`reindex`](Self::reindex) at a `Fresh` source's staging, ungated by
    /// necessity: it runs before `prepare` creates the slot (#120), so no
    /// slot exists for anyone to hold.
    pub(super) async fn reindex_at_a_fresh_start(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<()> {
        let reindexed = self.sink.reindex(mapping).await;
        let at = Some(self.rereads.read_begins());
        match &reindexed {
            Ok(()) => self.known.wrote(&mapping.index, &Wrote::Marker, at),
            Err(_) => self.known.unsure(&mapping.index, &Wrote::Marker, at),
        }
        reindexed
    }

    /// Begin a rebuild: other generations deleted, the marker moved, the
    /// record rewritten into `rebuilding`.
    pub(super) async fn begin_rebuild(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<u64>, NotHeld> {
        self.held()?;
        let begun = self.sink.begin_rebuild(mapping).await;
        let wrote = Wrote::Rebuild {
            attempts: begun.as_ref().ok().copied(),
        };
        Ok(self.told(mapping, wrote, begun).await)
    }

    /// Put a rebuild back to detached: every generation but the serving one
    /// deleted, the walk checkpoint with it, the marker and the record
    /// rewritten. Gated: what it deletes may be the rebuild another process
    /// holding the slot is walking (the sixth re-check, S-2).
    pub(super) async fn demote(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        let demoted = self.sink.demote(mapping, live_only).await;
        Ok(self
            .told(mapping, Wrote::Demote { live_only }, demoted)
            .await)
    }

    /// Detach `mapping`'s index: the marker moved, the record written, every
    /// generation but the serving one deleted. Gated as
    /// [`demote`](Self::demote) is.
    pub(super) async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &Detach,
    ) -> Result<sink::Result<Recorded>, NotHeld> {
        self.held()?;
        let recorded = self.sink.detach(mapping, detach).await;
        let wrote = Wrote::Detach {
            live_only: detach.live_only,
            through: detach.through.clone(),
        };
        let recorded = match recorded {
            // The sink keeps no records: nothing was written.
            Ok(Recorded::NotKept) => Ok(Recorded::NotKept),
            recorded => self.told(mapping, wrote, recorded).await,
        };
        Ok(recorded)
    }
}
