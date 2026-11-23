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
//!
//! **Key reads with walks at once** (ADR 0012 stage 8a, 6b-3, K1). Every
//! walk shares one key-read permit, taken inside the read from before its
//! query until it returns. With two or more builders each index walks on its
//! own, and a builder may stop stepping its walk mid-read (it awaits a
//! publish on a full lane, its batch waits on the budget): so each read runs
//! on a task of its own ([`Walk::reading_apart`]), finishes without its walk
//! being stepped, gives the permit back, and keeps its page until the walk
//! takes it. The task is aborted when the walk is dropped (a stop, a failed
//! or aborted run), so none outlives it by more than one poll (`abort` is
//! asynchronous: the task is dropped at its next poll, a read awaiting the
//! database at its next await). One builder reads inline, as before.
//! A walk reads no page ahead: per walk at most one page (`KEY_PAGE` keys) is
//! held, in hand or in a finished read. Planning a resumed walk (reading
//! the lanes' recorded points back and ordering them) takes the permit too,
//! on the builder's task, for as long as its reads last.
//!
//! ```text
//! orders' walk: read ──permit──▶ page (kept on its task)   builder blocked
//! users' walk:        wait ─────▶ read ──permit──▶ page ──▶ batch ──▶ …
//! ```

use std::cmp::Ordering as KeyOrder;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use futures::future::{BoxFuture, try_join_all};
use kernel::{IndexName, SinkName};
use source::document::{DocumentBuilder, DocumentId};
use source::{RowKey, SourceError};
use stream::{AckHandle, Consumer, LaneItem, LaneKey, Request, Ticket};
use tokio::sync::Semaphore;
use tokio::time::Duration;

use crate::error::Result;

use super::live::PendingBatch;
use super::{IngestEngine, Lanes};

/// How many root keys a backfill reads per page. The keys are built in
/// batches of [`BatchPolicy::max_changes`](crate::BatchPolicy::max_changes).
pub const KEY_PAGE: usize = 1024;

