//! The one way the sink engine makes a request that may erase, or loosen, a
//! record another process may rely on (ADR 0012 stage 10, O29 as the owner
//! narrowed it on 2026-11-26): this process destroys or rewrites no such
//! record unless it holds the slot.
//!
//! Each gated method checks [`Rereads::may_erase_records`] immediately before
//! its **sink call**, after every await that precedes it in its caller, so a
//! stream that ended while the caller read, ensured or waited is seen (the
//! sixth re-check, B-1). It also needs the index's records read under the
//! opening held now: records read under an earlier stream are read again
//! first, and the write goes out only when they are the same; another
//! process's change found there stops the deployment (the review of the
//! cut). The check is per sink call, not per HTTP request:
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
//! Every request that writes a record, a gap or the marker carries a
//! [`Stamp`](sink::Stamp) from [`KnownRecords`], which the sink stores in
//! what it writes (the review of round 10). A write of the record is then
//! read back while the slot is still held; a call that failed counts as
//! landed exactly when the record read back carries its own stamp. The gate
//! keeps this process from erasing another's record, and `known` tells it,
//! by who wrote what it reads, when another process changed one, which
//! stops the deployment (the cut of 2026-11-27).
//!
//! Every request of the [`Sink`] trait that writes a record, the seed marker
//! or a generation, from the OpenSearch adapter's requests, and why it is
//! gated or not (`gated`: refused here with no slot held):
//!
//! | Request (OpenSearch requests) | Without the slot |
//! | --- | --- |
//! | `mark_seeded` (aliases swapped, every other generation deleted, the marker written whole, a finished rebuild's record forgotten **best effort**: a failed forget only warns, and the call returns `Ok`) | gated: the marker erases any record and the gaps. Read back while held, a record the forget left is forgotten here at once (`forget_finished`, the review of the cut, B-1) |
//! | `forget_detachment` (a tombstone over `detach:…`, on the version read): the failed-detach clear (only a record this process wrote), a reattach | gated; a reattach also writes its gap first, which stays and is reused when asked again |
//! | `forget_detachment` of a rebuild a newer seeded generation finished (`forget_finished`) | safe: the record is obsolete, and the tombstone is conditional on the version read, so any write since fails it |
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
    Recorded, Seeding, Sink, Stamp, WriteReport,
};

use super::known::{Change, KnownRecords};
use crate::Rereads;

