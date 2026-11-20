//! Granting builds and publishing them: live batches to every lane, snapshot
//! batches to the lanes that asked, each in the order it was granted
//! (see [`Builds`]).
//!
//! The loop calls [`grant`](IngestEngine::grant) whenever a batch may have
//! become ready or a slot free: after buffering live changes, after the
//! snapshot took keys or finished its walk, after a build landed. It grants
//! what the gate allows and publishes every slot that is ready, in order.
//!
//! A live grant marks the run's documents pending on their indexes in the
//! [`Progress`](crate::Progress) and takes every index's waiting documents
//! into one build, or none while one of them is still building (ADR 0012
//! stage 8a): from the grant until each lane's registration, the pending
//! build holds that lane's confirmation and fences before its oldest change.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use futures::future::try_join_all;
use kernel::{Envelope, IndexName, Position, SinkName};
use source::document::{BuildFailure, Document, DocumentId};
use stream::{Batch, Hold, ItemKey, LaneItem, LaneKey, Producer};
use tokio::time::Instant;

use super::builds::{Builds, Built, Grant, Job};
use super::confirm::CONFIRM_TICK;
use super::live::LiveRun;
use super::snapshot::Snapshot;
use super::{IngestEngine, Lanes, document_id};
use crate::error::{EngineError, Result};
use crate::observer::{BuildStats, Stage, StageTiming, Wake, Work};
use crate::policy::FailurePolicy;
use crate::progress::{Holds, Token};

impl IngestEngine {
    /// Grant every batch the gate allows and publish every slot that is
    /// ready, until neither moves.
    pub(super) async fn grant(
        &self,
        pending: &mut LiveRun,
        snapshot: &mut Option<Snapshot>,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
        // A take that found nothing it could build (what waits is building)
        // isn't tried again until something else happens.
        let mut live_blocked = false;
        loop {
            if let Some(active) = snapshot.as_mut()
                && active.done
                && active.batch.count == 0
            {
                self.report_page(active);
                if let Some(finished) = snapshot.take() {
                    builds.complete(finished);
                }
            }
            let waiting = self.progress.waiting();
            let live = (!live_blocked && pending.count > 0)
                .then(|| (pending.marks.len() + waiting, pending.is_full()));
            let backfill = snapshot
                .as_ref()
                .filter(|active| active.batch.count > 0 && (active.batch.is_full() || active.done))
                .map(|active| active.batch.ids.as_slice());
            match builds.choose(live, backfill) {
                Some(Grant::Live) => {
                    if !self.start_live(pending, builds, lanes).await {
                        live_blocked = true;
                    }
                }
                Some(Grant::Backfill) => {
                    if let Some(active) = snapshot.as_mut() {
                        self.start_backfill(active, builds).await;
                    }
                }
                None => {
                    if !self.publish_ready(builds, lanes).await? {
                        return Ok(());
                    }
                    // A publish releases its documents: what waited on them
                    // can be taken now.
                    live_blocked = false;
                }
            }
        }
    }

    /// Record a spawned build that finished, then grant and publish what
    /// that allows.
    pub(super) async fn landed(
        &self,
        landed: Option<(u64, Option<super::builds::Output>)>,
        pending: &mut LiveRun,
        snapshot: &mut Option<Snapshot>,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
        if let Some((seq, output)) = landed {
            builds.land(seq, output, &|built| self.halts(built));
        }
        self.grant(pending, snapshot, builds, lanes).await
    }

    /// The end of a run: grant the buffered live batch once its documents
    /// are free, finish every build, and publish everything in order. A
    /// snapshot batch not yet granted is not built.
    pub(super) async fn settle(
        &self,
        pending: &mut LiveRun,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
        builds.woke(Wake::Settle);
        loop {
            self.publish_ready(builds, lanes).await?;
            let waiting = self.progress.waiting();
            let live = pending.count > 0 || waiting > 0;
            if live
                && builds.can_take_last(pending.marks.len() + waiting)
                && self.start_live(pending, builds, lanes).await
            {
                continue;
            }
            if builds.running() == 0 {
                // Nothing builds: what holds the last batch back is the
                // budget, which the sinks' acknowledgements free.
                match (live, builds.refused(), self.budget.as_deref()) {
                    (true, Some(_), Some(budget)) => {
                        // The release wakes this; the timeout is a backstop
                        // should one ever go unheard.
                        let _ = tokio::time::timeout(CONFIRM_TICK, budget.released()).await;
                        continue;
                    }
                    _ => return Ok(()),
                }
            }
            match builds.next().await {
                Some((seq, output)) => builds.land(seq, output, &|built| self.halts(built)),
                None => return Ok(()),
            }
        }
    }

