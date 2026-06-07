use async_trait::async_trait;
use schema_core::{GenericValue, IndexMapping, IndexName, TableName};

use crate::{Result, RowKey, SnapshotTable};

/// Addresses one document in a target index: which index, and the root row's
/// key within it. The same source row can map to documents in several indexes,
/// so the [`index`](Self::index) is part of the identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocumentId {
    pub index: IndexName,
    /// The root table's primary key — the natural identifier of the document.
    pub key: RowKey,
}

/// The result of assembling a document: a body to upsert into the index, or a
/// tombstone when the root row is gone (or soft-deleted).
#[derive(Debug, Clone)]
pub enum Document {
    /// Upsert the assembled body under [`id`](Self::Upsert::id).
    Upsert { id: DocumentId, body: GenericValue },
    /// Remove the document from the index.
    Delete { id: DocumentId },
}

impl Document {
    /// The id this outcome addresses, whichever variant it is.
    pub fn id(&self) -> &DocumentId {
        match self {
            Document::Upsert { id, .. } | Document::Delete { id } => id,
        }
    }
}

/// What an index needs in order to be seeded: its name and the source table to
/// snapshot for it. A document is identified by its root row, so snapshotting
/// the **root table** alone seeds the whole index — `build` pulls in every join
/// and aggregate server-side when each root row is assembled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexScope {
    pub index: IndexName,
    /// The index's root table — the one whose rows map one-to-one to documents.
    pub root: SnapshotTable,
}

/// Turns changed rows into target documents — the read half of a source.
///
/// Used in two steps so the engine can deduplicate between them:
///
/// 1. [`resolve`](Self::resolve) maps a changed row — given only its `table`
///    and `key` — to the ids of every document it affects. A change on a
///    document's own root table resolves to that one id; a change on a
///    *related* table (one folded in by a join or aggregate) is a reverse
///    lookup whose result size is not known until queried.
/// 2. [`build`](Self::build) assembles one document by id — the root row plus
///    its joins and aggregates — or reports it deleted.
///
/// The engine resolves every change in a batch, deduplicates the ids (the same
/// document is often touched by several changes in one transaction), and builds
/// each unique id once.
///
/// Note that `resolve` takes the table and key as plain values rather than a
/// capture event: document construction is independent of how the change was
/// captured.
#[async_trait]
pub trait DocumentBuilder: std::fmt::Debug + Send + Sync {
    /// The documents the changed row affects. Empty if it touches nothing any
    /// index cares about.
    async fn resolve(&self, table: &TableName, key: &RowKey) -> Result<Vec<DocumentId>>;

    /// Assemble one document, or report it deleted if its root row is absent.
    async fn build(&self, id: &DocumentId) -> Result<Document>;

    /// Assemble many documents at once. Returns one [`Document`] per requested
    /// id — an `Upsert`, or a `Delete` tombstone when the root row is absent —
    /// in any order; callers match results back by [`Document::id`].
    ///
    /// The default builds each id independently, so it matches [`build`] one
    /// for one. Sources that can assemble a set in fewer round-trips (e.g. one
    /// `WHERE pk IN (…)` query per index) should override it; the engine builds
    /// a whole batch's deduplicated ids through this in a single call.
    ///
    /// [`build`]: Self::build
    async fn build_many(&self, ids: &[DocumentId]) -> Result<Vec<Document>> {
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            out.push(self.build(id).await?);
        }
        Ok(out)
    }

    /// The enabled indexes this builder serves, each with the root table to
    /// snapshot when seeding it. The engine uses this to scope an initial
    /// backfill per index. The default is empty — a builder with no backfillable
    /// indexes, which the engine simply never seeds.
    fn backfill_scopes(&self) -> Vec<IndexScope> {
        Vec::new()
    }

    /// The resolved mapping of every index this builder serves: each field
    /// typed from the schema's explicit `mapping` where one is given, and from
    /// the source's own column types otherwise. Sinks that own their index use
    /// this to create it up front. The default is empty — a builder that leaves
    /// index creation to whatever the sink does on first write.
    async fn index_mappings(&self) -> Result<Vec<IndexMapping>> {
        Ok(Vec::new())
    }
}
