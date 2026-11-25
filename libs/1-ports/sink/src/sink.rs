use async_trait::async_trait;
use kernel::{Envelope, IndexMapping, IndexName};

use crate::Result;

/// The outcome of a [`write`](Sink::write): which documents the destination
/// **rejected at the item level**, and when the written ones become searchable.
///
/// This is distinct from a write returning `Err`. An `Err` is a write-wide
/// failure (transport down, the whole request refused) — nothing in the write
/// is known durable, so the sink engine stops and the lane redelivers it. A
/// `WriteReport` instead means the write *succeeded* and the destination applied
/// it, but rejected specific documents (a mapping conflict, a malformed
/// value) while accepting the rest. Those rejections are the document's fault,
/// not the destination's, so retrying redelivers the same poison — the engine
/// handles them per its failure policy (stop, or quarantine and continue)
/// instead of looping. An empty report means everything was written cleanly.
/// Built with [`WriteReport::clean`] and its builders; more fields may come.
///
/// Between the two sit the **index-wide** outcomes, about one index of the
/// write and not its documents (ADR 0012 stage 9): the destination refuses
/// writes to that index for now ([`blocked`](Self::blocked): a write block, a
/// role without write permission on it), or the index is gone
/// ([`missing`](Self::missing)). No document of a named index counts as
/// applied, whatever the destination did with some of them (writing them again
/// is idempotent); the other indexes of the write landed. The engine parks that
/// index's lane, or stages it again, and goes on with the others; these never
/// reach the failure policy.
///
/// ```
/// use kernel::IndexName;
/// use sink::WriteReport;
///
/// let orders = IndexName::try_new("orders").unwrap();
/// let report = WriteReport::clean().blocking(orders.clone(), "index write (api)");
/// assert_eq!(report.blocked[0].index, orders);
/// assert!(!report.is_clean());
/// ```
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct WriteReport {
    /// Documents the destination accepted the write but rejected individually.
    pub rejected: Vec<RejectedDocument>,
    /// Indexes the destination refuses to write to for now: none of their
    /// documents in this write counts as applied, and they are written again
    /// once the refusal is lifted. Never a document's fault.
    pub blocked: Vec<IndexRefusal>,
    /// Indexes whose target is gone (deleted while the sink runs): none of
    /// their documents in this write counts as applied, and the engine stages
    /// the index again ([`Sink::ensure_index`]) before writing them again.
    pub missing: Vec<IndexRefusal>,
    /// When what this write wrote can be *searched*, as opposed to fetched by
    /// id. The sink only reports what its write did; the sink engine turns the
    /// reports into the sink's searchable position.
    pub visibility: Visibility,
    /// Bytes the write sent to the destination, when the sink counts them
    /// (the OpenSearch sink: its `_bulk` bodies). Reported as a metric only.
    pub bytes: Option<u64>,
    /// Requests the write sent that refresh the destination (the OpenSearch
    /// sink: `_refresh` calls and `_bulk` requests with `?refresh=true`).
    /// Reported as a metric only.
    pub refreshes: u64,
}

/// When the documents of one [`write`](Sink::write) become searchable.
///
/// Applied and searchable are different moments for a destination that
/// refreshes on a schedule: a written document is fetched by id at once but
/// matched by a search only after the next refresh. A sink reports which of
/// the three applies to the write it just did; it never refreshes because
/// somebody is waiting.
///
/// ```
/// use sink::{Visibility, WriteReport};
/// use std::time::Duration;
///
/// assert_eq!(WriteReport::clean().visibility, Visibility::Immediate);
/// let lagging = WriteReport::clean().visible(Visibility::Within(Duration::from_secs(10)));
/// assert_eq!(lagging.visibility, Visibility::Within(Duration::from_secs(10)));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    /// Searchable as the write returned. The default: right for a destination
    /// with no such distinction, and for a write that wrote nothing.
    #[default]
    Immediate,
    /// Searchable within this long of the write returning (the destination's
    /// own periodic refresh).
    Within(std::time::Duration),
    /// Not searchable until a later *caught-up* write of this sink returns
    /// (see [`Sink::write`]), which then covers this one too. A sink that
    /// reports this must make that true.
    Deferred,
}