    /// Mark the run's documents pending on their indexes, then take every
    /// index's waiting documents into one build, as one batch over every
    /// index, or none: while a waiting document is still building, nothing
    /// is taken and the run stays as it is, so the reading waits, as for one
    /// batch. Returns whether a build started; when none did, the permit the
    /// grant took is given back.
    async fn start_live(&self, pending: &mut LiveRun, builds: &mut Builds, lanes: &Lanes) -> bool {
        self.progress.mark(pending.marks.drain(..));
        pending.last = pending.last.max(pending.position);
        let Some((ids, taken, position)) = self.take_all(pending.last, builds) else {
            builds.drop_grant();
            return false;
        };
        pending.last = position;
        let changes = pending.count;
        pending.clear();
        if ids.is_empty() {
            builds.drop_grant();
            // Changes that touched no document any lane receives: every
            // lane has all of them it needs, as an empty batch told it.
            if let Some(last) = position.filter(|_| changes > 0) {
                self.settled(last, changes, lanes);
            }
            return false;
        }
        let job = Job::Live {
            taken,
            position,
            changes,
        };
        builds
            .start(&self.documents, ids, job, &|built| self.halts(built))
            .await;
        true
    }

    /// Changes through `last` built nothing for any lane: each lane has all
    /// of them it needs, and each sink is told they settled.
    fn settled(&self, last: Position, changes: usize, lanes: &Lanes) {
        let mut told: Vec<&SinkName> = Vec::new();
        for lane in lanes.keys() {
            self.progress.skipped(lane, last);
            if !told.contains(&&lane.sink) {
                told.push(&lane.sink);
                self.observer.on_changes_settled(&lane.sink, changes);
            }
        }
    }

    /// Every index's waiting documents, unless one is still building: the
    /// ids, each index's build, and the position the batch answers for:
    /// `last`, the newest change this run read (positions never go back
    /// along a lane), or, before the run read any, the newest its documents
    /// waited at.
    #[allow(clippy::type_complexity)]
    fn take_all(
        &self,
        last: Option<Position>,
        builds: &mut Builds,
    ) -> Option<(
        Vec<DocumentId>,
        Vec<(IndexName, Token, Position)>,
        Option<Position>,
    )> {
        let held = |index: &IndexName, key: &source::RowKey| builds.holds(index, key);
        let held: Option<Holds<'_>> = builds.holds_some().then_some(&held);
        if self.progress.any_waiting_held(held) {
            return None;
        }
        let mut ids = builds.spare(self.batch.max_changes);
        let mut taken = Vec::new();
        let mut newest = None;
        for index in self.progress.indexes() {
            if let Some(take) = self
                .progress
                .take(index, usize::MAX, &|key| builds.holds(index, key))
            {
                ids.extend(take.ids);
                newest = newest.max(Some(take.newest));
                taken.push((index.clone(), take.token, take.oldest));
            }
        }
        Some((ids, taken, last.or(newest)))
    }

    async fn start_backfill(&self, snapshot: &mut Snapshot, builds: &mut Builds) {
        self.report_page(snapshot);
        let ids = std::mem::replace(
            &mut snapshot.batch.ids,
            builds.spare(self.batch.max_changes),
        );
        snapshot.batch.clear();
        let marks = self.walk_marks(&ids).await;
        let job = Job::Backfill {
            requested: snapshot.lanes_now(),
            marks,
        };
        builds
            .start(&self.documents, ids, job, &|built| self.halts(built))
            .await;
    }

    /// Each index's mark for this batch: its greatest key, the last of the
    /// index's ids (a walk buffers them in key order), as the source writes
    /// it. An index the source can't mark gets none, and its lanes record
    /// nothing for this batch.
    async fn walk_marks(
        &self,
        ids: &[source::document::DocumentId],
    ) -> BTreeMap<IndexName, stream::WalkMark> {
        let mut last: BTreeMap<&IndexName, &source::RowKey> = BTreeMap::new();
        for id in ids {
            last.insert(&id.index, &id.key);
        }
        let mut marks = BTreeMap::new();
        for (index, key) in last {
            match self.documents.walk_mark(index, key).await {
                Ok(Some(mark)) => {
                    marks.insert(index.clone(), stream::WalkMark::new(mark));
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(%index, %error, "could not mark how far this backfill batch got; its lanes record nothing for it")
                }
            }
        }
        marks
    }

    /// The time the snapshot spent getting keys since its last batch.
    fn report_page(&self, snapshot: &mut Snapshot) {
        let page = std::mem::take(&mut snapshot.page_wait);
        if !page.is_zero() {
            self.observer
                .on_stage(StageTiming::new(Stage::Page, page).work(Work::Backfill));
        }
    }

