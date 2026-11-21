//! Granting builds and publishing them: live batches to every lane, snapshot
//! batches to the lanes that asked, each in the order it was granted
//! (see [`Builds`]).
//!
//! The loop calls [`grant`](IngestEngine::grant) whenever a batch may have
//! become ready or a slot free: after buffering live changes, after the
//! snapshot took keys or finished its walk, after a build landed. It grants
//! what the gate allows and publishes every slot that is ready, in order.
//!
//! The reader marks each run's documents pending on their indexes in the
//! [`Progress`](crate::Progress) before handing the run over; the live batch
//! is the runs a grant may join ([`Runs::candidate`]), offered only while
//! none of its documents is in a running build (ADR 0012 stage 8a, R3–R4). A
//! grant takes, from each index the budget admits, the documents marked
//! through the batch's last change into one build; until each lane registers
//! its item (or is told it was skipped), that build holds the lane's
//! confirmation and fences before its oldest change.

use std::collections::BTreeMap;
use std::sync::Arc;

use chrono::Utc;
use futures::future::try_join_all;
use kernel::{Envelope, IndexName, Position, SinkName};
use source::document::{BuildFailure, Document, DocumentId};
use stream::{Batch, Hold, ItemKey, LaneItem, LaneKey, Producer};
use tokio::time::Instant;

use super::builds::{Builds, Built, Grant, Job, LiveCandidate};
use super::confirm::CONFIRM_TICK;
use super::live::{Candidate, Runs};
use super::snapshot::Snapshot;
use super::{IngestEngine, Lanes, document_id};
use crate::error::{EngineError, Result};
use crate::observer::{BuildStats, Stage, StageTiming, Wake, Work};
use crate::policy::FailurePolicy;
use crate::progress::{Holds, Step, Token};

