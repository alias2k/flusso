use async_trait::async_trait;
use schema_core::{GenericValue, IndexMapping, IndexName};

use crate::Result;

/// A destination for assembled documents.
///
/// The engine calls [`upsert`](Self::upsert) / [`delete`](Self::delete) as it
/// processes changes, then [`flush`](Self::flush) at a commit or batch boundary
/// — so a buffering sink (e.g. OpenSearch bulk) can hold writes until then,
/// while a streaming sink can write immediately and flush cheaply.
///
/// `id` is the document's identifier within the index (the search engine's
/// `_id`); the engine derives it from the document's key.
#[async_trait]
pub trait Sink: std::fmt::Debug + Send + Sync {
    /// Ensure the destination index exists, creating it from `mapping` if it is
    /// absent. The engine calls this once per index at startup, before any
    /// writes, so a sink that owns its index can pin field types up front
    /// instead of letting the destination guess them. The default is a no-op —
    /// correct for sinks with no schema-bound index (e.g. stdout).
    async fn ensure_index(&self, _mapping: &IndexMapping) -> Result<()> {
        Ok(())
    }

    /// Index (insert or replace) `document` under `id` in `index`.
    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()>;

    /// Remove the document `id` from `index`.
    async fn delete(&self, index: &IndexName, id: &str) -> Result<()>;

    /// Flush any buffered writes so everything written so far is durable.
    ///
    /// `caught_up` tells the sink the engine has drained the queue with this
    /// batch — there is no backlog waiting behind it. A sink whose destination
    /// has a cost to making writes *visible* (distinct from durable) can use
    /// this to take that cost only when it's cheap: do it on a caught-up flush
    /// (the pipeline is idle), skip it while a backlog is draining. Sinks with
    /// no such distinction ignore it. See the OpenSearch sink, which forces an
    /// index refresh only when `caught_up`.
    async fn flush(&self, caught_up: bool) -> Result<()>;

    /// Whether `index` has already been seeded — its initial backfill completed
    /// and durably applied here. The engine asks this at startup and skips the
    /// backfill for indexes that report `true`.
    ///
    /// Seeded-state is destination knowledge, so it belongs to the sink: only
    /// the sink knows whether its target already holds the data. The default is
    /// `false` (never seeded) — correct for sinks that can't persist this, which
    /// then re-seed on every run. Sinks that can store it (a metadata document,
    /// a row, a sidecar) should override both methods.
    async fn is_seeded(&self, _: &IndexName) -> Result<bool> {
        Ok(false)
    }

    /// Record that `index` has been seeded, so a later run skips its backfill.
    /// The default is a no-op (paired with `is_seeded` returning `false`).
    async fn mark_seeded(&self, _: &IndexName) -> Result<()> {
        Ok(())
    }
}