/// The sink as the engine holds it: only the requests that erase or loosen
/// no record. The erasing ones exist only on [`Erasing`], made from this,
/// and the `Sink` inside is reachable from this module alone: a call that
/// skips the slot's check doesn't compile (the seventh re-check, S-1).
/// Every request that writes a record or the marker is stamped by
/// [`KnownRecords`], so this process's own writes are never taken for
/// another's (`known`).
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

    /// Ensure `mapping`'s index; a marker it writes is stamped.
    pub(super) async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<Ensured> {
        self.0.ensure_index_as(mapping, &self.1.stamp()).await
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
    /// version read, stamped (the table above: safe).
    pub(super) async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        self.0.detachment(mapping, &self.1.stamp()).await
    }

    /// The gaps kept with the marker as `decide` says, stamped. A failed
    /// write may have landed the list it was deciding.
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
        let gaps = self
            .0
            .record_gaps(mapping, &deciding, &self.1.stamp())
            .await;
        match &gaps {
            Ok(gaps) => self.1.wrote_gaps(&mapping.index, gaps, true),
            Err(_) => {
                let decided = decided
                    .into_inner()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                self.1
                    .wrote_gaps(&mapping.index, &decided.unwrap_or_default(), false);
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
    /// `result`, of an own write of `mapping`'s record stamped `stamp`, read
    /// back while the slot is still held. A failed call counts as landed
    /// exactly when the record read back carries `stamp` (no other process
    /// writes it): `Ok` with what `landed` reads off it, so the caller goes
    /// on as after a success (the review of the cut, B-2; of round 10,
    /// S-2).
    async fn settled<T>(
        &self,
        mapping: &IndexMapping,
        stamp: &Stamp,
        result: sink::Result<T>,
        landed: impl FnOnce(&Detachment) -> Option<T>,
    ) -> sink::Result<T> {
        self.known
            .wrote_record(&mapping.index, self.rereads.read_begins());
        let found = self.read_back(mapping).await;
        match (result, found) {
            (Err(error), Some(found)) if found.record_stamp.as_ref() == Some(stamp) => {
                match landed(&found) {
                    Some(value) => {
                        tracing::warn!(index = %mapping.index, %error, "the call failed, yet its write landed (its own stamp read back while this process holds the slot): taken as done");
                        Ok(value)
                    }
                    None => Err(error),
                }
            }
            (result, _) => result,
        }
    }

    /// Right after an own write, the records read back while the slot is
    /// still held. No other process writes while this one holds the slot,
    /// so what the read finds is adopted: it learns the record's version
    /// and stamp. A stream that ended meanwhile, or a failed read, leaves
    /// what was known; the next read judges by the stamps (a call in flight
    /// as the stream ends: #490). The records read, when held throughout.
    async fn read_back(&self, mapping: &IndexMapping) -> Option<Detachment> {
        let at = self.rereads.read_begins();
        self.held().ok()?;
        let found = self
            .sink
            .detachment(mapping, &self.known.stamp())
            .await
            .ok()?;
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

    /// [`held`](Self::held), and `index`'s records read under the opening
    /// held now (or, before any opening, at all): a write that may erase a
    /// record stands on records no other process changed since they were
    /// read (the review of the cut). Records read under an earlier stream
    /// are read again here first: the same, the decision made on them still
    /// stands; changed by another process, nothing is written, and the
    /// engine's next read finds the change and stops.
    async fn held_fresh(&self, index: &IndexName) -> Result<(), NotHeld> {
        self.held()?;
        let now = self.rereads.read_begins();
        let fresh = match self.known.learnt_at(index) {
            Some(at) => at == now,
            None => self.rereads.opened() == 0,
        };
        if fresh {
            return Ok(());
        }
        let mapping = self
            .mappings
            .iter()
            .find(|mapping| &mapping.index == index)
            .ok_or(NotHeld)?;
        let found = self
            .sink
            .detachment(mapping, &self.known.stamp())
            .await
            .map_err(|_| NotHeld)?;
        if !(self.rereads.held_or_alone() && self.rereads.read_begins() == now) {
            return Err(NotHeld);
        }
        match self.known.judge(index, &found, now, false) {
            Change::Same => self.held(),
            Change::Foreign(what) => {
                tracing::error!(%index, %what, "another flusso process changed the index's detach records while this one didn't hold the slot: stopping the deployment; no change of this index confirmed past what its records cover");
                Err(NotHeld)
            }
        }
    }

    /// Seed `index`: its marker is written whole, erasing any gap; a
    /// finished rebuild's record is forgotten best effort. Read back while
    /// held, a record the forget left is forgotten again here, as a staging
    /// would (the review of the cut, B-1).
    pub(super) async fn mark_seeded(&self, index: &IndexName) -> Result<sink::Result<()>, NotHeld> {
        self.held_fresh(index).await?;
        let seeded = self.sink.mark_seeded_as(index, &self.known.stamp()).await;
        let Some(mapping) = self.mappings.iter().find(|mapping| &mapping.index == index) else {
            return Ok(seeded);
        };
        self.known.wrote_record(index, self.rereads.read_begins());
        if let Some(found) = self.read_back(mapping).await
            && seeded.is_ok()
            && let (
                Some(DetachRecord::Rebuilding { live_only: false }),
                Some(MarkerAt::Newer { seeded: true }),
                Some(version),
            ) = (&found.record, found.marker, &found.version)
        {
            tracing::warn!(%index, "the seed left the finished rebuild's detach record (its forget failed); forgetting it now");
            let _ = self.forget_finished(mapping, version).await;
        }
        Ok(seeded)
    }

    /// Forget the detach record read at `version`: a tombstone, stamped.
    pub(super) async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held_fresh(&mapping.index).await?;
        let stamp = self.known.stamp();
        let forgotten = self.sink.forget_detachment(mapping, version, &stamp).await;
        Ok(self.settled(mapping, &stamp, forgotten, |_| Some(())).await)
    }

    /// Forget the record of a rebuild a newer seeded generation finished.
    /// Ungated: the record is obsolete, and the tombstone is conditional on
    /// the version read, so any write since fails it.
    pub(super) async fn forget_finished(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        let stamp = self.known.stamp();
        let forgotten = self.sink.forget_detachment(mapping, version, &stamp).await;
        self.settled(mapping, &stamp, forgotten, |_| Some(())).await
    }

    /// Move `mapping`'s index to a new generation; its marker is written
    /// whole, stamped, for the ensure that follows to create its generation.
    pub(super) async fn reindex(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held_fresh(&mapping.index).await?;
        Ok(self.sink.reindex_as(mapping, &self.known.stamp()).await)
    }

    /// [`reindex`](Self::reindex) at a `Fresh` source's staging, ungated by
    /// necessity: it runs before `prepare` creates the slot (#120), so no
    /// slot exists for anyone to hold.
    pub(super) async fn reindex_at_a_fresh_start(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<()> {
        self.sink.reindex_as(mapping, &self.known.stamp()).await
    }

    /// Begin a rebuild: other generations deleted, the marker moved, the
    /// record rewritten into `rebuilding`.
    pub(super) async fn begin_rebuild(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<u64>, NotHeld> {
        self.held_fresh(&mapping.index).await?;
        let stamp = self.known.stamp();
        let begun = self.sink.begin_rebuild(mapping, &stamp).await;
        Ok(self
            .settled(mapping, &stamp, begun, |found| found.attempts)
            .await)
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
        self.held_fresh(&mapping.index).await?;
        let stamp = self.known.stamp();
        let demoted = self.sink.demote(mapping, live_only, &stamp).await;
        Ok(self.settled(mapping, &stamp, demoted, |_| Some(())).await)
    }

    /// Detach `mapping`'s index: the marker moved, the record written, every
    /// generation but the serving one deleted. Gated as
    /// [`demote`](Self::demote) is.
    pub(super) async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &Detach,
    ) -> Result<sink::Result<Recorded>, NotHeld> {
        self.held_fresh(&mapping.index).await?;
        let stamp = self.known.stamp();
        let recorded = self
            .sink
            .detach(mapping, &detach.clone().stamp(stamp.clone()))
            .await;
        let recorded = match recorded {
            // The sink keeps no records: nothing was written.
            Ok(Recorded::NotKept) => Ok(Recorded::NotKept),
            recorded => {
                self.settled(mapping, &stamp, recorded, |_| Some(Recorded::Kept))
                    .await
            }
        };
        Ok(recorded)
    }
}