impl IngestEngine {
    /// Grant every batch the gate allows and publish every slot that is
    /// ready, until neither moves.
    pub(super) async fn grant(
        &self,
        runs: &mut Runs,
        snapshot: &mut Option<Snapshot>,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
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
            // The live batch is every index's documents waiting through the
            // candidate's last change, offered only once none of them is in
            // a running build: `main`'s admission, checked before a permit is
            // taken. Its estimate counts distinct documents.
            // Beside a running live build only a full batch may start, so a
            // hand-over that can't fill one costs no walk.
            let candidate = if builds.folding() {
                self.backlog(builds)
            } else if builds.live_running() {
                runs.full_candidate()
            } else {
                runs.candidate(true)
            };
            let live = candidate.and_then(|candidate| {
                Some(LiveCandidate {
                    live: candidate,
                    documents: self.waiting(builds, candidate.position)?,
                    full: candidate.full,
                    changes: candidate.changes > 0,
                })
            });
            let backfill = snapshot
                .as_ref()
                .filter(|active| active.batch.count > 0 && (active.batch.is_full() || active.done))
                .map(|active| active.batch.ids.as_slice());
            match builds.choose(live, backfill) {
                Some(Grant::Live(candidate, admitted, deferred)) => {
                    self.start_live(runs, builds, candidate, &admitted, deferred)
                        .await;
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
                }
            }
        }
    }

    /// Record a spawned build that finished, then grant and publish what
    /// that allows.
    pub(super) async fn landed(
        &self,
        landed: Option<(u64, Option<super::builds::Output>)>,
        runs: &mut Runs,
        snapshot: &mut Option<Snapshot>,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
        if let Some((seq, output)) = landed {
            builds.land(seq, output, &|built| self.halts(built));
        }
        self.grant(runs, snapshot, builds, lanes).await
    }

    /// The end of a run, once the reader ended: grant every run it handed
    /// over, in `main`'s batches, once their documents are free, finish
    /// every build, and publish everything in order. A snapshot batch not
    /// yet granted is not built.
    pub(super) async fn settle(
        &self,
        runs: &mut Runs,
        builds: &mut Builds,
        lanes: &Lanes,
    ) -> Result<()> {
        builds.woke(Wake::Settle);
        loop {
            self.publish_ready(builds, lanes).await?;
            let live = if builds.folding() {
                builds
                    .index()
                    .and_then(|index| self.progress.newest_waiting(index))
                    .map(|position| Candidate::backlog(position, true))
            } else {
                runs.candidate(false)
            };
            if let Some(candidate) = live
                && let Some(documents) = self.waiting(builds, candidate.position)
                && let Some((admitted, deferred)) = builds.can_take_last(&documents)
            {
                self.start_live(runs, builds, candidate, &admitted, deferred)
                    .await;
                continue;
            }
            if builds.running() == 0 {
                // Nothing builds: what holds the last batch back is the
                // budget, which the sinks' acknowledgements free.
                match (live.is_some(), builds.refused(), self.budget.as_deref()) {
                    (true, Some(_), Some(budget)) => {
                        // The release wakes this; the timeout is a backstop
                        // should one ever go unheard.
                        let since = builds.refused_since();
                        let _ =
                            tokio::time::timeout(CONFIRM_TICK, budget.released_since(since)).await;
                        continue;
                    }
                    // Another builder holds the permits: its turn comes when
                    // one is freed (G9′).
                    (true, _, _) if builds.in_line() => {
                        builds.turn().await;
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

    /// Each index's documents waiting through `through` (the builder's own
    /// index alone), oldest waiting first, or `None` while a running build
    /// holds one of them: a take now would leave it behind, so the live
    /// batch waits, as `main`'s did.
    fn waiting(&self, builds: &Builds, through: Position) -> Option<Vec<(IndexName, usize)>> {
        let held = |index: &IndexName, key: &source::RowKey| builds.holds(index, key);
        let held: Option<Holds<'_>> = builds.holds_some().then_some(&held);
        self.progress.waiting_of(builds.index(), through, held)
    }

    /// While the builder folds (§3.9 I7), its standing candidate: the
    /// documents waiting on its index, through the newest of them, full
    /// once `max_changes` wait (a full one may start beside a running live
    /// build). `None` with nothing waiting.
    fn backlog(&self, builds: &Builds) -> Option<Candidate> {
        let index = builds.index()?;
        let waiting = self.progress.waiting_on(index);
        let position = self.progress.newest_waiting(index)?;
        let full = waiting >= self.batch.max_changes;
        (!builds.live_running() || full).then_some(Candidate::backlog(position, full))
    }

    /// Take the documents waiting through `candidate`'s last change of every
    /// index in `admitted` (the ones the budget has room for) into one
    /// build, as one batch over those indexes (the grant checked none is in
    /// a running build). A batch whose changes touched no document any lane
    /// receives is an empty build, as on `main`: its slot settles those
    /// changes in grant order. The batch carries the candidate's newest
    /// change, and its step closes at the candidate's read count, so a
    /// newer run's change, whose documents it doesn't take, falls in a later
    /// step (R4). An index left waiting is **deferred**: its lanes are not
    /// registered with this build's step, so no change of it counts as
    /// committed before its documents land, and the candidate is kept so a
    /// later grant takes them.
    async fn start_live(
        &self,
        runs: &mut Runs,
        builds: &mut Builds,
        candidate: Candidate,
        admitted: &[IndexName],
        deferred: Vec<IndexName>,
    ) {
        let position = candidate.position;
        let backlog = builds.folding();
        let (ids, taken, mut frontiers) = self.take_all(builds, admitted, position, backlog);
        if !backlog {
            // Documents a capped take left behind keep the candidate: a later
            // grant takes them whether or not a change comes (F2).
            let left = self
                .progress
                .waiting_of(builds.index(), position, None)
                .is_some_and(|waiting| !waiting.is_empty());
            runs.granted(candidate, !deferred.is_empty() || left);
        }
        // Every other index this builder builds is registered with its
        // frontier now, at the grant: computed at publish, it could cover
        // documents a later grant takes, whose item comes after this one
        // (§3.9 I5). A builder of one index registers only its own.
        for index in self.progress.indexes() {
            if builds.index().is_none_or(|own| own == index)
                && !deferred.contains(index)
                && !frontiers.iter().any(|(taken, _)| taken == index)
            {
                frontiers.push((index.clone(), self.progress.frontier(index)));
            }
        }
        let steps = self.progress.close_steps(frontiers);
        let job = Job::Live {
            taken,
            position,
            changes: candidate.changes,
            steps,
            deferred,
        };
        builds
            .start(&self.documents, ids, job, &|built| self.halts(built))
            .await;
    }

    /// The waiting documents of each index in `admitted`: the ids, each
    /// index's build, and each index's frontier after its take.
    #[allow(clippy::type_complexity)]
    fn take_all(
        &self,
        builds: &mut Builds,
        admitted: &[IndexName],
        through: Position,
        backlog: bool,
    ) -> (
        Vec<DocumentId>,
        Vec<(IndexName, Token, Position)>,
        Vec<(IndexName, u64)>,
    ) {
        let mut ids = builds.spare(self.batch.max_changes);
        let mut taken = Vec::new();
        let mut frontiers = Vec::new();
        for index in self
            .progress
            .indexes()
            .filter(|index| admitted.contains(index))
        {
            let held = |key: &source::RowKey| builds.holds(index, key);
            // One index's build takes at most `max_changes` documents (O2);
            // a builder of every index (a lone `Builds`) takes them all, as
            // 6b-1b did.
            let most = if builds.index().is_some() {
                self.batch.max_changes
            } else {
                usize::MAX
            };
            let take = if backlog {
                self.progress.take(index, most, &held)
            } else {
                self.progress
                    .take_through_at_most(index, through, most, &held)
            };
            if let Some(take) = take {
                ids.extend(take.ids);
                taken.push((index.clone(), take.token, take.oldest));
                frontiers.push((index.clone(), take.frontier));
            }
        }
        (ids, taken, frontiers)
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
        while let Some((job, built, charges)) = builds.pop_ready() {
            published = true;
            // Every lane's item of an index shares the index's one charge:
            // its bytes come back when the last copy is dropped (every lane
            // of the index acknowledged it, and no ledger can redeliver it).
            let holds: BTreeMap<IndexName, Hold> = charges
                .into_iter()
                .map(|(index, charge)| (index, Hold::new(charge)))
                .collect();
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
                        steps,
                        deferred,
                    },
                    Some(built),
                ) => {
                    let granted = Granted {
                        position,
                        changes,
                        steps,
                    };
                    self.publish_live(built, &taken, &deferred, granted, &holds, lanes)
                        .await?;
                }
                (Job::Backfill { requested, marks }, Some(built)) => {
                    self.publish_backfill(built, &requested, &marks, &holds, lanes)
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
    #[tracing::instrument(name = "ingest.commit", level = "debug", skip_all, fields(changes = granted.changes))]
    async fn publish_live(
        &self,
        built: Built,
        taken: &[(IndexName, Token, Position)],
        deferred: &[IndexName],
        granted: Granted,
        holds: &BTreeMap<IndexName, Hold>,
        lanes: &Lanes,
    ) -> Result<()> {
        let Granted {
            position,
            changes,
            steps,
        } = granted;
        let (envelopes, by_index) = self.envelopes(built.documents?, Some(position))?;
        self.observer
            .on_stage(StageTiming::new(Stage::Build, built.took).work(Work::Live));
        // From here a lane that hasn't taken its index's item answers for it.
        for (index, token, _) in taken {
            self.progress.build_sending(index, *token);
        }
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
        // index's lanes are adjacent). Every lane, with an item or not, is
        // registered with its index's step (what a sink's committed count
        // follows), except a deferred index's: its documents still wait.
        let mut by_lane: Vec<Item<'_>> = Vec::new();
        let mut skipped: Vec<(&LaneKey, Step)> = Vec::new();
        let mut keyed: Option<(&IndexName, ItemKey)> = None;
        let groups = group_by_index(envelopes);
        for (lane, producer) in lanes {
            if deferred.contains(&lane.index) {
                continue;
            }
            let Some(step) = steps.get(&lane.index).copied() else {
                continue;
            };
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
                    // Every document of the batch came from one of its
                    // takes, so its index has a pending build to register
                    // under; an item without one would land unregistered.
                    let Some((_, token, oldest)) =
                        taken.iter().find(|(index, _, _)| *index == lane.index)
                    else {
                        return Err(EngineError::UnregisteredLiveItem(lane.to_string()));
                    };
                    by_lane.push((
                        lane,
                        producer,
                        Some((*oldest, position, *token, step)),
                        LaneItem::Batch(Batch {
                            key: Some(item_key),
                            position: Some(position),
                            envelopes: Arc::clone(envelopes),
                            hold: holds.get(&lane.index).cloned().unwrap_or_default(),
                            walked: None,
                            ticket: None,
                        }),
                    ));
                }
                None => skipped.push((lane, step)),
            }
        }
        // Each lane is registered as its own send succeeds, so a full lane
        // holds back only itself among its index's items; until every lane
        // of the index has it, the index's pending build holds all of them
        // back. A lane that lands the item first leaves its mark for the
        // registration to find.
        for (key, step) in &skipped {
            let token = taken
                .iter()
                .find(|(index, _, _)| *index == key.index)
                .map(|(_, token, _)| *token);
            self.progress.skipped_build(key, position, token, *step);
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
        holds: &BTreeMap<IndexName, Hold>,
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
                        envelopes: Arc::clone(envelopes),
                        hold: holds.get(index).cloned().unwrap_or_default(),
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
                    if let (Some((oldest, last, token, step)), Some(key)) = (routed, key) {
                        self.progress
                            .published_build(lane, key, oldest, last, token, step);
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

/// A live build's grant: its live run's newest position, how many changes
/// the run read, and each index's step.
#[derive(Debug, Clone)]
struct Granted {
    position: Position,
    changes: usize,
    steps: BTreeMap<IndexName, Step>,
}

/// One lane's item of a batch, with its lane, its producer and, for a live
/// item, the oldest and last position it registers under and the pending
/// build it carries.
type Item<'a> = (
    &'a LaneKey,
    &'a Box<dyn Producer<LaneItem>>,
    Option<(Position, Position, Token, Step)>,
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