/// What a caught-up write must make searchable beyond its own documents
/// (see [`Sink::write_parts`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaughtUp<'a> {
    /// Nothing: the write forces no refresh.
    No,
    /// Every write of these (logical) indexes that returned before it.
    Indexes(&'a [IndexName]),
    /// Every write of every index that returned before it.
    All,
}

impl CaughtUp<'_> {
    /// Whether the write is caught up at all.
    pub fn is_caught_up(&self) -> bool {
        !matches!(self, CaughtUp::No)
    }

    /// Whether the write must cover earlier writes of the logical `index`.
    pub fn covers(&self, index: &str) -> bool {
        match self {
            CaughtUp::No => false,
            CaughtUp::Indexes(indexes) => indexes.iter().any(|named| named.as_ref() == index),
            CaughtUp::All => true,
        }
    }
}

impl Visibility {
    /// The later of two: what a write holding both parts reports.
    #[must_use]
    pub fn later(self, other: Self) -> Self {
        match (self, other) {
            (Self::Deferred, _) | (_, Self::Deferred) => Self::Deferred,
            (Self::Within(a), Self::Within(b)) => Self::Within(a.max(b)),
            (Self::Within(a), Self::Immediate) | (Self::Immediate, Self::Within(a)) => {
                Self::Within(a)
            }
            (Self::Immediate, Self::Immediate) => Self::Immediate,
        }
    }
}

fn sum_bytes(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
    }
}

impl WriteReport {
    /// A report with no rejections — everything in the write was applied.
    pub fn clean() -> Self {
        Self::default()
    }

    /// Set when the write's documents become searchable.
    #[must_use]
    pub fn visible(mut self, visibility: Visibility) -> Self {
        self.visibility = visibility;
        self
    }

    /// Add documents the destination rejected individually.
    #[must_use]
    pub fn rejecting(mut self, rejected: impl IntoIterator<Item = RejectedDocument>) -> Self {
        self.rejected.extend(rejected);
        self
    }

    /// Report every document of `index` in the write unapplied: the
    /// destination refuses writes to it for now, for `reason`.
    #[must_use]
    pub fn blocking(mut self, index: IndexName, reason: impl Into<String>) -> Self {
        self.blocked.push(IndexRefusal {
            index,
            reason: reason.into(),
        });
        self
    }

    /// Report every document of `index` in the write unapplied: its target is
    /// gone, for `reason`.
    #[must_use]
    pub fn missing_target(mut self, index: IndexName, reason: impl Into<String>) -> Self {
        self.missing.push(IndexRefusal {
            index,
            reason: reason.into(),
        });
        self
    }

    /// Set how many bytes the write sent.
    #[must_use]
    pub fn sent(mut self, bytes: u64) -> Self {
        self.bytes = Some(bytes);
        self
    }

    /// Set how many requests of the write refreshed the destination.
    #[must_use]
    pub fn refreshed(mut self, refreshes: u64) -> Self {
        self.refreshes = refreshes;
        self
    }

    /// Whether the write applied every document: no item-level rejection and
    /// no index blocked or missing.
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty() && self.blocked.is_empty() && self.missing.is_empty()
    }
}

/// One index of a write the destination refused as a whole, with why
/// (see [`WriteReport::blocked`] and [`WriteReport::missing`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRefusal {
    /// The logical index, as the envelopes name it.
    pub index: IndexName,
    /// Why, in the destination's words.
    pub reason: String,
}

/// One document a sink rejected at the item level during a [`write`](Sink::write).
/// The names are the destination's own (e.g. an OpenSearch physical index), for
/// diagnostics and quarantine records.
#[derive(Debug, Clone)]
pub struct RejectedDocument {
    /// The destination index the document was bound for.
    pub index: String,
    /// The document's id within that index (the search engine's `_id`).
    pub id: String,
    /// Why the destination rejected it.
    pub reason: String,
}

/// The universal per-sink keys of a `[sinks.<name>]` table: what every sink
/// engine honors regardless of adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkOptions {
    /// Whether the sink is ever backfilled. `false` makes the sink engine treat
    /// every index as seeded from the start, so the sink only ever receives
    /// live changes — the opt-out for a stateless sink beside a stateful one.
    pub backfill: bool,
}

impl Default for SinkOptions {
    fn default() -> Self {
        Self { backfill: true }
    }
}

