//! The builder: the future of an engine run that builds and publishes (ADR
//! 0012 stage 8a, §3.9 §2). In 6b-1b there is one, over every index: 6a's
//! builder loop without the request lane, confirmation and the wind-down's
//! drain and close, which are the dispatcher's.
//!
//! It owns the runs the reader handed over, the builds, the snapshot, the
//! lanes' producers, its request inbox and the coalescing set. It hands its
//! end (the reader's, with the live stream) or its error to the dispatcher
//! over a one-shot, then returns.
//!
//! **Coalescing on a deadline.** The first request opens a coalescing set
//! with a deadline at `max_delay`; the set starts its snapshot once every
//! sink with a lane asked and the inbox is empty, else at the deadline
//! (`main`'s rule, over every index while there is one builder). Live
//! batches are granted meanwhile: on `main` and 6a they waited the
//! coalescing out.
//!
//! **The end and the stop.** Once the reader ended with its stream and
//! nothing is left to serve, the builder reports itself idle and waits for
//! the dispatcher's end, then settles. At the stop (the dispatcher raises it)
//! it drops its snapshot and coalescing set (their requests are served again
//! on the next start), waits for the reader's end (R7), and settles.

use std::collections::HashSet;

use kernel::{IndexName, SinkName};
use stream::{Delivery, Request};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

use super::builds::Builds;
use super::dispatcher::{Signal, Signals};
use super::live::Runs;
use super::reader::{self, ReaderEnd};
use super::snapshot::{Snapshot, Step, next_snapshot_step};
use super::{IngestEngine, Lanes, released};
use crate::error::Result;
use crate::observer::{Stage, StageTiming, Wake, Work};

/// How the builder ended without an error: the reader's end, and whether
/// the stop ended it.
pub(super) struct BuilderEnd {
    pub(super) reader: reader::Ended,
    pub(super) stopped: bool,
}

/// Requests gathered for one snapshot, and when it starts at the latest.
struct Coalescing {
    deliveries: Vec<Delivery<Request>>,
    deadline: Instant,
    /// When the first request arrived: the snapshot's start is timed from
    /// here, as `main` timed its gathering.
    started: Instant,
}

/// What the builder borrows for one engine run.
pub(super) struct Building<'a> {
    pub(super) runs: Runs,
    pub(super) reader: ReaderEnd,
    pub(super) inbox: mpsc::UnboundedReceiver<Delivery<Request>>,
    pub(super) signals: &'a Signals,
    pub(super) lanes: &'a Lanes,
    pub(super) scopes: &'a HashSet<IndexName>,
    pub(super) builds: &'a mut Builds,
}

impl IngestEngine {
    /// The builder of one engine run: build until the end or the stop, then
    /// hand the end to the dispatcher over `done`.
    pub(super) async fn build(
        &self,
        building: Building<'_>,
        done: oneshot::Sender<Result<BuilderEnd>>,
    ) {
        let result = self.building(building).await;
        // The dispatcher is gone only once the run ended.
        let _ = done.send(result);
    }

