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
//!   forced exit ([`forced`]: the same sender twice within 500 ms, or any
//!   three signals within 1 s). At-least-once delivery makes giving up safe:
//!   whatever was not acknowledged is read again on the next start.
//!
//! ```text
//! first signal ─▶ drain the pipeline ─▶ close HTTP ─▶ flush telemetry ─▶ exit 0
//!                 └──────────── at most --shutdown-timeout-secs ─────┘
//!                     deadline passed ─▶ log the step and the sinks ─▶ exit 1
//!                     forced          ─▶ exit 128 + n, at once
//! ```

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

/// How long a stop may take when `--shutdown-timeout-secs` isn't given. A
/// supervisor's grace period must be longer (the Helm chart sets 45 s).
pub(crate) const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// The step of a stop that lands what is in flight.
pub(crate) const DRAINING: &str = "draining the pipeline";

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
    Signal(StopSignal, Sender),
    /// `--exit-with-parent`: the process that started flusso is gone.
    ParentExited {
        parent: u32,
    },
}

/// The process's stop signals, caught from [`install`](Signals::install) on,
/// each with its [`Sender`].
///
/// On Unix they are read with `signal-hook`'s origin-carrying iterator on a
/// thread of their own (tokio's signals don't say who sent one) and handed
/// over a channel. The kernel keeps one pending signal of a kind, so two of
/// a kind sent at the same instant can arrive as one, from either sender.
#[derive(Debug)]
pub(crate) struct Signals {
    received: tokio::sync::mpsc::UnboundedReceiver<(StopSignal, Sender)>,
    #[cfg(unix)]
    handle: signal_hook::iterator::Handle,
}

#[cfg(unix)]
impl Drop for Signals {
    fn drop(&mut self) {
        self.handle.close();
    }
}

#[cfg(unix)]
/// Whether flusso should catch `SIGHUP` as a stop. Not when it started with
/// `SIGHUP` ignored (`nohup`, `trap '' HUP`): catching it would replace the
/// ignore, and a run meant to outlive its terminal would stop with it. Linux
/// reports the ignored set in `/proc/self/status` (`SigIgn`); where that
/// can't be read (macOS has no `/proc`), `SIGHUP` keeps its default action,
/// which ends the process, as before 0.18, and `nohup` keeps working.
pub(crate) fn catch_hangup() -> bool {
    match std::fs::read_to_string("/proc/self/status") {
        Ok(status) => !hangup_ignored(&status),
        Err(_) => false,
    }
}

#[cfg(unix)]
/// Whether a `/proc/<pid>/status` text lists `SIGHUP` (signal 1, the lowest
/// bit of the hex `SigIgn` mask) as ignored.
fn hangup_ignored(status: &str) -> bool {
    status
        .lines()
        .find_map(|line| line.strip_prefix("SigIgn:"))
        .and_then(|mask| u64::from_str_radix(mask.trim(), 16).ok())
        .is_some_and(|mask| mask & 1 == 1)
}

impl Signals {
    /// Catch the stop signals. Until this runs, each one ends the process
    /// the default way, at once.
    pub(crate) fn install() -> anyhow::Result<Self> {
        let (sent, received) = tokio::sync::mpsc::unbounded_channel();
        #[cfg(unix)]
        {
            use anyhow::Context;
            use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
            use signal_hook::iterator::SignalsInfo;
            use signal_hook::iterator::exfiltrator::WithOrigin;

            let mut kinds = vec![SIGINT, SIGTERM];
            if catch_hangup() {
                kinds.push(SIGHUP);
            } else {
                tracing::debug!("SIGHUP is ignored or can't be checked; leaving it alone");
            }
            let mut signals =
                SignalsInfo::<WithOrigin>::new(kinds).context("catching the stop signals")?;
            let handle = signals.handle();
            std::thread::Builder::new()
                .name("flusso-signals".to_owned())
                .spawn(move || {
                    for origin in signals.forever() {
                        let signal = match origin.signal {
                            SIGHUP => StopSignal::Hangup,
                            SIGINT => StopSignal::Interrupt,
                            _ => StopSignal::Terminate,
                        };
                        if sent.send((signal, sender_of(&origin))).is_err() {
                            return;
                        }
                    }
                })
                .context("starting the signal thread")?;
            Ok(Self { received, handle })
        }
        #[cfg(windows)]
        {
            use anyhow::Context;
            use tokio::signal::windows::{ctrl_break, ctrl_c, ctrl_close, ctrl_shutdown};

            let mut interrupt = ctrl_c().context("catching Ctrl-C")?;
            let mut interrupt_break = ctrl_break().context("catching Ctrl-Break")?;
            let mut close = ctrl_close().context("catching the console closing")?;
            let mut shutdown = ctrl_shutdown().context("catching the system shutting down")?;
            tokio::spawn(async move {
                loop {
                    let signal = tokio::select! {
                        Some(()) = interrupt.recv() => StopSignal::Interrupt,
                        Some(()) = interrupt_break.recv() => StopSignal::Interrupt,
                        Some(()) = close.recv() => StopSignal::Hangup,
                        Some(()) = shutdown.recv() => StopSignal::Terminate,
                        else => return,
                    };
                    if sent.send((signal, None)).is_err() {
                        return;
                    }
                }
            });
            Ok(Self { received })
        }
        #[cfg(not(any(unix, windows)))]
        {
            tokio::spawn(async move {
                while tokio::signal::ctrl_c().await.is_ok() {
                    if sent.send((StopSignal::Interrupt, None)).is_err() {
                        return;
                    }
                }
            });
            Ok(Self { received })
        }
    }