/// What [`Sink::ensure_index`] did to an index's target.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Ensured {
    /// Which target the index now writes to, if the sink can name it. Two
    /// answers naming different targets mean the second is not the first:
    /// whatever was written to the first is not in it.
    pub target: Option<TargetId>,
    /// The target was created by this call, empty: whatever was written to
    /// the index before went with a target that is gone. It carries no
    /// checkpoint.
    pub created: bool,
}

impl Ensured {
    /// A target that was already there, holding what was written to it.
    pub fn existing(target: Option<TargetId>) -> Self {
        Self {
            target,
            created: false,
        }
    }

    /// A target this call created, empty.
    pub fn created(target: Option<TargetId>) -> Self {
        Self {
            target,
            created: true,
        }
    }
}

/// Names one target of an index, as the sink tells them apart: a target
/// created anew (after a deletion, or recreated by anyone else) gets another.
/// The OpenSearch sink names a generation by its `index.uuid`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetId(String);

impl TargetId {
    /// The target named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Its name, as the sink wrote it.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TargetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// What [`Sink::seeding`] answers for an index.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Seeding {
    /// Its backfill completed; a start skips it.
    Seeded,
    /// It needs a backfill; `walked` is where the last one durably got, as
    /// the source wrote it, if the sink recorded one.
    Unseeded { walked: Option<String> },
}

/// What a sink keeps about detaching an index from the slot's minimum (ADR
/// 0012 stage 10), as [`Sink::detachment`] read it at staging, before
/// anything else touches the index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Detachment {
    /// The index's detach record, if the sink keeps one.
    pub record: Option<DetachRecord>,
    /// The index's stuck clock, if the sink keeps one.
    pub stuck: Option<StuckClock>,
    /// The seed marker named a generation a detach moved it to, with no
    /// record beside it: the detach died between its two writes. The marker
    /// was put back on this generation before anything else was staged.
    pub restored: Option<u64>,
    /// The seed marker carries a `detaching_from` this version can't read
    /// (why): nothing was put back, and the index is staged as before.
    pub unreadable_marker: Option<String>,
}

impl Detachment {
    /// Nothing kept, and nothing put back.
    pub fn is_empty(&self) -> bool {
        self.record.is_none()
            && self.stuck.is_none()
            && self.restored.is_none()
            && self.unreadable_marker.is_none()
    }
}

/// An index's detach record, as staging read it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DetachRecord {
    /// The index is detached: reads are frozen at the detach and changes
    /// since are not applied. `live_only`: the sink can't rebuild it.
    Detached { live_only: bool },
    /// The index is being rebuilt into a new generation.
    Rebuilding { live_only: bool },
    /// A record this version can't read: another state, or malformed.
    Unreadable { reason: String },
}

/// An index's stuck clock, as staging read it: the times as the sink wrote
/// them.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct StuckClock {
    /// When the lane was first seen stuck.
    pub since: Option<String>,
    /// When the lane was last seen stuck.
    pub seen_at: Option<String>,
}

impl StuckClock {
    /// A clock read as `since` and `seen_at`.
    pub fn new(since: Option<String>, seen_at: Option<String>) -> Self {
        Self { since, seen_at }
    }
}

