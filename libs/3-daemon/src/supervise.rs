//! The supervisor: stage every sink engine, then run the ingest engine and the
//! sink engines as independent `tokio` tasks, restarting a failed one with
//! backoff while the others keep going. The supervisor itself only waits on
//! join handles, so no engine's loop can stall it.
//!
//! Ordering is what this module exists for. Every sink engine finishes its
//! first [`stage`](engine::SinkEngine::stage) (ensure indexes, stage stale rebuilds under a
//! `Fresh` source, queue its backfill request) **before** the ingest engine
//! runs [`ChangeCapture::prepare`] and
//! starts serving requests. That is the #120 guarantee without a shared engine:
//! rebuilds are staged before the resume point exists, and the resume point
//! exists before any snapshot runs.
//!
//! Isolation, not fail-fast: a sink engine that stops on an error (a flush-wide
//! failure, or a `stop` policy rejection) is restarted with exponential backoff
//! and its lane redelivers the batch it left unacknowledged, so a poison
//! document loops on that one sink while the others continue. Its lane fills up
//! meanwhile, so with the in-process stream the ingest engine eventually waits
//! on it: a stalled sink pins WAL until it recovers, and the per-sink lag and
//! in-flight numbers are the alarm. The ingest engine ending its run cleanly
//! (the live stream closed) ends the deployment.
//!
//! A [`Stop`] ends it gracefully instead: a stop during staging ends the run
//! before anything is captured; afterwards the ingest engine winds down (no
//! further read, the buffered batch published, every lane drained and
//! confirmed, the live stream closed), is not restarted, and the sink engines
//! are stopped once their lanes are idle. Nothing here bounds that: the
//! caller who asked for the stop owns the deadline.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use engine::{IngestEngine, SinkControl, SinkEngine, Stop, drain_lanes};
use futures::future::{self, FutureExt};
use kernel::{IndexName, SinkName};
use source::cdc::{ChangeCapture, Continuity};
use stream::Stream;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::DaemonOptions;
use crate::fence::Fence;

const CONTROL_CAPACITY: usize = 8;

/// How long to wait for the sinks to drain their lanes once the source stream
/// ended before stopping them anyway.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// The operations a transport can perform on a running deployment.
#[derive(Debug, Clone)]
pub struct DaemonControl {
    senders: Arc<BTreeMap<SinkName, mpsc::Sender<SinkControl>>>,
    stop: Stop,
}

/// The receiving end of each sink engine's control channel, by sink.
pub(crate) type ControlReceivers = BTreeMap<SinkName, mpsc::Receiver<SinkControl>>;

/// Why an operation was refused.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// No sink of that name is configured.
    #[error("unknown sink `{0}`")]
    UnknownSink(String),
    /// The sink's engine ended for good, so nothing would act on the operation.
    #[error("sink `{0}` is not accepting operations (its engine has stopped for good)")]
    Closed(String),
    /// The sink's operation queue is full; the engine is busy between batches.
    #[error("too many operations queued for sink `{0}`; retry shortly")]
    Busy(String),
    /// The deployment is stopping and takes no new work.
    #[error("the deployment is stopping and takes no new operations")]
    Stopping,
}

impl DaemonControl {
    /// One control channel per sink: the handle keeps the senders, the
    /// supervisor hands each receiver to its sink engine.
    pub(crate) fn new(sinks: &[SinkName], stop: Stop) -> (Self, ControlReceivers) {
        let mut senders = BTreeMap::new();
        let mut receivers = BTreeMap::new();
        for sink in sinks {
            let (tx, rx) = mpsc::channel(CONTROL_CAPACITY);
            senders.insert(sink.clone(), tx);
            receivers.insert(sink.clone(), rx);
        }
        (
            Self {
                senders: Arc::new(senders),
                stop,
            },
            receivers,
        )
    }

    /// The sinks that accept operations.
    pub fn sinks(&self) -> impl Iterator<Item = &SinkName> {
        self.senders.keys()
    }

    /// Rebuild `index` into a fresh generation on `sink`, or on every sink when
    /// `sink` is `None`. Each targeted engine stages the rebuild between two
    /// batches and requests its own snapshot; concurrent requests for the same
    /// index coalesce on the ingest side into one pass over the table.
    pub fn reindex(&self, index: IndexName, sink: Option<&SinkName>) -> Result<(), ControlError> {
        if self.stop.is_requested() {
            return Err(ControlError::Stopping);
        }
        let targets: Vec<(&SinkName, &mpsc::Sender<SinkControl>)> = match sink {
            Some(sink) => vec![(
                sink,
                self.senders
                    .get(sink)
                    .ok_or_else(|| ControlError::UnknownSink(sink.to_string()))?,
            )],
            None => self.senders.iter().collect(),
        };
        for (name, sender) in targets {
            sender
                .try_send(SinkControl::Reindex {
                    indexes: vec![index.clone()],
                })
                .map_err(|error| match error {
                    mpsc::error::TrySendError::Full(_) => ControlError::Busy(name.to_string()),
                    mpsc::error::TrySendError::Closed(_) => ControlError::Closed(name.to_string()),
                })?;
        }
        Ok(())
    }
}

