//! Building a batch once and publishing it to the lanes: live batches to
//! every lane, snapshot batches to the lanes that asked.

use std::collections::BTreeMap;

use chrono::Utc;
use futures::future::try_join_all;
use kernel::{Envelope, IndexName, Position, SinkName};
use source::document::{BuildFailure, Document, DocumentId};
use stream::{Batch, LaneItem, Producer};
use tokio::time::Instant;

use crate::error::{EngineError, Result};
use crate::observer::{BuildStats, Stage, StageTiming, Work};
use crate::policy::FailurePolicy;

use super::live::PendingBatch;
use super::snapshot::Snapshot;
use super::{IngestEngine, Lanes, document_id};

impl IngestEngine {
    /// Build the batch's documents once and publish them to every lane with
    /// the batch's position, concurrently, so a full lane delays only its own
    /// copy. A batch that resolved to no document is still published (empty),
    /// so the lanes acknowledge its position.
    #[tracing::instrument(name = "ingest.commit", level = "debug", skip_all, fields(changes = pending.count, documents = pending.ids.len()))]
    pub(super) async fn commit_live(
        &self,
        pending: &mut PendingBatch,
        lanes: &Lanes,
    ) -> Result<()> {
        if pending.count == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&pending.ids, pending.position).await?;
        let built = Instant::now();
        let build = built - started;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, build).work(Work::Live));
        let stats = BuildStats {
            changes: pending.count,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build,
        };
        let item = LaneItem::Batch(Batch {
            position: pending.position,
            changes: pending.count,
            envelopes: envelopes.into(),
        });
        self.publish(
            lanes
                .iter()
                .map(|(sink, producer)| (sink, producer, item.clone())),
            Work::Live,
            built,
        )
        .await?;
        pending.clear();
        self.observer.on_batch_built(stats);
        tracing::debug!("batch built and published");
        Ok(())
    }

    /// Publish each item to its lane, all at once, so a full lane delays only
    /// its own copy. Times each lane ([`Stage::LaneWait`]) and the whole
    /// ([`Stage::Publish`]) from `started`, the end of the build, so cutting
    /// the items is publishing too.
    pub(super) async fn publish<'a>(
        &self,
        items: impl IntoIterator<Item = (&'a SinkName, &'a Box<dyn Producer<LaneItem>>, LaneItem)>,
        work: Work,
        started: Instant,
    ) -> Result<()> {
        try_join_all(items.into_iter().map(|(sink, producer, item)| async move {
            producer.publish(item).await?;
            self.observer.on_stage(
                StageTiming::new(Stage::LaneWait, started.elapsed())
                    .work(work)
                    .sink(sink),
            );
            Ok::<_, EngineError>(())
        }))
        .await?;
        self.observer
            .on_stage(StageTiming::new(Stage::Publish, started.elapsed()).work(work));
        Ok(())
    }

    /// Build `ids` into envelopes tagged with `position`.
    pub(super) async fn build(
        &self,
        ids: &[DocumentId],
        position: Option<Position>,
    ) -> Result<(Vec<Envelope>, BTreeMap<IndexName, usize>)> {
        let mut by_index: BTreeMap<IndexName, usize> = BTreeMap::new();
        let mut envelopes = Vec::with_capacity(ids.len());
        let ts = Utc::now();
        for document in self.documents.build_many(ids).await? {
            let envelope = match document {
                Document::Upsert { id, body } => {
                    Envelope::upsert(id.index.clone(), document_id(&id), body, position, ts)
                }
                Document::Delete { id } => {
                    Envelope::delete(id.index.clone(), document_id(&id), position, ts)
                }
                Document::Failed { id, failure } => {
                    self.document_failed(&id, failure)?;
                    continue;
                }
            };
            *by_index.entry(envelope.index.clone()).or_insert(0) += 1;
            envelopes.push(envelope);
        }
        Ok((envelopes, by_index))
    }

    /// Report a document that could not be built, then apply its policy: `stop`
    /// fails the batch (nothing in it is published, so the change is retried),
    /// `skip` leaves every sink's indexed version alone.
    pub(super) fn document_failed(&self, id: &DocumentId, failure: BuildFailure) -> Result<()> {
        let rendered = document_id(id);
        let policy = self.failure_policies.resolve(id.index.as_ref());
        self.observer
            .on_document_failed(&id.index, &rendered, &failure, policy);
        match policy {
            FailurePolicy::Stop => Err(EngineError::DocumentFailed {
                index: id.index.to_string(),
                id: rendered,
                failure: Box::new(failure),
            }),
            FailurePolicy::Skip => Ok(()),
        }
    }

    /// Build the snapshot batch once and publish each lane the slice of it that
    /// lane requested, with no position.
    pub(super) async fn commit_snapshot(
        &self,
        snapshot: &mut Snapshot,
        lanes: &Lanes,
    ) -> Result<()> {
        let page = std::mem::take(&mut snapshot.page_wait);
        if snapshot.batch.count == 0 && page.is_zero() {
            return Ok(());
        }
        self.observer
            .on_stage(StageTiming::new(Stage::Page, page).work(Work::Backfill));
        if snapshot.batch.count == 0 {
            return Ok(());
        }
        let started = Instant::now();
        let (envelopes, by_index) = self.build(&snapshot.batch.ids, None).await?;
        let built = Instant::now();
        let build = built - started;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, build).work(Work::Backfill));
        // Cutting each lane's slice copies its documents: part of publishing.
        let mut publishes = Vec::with_capacity(snapshot.requested.len());
        for (sink, indexes) in &snapshot.requested {
            let slice: Vec<Envelope> = envelopes
                .iter()
                .filter(|envelope| indexes.contains(&envelope.index))
                .cloned()
                .collect();
            if slice.is_empty() {
                continue;
            }
            let Some(producer) = lanes.get(sink) else {
                tracing::warn!(%sink, "snapshot requested by a sink with no lane; dropping its slice");
                continue;
            };
            publishes.push((
                sink,
                producer,
                LaneItem::Batch(Batch {
                    position: None,
                    changes: 0,
                    envelopes: slice.into(),
                }),
            ));
        }
        self.publish(publishes, Work::Backfill, built).await?;
        self.observer.on_batch_built(BuildStats {
            changes: 0,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build,
        });
        snapshot.batch.clear();
        Ok(())
    }
}