    /// Whether a build's result ends the run once it is published: the build
    /// failed, or a document failed under `on_error = "stop"`. Nothing more
    /// is granted from the moment it lands.
    fn halts(&self, built: &Built) -> bool {
        match &built.documents {
            Err(_) => true,
            Ok(documents) => documents.iter().any(|document| match document {
                Document::Failed { id, .. } => {
                    self.failure_policies.resolve(id.index.as_ref()) == FailurePolicy::Stop
                }
                _ => false,
            }),
        }
    }

    /// Publish every slot at the front that is ready, in order. Returns
    /// whether it published any. A failed build's slot returns its error,
    /// with nothing after it published.
    async fn publish_ready(&self, builds: &mut Builds, lanes: &Lanes) -> Result<bool> {
        let mut published = false;
        while let Some((job, built, charge)) = builds.pop_ready() {
            published = true;
            // Every lane's item of the batch shares the one charge: its bytes
            // come back when the last copy is dropped (every lane acknowledged
            // it, and no ledger can redeliver it).
            let hold = charge.map_or_else(Hold::default, |charge| Hold::new(charge));
            match (job, built) {
                (Job::Complete(mut finished), _) => {
                    let started = Instant::now();
                    self.finish_snapshot(&mut finished, lanes).await?;
                    self.observer.on_stage(
                        StageTiming::new(Stage::Snapshot, started.elapsed()).work(Work::Backfill),
                    );
                }
                (
                    Job::Live {
                        taken,
                        position,
                        changes,
                    },
                    Some(built),
                ) => {
                    self.publish_live(built, &taken, position, changes, &hold, lanes)
                        .await?;
                }
                (Job::Backfill { requested, marks }, Some(built)) => {
                    self.publish_backfill(built, &requested, &marks, &hold, lanes)
                        .await?;
                }
                (_, None) => {}
            }
        }
        Ok(published)
    }

