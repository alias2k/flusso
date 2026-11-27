//! The one way the sink engine makes a request that may erase, or loosen, a
//! record another process may rely on (ADR 0012 stage 10, O29 as the owner
//! narrowed it on 2026-11-26): this process destroys or rewrites no such
//! record unless it holds the slot.
//!
//! Each gated method checks [`Rereads::may_erase_records`] immediately before
//! the request is sent, after every await that precedes it in its caller, so
//! a stream that ended while the caller read, ensured or waited is seen
//! (the sixth re-check, B-1). What stays open is a request already sent when
//! the stream ends: an owner lease closes that (#490).
//!
//! The engine holds its sink only as [`Plain`], whose `Sink` this module
//! alone can reach: it exposes the requests that erase nothing, and makes
//! [`Erasing`]. A call of an erasing request anywhere else in the engine
//! doesn't compile, however it is spelled (the seventh re-check, S-1).
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
//! | `demote` (every generation but the serving one deleted with its checkpoint, the marker and the record rewritten) | gated (the sixth re-check, S-2): what it deletes may be the rebuild the holder walks. Staging and the read again take the lane as detached and leave its read uncounted, so the read once the slot is held demotes |
//! | `detach` (the marker moved, the record written, every generation but the serving one deleted), staging's `DetachAgain` | gated, as `demote`: a refused lane stays as it was, staging takes the lane as detached for the read once held |
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
    CaughtUp, DecideGaps, Detach, Detachment, Ensured, Gap, RecordVersion, Recorded, Seeding, Sink,
    WriteReport,
};

use crate::Rereads;

/// The sink as the engine holds it: only the requests that erase or loosen
/// no record. The erasing ones exist only on [`Erasing`], made from this,
/// and the `Sink` inside is reachable from this module alone: a call that
/// skips the slot's check doesn't compile (the seventh re-check, S-1).
#[derive(Debug, Clone)]
pub(super) struct Plain(Arc<dyn Sink>);

impl Plain {
    pub(super) fn new(sink: Arc<dyn Sink>) -> Self {
        Self(sink)
    }

    /// The erasing requests, each checked against `rereads`' slot right
    /// before its sink call.
    pub(super) fn erasing<'a>(&'a self, rereads: &'a Rereads) -> Erasing<'a> {
        Erasing {
            sink: self.0.as_ref(),
            rereads,
        }
    }

    pub(super) async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<Ensured> {
        self.0.ensure_index(mapping).await
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

    pub(super) async fn record_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &DecideGaps<'_>,
    ) -> sink::Result<Vec<Gap>> {
        self.0.record_gaps(mapping, decide).await
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
}

impl std::fmt::Debug for Erasing<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Erasing").finish_non_exhaustive()
    }
}

impl Erasing<'_> {
    fn held(&self) -> Result<(), NotHeld> {
        if self.rereads.may_erase_records() {
            Ok(())
        } else {
            Err(NotHeld)
        }
    }

    /// Seed `index`: its marker is written whole, erasing any record.
    pub(super) async fn mark_seeded(&self, index: &IndexName) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.mark_seeded(index).await)
    }

    /// Delete the detach record read at `version`.
    pub(super) async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.forget_detachment(mapping, version).await)
    }

    /// Delete the record of a rebuild a newer seeded generation finished.
    /// Ungated: the record is obsolete, and the delete is conditional on the
    /// version read, so any write since fails it.
    pub(super) async fn forget_finished(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
    ) -> sink::Result<()> {
        self.sink.forget_detachment(mapping, version).await
    }

    /// Move `mapping`'s index to a new generation; its marker is written
    /// whole.
    pub(super) async fn reindex(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<()>, NotHeld> {
        self.held()?;
        Ok(self.sink.reindex(mapping).await)
    }

    /// [`reindex`](Self::reindex) at a `Fresh` source's staging, ungated by
    /// necessity: it runs before `prepare` creates the slot (#120), so no
    /// slot exists for anyone to hold.
    pub(super) async fn reindex_at_a_fresh_start(
        &self,
        mapping: &IndexMapping,
    ) -> sink::Result<()> {
        self.sink.reindex(mapping).await
    }

    /// Begin a rebuild: other generations deleted, the marker moved, the
    /// record rewritten into `rebuilding`.
    pub(super) async fn begin_rebuild(
        &self,
        mapping: &IndexMapping,
    ) -> Result<sink::Result<u64>, NotHeld> {
        self.held()?;
        Ok(self.sink.begin_rebuild(mapping).await)
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
        Ok(self.sink.demote(mapping, live_only).await)
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
        Ok(self.sink.detach(mapping, detach).await)
    }
}
