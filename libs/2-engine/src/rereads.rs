//! What the sink engines need of the source once it holds the slot (ADR 0012
//! stage 10, O29 and S3).
//!
//! **[`Rereads`]: the detach records are read again once the slot is held.**
//! A process stages before its live stream opens. Another process can still
//! hold the slot then, and detach a lane after this one read its records.
//! Every read before the slot is held is only advisory. So:
//!
//! - the ingest engine opens its live stream through [`Rereads::open`]:
//!   every sink counts as unread from before the new stream can answer, and
//!   only a read begun once the opening returned counts again;
//! - a sink engine whose last read of its records counts for an earlier
//!   opening reads them again before it writes anything or opens a lane
//!   ([`Rereads::is_due`]), and says so ([`Rereads::read`]); every path
//!   that opens a lane or writes, and its check, is listed in the sink
//!   engine's module doc;
//! - a fence vouches for nothing on a lane not read since
//!   ([`Rereads::current`]): it answers `starting`. The opening wakes every
//!   sink engine ([`Rereads::openings`]), so the read doesn't wait for
//!   traffic.
//!
//! The order lives in [`open`](Rereads::open) alone, the one place outside
//! tests that opens a live stream (a test here fails if another appears), so
//! nothing opens the window between the new stream answering and the sinks
//! counting as unread. [`Rereads::closed`] says the stream ended: until the next opening
//! nothing settles a gap end ([`Rereads::holding`]).
//!
//! Nothing waits on a sink: one whose destination is down reads again when
//! its run comes back, and its lanes hold the slot meanwhile. The other
//! sinks go on.
//!
//! **[`SourcePositions`]: a lane's place in the source's own text.** A
//! reattach accepts a gap: the changes from where reads stood at the detach
//! to where the lane was attached again. The sink keeps both ends with the
//! index, in the source's own text, so a fence after a restart refuses the
//! same positions. Both ends are the source's answer for the lane
//! ([`ChangeCapture::reached`]) at one moment: at the detach, before the
//! lane leaves the minimum; at the attach, before the lane lands anything.
//!
//! ```
//! use engine::Rereads;
//! use kernel::{IndexName, SinkName};
//! use stream::LaneKey;
//!
//! # tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
//! let rereads = Rereads::default();
//! let primary = LaneKey::new(
//!     IndexName::try_new("users").unwrap(),
//!     SinkName::try_new("primary").unwrap(),
//! );
//! assert!(rereads.current(&primary), "nothing opened yet");
//! let read_at = rereads.read_begins();
//! rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
//! assert!(rereads.is_due(read_at));
//! assert!(!rereads.current(&primary));
//! assert!(rereads.holding());
//! rereads.read(&primary, rereads.read_begins());
//! assert!(rereads.current(&primary));
//! rereads.closed();
//! assert!(!rereads.holding());
//! # });
//! ```

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use source::cdc::{ChangeCapture, Landed};
use stream::LaneKey;

/// How many times the live stream opened, and how far each sink read its
/// detach records since. Shared by the ingest engine, the sink engines and
/// the fence.
pub struct Rereads {
    /// Openings begun.
    opened: AtomicU64,
    /// The last opening that returned Ok: a read begun from here counts for
    /// it.
    held: AtomicU64,
    /// The stream of the last opening is open.
    holding: AtomicBool,
    /// The last opening's run ended cleanly ([`finished`](Rereads::finished)).
    finished: AtomicBool,
    /// An ingest engine opens the live stream through this
    /// ([`will_open`](Rereads::will_open)).
    opener: AtomicBool,
    read: Mutex<BTreeMap<LaneKey, u64>>,
    listeners: Mutex<Vec<RereadListener>>,
    /// The last opening that returned Ok, for sink engines to wake on: an
    /// unread lane vouches for nothing, so its read can't wait for traffic.
    openings: tokio::sync::watch::Sender<u64>,
}

impl Default for Rereads {
    fn default() -> Self {
        Self {
            opened: AtomicU64::new(0),
            held: AtomicU64::new(0),
            holding: AtomicBool::new(false),
            finished: AtomicBool::new(false),
            opener: AtomicBool::new(false),
            read: Mutex::new(BTreeMap::new()),
            listeners: Mutex::new(Vec::new()),
            openings: tokio::sync::watch::Sender::new(0),
        }
    }
}

/// What a [`Rereads`] listener hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reread {
    /// An opening begins: every lane's records are unread from here.
    Opening,
    /// A lane's records were read.
    Read,
}