/// A destination for built documents.
///
/// A sink engine hands each lane item's envelopes to [`write`](Self::write),
/// with several writes in flight at once: the engine never runs two writes
/// holding the same document at the same time, so a sink needs no ordering of
/// its own, only state that is safe to share between concurrent calls. Sinks
/// never build a document: the envelope carries the built document, its id,
/// and its position.
///
/// **Staging is concurrent across indexes.** The sink engine stages a sink's
/// indexes several at a time: [`detachment`](Self::detachment) (which may
/// write: it puts a half-moved seed marker back),
/// [`ensure_index`](Self::ensure_index), [`is_seeded`](Self::is_seeded) and
/// [`reindex`](Self::reindex) may run at once for *different* indexes, never
/// two at once for the same index. They
/// may run beside a [`write`](Self::write) or a
/// [`mark_seeded`](Self::mark_seeded) of **other** indexes (an index whose
/// target went missing is staged again while the others are written), never
/// beside one holding the same index: a write never holds a document of an
/// index being staged.
///
/// **Seeding is concurrent across indexes.** [`mark_seeded`](Self::mark_seeded)
/// may run at once for different indexes, and beside `write` for other
/// indexes; never beside a write, a `mark_seeded` or staging of the same
/// index, and never beside staging. State a sink shares between indexes, such
/// as one metadata store, must take concurrent calls for different indexes.
#[async_trait]
pub trait Sink: std::fmt::Debug + Send + Sync {
    /// Ensure the destination index exists, creating it from `mapping` if it is
    /// absent. The sink engine calls this once per index at startup, before any
    /// writes, so a sink that owns its index can pin field types up front
    /// instead of letting the destination guess them. The default is a no-op —
    /// correct for sinks with no schema-bound index (e.g. stdout).
    ///
    /// This is also where a sink reconciles its seed marker with reality: a
    /// marker that says "seeded" for an index that had to be created here is
    /// wrong, and must be retracted so [`is_seeded`](Self::is_seeded) reports
    /// `false`. The engine always asks `is_seeded` *after* `ensure_index`, so
    /// this is the one place that has both facts. Never let a marker outlive
    /// the data it describes — a recreated-empty index that claims to be seeded
    /// is served, silently, with nothing in it.
    ///
    /// The answer names the target the index now writes to
    /// ([`Ensured::target`]) and says whether this call created it. The
    /// engine keeps the target an index's backfill was asked for under; a
    /// later answer naming another means the pages that backfill wrote are
    /// not there, so it can't seed the index and a fresh backfill is asked
    /// for. That holds even when the call that created the new target failed
    /// on a later step: the next one names it. So a sink that can name its
    /// targets should, and must give a target created anew a new name, also
    /// one recreated by anyone else (an auto-create, a restore). A created
    /// target carries no checkpoint ([`record_walked`](Self::record_walked)).
    /// The default names none and creates nothing.
    async fn ensure_index(&self, _mapping: &IndexMapping) -> Result<Ensured> {
        Ok(Ensured::default())
    }

    /// Write `envelopes`: index each upserted document, remove each deleted
    /// one, or forward them for an emitting sink. Several calls may run at
    /// once; no two of them hold the same document.
    ///
    /// `caught_up` tells the sink the engine has nothing else queued behind
    /// this write. A sink whose destination has a cost to making writes
    /// *searchable* (distinct from durable) pays it only then: a caught-up
    /// write makes searchable everything written by every write that
    /// **returned before this call**, as well as its own documents, and
    /// reports [`Visibility::Immediate`]. The engine sends an empty caught-up
    /// write when it goes idle with written documents not yet searchable, so
    /// the last writes of a burst, which may finish after the last caught-up
    /// one, don't wait for the next traffic. Sinks with no such distinction
    /// ignore it. See the OpenSearch sink.
    ///
    /// Returns a [`WriteReport`]: `Ok` with an empty report means every
    /// document was applied; `Ok` with rejections means the write succeeded
    /// but the destination refused specific documents (see [`WriteReport`] for
    /// why that differs from an `Err`). A document the destination refuses for
    /// now (busy) is retried inside the write and never reported; still
    /// refused after the sink's retries, it fails the write. `Err` is
    /// write-wide: nothing in the write is known applied, and the lane
    /// redelivers it to the restarted engine, which writes it again whole: a
    /// failure of the destination or the client (unreachable, credentials
    /// refused, the whole request refused). A refusal of one *index* is a
    /// report, not an `Err`: an index closed to writes for now is
    /// [`blocked`](WriteReport::blocked), one whose target is gone (deleted
    /// under the sink) is [`missing`](WriteReport::missing), and the other
    /// indexes of the write still land.
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> Result<WriteReport>;

