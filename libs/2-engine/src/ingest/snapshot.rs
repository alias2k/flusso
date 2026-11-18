//! Serving backfill requests: coalescing them into one snapshot, walking
//! each index's root keys in pages, and completing it.

use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::future::{BoxFuture, try_join_all};
use kernel::{IndexName, SinkName};
use source::RowKey;
use source::document::{DocumentBuilder, DocumentId};
use stream::{AckHandle, Consumer, LaneItem, Request};
use tokio::time::{Duration, Instant, timeout_at};

use crate::error::Result;

use super::live::PendingBatch;
use super::{IngestEngine, Lanes};

/// How many root keys a backfill reads per page. The keys are built in
/// batches of [`BatchPolicy::max_changes`](crate::BatchPolicy::max_changes).
pub const KEY_PAGE: usize = 1024;

impl IngestEngine {
    /// Gather the requests one snapshot will serve: everything already queued
    /// behind `first`, then, only while some lane has not asked yet, stragglers
    /// for up to `max_delay`. Returns as soon as every lane has asked, so a
    /// single-sink backfill starts at once; a lane that never asks (a seeded
    /// sibling) costs one `max_delay`. A straggler past the window simply gets
    /// its own snapshot.
    pub(super) async fn collect_requests(
        &self,
        first: stream::Delivery<Request>,
        requests: &mut Box<dyn Consumer<Request>>,
        lanes: &Lanes,
    ) -> Result<Vec<stream::Delivery<Request>>> {
        let mut deliveries = vec![first];
        let deadline = Instant::now() + self.batch.max_delay;
        loop {
            let asked: HashSet<&SinkName> = deliveries
                .iter()
                .map(|delivery| {
                    let Request::Backfill { sink, .. } = delivery.item();
                    sink
                })
                .collect();
            if lanes.keys().all(|lane| asked.contains(lane)) {
                break;
            }
            let next = if requests.is_empty() {
                match timeout_at(deadline, requests.recv()).await {
                    Ok(received) => received?,
                    Err(_) => break,
                }
            } else {
                requests.recv().await?
            };
            match next {
                Some(more) => deliveries.push(more),
                None => break,
            }
        }
        Ok(deliveries)
    }

    /// Coalesce the delivered requests into one snapshot: the union of their
    /// indexes, each walked once, remembering which lane asked for which.
    /// Requests for indexes the builder knows nothing about are answered with
    /// an immediate `SnapshotComplete` for whatever *is* known.
    pub(super) async fn start_snapshot(
        &self,
        deliveries: Vec<stream::Delivery<Request>>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
    ) -> Result<Option<Snapshot>> {
        let mut requested: BTreeMap<SinkName, BTreeSet<IndexName>> = BTreeMap::new();
        let mut handles = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let (request, handle) = delivery.into_parts();
            let Request::Backfill { sink, indexes } = request;
            let known: BTreeSet<IndexName> = indexes
                .into_iter()
                .filter(|index| {
                    let known = scopes.contains(index);
                    if !known {
                        tracing::warn!(index = %index, %sink, "backfill requested for an index the source does not build; ignoring");
                    }
                    known
                })
                .collect();
            requested.entry(sink).or_default().extend(known);
            handles.push(handle);
        }
        let indexes: BTreeSet<IndexName> = requested.values().flatten().cloned().collect();
        let mut snapshot = Snapshot {
            walk: Walk::new(Arc::clone(&self.documents), indexes.iter().cloned()),
            requested,
            indexes: indexes.iter().cloned().collect(),
            handles,
            batch: PendingBatch::new(self.batch.max_changes),
            page_wait: Duration::ZERO,
            done: false,
        };
        if indexes.is_empty() {
            self.finish_snapshot(&mut snapshot, lanes).await?;
            return Ok(None);
        }
        let ordered: Vec<IndexName> = indexes.into_iter().collect();
        tracing::info!(
            indexes = ordered.len(),
            lanes = snapshot.requested.len(),
            "starting snapshot"
        );
        self.observer.on_snapshot_started(&ordered);
        Ok(Some(snapshot))
    }

    /// Publish `SnapshotComplete` to every requesting lane, then acknowledge
    /// the requests: a crash before this point redelivers them.
    pub(super) async fn finish_snapshot(
        &self,
        snapshot: &mut Snapshot,
        lanes: &Lanes,
    ) -> Result<()> {
        let markers = snapshot.requested.iter().filter_map(|(sink, indexes)| {
            lanes.get(sink).map(|producer| {
                producer.publish(LaneItem::SnapshotComplete {
                    indexes: indexes.iter().cloned().collect(),
                })
            })
        });
        try_join_all(markers).await?;
        for handle in snapshot.handles.drain(..) {
            handle.ack().await?;
        }
        let indexes: Vec<IndexName> = snapshot.indexes.iter().cloned().collect();
        tracing::info!(indexes = indexes.len(), "snapshot complete");
        self.advances.fetch_add(1, Ordering::Relaxed);
        self.observer.on_snapshot_completed(&indexes);
        Ok(())
    }
}