/// Called on each [`Reread`]: cheap, it only wakes or notes something.
pub type RereadListener = Arc<dyn Fn(Reread) + Send + Sync>;

impl std::fmt::Debug for Rereads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rereads")
            .field("opened", &self.opened)
            .field("held", &self.held)
            .field("holding", &self.holding)
            .field("read", &self.read)
            .finish_non_exhaustive()
    }
}

impl Rereads {
    /// Open the live stream with `opening` (the source's `live()`): the one
    /// place it is opened outside tests. Before `opening` runs, every sink counts as unread and
    /// listeners hear [`Reread::Opening`] while only the previous stream can
    /// answer; once it returns Ok, a read begun from then counts. Failed,
    /// nothing counts as read until an opening succeeds.
    pub async fn open<T, E>(&self, opening: impl Future<Output = Result<T, E>>) -> Result<T, E> {
        let epoch = self.opened.fetch_add(1, Ordering::AcqRel) + 1;
        self.holding.store(false, Ordering::Release);
        self.finished.store(false, Ordering::Release);
        self.tell(Reread::Opening);
        let opened = opening.await;
        if opened.is_ok() {
            self.held.store(epoch, Ordering::Release);
            self.holding.store(true, Ordering::Release);
            self.openings.send_replace(epoch);
        }
        opened
    }

    /// Wakes on every opening that returned Ok: a sink engine reads its
    /// records at once, not at its next write or idle tick.
    pub fn openings(&self) -> tokio::sync::watch::Receiver<u64> {
        self.openings.subscribe()
    }

    /// The stream of the last opening ended: this process may no longer hold
    /// the slot.
    pub fn closed(&self) {
        self.holding.store(false, Ordering::Release);
    }

    /// The run of the last opening ended cleanly: the source finished its
    /// own stream, or a stop drained and confirmed. The deployment ends
    /// after the lanes drain, with no restart another process could precede,
    /// so what the lanes still hold may seed ([`may_erase_records`](Self::may_erase_records)).
    /// The slot still counts as not held for a gap end.
    pub fn finished(&self) {
        self.finished.store(true, Ordering::Release);
    }

    /// Whether the stream of the last opening is open: this process holds
    /// the slot.
    pub fn holding(&self) -> bool {
        self.holding.load(Ordering::Acquire)
    }

    /// An ingest engine opens the live stream through this: from now on
    /// nothing erases a record unless the stream is held
    /// ([`may_erase_records`](Self::may_erase_records)), before the first
    /// opening too (a process whose `prepare` keeps failing while another
    /// holds the slot).
    pub fn will_open(&self) {
        self.opener.store(true, Ordering::Release);
    }

    /// Whether this process may erase, or loosen, a record another process
    /// may rely on (O29, the fifth re-check): only while it holds the slot,
    /// or once its run ended cleanly (the deployment ends after the drain).
    /// With nothing to open a stream here (no [`will_open`](Self::will_open)
    /// and no opening counted: a sink engine on its own, as in tests) there
    /// is no slot to hold, and it may: as on `main`.
    pub fn may_erase_records(&self) -> bool {
        self.holding()
            || self.finished.load(Ordering::Acquire)
            || (self.opened() == 0 && !self.opener.load(Ordering::Acquire))
    }

    /// Hear every opening and every read, for a fence to note and wake on.
    pub fn subscribe(&self, listener: RereadListener) {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(listener);
    }

    fn tell(&self, event: Reread) {
        let listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(event);
        }
    }

    /// How many openings began: a source that counts the streams it opened
    /// ([`ChangeCapture::streams_opened`]) never counts more, unless a stream
    /// was opened past [`open`](Self::open).
    pub fn opened(&self) -> u64 {
        self.opened.load(Ordering::Acquire)
    }

    /// What a read of the records beginning now counts for: the last
    /// opening that returned. A sink engine takes it before it reads, and
    /// hands it to [`read`](Self::read) after.
    pub fn read_begins(&self) -> u64 {
        self.held.load(Ordering::Acquire)
    }

    /// Whether a read that counts for `read_at` is older than the last
    /// opening that returned. One atomic load.
    pub fn is_due(&self, read_at: u64) -> bool {
        self.read_begins() > read_at
    }

    /// `lane`'s records were read, the read counting for `read_at`. Reads
    /// count per lane, never per sink: a sink whose read missed one index
    /// leaves that lane unread (review of the third fix round, B1''').
    pub fn read(&self, lane: &LaneKey, read_at: u64) {
        {
            let mut read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
            let at = read.entry(lane.clone()).or_default();
            *at = (*at).max(read_at);
        }
        self.tell(Reread::Read);
    }

    /// Whether `lane`'s records were read since the last opening began, the
    /// read begun once it returned. True before any opening: nothing
    /// vouches before then anyway.
    pub fn current(&self, lane: &LaneKey) -> bool {
        let opened = self.opened.load(Ordering::Acquire);
        opened == 0
            || (self.read_begins() == opened
                && self
                    .read
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(lane)
                    .is_some_and(|at| *at >= opened))
    }
}

