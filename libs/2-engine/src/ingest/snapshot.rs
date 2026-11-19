//! Serving backfill requests: coalescing them into one snapshot, walking
//! each index's root keys in pages, and completing it.
//!
//! **A resumed walk** (ADR 0012 stage 8b). A request may carry where its
//! lane's last walk durably got (`Request::Backfill.after`). The walk of an
//! index starts after the least of its lanes' points and is cut into
//! segments at the others, in the source's own key order
//! ([`DocumentBuilder::compare_keys`]): a lane receives the walk's batches
//! from the segment after its point on, so a sink that already holds 90% of
//! the table isn't sent it again. A batch that straddles a segment's end goes
//! to the lanes of its last key: a lane joining there gets a few documents it
//! holds already, never one fewer.
//!
//! ```text
//! lanes   A (no point)   B after k₁   C after k₂
//! walk    (·, k₁] → A    (k₁, k₂] → A, B    (k₂, last] → A, B, C
//! ```
//!
//! **Tickets.** Every item a lane receives from a snapshot, its batches and
//! its `SnapshotComplete`, names the greatest [`Ticket`] of the lane's
//! requests the snapshot serves. A lane that asked twice before the snapshot
//! started (its sink engine asked again after its target was created anew)
//! has both answered by one walk, which started after the second request: it
//! names the second, and seeds.