    /// The next stop signal and who sent it.
    pub(crate) async fn next(&mut self) -> (StopSignal, Sender) {
        match self.received.recv().await {
            Some(received) => received,
            None => std::future::pending().await,
        }
    }
}

/// Who sent a signal (see [`Sender`]). macOS fills in no `si_code`, and
/// `signal-hook` reports a sender with pid and uid `0` there as none (a
/// terminal's Ctrl-C): only the three-rapid rule applies to that signal.
#[cfg(unix)]
fn sender_of(origin: &signal_hook::low_level::siginfo::Origin) -> Sender {
    match (origin.process, origin.cause) {
        (Some(process), _) => Some(i64::from(process.pid)),
        (None, signal_hook::low_level::siginfo::Cause::Kernel) => Some(KERNEL),
        (None, _) => None,
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
        (signal, sender) = signals.next() => Cause::Signal(signal, sender),
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

/// The stop signal that forces the exit, by [`Repeats`]: `first` is when and
/// from whom the stop came (no sender: it wasn't a signal). Every other
/// repeat is handed to `repeated`, to tell the sender the stop goes on.
pub(crate) async fn forced(
    signals: &mut Signals,
    first: (tokio::time::Instant, Sender),
    mut repeated: impl FnMut(StopSignal),
) -> StopSignal {
    let mut repeats = Repeats::new(first.0, first.1);
    loop {
        let (signal, sender) = signals.next().await;
        tracing::debug!(
            signal = signal.name(),
            ?sender,
            rule = if sender.is_some() {
                "same sender twice within 500 ms"
            } else {
                "three within 1 s"
            },
            "stop signal during a stop"
        );
        match repeats.record(tokio::time::Instant::now(), sender) {
            Verdict::Forced => return signal,
            Verdict::Repeat => repeated(signal),
        }
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

/// Who sent a stop signal, when the platform says: the sending process's
/// id, `0` for a sender outside this pid namespace (`docker stop`, a
/// Kubernetes termination), or [`KERNEL`] for the kernel (a terminal's Ctrl-C
/// or hang-up). `None` when it can't be known.
///
/// The process, not its user: a user signalling a process group that holds
/// both a forwarding task runner and flusso reaches flusso twice at once,
/// once from their shell and once from the runner, both as that user. Those
/// are two senders, one request.
pub(crate) type Sender = Option<i64>;

/// The [`Sender`] of a signal the kernel raised.
#[cfg(unix)]
pub(crate) const KERNEL: i64 = -1;

/// How close two signals from one sender must be to force the exit: a
/// person pressing Ctrl-C twice. A stop loop re-sending `SIGTERM` every
/// second is slower and never forces.
pub(crate) const SAME_SENDER_WINDOW: Duration = Duration::from_millis(500);

/// How many stop signals inside [`BURST_WINDOW`] force the exit, whoever
/// sent them: the rule where senders are unknown, and the way to force with
/// `/bin/kill` or `pkill`, a new process each time. One Ctrl-C under a
/// forwarding task runner is two signals, so it takes three.
pub(crate) const BURST: usize = 3;

/// The window [`BURST`] signals must fall in.
pub(crate) const BURST_WINDOW: Duration = Duration::from_secs(1);

/// What a stop signal after the first means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The same request again: the drain carries on.
    Repeat,
    /// Exit now.
    Forced,
}

/// The stop signals seen so far, to tell a repeat from a request to force.
///
/// The same known sender twice inside [`SAME_SENDER_WINDOW`] forces, and so
/// do [`BURST`] signals inside [`BURST_WINDOW`] from anyone. Two signals from
/// different senders are one request (a terminal's Ctrl-C and a task
/// runner's forwarded copy).
#[derive(Debug)]
pub(crate) struct Repeats {
    seen: VecDeque<(tokio::time::Instant, Sender)>,
}

impl Repeats {
    /// After the first stop signal, at `at` from `sender`.
    pub(crate) fn new(at: tokio::time::Instant, sender: Sender) -> Self {
        Self {
            seen: VecDeque::from([(at, sender)]),
        }
    }

    /// Another stop signal, at `at` from `sender`.
    pub(crate) fn record(&mut self, at: tokio::time::Instant, sender: Sender) -> Verdict {
        while self
            .seen
            .front()
            .is_some_and(|(seen, _)| at.saturating_duration_since(*seen) > BURST_WINDOW)
        {
            self.seen.pop_front();
        }
        let same_sender = sender.is_some_and(|sender| {
            self.seen.iter().any(|(seen, from)| {
                *from == Some(sender) && at.saturating_duration_since(*seen) <= SAME_SENDER_WINDOW
            })
        });
        let forced = same_sender || self.seen.len() + 1 >= BURST;
        self.seen.push_back((at, sender));
        if forced {
            Verdict::Forced
        } else {
            Verdict::Repeat
        }
    }
}

/// Lets a message through at most once a second.
#[derive(Debug, Default)]
pub(crate) struct Throttle {
    last: Option<tokio::time::Instant>,
}

impl Throttle {
    /// Whether to show the message now.
    pub(crate) fn allow(&mut self, at: tokio::time::Instant) -> bool {
        let allowed = self
            .last
            .is_none_or(|last| at.saturating_duration_since(last) >= Duration::from_secs(1));
        if allowed {
            self.last = Some(at);
        }
        allowed
    }
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
        // Finished work wins a tie with the deadline or a signal.
        biased;
        done = drain => Drained::Done(done),
        () = tokio::time::sleep(timeout) => Drained::TimedOut { step: progress.current() },
        signal = interrupt => Drained::Forced(signal),
    }
}

/// The sinks a drain cut short was still waiting on, for the log line:
/// `primary: 120 changes in flight, failed`, and each lane of it that is not
/// written: `primary: 3 changes in flight, lane users blocked since
/// 2026-11-19T12:03:11Z (blocked: index [users] blocked by: …)`. Empty when
/// every sink is done.
pub(crate) fn unfinished_sinks(status: &daemon::StatusSnapshot) -> String {
    status
        .sinks
        .iter()
        .filter_map(|(name, sink)| {
            let failed = sink.phase == daemon::SinkPhase::Failed;
            let lanes: Vec<String> = sink
                .lanes
                .iter()
                .filter_map(|(index, lane)| lane_note(index, lane))
                .collect();
            (sink.changes_in_flight > 0 || failed || !lanes.is_empty()).then(|| {
                let mut said = format!("{name}: {} changes in flight", sink.changes_in_flight);
                if failed {
                    said.push_str(", failed");
                }
                for lane in lanes {
                    said.push_str(", ");
                    said.push_str(&lane);
                }
                said
            })
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// The indexes a stop cut short leaves documents read and not built on, for
/// the log line: `orders: 1200 documents read and not built`. Empty when
/// every document read was built (ADR 0012 stage 8a, O3).
pub(crate) fn unbuilt_indexes(waiting: &[(kernel::IndexName, usize)]) -> String {
    waiting
        .iter()
        .map(|(index, count)| format!("{index}: {count} documents read and not built"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// `lane users refused since … (refused: id 42 …)` for a lane that is not
/// written; `None` for an open one.
fn lane_note(index: &str, lane: &daemon::LaneSnapshot) -> Option<String> {
    if lane.health == daemon::LaneHealthState::Open {
        return None;
    }
    let mut note = format!("lane {index} {}", lane.health.name());
    if let Some(since) = &lane.parked_since {
        note.push_str(&format!(" since {since}"));
    }
    if let Some(reason) = &lane.reason {
        note.push_str(&format!(" ({reason})"));
    }
    Some(note)
}

#[cfg(test)]
mod tests;
