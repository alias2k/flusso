//! How `flusso run` stops: the signals it honours, the parent it may follow,
//! and the deadline on the stop (issue #371).
//!
//! - **What stops it.** `SIGINT`, `SIGTERM` and `SIGHUP` ([`Signals`]), and,
//!   with `--exit-with-parent`, the parent process exiting ([`parent_exited`]).
//!   [`requested`] resolves on the first of them.
//! - **What a stop does.** A graceful drain, owned by the daemon: nothing new
//!   is read, the batch in flight is published, every sink flushes and
//!   acknowledges its lane, the final position is confirmed to the source and
//!   the replication stream closed. Then the HTTP surfaces close and telemetry
//!   is flushed. The whole of it runs under [`within`]: a deadline, and a
//!   second signal that ends it at once. At-least-once delivery makes giving
//!   up safe: whatever was not acknowledged is read again on the next start.
//!
//! ```text
//! first signal ─▶ drain the pipeline ─▶ close HTTP ─▶ flush telemetry ─▶ exit 0
//!                 └──────────── at most --shutdown-timeout-secs ─────┘
//!                     deadline passed ─▶ log the step and the sinks ─▶ exit 1
//!                     second signal   ─▶ exit 128 + n, at once
//! ```

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How long a stop may take when `--shutdown-timeout-secs` isn't given. A
/// supervisor's grace period must be longer (the Helm chart sets 45 s).
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// The step of a stop that lands what is in flight.
pub(crate) const DRAINING: &str = "draining the pipeline";

/// A stop signal this soon after the stop began is the same request, not a
/// second one: Ctrl-C or a closed terminal reaches flusso directly (the
/// kernel signals the whole foreground process group) and again through a
/// task runner that forwards it, milliseconds apart. A person pressing
/// Ctrl-C again to force the exit takes longer than this.
pub(crate) const REPEAT_WINDOW: Duration = Duration::from_secs(1);

/// The most closing the HTTP surfaces, or flushing telemetry, may take of
/// what is left of the deadline. Neither is the data path: giving up on one
/// is logged and the stop still ends with exit 0.
pub(crate) const SIDE_STEP: Duration = Duration::from_secs(5);

/// How often `--exit-with-parent` checks the parent process.
const PARENT_POLL: Duration = Duration::from_millis(500);

/// A signal flusso stops on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopSignal {
    Interrupt,
    Terminate,
    Hangup,
}

impl StopSignal {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Interrupt => "SIGINT",
            Self::Terminate => "SIGTERM",
            Self::Hangup => "SIGHUP",
        }
    }

    /// The shell's code for a process ended by this signal (`128 + n`).
    pub(crate) fn exit_code(self) -> i32 {
        128 + match self {
            Self::Hangup => 1,
            Self::Interrupt => 2,
            Self::Terminate => 15,
        }
    }
}

/// Why the run is stopping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cause {
    Signal(StopSignal),
    /// `--exit-with-parent`: the process that started flusso is gone.
    ParentExited {
        parent: u32,
    },
}

/// The process's stop signals, caught from [`install`](Signals::install) on.
#[derive(Debug)]
pub(crate) struct Signals {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(unix)]
    hangup: tokio::signal::unix::Signal,
}

impl Signals {
    /// Catch the stop signals. Until this runs, each one ends the process
    /// the default way, at once.
    pub(crate) fn install() -> anyhow::Result<Self> {
        #[cfg(unix)]
        {
            use anyhow::Context;
            use tokio::signal::unix::{SignalKind, signal};
            Ok(Self {
                interrupt: signal(SignalKind::interrupt()).context("catching SIGINT")?,
                terminate: signal(SignalKind::terminate()).context("catching SIGTERM")?,
                hangup: signal(SignalKind::hangup()).context("catching SIGHUP")?,
            })
        }
        #[cfg(not(unix))]
        Ok(Self {})
    }

    /// The next stop signal.
    pub(crate) async fn next(&mut self) -> StopSignal {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = self.interrupt.recv() => StopSignal::Interrupt,
                _ = self.terminate.recv() => StopSignal::Terminate,
                _ = self.hangup.recv() => StopSignal::Hangup,
            }
        }
        #[cfg(not(unix))]
        {
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
            StopSignal::Interrupt
        }
    }
}

