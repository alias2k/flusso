//! What the sink engines need of the source once it holds the slot (ADR 0012
//! stage 10, O29 and S3).
//!
//! **[`Rereads`]: the detach records are read again once the slot is held.**
//! A process stages before its live stream opens. Another process can still
//! hold the slot then, and detach a lane after this one read its records.
//! Every read before the slot is held is only advisory. So:
//!
//! - the ingest engine calls [`Rereads::live_opened`] each time its live
//!   stream opens, before it publishes anything;
//! - a sink engine whose last read of its records began before that reads
//!   them again before it writes anything ([`Rereads::is_due`]), and says so
//!   ([`Rereads::read`]);
//! - a fence vouches for no lane of a sink whose records are not read since
//!   ([`Rereads::current`]).
//!
//! Nothing waits on a sink: one whose destination is down reads again when
//! its run comes back, and its lanes hold the slot and vouch for nothing
//! meanwhile. The other sinks go on.
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
//! let rereads = Rereads::default();
//! let primary = SinkName::try_new("primary").unwrap();
//! assert!(rereads.current(&primary), "nothing opened yet");
//! let read_at = rereads.live_starts();
//! rereads.live_opened();
//! assert!(rereads.is_due(read_at));
//! assert!(!rereads.current(&primary));
//! rereads.read(&primary, rereads.live_starts());
//! assert!(rereads.current(&primary));
//! ```

use std::collections::BTreeMap;
use std::fmt::Debug;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use kernel::SinkName;
use source::cdc::{ChangeCapture, Landed};

/// How many times the live stream opened, and how far each sink read its
/// detach records since. Shared by the ingest engine, the sink engines and
/// the fence.
#[derive(Default)]
pub struct Rereads {
    opened: AtomicU64,
    read: Mutex<BTreeMap<SinkName, u64>>,
    listeners: Mutex<Vec<RereadListener>>,
}

/// What a [`Rereads`] listener hears.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reread {
    /// The live stream opened: every sink's records are unread from here.
    Opened,
    /// A sink read its records.
    Read,
}

/// Called on each [`Reread`]: cheap, it only wakes or notes something.
pub type RereadListener = Arc<dyn Fn(Reread) + Send + Sync>;

impl std::fmt::Debug for Rereads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Rereads")
            .field("opened", &self.opened)
            .field("read", &self.read)
            .finish_non_exhaustive()
    }
}

impl Rereads {
    /// The live stream opened: the slot is held. Called before anything is
    /// published.
    pub fn live_opened(&self) {
        self.opened.fetch_add(1, Ordering::AcqRel);
        self.tell(Reread::Opened);
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

    /// How many times the live stream opened. A sink engine reads it before
    /// it reads its records, and hands it to [`read`](Self::read) after.
    pub fn live_starts(&self) -> u64 {
        self.opened.load(Ordering::Acquire)
    }

    /// Whether a read of the records that began at `read_at` is older than
    /// the slot's last opening. One atomic load.
    pub fn is_due(&self, read_at: u64) -> bool {
        self.live_starts() > read_at
    }

    /// `sink` read every index's records, the read beginning at `read_at`.
    pub fn read(&self, sink: &SinkName, read_at: u64) {
        {
            let mut read = self.read.lock().unwrap_or_else(PoisonError::into_inner);
            let at = read.entry(sink.clone()).or_default();
            *at = (*at).max(read_at);
        }
        self.tell(Reread::Read);
    }

    /// Whether `sink` read its records since the live stream last opened.
    /// True before it ever opened: nothing vouches before then anyway.
    pub fn current(&self, sink: &SinkName) -> bool {
        let opened = self.live_starts();
        opened == 0
            || self
                .read
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(sink)
                .is_some_and(|at| *at >= opened)
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
}