impl IngestEngine {
    /// Coalesce the delivered requests into one snapshot: the union of their
    /// indexes, each walked once, remembering which lane asked for which.
    /// Requests for indexes the builder knows nothing about are answered with
    /// an immediate `SnapshotComplete` for whatever *is* known.
    ///
    /// With `apart` (two or more builders walk at once) each key read runs
    /// on a task of its own.
    pub(super) async fn start_snapshot(
        &self,
        deliveries: Vec<stream::Delivery<Request>>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
        apart: bool,
    ) -> Result<Option<Snapshot>> {
        let mut requested: BTreeMap<SinkName, BTreeMap<IndexName, Ticket>> = BTreeMap::new();
        let mut points: BTreeMap<IndexName, Vec<(SinkName, Option<stream::WalkMark>)>> =
            BTreeMap::new();
        let mut handles = Vec::with_capacity(deliveries.len());
        for delivery in deliveries {
            let (request, handle) = delivery.into_parts();
            // Only backfills are gathered: fresh builds reach the builder on
            // their own channel.
            let Request::Backfill {
                index,
                sink,
                after,
                ticket,
            } = request.clone()
            else {
                drop(handle);
                continue;
            };
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
            handles.push((request, handle));
        }
        let indexes: BTreeSet<IndexName> = requested
            .values()
            .flat_map(BTreeMap::keys)
            .cloned()
            .collect();
        let mut plans = HashMap::with_capacity(points.len());
        for (index, points) in points {
            // Reading the recorded points back and ordering them are key
            // reads on the builder's pool: under K1 like the walk's own, so
            // walks at once plan one read at a time (review of #453).
            let _reading = if points.iter().any(|(_, after)| after.is_some()) {
                self.key_reads.acquire().await.ok()
            } else {
                None
            };
            let plan = plan_walk(self.documents.as_ref(), &index, points).await;
            plans.insert(index, plan);
        }
        let walk = Walk::new(
            Arc::clone(&self.documents),
            indexes.iter().cloned(),
            Arc::clone(&self.key_reads),
        )
        .planned(plans);
        // What `/status` shows of each index's walk (slice 7), until the
        // snapshot is dropped: completed, stopped or failed.
        let walking = (!indexes.is_empty()).then(|| {
            self.index_states.walking(indexes.iter().map(|index| {
                let sinks = requested
                    .iter()
                    .filter(|(_, asked)| asked.contains_key(index))
                    .map(|(sink, _)| sink.clone())
                    .collect();
                (index.clone(), sinks)
            }))
        });
        let mut snapshot = Snapshot {
            walk: if apart { walk.reading_apart() } else { walk },
            _walking: walking,
            requested,
            indexes: indexes.iter().cloned().collect(),
            handles,
            batch: PendingBatch::new(self.batch.max_changes),
            page_wait: Duration::ZERO,
            done: false,
            reached: Reached::default(),
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
                    .get_key_value(&LaneKey::new(index.clone(), sink.clone()))
                    .map(|(lane, producer)| {
                        self.publish_to(
                            lane,
                            producer.as_ref(),
                            LaneItem::SnapshotComplete {
                                index: index.clone(),
                                ticket: *ticket,
                            },
                        )
                    })
            })
        });
        try_join_all(markers).await?;
        for (_, handle) in snapshot.handles.drain(..) {
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
/// requests to acknowledge at the end, and the batch being filled.
pub(super) struct Snapshot {
    pub(super) walk: Walk,
    pub(super) requested: BTreeMap<SinkName, BTreeMap<IndexName, Ticket>>,
    pub(super) indexes: HashSet<IndexName>,
    /// Each request it serves, with its handle: acknowledged at the end, or
    /// handed back whole to a builder that restarts (slice 8).
    pub(super) handles: Vec<(Request, Box<dyn AckHandle>)>,
    pub(super) batch: PendingBatch,
    /// Time spent getting keys since the last batch: waiting for pages and
    /// buffering keys.
    pub(super) page_wait: Duration,
    /// The walk has read every index's keys; what is left is to build the
    /// batch in hand and complete.
    pub(super) done: bool,
    /// Shows its walks in `/status` while it lives.
    _walking: Option<crate::index_states::Walking>,
    /// The last walk mark published to each lane: everything up to it is on
    /// the lane, so a builder that restarts resumes each lane's request
    /// after it (slice 8, review of #456).
    pub(super) reached: Reached,
}

/// The last walk mark a snapshot published to each lane, shared with its
/// batches' slots, which record it as each is published.
pub(super) type Reached = Arc<std::sync::Mutex<BTreeMap<LaneKey, stream::WalkMark>>>;

impl Snapshot {
    /// Its requests, unacknowledged, for a builder that restarts to serve
    /// again (slice 8): dropping the snapshot stops its walk.
    /// Each request resumes after the last mark the walk published to its
    /// lane: a lane receives a walk's batches from its own point on, so
    /// that mark is past its point, and every batch up to it is on the lane
    /// and lands (the stream redelivers what isn't acknowledged).
    pub(super) fn into_requests(mut self) -> Vec<stream::Delivery<Request>> {
        let reached = std::mem::take(
            &mut *self
                .reached
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
        std::mem::take(&mut self.handles)
            .into_iter()
            .map(|(request, handle)| {
                let Request::Backfill {
                    index,
                    sink,
                    after,
                    ticket,
                } = request
                else {
                    return stream::Delivery::new(request, handle);
                };
                let after = reached
                    .get(&LaneKey::new(index.clone(), sink.clone()))
                    .cloned()
                    .or(after);
                stream::Delivery::new(
                    Request::Backfill {
                        index,
                        sink,
                        after,
                        ticket,
                    },
                    handle,
                )
            })
            .collect()
    }

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
    /// The read in flight: the walk's bound, then its pages. It holds the
    /// key-read permit while it runs (K1), so a dropped walk gives it back.
    fetch: Option<Read>,
    /// The one key-read permit every walk shares (ADR 0012 stage 8a, K1):
    /// one page or bound read at a time, as `main` read its one page.
    key_reads: Arc<Semaphore>,
    /// Per index, where its walk starts and which lanes join where; an
    /// index with none walks from the first key for every lane.
    pub(super) plans: HashMap<IndexName, WalkPlan>,
    /// Whether the front index's walk has started (its start set).
    started: bool,
    /// Lanes whose point the walk has read up to: they join once the keys
    /// still in hand (all at or before their point) are taken.
    joining: BTreeSet<SinkName>,
    /// Each read runs on a task of its own (two or more builders walk at
    /// once): K1 with several walks.
    apart: bool,
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

/// A walk's read in flight: polled where the walk is stepped, or on a task
/// of its own.
enum Read {
    Inline(BoxFuture<'static, source::Result<Fetched>>),
    Apart(KeyRead),
}

/// A read on its own task, aborted when dropped: a walk dropped mid-read
/// (a stop, a failed or aborted run) leaves no task behind.
struct KeyRead(tokio::task::JoinHandle<source::Result<Fetched>>);

impl Drop for KeyRead {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl Read {
    /// What the read brought back. Cancel-safe: dropped before it is ready,
    /// the read goes on and is asked again. A read that panicked on its own
    /// task (two or more builders) is a failed read, its index's failure, as
    /// a build that panics on its task is (slice 8); one run inline panics
    /// the run's task.
    async fn result(&mut self) -> source::Result<Fetched> {
        match self {
            Read::Inline(read) => read.await,
            Read::Apart(KeyRead(task)) => match task.await {
                Ok(fetched) => fetched,
                Err(error) => Err(SourceError::Query(if error.is_panic() {
                    "the key read panicked".to_owned()
                } else {
                    format!("the key read was cancelled: {error}")
                })),
            },
        }
    }
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
        key_reads: Arc<Semaphore>,
    ) -> Self {
        Self {
            documents,
            indexes: indexes.into_iter().collect(),
            through: None,
            after: None,
            keys: VecDeque::new(),
            last: false,
            fetch: None,
            key_reads,
            plans: HashMap::new(),
            started: false,
            joining: BTreeSet::new(),
            apart: false,
        }
    }

    /// The same walk, each read run on a task of its own once it pends, so a
    /// walk its builder stops stepping still finishes its read and gives the
    /// key-read permit back (two or more builders walk at once, 6b-3).
    pub(super) fn reading_apart(mut self) -> Self {
        self.apart = true;
        self
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
            let fetch = self.fetch.get_or_insert_with(|| {
                let documents = Arc::clone(&self.documents);
                let index = index.clone();
                let key_reads = Arc::clone(&self.key_reads);
                let read: BoxFuture<'static, source::Result<Fetched>> = match &self.through {
                    None => Box::pin(async move {
                        let _read = key_reads.acquire_owned().await;
                        documents.last_key(&index).await.map(Fetched::Through)
                    }),
                    Some(through) => {
                        let after = self.after.clone();
                        let through = segment_end.clone().unwrap_or_else(|| through.clone());
                        Box::pin(async move {
                            let _read = key_reads.acquire_owned().await;
                            documents
                                .keys(&index, after.as_ref(), &through, KEY_PAGE)
                                .await
                                .map(Fetched::Page)
                        })
                    }
                };
                if self.apart {
                    Read::Apart(KeyRead(tokio::spawn(read)))
                } else {
                    Read::Inline(read)
                }
            });
            let fetched = fetch.result().await;
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::sync::atomic::AtomicUsize;

    use async_trait::async_trait;
    use kernel::{ColumnName, GenericValue, TableName};
    use source::RowImage;
    use source::document::Document;

    use super::*;

    /// Keys `0..rows` of every index, each read taking 10 ms, counting how
    /// many reads run at once.
    #[derive(Debug, Default)]
    struct Counted {
        now: AtomicUsize,
        most: AtomicUsize,
    }

    fn key(id: i64) -> RowKey {
        RowKey(vec![(
            ColumnName::try_new("id").unwrap(),
            GenericValue::BigInt(id),
        )])
    }

    impl Counted {
        async fn read<T>(&self, value: T) -> source::Result<T> {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(10)).await;
            self.now.fetch_sub(1, Ordering::SeqCst);
            Ok(value)
        }
    }

    #[async_trait]
    impl DocumentBuilder for Counted {
        async fn resolve(
            &self,
            _: &TableName,
            _: &RowKey,
            _: Option<&RowImage>,
            _: Option<&RowImage>,
        ) -> source::Result<Vec<DocumentId>> {
            Ok(Vec::new())
        }

        async fn build(&self, id: &DocumentId) -> source::Result<Document> {
            Ok(Document::Delete { id: id.clone() })
        }

        fn backfill_indexes(&self) -> Vec<IndexName> {
            Vec::new()
        }

        async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
            self.read(Some(key(3 * KEY_PAGE as i64))).await
        }

        async fn keys(
            &self,
            _: &IndexName,
            after: Option<&RowKey>,
            _: &RowKey,
            limit: usize,
        ) -> source::Result<Vec<RowKey>> {
            let start = after.map_or(0, |after| match after.0.first() {
                Some((_, GenericValue::BigInt(id))) => *id + 1,
                _ => 0,
            });
            let end = (start + limit as i64).min(3 * KEY_PAGE as i64 + 1);
            self.read((start..end).map(key).collect()).await
        }
    }

    /// K1: walks sharing the key-read permit read one page or bound at a
    /// time, as `main` read its one page; both still walk to the end. Read
    /// inline and each on a task of its own alike.
    #[tokio::test(start_paused = true)]
    async fn walks_read_one_key_page_at_a_time() {
        walk_two_sharing_one_permit(false).await;
        walk_two_sharing_one_permit(true).await;
    }

    async fn walk_two_sharing_one_permit(apart: bool) {
        let documents = Arc::new(Counted::default());
        let key_reads = Arc::new(Semaphore::new(1));
        let walk = |name: &str| {
            let walk = Walk::new(
                Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
                [IndexName::try_new(name).unwrap()],
                Arc::clone(&key_reads),
            );
            if apart { walk.reading_apart() } else { walk }
        };
        let drive = |mut walk: Walk| async move {
            let mut read = 0;
            loop {
                match walk.step().await.unwrap() {
                    Step::Keys => read += std::mem::take(&mut walk.keys).len(),
                    Step::Done => return read,
                }
            }
        };
        let (users, orders) = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::join!(drive(walk("users")), drive(walk("orders")))
        })
        .await
        .unwrap();
        assert_eq!((users, orders), (3 * KEY_PAGE + 1, 3 * KEY_PAGE + 1));
        assert_eq!(documents.most.load(Ordering::SeqCst), 1);
    }

    async fn drive(mut walk: Walk) -> usize {
        let mut read = 0;
        loop {
            match walk.step().await.unwrap() {
                Step::Keys => read += std::mem::take(&mut walk.keys).len(),
                Step::Done => return read,
            }
        }
    }

    /// K1 with several walks (6b-3): `orders`' walk is stepped once, its
    /// first read starting with the key-read permit, then its builder stops
    /// stepping it (awaiting a publish on a full lane, say). `users`' walk
    /// still reads every key, and `orders`', stepped again, loses none.
    #[tokio::test(start_paused = true)]
    async fn one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads() {
        let documents = Arc::new(Counted::default());
        let key_reads = Arc::new(Semaphore::new(1));
        let walk = |name: &str| {
            Walk::new(
                Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
                [IndexName::try_new(name).unwrap()],
                Arc::clone(&key_reads),
            )
            .reading_apart()
        };
        let mut stuck = walk("orders");
        assert!(
            tokio::time::timeout(Duration::from_millis(1), stuck.step())
                .await
                .is_err(),
            "setup: its first read is in progress"
        );
        let users = tokio::time::timeout(Duration::from_secs(60), drive(walk("users")))
            .await
            .expect("users' key reads go on beside a walk nobody steps");
        assert_eq!(users, 3 * KEY_PAGE + 1);
        let orders = tokio::time::timeout(Duration::from_secs(60), drive(stuck))
            .await
            .unwrap();
        assert_eq!(orders, 3 * KEY_PAGE + 1, "stepped again, it loses no key");
        assert_eq!(
            documents.most.load(Ordering::SeqCst),
            1,
            "one read at a time"
        );
    }

    /// Reads that never answer, counted while they run.
    #[derive(Debug, Default)]
    struct Hanging {
        live: Arc<AtomicUsize>,
    }

    /// Counts one read while it is alive.
    struct Alive(Arc<AtomicUsize>);

    impl Alive {
        fn new(live: &Arc<AtomicUsize>) -> Self {
            live.fetch_add(1, Ordering::SeqCst);
            Self(Arc::clone(live))
        }
    }

    impl Drop for Alive {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl DocumentBuilder for Hanging {
        async fn resolve(
            &self,
            _: &TableName,
            _: &RowKey,
            _: Option<&RowImage>,
            _: Option<&RowImage>,
        ) -> source::Result<Vec<DocumentId>> {
            Ok(Vec::new())
        }

        async fn build(&self, id: &DocumentId) -> source::Result<Document> {
            Ok(Document::Delete { id: id.clone() })
        }

        fn backfill_indexes(&self) -> Vec<IndexName> {
            Vec::new()
        }

        async fn last_key(&self, _: &IndexName) -> source::Result<Option<RowKey>> {
            let _alive = Alive::new(&self.live);
            std::future::pending().await
        }

        async fn keys(
            &self,
            _: &IndexName,
            _: Option<&RowKey>,
            _: &RowKey,
            _: usize,
        ) -> source::Result<Vec<RowKey>> {
            std::future::pending().await
        }
    }

    /// A walk dropped mid-read (a stop, a failed run, an aborted one) ends
    /// its read: no task outlives it, and the key-read permit comes back.
    #[tokio::test(start_paused = true)]
    async fn a_dropped_walk_ends_its_key_read() {
        let documents = Arc::new(Hanging::default());
        let key_reads = Arc::new(Semaphore::new(1));
        let mut walk = Walk::new(
            Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
            [IndexName::try_new("users").unwrap()],
            Arc::clone(&key_reads),
        )
        .reading_apart();
        assert!(
            tokio::time::timeout(Duration::from_millis(1), walk.step())
                .await
                .is_err()
        );
        assert_eq!(documents.live.load(Ordering::SeqCst), 1, "setup: reading");
        drop(walk);
        tokio::time::timeout(Duration::from_secs(5), async {
            while documents.live.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("the read ended with its walk");
        assert_eq!(key_reads.available_permits(), 1, "the permit came back");
    }

    /// One builder (one walk at a time) reads inline, as before 6b-3: no
    /// task is spawned for its reads.
    #[tokio::test(start_paused = true)]
    async fn one_index_reads_its_keys_inline() {
        let documents = Arc::new(Hanging::default());
        let before = tokio::runtime::Handle::current()
            .metrics()
            .num_alive_tasks();
        let mut walk = Walk::new(
            Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
            [IndexName::try_new("users").unwrap()],
            Arc::new(Semaphore::new(1)),
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(1), walk.step())
                .await
                .is_err()
        );
        assert_eq!(documents.live.load(Ordering::SeqCst), 1, "setup: reading");
        assert_eq!(
            tokio::runtime::Handle::current()
                .metrics()
                .num_alive_tasks(),
            before,
            "no task spawned"
        );
    }

    /// Counts every page read started.
    #[derive(Debug, Default)]
    struct Paging {
        inner: Counted,
        pages: AtomicUsize,
    }

    #[async_trait]
    impl DocumentBuilder for Paging {
        async fn resolve(
            &self,
            _: &TableName,
            _: &RowKey,
            _: Option<&RowImage>,
            _: Option<&RowImage>,
        ) -> source::Result<Vec<DocumentId>> {
            Ok(Vec::new())
        }

        async fn build(&self, id: &DocumentId) -> source::Result<Document> {
            Ok(Document::Delete { id: id.clone() })
        }

        fn backfill_indexes(&self) -> Vec<IndexName> {
            Vec::new()
        }

        async fn last_key(&self, index: &IndexName) -> source::Result<Option<RowKey>> {
            self.inner.last_key(index).await
        }

        async fn keys(
            &self,
            index: &IndexName,
            after: Option<&RowKey>,
            through: &RowKey,
            limit: usize,
        ) -> source::Result<Vec<RowKey>> {
            self.pages.fetch_add(1, Ordering::SeqCst);
            self.inner.keys(index, after, through, limit).await
        }
    }

    /// No read ahead (6b-3, review of #453): a walk, inline or reading apart,
    /// its keys in hand not taken, starts no further read however long it
    /// waits and however often it is stepped: it holds one page at most.
    #[tokio::test(start_paused = true)]
    async fn a_walk_holds_one_page_and_reads_no_further_until_it_is_taken() {
        for apart in [false, true] {
            let documents = Arc::new(Paging::default());
            let walk = Walk::new(
                Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
                [IndexName::try_new("users").unwrap()],
                Arc::new(Semaphore::new(1)),
            );
            let mut walk = if apart { walk.reading_apart() } else { walk };
            assert!(matches!(walk.step().await.unwrap(), Step::Keys));
            assert_eq!(documents.pages.load(Ordering::SeqCst), 1);
            for _ in 0..5 {
                tokio::time::sleep(Duration::from_secs(1)).await;
                assert!(matches!(walk.step().await.unwrap(), Step::Keys));
            }
            assert_eq!(
                documents.pages.load(Ordering::SeqCst),
                1,
                "apart {apart}: no read ahead"
            );
            assert_eq!(walk.keys.len(), KEY_PAGE, "apart {apart}: one page held");
            walk.keys.clear();
            assert!(matches!(walk.step().await.unwrap(), Step::Keys));
            assert_eq!(documents.pages.load(Ordering::SeqCst), 2);
        }
    }

    /// The abort of a read still waiting for the permit (6b-3, review of
    /// #453).
    /// `orders`' read holds the key-read permit and never answers; `users'`
    /// read waits for the permit on its task. Dropping `users`' walk ends
    /// that wait: once `orders`' walk is dropped too, the permit comes back
    /// and no read is left alive (a waiter left running would take the
    /// permit and hang in its read).
    #[tokio::test(start_paused = true)]
    async fn a_walk_dropped_while_its_read_waits_for_the_permit_takes_no_permit_later() {
        let documents = Arc::new(Hanging::default());
        let key_reads = Arc::new(Semaphore::new(1));
        let walk = |name: &str| {
            Walk::new(
                Arc::clone(&documents) as Arc<dyn DocumentBuilder>,
                [IndexName::try_new(name).unwrap()],
                Arc::clone(&key_reads),
            )
            .reading_apart()
        };
        let mut holding = walk("orders");
        assert!(
            tokio::time::timeout(Duration::from_millis(1), holding.step())
                .await
                .is_err()
        );
        assert_eq!(
            documents.live.load(Ordering::SeqCst),
            1,
            "setup: orders reads"
        );
        let mut waiting = walk("users");
        assert!(
            tokio::time::timeout(Duration::from_millis(1), waiting.step())
                .await
                .is_err()
        );
        assert_eq!(key_reads.available_permits(), 0);
        assert_eq!(
            documents.live.load(Ordering::SeqCst),
            1,
            "setup: users waits for the permit"
        );
        drop(waiting);
        tokio::time::sleep(Duration::from_millis(10)).await;
        drop(holding);
        tokio::time::timeout(Duration::from_secs(5), async {
            while documents.live.load(Ordering::SeqCst) != 0 || key_reads.available_permits() != 1 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("no read alive and the permit back");
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert_eq!(
            documents.live.load(Ordering::SeqCst),
            0,
            "the dropped waiter never read"
        );
        assert_eq!(key_reads.available_permits(), 1);
    }
}