/// The first stop: a signal, or the exit of `parent` (the pid
/// [`parent`] returned when flusso started) when one is followed.
pub(crate) async fn requested(signals: &mut Signals, parent: Option<u32>) -> Cause {
    let exited = async {
        match parent {
            Some(parent) => parent_exited(parent, PARENT_POLL).await,
            None => std::future::pending().await,
        }
    };
    tokio::select! {
        signal = signals.next() => Cause::Signal(signal),
        parent = exited => Cause::ParentExited { parent },
    }
}

/// flusso's parent process, to follow with `--exit-with-parent`. `None` where
/// there is nothing to follow: a parent that is already `init` (a
/// container's PID 1 has none), or a platform without `getppid`.
pub(crate) fn parent() -> Option<u32> {
    #[cfg(unix)]
    {
        let parent = std::os::unix::process::parent_id();
        if parent > 1 {
            return Some(parent);
        }
        tracing::info!(
            parent,
            "--exit-with-parent: flusso was started by init, so there is no parent to follow"
        );
        None
    }
    #[cfg(not(unix))]
    {
        tracing::warn!("--exit-with-parent is not supported on this platform; ignoring it");
        None
    }
}

/// Resolves with `parent` once flusso's parent process is no longer it: the
/// process that started flusso exited and flusso was re-parented.
pub(crate) async fn parent_exited(parent: u32, poll: Duration) -> u32 {
    let mut tick = tokio::time::interval(poll);
    loop {
        tick.tick().await;
        if current_parent() != Some(parent) {
            return parent;
        }
    }
}

fn current_parent() -> Option<u32> {
    #[cfg(unix)]
    {
        Some(std::os::unix::process::parent_id())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// The step a stop is on, for the line that says what it gave up on.
#[derive(Debug, Clone)]
pub(crate) struct Progress(Arc<Mutex<&'static str>>);

impl Default for Progress {
    fn default() -> Self {
        Self(Arc::new(Mutex::new("stopping the deployment")))
    }
}

impl Progress {
    pub(crate) fn set(&self, step: &'static str) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = step;
    }

    pub(crate) fn current(&self) -> &'static str {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How a bounded stop ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Drained<T> {
    /// Everything finished in time.
    Done(T),
    /// The deadline passed during `step`.
    TimedOut { step: &'static str },
    /// Another stop signal arrived first.
    Forced(StopSignal),
}

/// The next stop signal that is a second request: one arriving within
/// [`REPEAT_WINDOW`] of `began` is the first one again and is ignored.
pub(crate) async fn second_signal(
    signals: &mut Signals,
    began: tokio::time::Instant,
) -> StopSignal {
    loop {
        let signal = signals.next().await;
        if began.elapsed() >= REPEAT_WINDOW {
            return signal;
        }
        tracing::info!(
            signal = signal.name(),
            "the same stop request again (a forwarded copy); still stopping"
        );
    }
}

/// Run `step`, giving up after [`SIDE_STEP`] or just before `deadline`,
/// whichever is sooner; `false` when it was given up on. Ending before the
/// deadline keeps a side step from turning a finished drain into exit 1.
pub(crate) async fn side_step(
    deadline: tokio::time::Instant,
    step: impl Future<Output = ()>,
) -> bool {
    let before_deadline = deadline
        .checked_sub(Duration::from_millis(100))
        .unwrap_or(deadline);
    let until = before_deadline.min(tokio::time::Instant::now() + SIDE_STEP);
    tokio::time::timeout_at(until, step).await.is_ok()
}

/// Run `drain` for at most `timeout`, or until `interrupt` (the next signal)
/// resolves, whichever comes first.
pub(crate) async fn within<T>(
    timeout: Duration,
    progress: &Progress,
    interrupt: impl Future<Output = StopSignal>,
    drain: impl Future<Output = T>,
) -> Drained<T> {
    tokio::select! {
        done = drain => Drained::Done(done),
        () = tokio::time::sleep(timeout) => Drained::TimedOut { step: progress.current() },
        signal = interrupt => Drained::Forced(signal),
    }
}

/// The sinks a drain cut short was still waiting on, for the log line:
/// `primary: 120 changes in flight, failed`. Empty when every sink is done.
pub(crate) fn unfinished_sinks(status: &daemon::StatusSnapshot) -> String {
    status
        .sinks
        .iter()
        .filter_map(|(name, sink)| {
            let failed = sink.phase == daemon::SinkPhase::Failed;
            (sink.changes_in_flight > 0 || failed).then(|| {
                let mut said = format!("{name}: {} changes in flight", sink.changes_in_flight);
                if failed {
                    said.push_str(", failed");
                }
                said
            })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests;
