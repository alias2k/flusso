//! The sink engine: one per sink. Stage the sink's indexes, request whatever
//! backfill the sink needs, then receive → apply → flush → acknowledge.
//!
//! Staging ([`SinkEngine::stage`]) is the sink's own seeding decision: ensure
//! every index, retire seeds the source can no longer honor (a `Fresh`
//! continuity), and send one `Backfill` request for the indexes still
//! unseeded. The daemon runs every sink engine's staging before it lets the
//! ingest engine `prepare` the source, which preserves the #120 ordering
//! without a shared engine: rebuilds are staged before the resume point exists,
//! and the resume point exists before any snapshot runs.
//!
//! The loop ([`SinkEngine::run`]) applies each batch's envelopes, flushes once
//! at the batch boundary, decides item-level rejections by the failure
//! policies, and acknowledges: flush-then-ack is what makes delivery
//! at-least-once, because the acknowledgement is what moves this lane's
//! watermark. A `SnapshotComplete` marker records its indexes as seeded. A
//! reindex arrives as a control message and is staged between two batches:
//! `reindex` + `ensure_index` on the sink, then a fresh `Backfill` request.

use std::sync::Arc;

use kernel::{IndexMapping, IndexName, SinkName};
use sink::{Sink, SinkOptions};
use source::cdc::Continuity;
use stream::{LaneItem, Request, Stream};
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::error::{EngineError, Result};
use crate::observer::{CommitStats, EngineId, NoopObserver, Observer};
use crate::policy::{FailurePolicies, FailurePolicy};

/// An operation the daemon hands a running sink engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinkControl {
    /// Rebuild `indexes` into fresh generations: stage them and request a
    /// snapshot. Handled between two batches; nothing restarts.
    Reindex { indexes: Vec<IndexName> },
}

/// One sink's engine over its lane.
#[derive(Debug)]
pub struct SinkEngine {
    name: SinkName,
    sink: Arc<dyn Sink>,
    stream: Arc<dyn Stream>,
    mappings: Vec<IndexMapping>,
    options: SinkOptions,
    observer: Arc<dyn Observer>,
    failure_policies: FailurePolicies,
    skip_backfill: bool,
}

impl SinkEngine {
    /// An engine for `sink`, named `name` in the config, over its lane on
    /// `stream`, maintaining `mappings`.
    pub fn new(
        name: SinkName,
        sink: Arc<dyn Sink>,
        stream: Arc<dyn Stream>,
        mappings: Vec<IndexMapping>,
    ) -> Self {
        Self {
            name,
            sink,
            stream,
            mappings,
            options: SinkOptions::default(),
            observer: Arc::new(NoopObserver),
            failure_policies: FailurePolicies::default(),
            skip_backfill: false,
        }
    }

    pub fn with_options(mut self, options: SinkOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = observer;
        self
    }

    pub fn with_failure_policies(mut self, policies: FailurePolicies) -> Self {
        self.failure_policies = policies;
        self
    }

    pub fn skip_backfill(mut self, skip: bool) -> Self {
        self.skip_backfill = skip;
        self
    }

    /// The sink this engine drives.
    pub fn name(&self) -> &SinkName {
        &self.name
    }

    /// Ensure every index, retire stale seeds under a `Fresh` source, and
    /// request a backfill for what is still unseeded. Idempotent; the daemon
    /// runs it before the ingest engine prepares the source, and again before
    /// each restart of this engine (with `Resumed`).
    #[tracing::instrument(name = "sink.stage", skip_all, fields(sink = %self.name, ?continuity))]
    pub async fn stage(&self, continuity: Continuity) -> Result<()> {
        for mapping in &self.mappings {
            self.sink.ensure_index(mapping).await?;
        }
        self.observer
            .on_indexes_ensured(&self.name, self.mappings.len());

        if !self.options.backfill {
            tracing::info!("backfill disabled for this sink; live changes only");
            self.observer.on_sink_started(&self.name);
            return Ok(());
        }

        if continuity == Continuity::Fresh {
            self.rebuild_stale_seeds().await?;
        }

        let mut unseeded = Vec::new();
        for mapping in &self.mappings {
            if !self.sink.is_seeded(&mapping.index).await? {
                unseeded.push(mapping.index.clone());
            }
        }
        if self.skip_backfill {
            if !unseeded.is_empty() {
                tracing::warn!(
                    indexes = ?unseeded.iter().map(|i| i.as_ref()).collect::<Vec<_>>(),
                    "skipping backfill (skip_backfill set); these indexes stay unseeded"
                );
            }
        } else if !unseeded.is_empty() {
            self.request_backfill(unseeded).await?;
        } else {
            tracing::info!("every index is seeded; no backfill needed");
        }
        self.observer.on_sink_started(&self.name);
        Ok(())
    }

    /// The source has no resume point, so every seed this sink recorded is
    /// stale: the changes between that seed and now are unobservable. Each
    /// index still reported as seeded gets a from-scratch rebuild staged (a
    /// fresh target, so rows gone from the source are dropped on the swap) and
    /// re-announced, so the backfill that follows reseeds it while the old copy
    /// keeps serving. Indexes already unseeded are left alone: they are about
    /// to be rebuilt anyway, and staging again would orphan the target already
    /// in flight. Under `skip_backfill` nothing is staged: a staged target
    /// nobody backfills would take the live writes while the old copy keeps
    /// serving reads.
    async fn rebuild_stale_seeds(&self) -> Result<()> {
        let mut stale: Vec<&IndexMapping> = Vec::new();
        for mapping in &self.mappings {
            if self.sink.is_seeded(&mapping.index).await? {
                stale.push(mapping);
            }
        }
        if stale.is_empty() {
            return Ok(());
        }
        let names: Vec<&str> = stale.iter().map(|m| m.index.as_ref()).collect();
        tracing::warn!(
            indexes = ?names,
            skip_backfill = self.skip_backfill,
            "the source has no resume point, so the changes since these indexes were seeded are \
             lost; rebuilding them from scratch (with skip_backfill they are served as-is)",
        );
        if self.skip_backfill {
            return Ok(());
        }
        for mapping in stale {
            self.sink.reindex(mapping).await?;
            self.sink.ensure_index(mapping).await?;
        }
        Ok(())
    }

