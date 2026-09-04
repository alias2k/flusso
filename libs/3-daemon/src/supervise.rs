//! The supervisor: stage every sink engine, then run the ingest engine and the
//! sink engines as independent tasks, restarting a failed one with backoff
//! while the others keep going.
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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use engine::{IngestEngine, SinkControl, SinkEngine};
use futures::future::{self, FutureExt};
use kernel::{IndexName, SinkName};
use source::cdc::{ChangeCapture, Continuity};
use stream::Stream;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::DaemonOptions;

const CONTROL_CAPACITY: usize = 8;

/// How long to wait for the sinks to drain their lanes once the source stream
/// ended before stopping them anyway.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// The operations a transport can perform on a running deployment.
#[derive(Debug, Clone)]
pub struct DaemonControl {
    senders: Arc<BTreeMap<SinkName, mpsc::Sender<SinkControl>>>,
    receivers: Arc<std::sync::Mutex<BTreeMap<SinkName, mpsc::Receiver<SinkControl>>>>,
}

/// Why an operation was refused.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("unknown sink `{0}`")]
    UnknownSink(String),
    #[error("sink `{0}` is not accepting operations (its engine has stopped for good)")]
    Closed(String),
    #[error("too many operations queued for sink `{0}`; retry shortly")]
    Busy(String),
}

impl DaemonControl {
    pub(crate) fn new(sinks: &[SinkName]) -> Self {
        let mut senders = BTreeMap::new();
        let mut receivers = BTreeMap::new();
        for sink in sinks {
            let (tx, rx) = mpsc::channel(CONTROL_CAPACITY);
            senders.insert(sink.clone(), tx);
            receivers.insert(sink.clone(), rx);
        }
        Self {
            senders: Arc::new(senders),
            receivers: Arc::new(std::sync::Mutex::new(receivers)),
        }
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

    fn take_receiver(&self, sink: &SinkName) -> Option<mpsc::Receiver<SinkControl>> {
        self.receivers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(sink)
    }
}

/// Stage every sink, then run everything supervised until the ingest engine
/// ends its run cleanly.
pub(crate) async fn run_all(
    ingest: IngestEngine,
    sink_engines: Vec<SinkEngine>,
    control: DaemonControl,
    continuity: Continuity,
    source: &Arc<dyn ChangeCapture>,
    stream: &Arc<dyn Stream>,
    options: &DaemonOptions,
) -> anyhow::Result<()> {
    let max_backoff = options.max_restart_backoff;

    // First staging, every sink, retried until it succeeds: the ingest engine
    // must not create the resume point before every rebuild is staged.
    future::join_all(
        sink_engines
            .iter()
            .map(|engine| stage_until_ok(engine, continuity, max_backoff).map(|()| ())),
    )
    .await;

    let mut tasks = JoinSet::new();
    for engine in sink_engines {
        let Some(mut receiver) = control.take_receiver(engine.name()) else {
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

    let ingest_task = async move {
        let mut backoff = Backoff::new(max_backoff);
        loop {
            match ingest.run().await {
                Ok(()) => return Ok::<(), anyhow::Error>(()),
                Err(_) => backoff.wait().await,
            }
        }
    };

    tokio::select! {
        result = ingest_task => {
            // The source stream ended: let the sinks finish what is already on
            // their lanes, confirm the final watermark, then stop them.
            let drained = tokio::time::timeout(DRAIN_TIMEOUT, async {
                while !stream.is_idle() {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await;
            if drained.is_err() {
                tracing::warn!("sinks did not drain their lanes in time; stopping them");
            }
            if let Some(watermark) = stream.watermark() {
                source.confirm(watermark);
            }
            tasks.abort_all();
            result
        }
        joined = tasks.join_next(), if !tasks.is_empty() => {
            match joined {
                Some(Ok(Ok(()))) | None => {
                    // A sink engine's lane closed for good; the others and the
                    // ingest engine carry on.
                    Ok(())
                }
                Some(Ok(Err(error))) => Err(error),
                Some(Err(join)) => Err(anyhow::anyhow!("sink engine task failed: {join}")),
            }
        }
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