    async fn building(&self, building: Building<'_>) -> Result<BuilderEnd> {
        let Building {
            mut runs,
            reader: mut reader_end,
            mut inbox,
            signals,
            lanes,
            scopes,
            builds,
        } = building;
        let mut snapshot: Option<Snapshot> = None;
        let mut coalescing: Option<Coalescing> = None;
        // The reader's end, once joined: the stream to close, and whether it
        // ended by itself.
        let mut ended: Option<reader::Ended> = None;
        // Where the last arm's work ended: every arm times its wait from here,
        // so the loop reads no clock of its own.
        let mut mark = Instant::now();

        loop {
            // A live stream that ends while a snapshot runs or requests wait
            // still gets those served; the builder settles once the
            // dispatcher saw nothing left to forward either.
            if ended.as_ref().is_some_and(|ended| ended.stream_ended)
                && snapshot.is_none()
                && coalescing.is_none()
                && inbox.is_empty()
            {
                if signals.ended() {
                    self.settle(&mut runs, builds, lanes).await?;
                    if let Some(reader) = ended.take() {
                        return Ok(BuilderEnd {
                            reader,
                            stopped: false,
                        });
                    }
                }
                signals.report_idle();
            }
            if self.stop.is_requested() {
                if let Some(abandoned) = snapshot.take() {
                    tracing::info!(
                        indexes = abandoned.indexes.len(),
                        "stop requested: abandoning the snapshot in progress; its request is \
                         served again on the next start"
                    );
                }
                if let Some(gathered) = coalescing.take() {
                    tracing::info!(
                        requests = gathered.deliveries.len(),
                        "stop requested: dropping the backfill requests gathered; they are served \
                         again on the next start"
                    );
                }
                // The barrier (R7): the reader hands over what it read and
                // ends before anything is settled.
                let reader = match ended.take() {
                    Some(ended) => ended,
                    None => self.reader_end(&mut reader_end, &mut runs).await?,
                };
                tracing::info!(
                    buffered = runs.changes(),
                    "stop requested: reading no further changes; landing what is in flight"
                );
                self.settle(&mut runs, builds, lanes).await?;
                return Ok(BuilderEnd {
                    reader,
                    stopped: true,
                });
            }
            let building = builds.running() > 0;
            let quiet = building || coalescing.is_some();
            let deadline = coalescing.as_ref().map(|gathered| gathered.deadline);
            tokio::select! {
                run = runs.next(), if ended.is_none() => match run {
                    None => {
                        let end = reader_end.join().await?;
                        mark = if end.waiting {
                            self.waited(mark, None, &mut snapshot, quiet)
                        } else {
                            Instant::now()
                        };
                        ended = Some(end);
                    }
                    Some(run) => {
                        self.waited(mark, run.started(), &mut snapshot, quiet);
                        runs.push(run);
                        builds.woke(Wake::Change);
                        self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                        mark = Instant::now();
                    }
                },
                () = released(self.budget.as_deref(), builds.refused_since()),
                    if builds.refused().is_some() =>
                {
                    builds.woke(Wake::Released);
                    self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                    mark = Instant::now();
                }
                landed = builds.next(), if building => {
                    builds.woke(Wake::Landed);
                    self.landed(landed, &mut runs, &mut snapshot, builds, lanes).await?;
                    mark = Instant::now();
                }
                step = next_snapshot_step(&mut snapshot),
                    if snapshot.as_ref().is_some_and(Snapshot::wants_keys) =>
                {
                    let Some(active) = snapshot.as_mut() else { continue };
                    // The wait for a page, if any, is `Page`, unless builds
                    // ran meanwhile: then the loop was waiting on them too.
                    if !building {
                        active.page_wait += mark.elapsed();
                    }
                    match step {
                        Err(error) => return Err(error.into()),
                        Ok(Step::Keys) => active.take_keys(),
                        Ok(Step::Done) => active.done = true,
                    }
                    builds.woke(Wake::Page);
                    self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                    mark = Instant::now();
                },
                delivery = inbox.recv(), if snapshot.is_none() => {
                    let Some(delivery) = delivery else { continue };
                    let at = self.waited(mark, None, &mut snapshot, quiet);
                    signals.received();
                    let gathered = coalescing.get_or_insert_with(|| Coalescing {
                        deliveries: Vec::new(),
                        deadline: at + self.batch.max_delay,
                        started: at,
                    });
                    gathered.deliveries.push(delivery);
                    while let Ok(more) = inbox.try_recv() {
                        signals.received();
                        gathered.deliveries.push(more);
                    }
                    if everyone_asked(&gathered.deliveries, lanes) {
                        snapshot = self.start_coalesced(&mut coalescing, scopes, lanes).await?;
                    }
                    mark = Instant::now();
                }
                () = until(deadline), if deadline.is_some() => {
                    snapshot = self.start_coalesced(&mut coalescing, scopes, lanes).await?;
                    mark = Instant::now();
                }
                signal = signals.next() => {
                    mark = self.waited(mark, None, &mut snapshot, quiet);
                    // A backstop, not the mechanism: a batch the budget held
                    // back is granted on a release (the arm above). Should a
                    // release ever go unheard, the next tick still grants it,
                    // so it costs a second, never the run.
                    if signal == Signal::Tick && builds.refused().is_some() {
                        builds.woke(Wake::Tick);
                        self.grant(&mut runs, &mut snapshot, builds, lanes).await?;
                        mark = Instant::now();
                    }
                }
            }
        }
    }

    /// Start the gathered requests' snapshot, timed from the first request
    /// as `main` timed its gathering.
    async fn start_coalesced(
        &self,
        coalescing: &mut Option<Coalescing>,
        scopes: &HashSet<IndexName>,
        lanes: &Lanes,
    ) -> Result<Option<Snapshot>> {
        let Some(gathered) = coalescing.take() else {
            return Ok(None);
        };
        let snapshot = self
            .start_snapshot(gathered.deliveries, scopes, lanes)
            .await?;
        self.observer.on_stage(
            StageTiming::new(Stage::Snapshot, gathered.started.elapsed()).work(Work::Backfill),
        );
        Ok(snapshot)
    }
}

/// Whether every sink with a lane asked (`main`'s rule: requests come one per
/// index, and once every sink asked a snapshot starts at once).
fn everyone_asked(deliveries: &[Delivery<Request>], lanes: &Lanes) -> bool {
    let asked: HashSet<&SinkName> = deliveries
        .iter()
        .map(|delivery| {
            let Request::Backfill { sink, .. } = delivery.item();
            sink
        })
        .collect();
    lanes.keys().all(|lane| asked.contains(&lane.sink))
}

/// The coalescing deadline; never without one.
async fn until(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}