/// A running snapshot: the walk over its indexes' keys, who asked for what,
/// the request handles to acknowledge at the end, and the batch being filled.
pub(super) struct Snapshot {
    pub(super) walk: Walk,
    pub(super) requested: BTreeMap<SinkName, BTreeSet<IndexName>>,
    pub(super) indexes: HashSet<IndexName>,
    pub(super) handles: Vec<Box<dyn AckHandle>>,
    pub(super) batch: PendingBatch,
    /// Time spent getting keys since the last batch: waiting for pages and
    /// buffering keys.
    pub(super) page_wait: Duration,
    /// The walk has read every index's keys; what is left is to build the
    /// batch in hand and complete.
    pub(super) done: bool,
}

impl Snapshot {
    /// Whether the loop should read keys for it: its walk goes on and its
    /// batch has room. A full batch waits for a build slot.
    pub(super) fn wants_keys(&self) -> bool {
        !self.done && !self.batch.is_full()
    }

    /// Buffer the keys in hand as their index's documents, until the batch is
    /// full. A key with a null part addresses no document.
    pub(super) fn take_keys(&mut self) {
        let Some(index) = self.walk.indexes.front() else {
            return;
        };
        while !self.batch.is_full() {
            let Some(key) = self.walk.keys.pop_front() else {
                break;
            };
            if key.has_null() {
                self.batch.count += 1;
                continue;
            }
            self.batch.add([DocumentId {
                index: index.clone(),
                key,
            }]);
        }
    }
}

/// A snapshot's walk over its indexes' root keys, one index after another,
/// one page at a time ([`DocumentBuilder::keys`]). The page being read is
/// kept here, not in the `select!` that polls it, so a live change winning
/// the `select!` doesn't drop a page half read.
pub(super) struct Walk {
    pub(super) documents: Arc<dyn DocumentBuilder>,
    /// The indexes still to walk, the one being walked first.
    pub(super) indexes: VecDeque<IndexName>,
    /// Where the index being walked ends: its greatest key when its walk
    /// started (`DocumentBuilder::last_key`). `None` until read.
    pub(super) through: Option<RowKey>,
    /// The last key read of the index being walked: the next page follows it.
    pub(super) after: Option<RowKey>,
    /// Keys read and not yet buffered.
    pub(super) keys: VecDeque<RowKey>,
    /// Whether the keys in hand are the index's last.
    pub(super) last: bool,
    /// The read in flight: the walk's bound, then its pages.
    pub(super) fetch: Option<BoxFuture<'static, source::Result<Fetched>>>,
}

/// What a walk's read brings back.
pub(super) enum Fetched {
    /// The index's greatest key when its walk starts; `None` for none.
    Through(Option<RowKey>),
    Page(Vec<RowKey>),
}

/// What the walk has for the loop.
pub(super) enum Step {
    /// Keys in hand, from the front index.
    Keys,
    /// Every index walked.
    Done,
}

impl Walk {
    pub(super) fn new(
        documents: Arc<dyn DocumentBuilder>,
        indexes: impl IntoIterator<Item = IndexName>,
    ) -> Self {
        Self {
            documents,
            indexes: indexes.into_iter().collect(),
            through: None,
            after: None,
            keys: VecDeque::new(),
            last: false,
            fetch: None,
        }
    }

    /// The next step: keys in hand at once, else the next read: the front
    /// index's bound when its walk starts, then its pages up to that bound.
    pub(super) async fn step(&mut self) -> source::Result<Step> {
        loop {
            if !self.keys.is_empty() {
                return Ok(Step::Keys);
            }
            if self.last {
                self.indexes.pop_front();
                self.through = None;
                self.after = None;
                self.last = false;
            }
            let Some(index) = self.indexes.front() else {
                return Ok(Step::Done);
            };
            let fetch =
                self.fetch.get_or_insert_with(|| {
                    let documents = Arc::clone(&self.documents);
                    let index = index.clone();
                    match &self.through {
                        None => Box::pin(async move {
                            documents.last_key(&index).await.map(Fetched::Through)
                        }),
                        Some(through) => {
                            let (after, through) = (self.after.clone(), through.clone());
                            Box::pin(async move {
                                documents
                                    .keys(&index, after.as_ref(), &through, KEY_PAGE)
                                    .await
                                    .map(Fetched::Page)
                            })
                        }
                    }
                });
            let fetched = fetch.await;
            self.fetch = None;
            match fetched? {
                Fetched::Through(None) => self.last = true,
                Fetched::Through(Some(through)) => self.through = Some(through),
                Fetched::Page(keys) => {
                    self.last = keys.len() < KEY_PAGE;
                    self.after = keys.last().cloned().or(self.after.take());
                    self.keys.extend(keys);
                }
            }
        }
    }
}

pub(super) async fn next_snapshot_step(snapshot: &mut Option<Snapshot>) -> source::Result<Step> {
    match snapshot {
        Some(active) => active.walk.step().await,
        None => std::future::pending().await,
    }
}

/// Stands in for the request consumer once the request lane has closed, so the
/// select loop keeps serving live changes instead of spinning on `None`.
#[derive(Debug)]
pub(super) struct ClosedConsumer;

#[async_trait::async_trait]
impl Consumer<Request> for ClosedConsumer {
    async fn recv(&mut self) -> stream::Result<Option<stream::Delivery<Request>>> {
        std::future::pending().await
    }

    fn is_empty(&self) -> bool {
        true
    }
}