/// Stage every sink, then run everything supervised until the ingest engine
/// ends its run cleanly: the live stream ended, or `stop` was requested and the
/// ingest engine wound down.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_all(
    ingest: IngestEngine,
    sink_engines: Vec<SinkEngine>,
    mut control: ControlReceivers,
    continuity: Continuity,
    source: &Arc<dyn ChangeCapture>,
    stream: &Arc<dyn Stream>,
    options: &DaemonOptions,
    stop: &Stop,
    fence: &Fence,
) -> anyhow::Result<()> {
    let max_backoff = options.max_restart_backoff;

    // First staging, every sink, retried until it succeeds: the ingest engine
    // must not create the resume point before every rebuild is staged. A stop
    // meanwhile ends the run: nothing has been captured yet.
    let staged = future::join_all(
        sink_engines
            .iter()
            .map(|engine| stage_until_ok(engine, continuity, max_backoff).map(|()| ())),
    );
    tokio::select! {
        _ = staged => {}
        () = stop.requested() => {
            tracing::info!("stop requested while staging; nothing was captured");
            return Ok(());
        }
    }

    let mut tasks = JoinSet::new();
    for engine in sink_engines {
        let Some(mut receiver) = control.remove(engine.name()) else {
            anyhow::bail!("no control channel for sink `{}`", engine.name());
        };
        tasks.spawn(async move {
            let mut backoff = Backoff::new(max_backoff);
            let mut first = true;
            loop {
                if !first {
                    stage_until_ok(&engine, Continuity::Resumed, max_backoff).await;
                }
                first = false;
                match engine.run(&mut receiver).await {
                    Ok(()) => return Ok::<(), anyhow::Error>(()),
                    Err(_) => backoff.wait().await,
                }
            }
        });
    }

    let ingest_stop = stop.clone();
    let mut ingest_task = AbortOnDrop(tokio::spawn(async move {
        let mut backoff = Backoff::new(max_backoff);
        loop {
            match ingest.run().await {
                Ok(()) => return Ok::<(), anyhow::Error>(()),
                // A failed engine can't wind down; a restart would read anew.
                Err(error) if ingest_stop.is_requested() => {
                    return Err(anyhow::anyhow!(
                        "the ingest engine failed while stopping: {error}"
                    ));
                }
                Err(_) => {
                    tokio::select! {
                        () = backoff.wait() => {}
                        () = ingest_stop.requested() => {
                            return Err(anyhow::anyhow!(
                                "stop requested while the ingest engine was restarting; \
                                 nothing in flight was published"
                            ));
                        }
                    }
                }
            }
        }
    }));

    loop {
        tokio::select! {
            joined = &mut ingest_task.0 => {
                let result = match joined {
                    Ok(result) => result,
                    Err(join) => Err(anyhow::anyhow!("ingest engine task failed: {join}")),
                };
                // The ingest engine is done: let the sinks finish what is
                // already on their lanes, confirming as they go, then stop
                // them. After a stop the engine drained them already.
                let mut confirmed = None;
                let drained = tokio::time::timeout(
                    DRAIN_TIMEOUT,
                    drain_lanes(&**stream, &**source, &mut confirmed),
                )
                .await;
                if drained.is_err() {
                    tracing::warn!("sinks did not drain their lanes in time; stopping them");
                } else if stop.is_requested() {
                    tracing::info!("every sink flushed; waiting for open searchable fences to be answered");
                    fence.settled().await;
                }
                tasks.abort_all();
                return result;
            }
            joined = tasks.join_next(), if !tasks.is_empty() => match joined {
                // A sink engine's lane closed for good; the ingest engine and
                // the other sinks carry on.
                Some(Ok(Ok(()))) | None => {}
                Some(Ok(Err(error))) => return Err(error),
                Some(Err(join)) => return Err(anyhow::anyhow!("sink engine task failed: {join}")),
            },
        }
    }
}

/// A spawned engine task that is cancelled with its supervisor: dropping the
/// supervisor's future (a shutdown) must not leave the engine running.
#[derive(Debug)]
struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn stage_until_ok(engine: &SinkEngine, continuity: Continuity, max_backoff: Duration) {
    let mut backoff = Backoff::new(max_backoff);
    loop {
        match engine.stage(continuity).await {
            Ok(()) => return,
            Err(error) => {
                tracing::error!(sink = %engine.name(), %error, "staging failed; retrying");
                backoff.wait().await;
            }
        }
    }
}

/// Exponential backoff from one second, doubling to a cap.
#[derive(Debug)]
struct Backoff {
    next: Duration,
    max: Duration,
}

impl Backoff {
    fn new(max: Duration) -> Self {
        Self {
            next: Duration::from_secs(1).min(max),
            max,
        }
    }

    async fn wait(&mut self) {
        tracing::info!(after = ?self.next, "restarting engine after backoff");
        tokio::time::sleep(self.next).await;
        self.next = (self.next * 2).min(self.max);
    }
}