    async fn request_backfill(&self, indexes: Vec<IndexName>) -> Result<()> {
        tracing::info!(indexes = indexes.len(), "requesting backfill");
        let producer = self.stream.requests()?.producer;
        producer
            .publish(Request::Backfill {
                sink: self.name.clone(),
                indexes: indexes.clone(),
            })
            .await?;
        self.observer.on_backfill_requested(&self.name, &indexes);
        Ok(())
    }

    /// Follow the lane until it closes (`Ok`) or a sink, stream, or `stop`
    /// policy error stops the engine (`Err`). Control messages arriving on
    /// `control` are handled between batches.
    #[tracing::instrument(name = "sink.run", skip_all, fields(sink = %self.name))]
    pub async fn run(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let result = self.run_inner(control).await;
        let id = EngineId::Sink(self.name.clone());
        match &result {
            Ok(()) => {
                tracing::info!("sink engine stopped: lane closed");
                self.observer.on_engine_stopped(&id);
            }
            Err(error) => {
                tracing::error!(%error, "sink engine stopped on error");
                self.observer.on_engine_error(&id, &error.to_string());
            }
        }
        result
    }

    async fn run_inner(&self, control: &mut mpsc::Receiver<SinkControl>) -> Result<()> {
        let mut lane = self.stream.lane(&self.name)?.consumer;
        let mut control_open = true;
        loop {
            tokio::select! {
                delivery = lane.recv() => match delivery? {
                    None => return Ok(()),
                    Some(delivery) => {
                        let (item, handle) = delivery.into_parts();
                        match item {
                            LaneItem::Batch(batch) => {
                                self.commit(&batch, lane.is_empty()).await?;
                            }
                            LaneItem::SnapshotComplete { indexes } => {
                                for index in &indexes {
                                    self.sink.mark_seeded(index).await?;
                                    self.observer.on_index_seeded(&self.name, index);
                                }
                                tracing::info!(indexes = indexes.len(), "backfill complete");
                            }
                        }
                        handle.ack().await?;
                    }
                },
                message = control.recv(), if control_open => match message {
                    None => control_open = false,
                    Some(SinkControl::Reindex { indexes }) => self.reindex(indexes).await?,
                },
            }
        }
    }

    /// Apply every envelope, flush once, decide rejections by policy. The
    /// caller acknowledges only after this returns `Ok`: a crash before the
    /// flush leaves the batch unacknowledged and redelivered, re-applied
    /// idempotently; a `stop` rejection returns `Err` with the batch still
    /// unacknowledged.
    #[tracing::instrument(name = "sink.commit", level = "debug", skip_all, fields(envelopes = batch.envelopes.len(), caught_up))]
    async fn commit(&self, batch: &stream::Batch, caught_up: bool) -> Result<()> {
        for envelope in &batch.envelopes {
            self.sink.apply(envelope).await?;
        }
        let flush_start = Instant::now();
        let report = self.sink.flush(caught_up).await?;
        let flush = flush_start.elapsed();

        // A single `stop`-policy rejection stops the run, decided *before* any
        // quarantine event is emitted, so a `skip` document in the same batch
        // isn't double-counted when the unacknowledged batch is redelivered.
        if !report.is_clean() {
            let mut stop_count = 0usize;
            let mut stop_example = String::new();
            for doc in &report.rejected {
                if self.failure_policies.resolve(&doc.index) == FailurePolicy::Stop {
                    if stop_count == 0 {
                        stop_example = format!("{}/{}: {}", doc.index, doc.id, doc.reason);
                    }
                    stop_count += 1;
                }
            }
            if stop_count > 0 {
                return Err(EngineError::DocumentsRejected(stop_count, stop_example));
            }
            for doc in &report.rejected {
                tracing::warn!(
                    index = %doc.index,
                    id = %doc.id,
                    reason = %doc.reason,
                    "document rejected by sink; quarantining and continuing",
                );
                self.observer
                    .on_document_quarantined(&self.name, &doc.index, &doc.id, &doc.reason);
            }
        }

        self.observer.on_batch_committed(
            &self.name,
            CommitStats {
                envelopes: batch.envelopes.len(),
                changes: batch.changes,
                flush,
            },
        );
        Ok(())
    }

    /// Stage a rebuild of `indexes` and request their snapshot. The current
    /// generation keeps serving until the snapshot completes and the sink swaps.
    async fn reindex(&self, indexes: Vec<IndexName>) -> Result<()> {
        let mut staged = Vec::new();
        for index in indexes {
            let Some(mapping) = self.mappings.iter().find(|m| m.index == index) else {
                tracing::warn!(%index, "reindex requested for an index this sink does not maintain; ignoring");
                continue;
            };
            tracing::info!(%index, "reindex requested; staging a fresh generation");
            self.sink.reindex(mapping).await?;
            self.sink.ensure_index(mapping).await?;
            staged.push(index);
        }
        if !staged.is_empty() {
            self.request_backfill(staged).await?;
        }
        Ok(())
    }
}
