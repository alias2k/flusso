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
//!   opening reads them again before it writes anything
//!   ([`Rereads::is_due`]), and says so ([`Rereads::read`]);
//! - a fence vouches, for a lane of a sink not read since
//!   ([`Rereads::current`]), no further than this process applied under the
//!   previous stream.
//!
//! The order lives in [`open`](Rereads::open) alone, so no caller can open
//! the window between the new stream answering and the sinks counting as
//! unread. [`Rereads::closed`] says the stream ended: until the next opening
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
//! use kernel::SinkName;
//!
//! # tokio::runtime::Builder::new_current_thread().build().unwrap().block_on(async {
//! let rereads = Rereads::default();
//! let primary = SinkName::try_new("primary").unwrap();
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

use kernel::SinkName;
use source::cdc::{ChangeCapture, Landed};

/// How many times the live stream opened, and how far each sink read its
/// detach records since. Shared by the ingest engine, the sink engines and
/// the fence.
#[derive(Default)]
pub struct Rereads {
    /// Openings begun.
    opened: AtomicU64,
    /// The last opening that returned Ok: a read begun from here counts for
    /// it.
    held: AtomicU64,
    /// The stream of the last opening is open.
    holding: AtomicBool,
    read: Mutex<BTreeMap<SinkName, u64>>,
    listeners: Mutex<Vec<RereadListener>>,
}

/// What a [`Rereads`] listener hears.
#[derive(Debug, Clone, Copy)]
pub enum Reread<'a> {
    /// An opening begins: the previous stream still answers, the new one
    /// not yet; every sink's records are unread from here. `read` says which
    /// sinks counted as read during the stream being left: only their
    /// answers then were given while read.
    Opening { read: &'a ReadBefore },
    /// A sink read its records.
    Read,
}

/// Which sinks counted as read just before an opening moved the count.
#[derive(Debug, Clone)]
pub struct ReadBefore {
    opened: u64,
    held: u64,
    read: BTreeMap<SinkName, u64>,
}

impl ReadBefore {
    /// Whether `sink` counted as read during the stream being left (any
    /// sink, before the first opening: nothing could vouch then).
    pub fn read(&self, sink: &SinkName) -> bool {
        self.opened == 0
            || (self.held == self.opened
                && self.read.get(sink).is_some_and(|at| *at >= self.opened))
    }
}

/// Called on each [`Reread`]: cheap, it only wakes or notes something.
pub type RereadListener = Arc<dyn Fn(Reread<'_>) + Send + Sync>;

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
    /// way to open it. Before `opening` runs, every sink counts as unread and
    /// listeners hear [`Reread::Opening`] while only the previous stream can
    /// answer; once it returns Ok, a read begun from then counts. Failed,
    /// nothing counts as read until an opening succeeds.
    pub async fn open<T, E>(&self, opening: impl Future<Output = Result<T, E>>) -> Result<T, E> {
        // Which sinks read during the stream being left, taken before the
        // count moves (once it has, none counts as read).
        let before = ReadBefore {
            opened: self.opened.load(Ordering::Acquire),
            held: self.held.load(Ordering::Acquire),
            read: self
                .read
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        };
        let epoch = self.opened.fetch_add(1, Ordering::AcqRel) + 1;
        self.holding.store(false, Ordering::Release);
        self.tell(Reread::Opening { read: &before });
        let opened = opening.await;
        if opened.is_ok() {
            self.held.store(epoch, Ordering::Release);
            self.holding.store(true, Ordering::Release);
        }
        opened
    }

    /// The stream of the last opening ended: this process may no longer hold
    /// the slot.
    pub fn closed(&self) {
        self.holding.store(false, Ordering::Release);
    }

    /// Whether the stream of the last opening is open: this process holds
    /// the slot.
    pub fn holding(&self) -> bool {
        self.holding.load(Ordering::Acquire)
    }

    /// Hear every opening and every read, for a fence to note and wake on.
    pub fn subscribe(&self, listener: RereadListener) {
        self.listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(listener);
    }

    fn tell(&self, event: Reread<'_>) {
        let listeners = self
            .listeners
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        for listener in listeners {
            listener(event);
        }
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

    /// `sink` read every index's records, the read counting for `read_at`.
    pub fn read(&self, sink: &SinkName, read_at: u64) {
        {
            let mut read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
            let at = read.entry(sink.clone()).or_default();
            *at = (*at).max(read_at);
        }
        self.tell(Reread::Read);
    }

    /// Whether `sink` read its records since the last opening began, the
    /// read begun once it returned. True before any opening: nothing
    /// vouches before then anyway.
    pub fn current(&self, sink: &SinkName) -> bool {
        let opened = self.opened.load(Ordering::Acquire);
        opened == 0
            || (self.read_begins() == opened
                && self
                    .read
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get(sink)
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
