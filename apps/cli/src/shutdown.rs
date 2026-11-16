//! How `flusso run` stops: the signals it honours, the parent it may follow,
//! and the deadline on what it does after (issue #371).
//!
//! - **What stops it.** `SIGINT`, `SIGTERM` and `SIGHUP` ([`Signals`]), and,
//!   with `--exit-with-parent`, the parent process exiting ([`parent_exited`]).
//!   [`requested`] resolves on the first of them.
//! - **What a stop does.** The daemon drops the deployment at once: an unacked
//!   batch is redelivered on the next start (at-least-once), so nothing waits
//!   on the source or the sinks. What is left (closing the HTTP surfaces,
//!   flushing telemetry) runs under [`within`]: it gets a deadline, and a
//!   second signal ends it at once. Either way the caller exits the process,
//!   whatever is still blocked.
//!
//! ```text
//! first signal ─▶ daemon dropped ─▶ HTTP surfaces closed ─▶ telemetry flushed ─▶ exit 0
//!                 └──────────── at most --shutdown-timeout-secs ────────────┘
//!                         deadline passed ─▶ log the step ─▶ exit 1
//!                         second signal   ─▶ exit 128 + n, at once
//! ```

use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How long a stop may take when `--shutdown-timeout-secs` isn't given:
/// under Docker's 10 s and Kubernetes' 30 s grace periods.
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 5;

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

#[cfg(test)]
mod tests;
