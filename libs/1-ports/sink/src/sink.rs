use async_trait::async_trait;
use kernel::{Envelope, IndexMapping, IndexName};

use crate::Result;

/// The outcome of a [`flush`](Sink::flush): which buffered documents the
/// destination **rejected at the item level**.
///
/// This is distinct from a flush returning `Err`. An `Err` is a flush-wide
/// failure (transport down, the whole request refused) — nothing in the batch
/// is known durable, so the sink engine stops and the batch is redelivered. A
/// `FlushReport` instead means the flush *succeeded* and the destination applied
/// the batch, but rejected specific documents (a mapping conflict, a malformed
/// value) while accepting the rest. Those rejections are the document's fault,
/// not the destination's, so retrying redelivers the same poison — the engine
/// handles them per its failure policy (stop, or quarantine and continue)
/// instead of looping. An empty report means everything flushed cleanly.
#[derive(Debug, Clone, Default)]
pub struct FlushReport {
    /// Documents the destination accepted the batch but rejected individually.
    pub rejected: Vec<RejectedDocument>,
}

impl FlushReport {
    /// A report with no rejections — everything in the flush was applied.
    pub fn clean() -> Self {
        Self::default()
    }

    /// Whether the flush applied every buffered document (no item-level
    /// rejections).
    pub fn is_clean(&self) -> bool {
        self.rejected.is_empty()
    }
}

/// One document a sink rejected at the item level during a [`flush`](Sink::flush).
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

/// A destination for built documents.
///
/// A sink engine calls [`apply`](Self::apply) for each envelope of a batch,
/// then [`flush`](Self::flush) once at the batch boundary — so a buffering sink
/// (e.g. OpenSearch bulk) can hold writes until then, while a streaming sink
/// can write immediately and flush cheaply. Sinks never build a document: the
/// envelope carries the built document, its id, and its position.
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
    async fn ensure_index(&self, _mapping: &IndexMapping) -> Result<()> {
        Ok(())
    }

    /// Apply one envelope: index the document on an upsert, remove it on a
    /// delete. An emitting sink forwards the envelope as-is.
    async fn apply(&self, envelope: &Envelope) -> Result<()>;

    /// Flush any buffered writes so everything applied so far is durable.
    ///
    /// `caught_up` tells the sink the engine has drained its lane with this
    /// batch — there is no backlog waiting behind it. A sink whose destination
    /// has a cost to making writes *visible* (distinct from durable) can use
    /// this to take that cost only when it's cheap: do it on a caught-up flush
    /// (the lane is idle), skip it while a backlog is draining. Sinks with no
    /// such distinction ignore it. See the OpenSearch sink, which forces an
    /// index refresh only when `caught_up`.
    ///
    /// Returns a [`FlushReport`]: `Ok` with an empty report means every buffered
    /// document was applied; `Ok` with rejections means the flush succeeded but
    /// the destination refused specific documents (see [`FlushReport`] for why
    /// that differs from an `Err`). `Err` is a flush-wide failure.
    async fn flush(&self, caught_up: bool) -> Result<FlushReport>;

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

    /// Record that `index` has been seeded, so a later run skips its backfill.
    /// The default is a no-op (paired with `is_seeded` returning `false`).
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
}