    /// A live batch to every lane with the batch's position, concurrently,
    /// so a full lane delays only its own copy. A batch that built no
    /// document is still published (empty), so the lanes acknowledge its
    /// position.
    #[tracing::instrument(name = "ingest.commit", level = "debug", skip_all, fields(changes = changes))]
    async fn publish_live(
        &self,
        built: Built,
        taken: &[(IndexName, Token, Position)],
        position: Option<Position>,
        changes: usize,
        hold: &Hold,
        lanes: &Lanes,
    ) -> Result<()> {
        let (envelopes, by_index) = self.envelopes(built.documents?, position)?;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, built.took).work(Work::Live));
        let started = Instant::now();
        let stats = BuildStats {
            changes,
            documents: envelopes.len(),
            documents_by_index: by_index.into_iter().collect(),
            build: built.took,
        };
        // One item per lane of an index the batch built something for: the
        // envelopes of that index, in build order, under the index's next key
        // (the same on each of its lanes; lanes are ordered by index, so an
        // index's lanes are adjacent). A sink's first item carries the batch's
        // changes; a sink with no item is told they settled.
        let mut by_lane: Vec<Item<'_>> = Vec::new();
        let mut skipped: Vec<&LaneKey> = Vec::new();
        let mut counted: Vec<&SinkName> = Vec::new();
        let mut keyed: Option<(&IndexName, ItemKey)> = None;
        let groups = group_by_index(envelopes);
        for (lane, producer) in lanes {
            match groups.get(&lane.index) {
                Some(envelopes) => {
                    let item_key = match keyed {
                        Some((index, item_key)) if *index == lane.index => item_key,
                        _ => {
                            let item_key = self
                                .progress
                                .next_key(&lane.index)
                                .ok_or_else(|| EngineError::LaneNotInProgress(lane.to_string()))?;
                            keyed = Some((&lane.index, item_key));
                            item_key
                        }
                    };
                    let first = !counted.contains(&&lane.sink);
                    if first {
                        counted.push(&lane.sink);
                    }
                    let routed = taken
                        .iter()
                        .find(|(index, _, _)| *index == lane.index)
                        .zip(position)
                        .map(|((_, token, oldest), last)| (*oldest, last, *token));
                    by_lane.push((
                        lane,
                        producer,
                        routed,
                        LaneItem::Batch(Batch {
                            key: Some(item_key),
                            position,
                            changes: if first { changes } else { 0 },
                            envelopes: Arc::clone(envelopes),
                            hold: hold.clone(),
                            walked: None,
                            ticket: None,
                        }),
                    ));
                }
                None => skipped.push(lane),
            }
        }
        // Each lane is registered as its own send succeeds, so a full lane
        // holds back only itself among its index's items; until every lane
        // of the index has it, the index's pending build holds all of them
        // back. A lane that lands the item first leaves its mark for the
        // registration to find.
        if let Some(last) = position {
            for key in &skipped {
                self.progress.skipped(key, last);
            }
        }
        for key in skipped {
            if !counted.contains(&&key.sink) {
                counted.push(&key.sink);
                self.observer.on_changes_settled(&key.sink, changes);
            }
        }
        self.publish(by_lane, Work::Live, started).await?;
        // Registered on every lane of each index: the builds no longer hold
        // anything back, whether or not they gave the index a document.
        for (index, token, _) in taken {
            self.progress.release(index, *token);
        }
        self.observer.on_batch_built(stats);
        tracing::debug!("batch built and published");
        Ok(())
    }

    /// A snapshot batch: each `(index, sink)` lane that asked for its index
    /// that index's envelopes, with no position.
    async fn publish_backfill(
        &self,
        built: Built,
        requested: &BTreeMap<SinkName, BTreeMap<IndexName, stream::Ticket>>,
        marks: &BTreeMap<IndexName, stream::WalkMark>,
        hold: &Hold,
        lanes: &Lanes,
    ) -> Result<()> {
        let (envelopes, by_index) = self.envelopes(built.documents?, None)?;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, built.took).work(Work::Backfill));
        let started = Instant::now();
        let groups = group_by_index(envelopes);
        let documents: usize = groups.values().map(|envelopes| envelopes.len()).sum();
        let mut publishes = Vec::new();
        for (sink, indexes) in requested {
            for (index, ticket) in indexes {
                let Some(envelopes) = groups.get(index) else {
                    continue;
                };
                let key = LaneKey::new(index.clone(), sink.clone());
                let Some((key, producer)) = lanes.get_key_value(&key) else {
                    tracing::warn!(%sink, %index, "snapshot requested for a lane that does not exist; dropping its slice");
                    continue;
                };
                publishes.push((
                    key,
                    producer,
                    None,
                    LaneItem::Batch(Batch {
                        key: None,
                        position: None,
                        changes: 0,
                        envelopes: Arc::clone(envelopes),
                        hold: hold.clone(),
                        walked: marks.get(index).cloned(),
                        ticket: Some(*ticket),
                    }),
                ));
            }
        }
        self.publish(publishes, Work::Backfill, started).await?;
        self.observer.on_batch_built(BuildStats {
            changes: 0,
            documents,
            documents_by_index: by_index.into_iter().collect(),
            build: built.took,
        });
        Ok(())
    }

    /// Publish each item to its lane, all at once, so a full lane delays only
    /// its own copy, and register each live item (under its key, with its
    /// oldest and last position) in the progress as its own lane takes it.
    /// Times each lane ([`Stage::LaneWait`]) and the whole
    /// ([`Stage::Publish`]) from `started`, the moment publishing began, so
    /// cutting the items is publishing too.
    pub(super) async fn publish<'a>(
        &self,
        items: impl IntoIterator<Item = Item<'a>>,
        work: Work,
        started: Instant,
    ) -> Result<()> {
        try_join_all(
            items
                .into_iter()
                .map(|(lane, producer, routed, item)| async move {
                    let key = item.key();
                    producer.publish(item).await?;
                    if let (Some((oldest, last, token)), Some(key)) = (routed, key) {
                        self.progress.published(lane, key, oldest, last);
                        self.progress.build_registered(lane, token);
                    }
                    self.observer.on_stage(
                        StageTiming::new(Stage::LaneWait, started.elapsed())
                            .work(work)
                            .sink(&lane.sink),
                    );
                    Ok::<_, EngineError>(())
                }),
        )
        .await?;
        self.observer
            .on_stage(StageTiming::new(Stage::Publish, started.elapsed()).work(work));
        Ok(())
    }

    /// The built documents as envelopes tagged with `position`, counted per
    /// index. A failed document is decided by its index's `on_error`.
    fn envelopes(
        &self,
        documents: Vec<Document>,
        position: Option<Position>,
    ) -> Result<(Vec<Envelope>, BTreeMap<IndexName, usize>)> {
        let mut by_index: BTreeMap<IndexName, usize> = BTreeMap::new();
        let mut envelopes = Vec::with_capacity(documents.len());
        let ts = Utc::now();
        for document in documents {
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
}

/// One lane's item of a batch, with its lane, its producer and, for a live
/// item, the oldest and last position it registers under and the pending
/// build it carries.
type Item<'a> = (
    &'a LaneKey,
    &'a Box<dyn Producer<LaneItem>>,
    Option<(Position, Position, Token)>,
    LaneItem,
);

/// The envelopes of each index, in build order, each group shared by every
/// lane of its index.
fn group_by_index(envelopes: Vec<Envelope>) -> BTreeMap<IndexName, Arc<[Envelope]>> {
    let mut groups: BTreeMap<IndexName, Vec<Envelope>> = BTreeMap::new();
    for envelope in envelopes {
        groups
            .entry(envelope.index.clone())
            .or_default()
            .push(envelope);
    }
    groups
        .into_iter()
        .map(|(index, envelopes)| (index, envelopes.into()))
        .collect()
}