    /// Write several lane items as **one** write: the engine merges items that
    /// are ready at once (ADR 0012 stage 7), so a burst over many indexes goes
    /// out in requests of about a batch, not one per index. `parts` are the
    /// items' envelopes, each part in its own order; parts never share a
    /// document. Everything [`write`](Self::write) promises holds for the whole
    /// call, as one write: a rejection names its document's index, a blocked
    /// or missing index names every part of that index, an `Err` fails every
    /// part.
    ///
    /// `caught_up` says what the write must make searchable beyond its own
    /// documents, when it returns `Immediate`: every write of the indexes it
    /// names that returned before the call ([`CaughtUp::Indexes`]: the
    /// engine's lanes with nothing else waiting, one index each), of every
    /// index ([`CaughtUp::All`], what [`write`](Self::write)'s `true` means),
    /// or nothing ([`CaughtUp::No`]). So a trickle on one index refreshes
    /// that index, not another index's backlog.
    ///
    /// The default writes each part with [`write`](Self::write), in order,
    /// passing whether it is caught up to the last only (the earlier ones have
    /// returned before it, so it covers them; a scope is widened to every
    /// index, which covers more than asked), and folds the reports:
    /// rejections together, bytes summed, visibility the latest of them, or
    /// `Immediate` when the caught-up last part reported it. Correct for any
    /// sink; a sink that can send several parts in one request (OpenSearch's
    /// `_bulk`), or refresh one index, overrides it.
    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        caught_up: CaughtUp<'_>,
    ) -> Result<WriteReport> {
        let caught_up = caught_up.is_caught_up();
        let Some((last, earlier)) = parts.split_last() else {
            return self.write(&[], caught_up).await;
        };
        let mut folded = WriteReport::clean();
        let mut bytes: Option<u64> = None;
        for part in earlier {
            let report = self.write(part, false).await?;
            folded.visibility = folded.visibility.later(report.visibility);
            bytes = sum_bytes(bytes, report.bytes);
            folded.refreshes += report.refreshes;
            folded.rejected.extend(report.rejected);
            folded.blocked.extend(report.blocked);
            folded.missing.extend(report.missing);
        }
        let report = self.write(last, caught_up).await?;
        folded.visibility = if caught_up && report.visibility == Visibility::Immediate {
            Visibility::Immediate
        } else {
            folded.visibility.later(report.visibility)
        };
        folded.bytes = sum_bytes(bytes, report.bytes);
        folded.refreshes += report.refreshes;
        folded.rejected.extend(report.rejected);
        folded.blocked.extend(report.blocked);
        folded.missing.extend(report.missing);
        Ok(folded)
    }

    /// The most documents one write should hold, if the sink splits larger
    /// ones into several requests (the OpenSearch sink's `batch_size`). The
    /// engine merges items only up to it, so a merged write is not then cut
    /// into a full request and a small one. `None`: no limit of the sink's
    /// own.
    fn write_limit(&self) -> Option<usize> {
        None
    }

    /// Whether `index` has already been seeded — its initial backfill completed
    /// and durably applied here. The sink engine asks this at startup, after
    /// [`ensure_index`](Self::ensure_index), and requests a snapshot only for
    /// indexes that report `false`.
    ///
    /// Seeded-state is destination knowledge, so it belongs to the sink: only
    /// the sink knows whether its target already holds the data. The default is
    /// `false` (never seeded) — correct for sinks that can't persist this, which
    /// then re-seed on every run unless configured with `backfill = false`.
    /// Sinks that can store it (a metadata document, a row, a sidecar) should
    /// override both methods.
    ///
    /// What the sink can't know is whether the *source* still remembers the
    /// changes since that seed. The daemon checks that separately
    /// ([`ChangeCapture::continuity`](https://docs.rs/flusso-source)) and,
    /// when the source's resume point is gone, treats every `true` here as
    /// stale and calls [`reindex`](Self::reindex) on it.
    async fn is_seeded(&self, _: &IndexName) -> Result<bool> {
        Ok(false)
    }

    /// What this sink keeps about detaching `mapping`'s index, read at the
    /// start of every staging step, before anything else touches the index.
    /// A seed marker a detach left half moved, with no record beside it, is
    /// put back first. Only "nothing kept" reads as empty: any other failure
    /// to read is an `Err`, so a record is never missed. The default keeps
    /// nothing.
    async fn detachment(&self, _: &IndexMapping) -> Result<Detachment> {
        Ok(Detachment::default())
    }

    /// What staging needs of `index`: seeded, or, when not, where its last
    /// backfill durably got, if this sink recorded it
    /// ([`record_walked`](Self::record_walked)). The default reads
    /// [`is_seeded`](Self::is_seeded) and records nothing, so a backfill
    /// starts from the first key.
    async fn seeding(&self, index: &IndexName) -> Result<Seeding> {
        Ok(if self.is_seeded(index).await? {
            Seeding::Seeded
        } else {
            Seeding::Unseeded { walked: None }
        })
    }

    /// Whether reads of `index` see another copy than the one this sink now
    /// writes: a rebuild is under way (a reindex, or a rebuild after the
    /// source lost its resume point), and readers stay on the old copy until
    /// [`mark_seeded`](Self::mark_seeded) swaps. Asked while staging, after
    /// [`ensure_index`](Self::ensure_index), for an unseeded index: a process
    /// that starts in the middle of a rebuild says so, and a fence then vouches
    /// for nothing of that index until it is seeded. The default, `false`, is
    /// right for a sink whose reads see what it writes.
    async fn is_rebuilding(&self, _: &IndexName) -> Result<bool> {
        Ok(false)
    }

    /// Remember that `index`'s backfill has landed every document up to
    /// `mark` (opaque text the source wrote), so the next start resumes after
    /// it. Only the index's current, unseeded target holds it: seeding it, a
    /// reindex, or a target created anew forgets it. A failure costs only
    /// redone work; the caller logs it and goes on. The default stores
    /// nothing.
    async fn record_walked(&self, _: &IndexName, _mark: &str) -> Result<()> {
        Ok(())
    }

    /// Forget where `index`'s backfill got, so the next one starts from the
    /// first key: the source lost the changes since (a fresh replication
    /// slot). Called while staging, before the source prepares. The default
    /// has nothing to forget.
    async fn forget_walked(&self, _: &IndexName) -> Result<()> {
        Ok(())
    }

    /// Record that `index` has been seeded, so a later run skips its backfill.
    /// The default is a no-op (paired with `is_seeded` returning `false`).
    ///
    /// Called once the index's backfill is written, possibly while other
    /// indexes are written or seeded (see the trait's doc on concurrency).
    /// A failure is retried by calling it again, so it must be idempotent.
    ///
    /// A target that is no longer the one [`ensure_index`](Self::ensure_index)
    /// named (deleted, or recreated by anyone else) must not be seeded:
    /// answer [`SinkError::TargetChanged`](crate::SinkError::TargetChanged),
    /// and the engine stages the index again and backfills it in full.
    ///
    /// A new target that is built but can't take over because the destination
    /// blocks retiring the old one answers
    /// [`SinkError::RetireBlocked`](crate::SinkError::RetireBlocked) with
    /// nothing applied: the engine parks the index's lane on it and calls this
    /// again, as is, on the lane's probe, until it succeeds (#479).
    async fn mark_seeded(&self, _: &IndexName) -> Result<()> {
        Ok(())
    }

    /// Request a from-scratch rebuild of `index` on the next backfill: mark it
    /// unseeded so [`is_seeded`](Self::is_seeded) reports `false` again, *without*
    /// disturbing what currently serves reads. A sink that builds into a fresh,
    /// swappable target (e.g. the OpenSearch sink's per-generation indexes behind
    /// a stable alias) prepares that target here so the seeding path rebuilds it
    /// and atomically swaps on completion — the live copy is untouched until then.
    ///
    /// An index that is already unseeded has a from-scratch rebuild pending, so a
    /// sink may treat the call as a no-op rather than stage a second target.
    ///
    /// This only flips the seeded-state and stages the target; the actual reseed
    /// runs through the normal [`ensure_index`](Self::ensure_index) → backfill →
    /// [`mark_seeded`](Self::mark_seeded) path. The default is a no-op (correct
    /// for sinks that re-seed every run anyway). Takes the full [`IndexMapping`]
    /// (not just the name) so a sink can stage the reindex without having run
    /// [`ensure_index`](Self::ensure_index) — it needs the schema hash to address
    /// the index, not the running engine's state.
    async fn reindex(&self, _: &IndexMapping) -> Result<()> {
        Ok(())
    }

    /// Whether the destination is a stream its readers consume in order (an
    /// emitting sink, such as stdout): the engine then hands it lane items in
    /// exact lane order, one write at a time, whatever `[concurrency] writes`
    /// says, with no live item overtaking an earlier backfill page. Order is
    /// **per lane** (one index): across lanes, live items go in position order
    /// while every lane is written, but a lane parked on a refusal, a block or
    /// a missing target (see [`WriteReport`]) is overtaken by the other lanes'
    /// later items, and its own follow once it opens. A sink that never reports
    /// one (stdout) never parks a lane. The
    /// default `false` lets the engine write several items at once and order
    /// them per document only, which a destination keyed by document id (a
    /// search index) is indifferent to.
    fn ordered(&self) -> bool {
        false
    }
}