use std::cmp::Ordering as KeyOrder;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::future::{BoxFuture, try_join_all};
use kernel::{IndexName, SinkName};
use source::RowKey;
use source::document::{DocumentBuilder, DocumentId};
use stream::{AckHandle, Consumer, LaneItem, LaneKey, Request, Ticket};
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
            // Requests come one per index: once every sink asked, what is
            // already queued still joins (a sink's other indexes).
            let everyone = lanes.keys().all(|lane| asked.contains(&lane.sink));
            if everyone && requests.is_empty() {
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
        let mut requested: BTreeMap<SinkName, BTreeMap<IndexName, Ticket>> = BTreeMap::new();
        let mut points: BTreeMap<IndexName, Vec<(SinkName, Option<stream::WalkMark>)>> =
            BTreeMap::new();
        let mut handles = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let (request, handle) = delivery.into_parts();
            let Request::Backfill {
                index,
                sink,
                after,
                ticket,
            } = request;
            if scopes.contains(&index) {
                let answered = requested
                    .entry(sink.clone())
                    .or_default()
                    .entry(index.clone())
                    .or_default();
                *answered = (*answered).max(ticket);
                points.entry(index).or_default().push((sink, after));
            } else {
                tracing::warn!(index = %index, %sink, "backfill requested for an index the source does not build; ignoring");
            }
            handles.push(handle);
        }
        let indexes: BTreeSet<IndexName> = requested
            .values()
            .flat_map(BTreeMap::keys)
            .cloned()
            .collect();
        let mut plans = HashMap::with_capacity(points.len());
        for (index, points) in points {
            let plan = plan_walk(self.documents.as_ref(), &index, points).await;
            plans.insert(index, plan);
        }
        let mut snapshot = Snapshot {
            walk: Walk::new(Arc::clone(&self.documents), indexes.iter().cloned()).planned(plans),
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
        let markers = snapshot.requested.iter().flat_map(|(sink, indexes)| {
            indexes.iter().filter_map(|(index, ticket)| {
                lanes
                    .get(&LaneKey::new(index.clone(), sink.clone()))
                    .map(|producer| {
                        producer.publish(LaneItem::SnapshotComplete {
                            index: index.clone(),
                            ticket: *ticket,
                        })
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

/// A running snapshot: the walk over its indexes' keys, who asked for what
/// (each lane with the greatest ticket it is answered under), the request
/// handles to acknowledge at the end, and the batch being filled.
pub(super) struct Snapshot {
    pub(super) walk: Walk,
    pub(super) requested: BTreeMap<SinkName, BTreeMap<IndexName, Ticket>>,
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
    /// The lanes a batch built now goes to: each requested index to the
    /// lanes its walk has reached, with the ticket each lane is answered
    /// under.
    pub(super) fn lanes_now(&self) -> BTreeMap<SinkName, BTreeMap<IndexName, Ticket>> {
        self.requested
            .iter()
            .map(|(sink, indexes)| {
                let reached = indexes
                    .iter()
                    .filter(|(index, _)| self.walk.reaches(index, sink))
                    .map(|(index, ticket)| (index.clone(), *ticket))
                    .collect();
                (sink.clone(), reached)
            })
            .filter(|(_, indexes): &(SinkName, BTreeMap<IndexName, Ticket>)| !indexes.is_empty())
            .collect()
    }

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
    /// Per index, where its walk starts and which lanes join where; an
    /// index with none walks from the first key for every lane.
    pub(super) plans: HashMap<IndexName, WalkPlan>,
    /// Whether the front index's walk has started (its start set).
    started: bool,
    /// Lanes whose point the walk has read up to: they join once the keys
    /// still in hand (all at or before their point) are taken.
    joining: BTreeSet<SinkName>,
}

/// Where one index's walk starts and which lanes it serves from where.
#[derive(Debug, Default)]
pub(super) struct WalkPlan {
    /// The key the walk starts after; `None` for the first key.
    start: Option<RowKey>,
    /// The lanes the walk serves now.
    active: BTreeSet<SinkName>,
    /// The points further lanes join at, in key order: a segment ends at
    /// each, and its lanes are served from there on.
    joins: VecDeque<(RowKey, BTreeSet<SinkName>)>,
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
            plans: HashMap::new(),
            started: false,
            joining: BTreeSet::new(),
        }
    }

    /// The same walk, with each index's start and lanes' points.
    pub(super) fn planned(mut self, plans: HashMap<IndexName, WalkPlan>) -> Self {
        self.plans = plans;
        self
    }

    /// Whether the walk of `index` serves `sink`'s lane now: from its start
    /// with no plan, else once the walk passed the lane's point.
    pub(super) fn reaches(&self, index: &IndexName, sink: &SinkName) -> bool {
        self.plans
            .get(index)
            .is_none_or(|plan| plan.active.contains(sink))
    }

    /// The next step: keys in hand at once, else the next read: the front
    /// index's bound when its walk starts, then its pages up to that bound.
    pub(super) async fn step(&mut self) -> source::Result<Step> {
        loop {
            if !self.keys.is_empty() {
                return Ok(Step::Keys);
            }
            if !self.joining.is_empty() {
                let joining = std::mem::take(&mut self.joining);
                if let Some(plan) = self
                    .indexes
                    .front()
                    .and_then(|index| self.plans.get_mut(index))
                {
                    plan.active.extend(joining);
                }
            }
            if self.last {
                if let Some(done) = self.indexes.pop_front()
                    && let Some(plan) = self.plans.get_mut(&done)
                {
                    let joining: Vec<SinkName> =
                        plan.joins.drain(..).flat_map(|(_, sinks)| sinks).collect();
                    plan.active.extend(joining);
                }
                self.through = None;
                self.after = None;
                self.last = false;
                self.started = false;
            }
            let Some(index) = self.indexes.front() else {
                return Ok(Step::Done);
            };
            if !self.started {
                self.started = true;
                self.after = self.plans.get(index).and_then(|plan| plan.start.clone());
            }
            // The segment ends at the next lane's point, else at the bound.
            let segment_end = self
                .plans
                .get(index)
                .and_then(|plan| plan.joins.front())
                .map(|(point, _)| point.clone());
            let fetch =
                self.fetch.get_or_insert_with(|| {
                    let documents = Arc::clone(&self.documents);
                    let index = index.clone();
                    match &self.through {
                        None => Box::pin(async move {
                            documents.last_key(&index).await.map(Fetched::Through)
                        }),
                        Some(through) => {
                            let after = self.after.clone();
                            let through = segment_end.clone().unwrap_or_else(|| through.clone());
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
                    let short = keys.len() < KEY_PAGE;
                    self.after = keys.last().cloned().or(self.after.take());
                    self.keys.extend(keys);
                    // A short page ends the segment: the lanes whose point
                    // it reached join, and the next segment follows the
                    // point itself (every key up to it is read).
                    let joined = short
                        .then(|| {
                            let index = self.indexes.front()?;
                            self.plans.get_mut(index)?.joins.pop_front()
                        })
                        .flatten();
                    match joined {
                        Some((point, sinks)) => {
                            self.joining.extend(sinks);
                            self.after = Some(point);
                        }
                        None => self.last = short,
                    }
                }
            }
        }
    }
}

/// Where `index`'s walk starts, and which lanes join where, from each
/// requesting lane's recorded point: the least point starts it, a lane with
/// none starts it from the first key, and every other lane joins at its own
/// point. Points are read back and ordered by the source; one it no longer
/// accepts, or an order it can't tell, falls back to the first key for that
/// lane, never past a key the lane lacks.
async fn plan_walk(
    documents: &dyn DocumentBuilder,
    index: &IndexName,
    points: Vec<(SinkName, Option<stream::WalkMark>)>,
) -> WalkPlan {
    let mut from_first: BTreeSet<SinkName> = BTreeSet::new();
    let mut marked: Vec<(RowKey, SinkName)> = Vec::new();
    for (sink, after) in points {
        let key = match after {
            None => None,
            Some(mark) => match documents.resume_after(index, mark.as_str()).await {
                Ok(key) => key,
                Err(error) => {
                    tracing::warn!(%index, %sink, %error, "could not read the recorded backfill checkpoint; this lane's backfill starts from the first key");
                    None
                }
            },
        };
        match key {
            Some(key) => marked.push((key, sink)),
            None => {
                from_first.insert(sink);
            }
        }
    }
    let mut groups: Vec<(RowKey, BTreeSet<SinkName>)> = Vec::new();
    for (key, sink) in marked {
        match place(documents, index, &groups, &key).await {
            Some(Placed::Equal(at)) => {
                if let Some((_, sinks)) = groups.get_mut(at) {
                    sinks.insert(sink);
                }
            }
            Some(Placed::Before(at)) => groups.insert(at, (key, BTreeSet::from([sink]))),
            None => {
                tracing::warn!(%index, %sink, "the source can't order this lane's checkpoint against the others'; its backfill starts from the first key");
                from_first.insert(sink);
            }
        }
    }
    for (_, sinks) in &groups {
        for sink in sinks {
            tracing::info!(%index, %sink, "resuming this lane's backfill after the point its last walk durably got to");
        }
    }
    let mut joins: VecDeque<(RowKey, BTreeSet<SinkName>)> = groups.into();
    let (start, active) = if from_first.is_empty() {
        match joins.pop_front() {
            Some((start, sinks)) => (Some(start), sinks),
            None => (None, BTreeSet::new()),
        }
    } else {
        (None, from_first)
    };
    WalkPlan {
        start,
        active,
        joins,
    }
}

/// Where `key` goes among `groups` (in key order): at a group equal to it,
/// or before the group at an index. `None` when the source can't order it.
enum Placed {
    Equal(usize),
    Before(usize),
}

async fn place(
    documents: &dyn DocumentBuilder,
    index: &IndexName,
    groups: &[(RowKey, BTreeSet<SinkName>)],
    key: &RowKey,
) -> Option<Placed> {
    for (at, (point, _)) in groups.iter().enumerate() {
        match documents.compare_keys(index, key, point).await {
            Ok(Some(KeyOrder::Less)) => return Some(Placed::Before(at)),
            Ok(Some(KeyOrder::Equal)) => return Some(Placed::Equal(at)),
            Ok(Some(KeyOrder::Greater)) => {}
            Ok(None) | Err(_) => return None,
        }
    }
    Some(Placed::Before(groups.len()))
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
