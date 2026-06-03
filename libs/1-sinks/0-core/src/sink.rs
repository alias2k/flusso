use async_trait::async_trait;
use schema_core::{GenericValue, IndexName};

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
    /// Index (insert or replace) `document` under `id` in `index`.
    async fn upsert(&self, index: &IndexName, id: &str, document: &GenericValue) -> Result<()>;

    /// Remove the document `id` from `index`.
    async fn delete(&self, index: &IndexName, id: &str) -> Result<()>;

    /// Flush any buffered writes so everything written so far is durable.
    async fn flush(&self) -> Result<()>;

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