/// A lane's place in the source's own terms, as text: the source's answer for
/// what the lane landed ([`ChangeCapture::reached`]), printed as the source
/// prints it. `None` while the source can't say (no live stream yet).
pub trait SourcePositions: Debug + Send + Sync {
    /// Where `landed` stands in the source's own text.
    fn place(&self, landed: Landed) -> Option<String>;

    /// Whether `text` is a position this source reads.
    fn parses(&self, text: &str) -> bool;

    /// How two positions in the source's text compare; `None` when either
    /// doesn't parse.
    fn compare(&self, a: &str, b: &str) -> Option<std::cmp::Ordering>;
}

/// [`SourcePositions`] over a change capture.
#[derive(Debug, Clone)]
pub struct CapturePositions(pub Arc<dyn ChangeCapture>);

impl SourcePositions for CapturePositions {
    fn place(&self, landed: Landed) -> Option<String> {
        self.0
            .reached(landed)
            .map(|position| self.0.format_position(position))
    }

    fn parses(&self, text: &str) -> bool {
        self.0.parse_position(text).is_ok()
    }

    fn compare(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        let a = self.0.parse_position(a).ok()?;
        let b = self.0.parse_position(b).ok()?;
        Some(a.cmp(&b))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::Path;

    /// Every `.rs` file under `dir`, test and bench code left out.
    fn sources(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !matches!(
                    name.as_str(),
                    "target" | "tests" | "benches" | "fuzz" | "node_modules" | "dist"
                ) {
                    sources(&path, found);
                }
            } else if name.ends_with(".rs") && name != "tests.rs" {
                found.push(path);
            }
        }
    }

    /// Review of the second fix round, m-h: the one place outside tests that
    /// opens a live stream does it through `Rereads::open`, so nothing opens
    /// the window between a new stream answering and the lanes counting as
    /// unread. A second caller fails here.
    ///
    /// A tripwire, not a boundary (third re-check, m-1). A token only
    /// `Rereads::open` can make would have to be a type the source port
    /// (layer 1) names and only the engine (layer 2) constructs; Rust has no
    /// visibility for "another crate above me", so any constructor the port
    /// exposes is open to every caller, and the token would be a convention
    /// too, at the cost of every capture's `live` signature. What it misses:
    /// `live` passed as a function value without naming it on the line
    /// (`.then(f)` where `f` was bound elsewhere), or a call built by a macro.
    /// What trips it falsely: a capture decorator forwarding
    /// `self.inner.live()`, or an inline `#[cfg(test)] mod` in a file not
    /// named `tests.rs` (add the file to the walk's exclusions then).
    /// Calls through the trait's path (`ChangeCapture::live(..)`, or
    /// `ChangeCapture::live` as a value) are caught too.
    #[test]
    fn the_live_stream_is_opened_only_through_rereads_open() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut files = Vec::new();
        for dir in ["libs", "apps", "sdk", "dev"] {
            sources(&root.join(dir), &mut files);
        }
        // Spelled in parts so this test doesn't find itself.
        let calls = [[".live", "()"].concat(), ["Capture::", "live"].concat()];
        let mut callers = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for (at, line) in text.lines().enumerate() {
                let code = !line.trim_start().starts_with("//");
                if code && calls.iter().any(|call| line.contains(call.as_str())) {
                    callers.push((file.clone(), at + 1, line.trim().to_owned()));
                }
            }
        }
        assert!(!callers.is_empty(), "the ingest engine opens the stream");
        for (file, line, text) in &callers {
            assert!(
                text.contains("rereads.open("),
                "{}:{line} opens a live stream outside `Rereads::open`: {text}",
                file.display()
            );
        }
    }
}
