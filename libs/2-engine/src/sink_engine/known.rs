//! What this process knows of each lane's detach records, and whether a read
//! of them shows that another process changed them (ADR 0012 stage 10, the
//! cut the owner decided on 2026-11-27, and the review of round 10).
//!
//! flusso doesn't reconcile with another process's record changes: a read
//! that finds one ([`Change::Foreign`]) stops the deployment, and the next
//! start decides from the records with empty memory. So the one question here
//! is "did anyone but this process change these records since it last read
//! them?", answered by **who wrote them**, never by predicting what a write
//! leaves:
//!
//! - **Every own write is stamped.** [`KnownRecords::stamp`] issues a
//!   [`Stamp`]: this sink engine's writer token (random, made once per
//!   process start, never persisted) and the next number of its writes. The
//!   sink stores it in the record or marker it writes, a forgotten record
//!   becomes a tombstone carrying it, and a read returns both stamps.
//! - **Each document is judged on its own.** The record (or its tombstone)
//!   is the same when it carries this writer, or is exactly what was last
//!   read: present or not, version, stamp, state, attempts, `through`. The
//!   marker is the same when it carries this writer, or its stamp and gaps
//!   are what was last read. A read is the same when both are.
//! - **An own write of unknown outcome needs nothing kept.** The next read
//!   shows its stamp (it landed), what was last read (it didn't), or
//!   anything else (another process).
//! - **Nothing known yet** (no read here): adopted. **A read under the same
//!   held opening**: adopted, since no other process writes while this one
//!   holds the slot (the narrowed O29 rule, `slot`).
//! - **An own write of the record with nothing read yet** makes the record
//!   known as only this process's: the next read is the same only when the
//!   record carries this writer. The marker is still unknown, so adopted.
//!
//! ```text
//! known                              found (a read)                    →
//! nothing (never read here)          anything                           Same
//! anything                           read under the same held opening   Same
//! a read                             each document: this writer's, or   Same
//!                                    exactly as last read
//! the record only this process's     record: this writer's              Same
//! anything else                                                         Foreign
//! ```
//!
//! An unstamped document (written before stamps, or by hand) is written by
//! nobody known: adopted at a start, foreign to a running process once it
//! differs from what was read. The marker is judged by its stamp and gaps,
//! not its version: the walk checkpoint and the `uuid` are partial writes
//! that keep the stamp and change nothing a read is judged on.

use std::collections::BTreeMap;
use std::hash::{BuildHasher, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use kernel::IndexName;
use sink::{DetachRecord, Detachment, Gap, RecordVersion, Stamp};

/// What a read of a lane's records says about who changed them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    /// Nothing but this process changed them: go on.
    Same,
    /// Another process changed them since this one last knew them: what.
    Foreign(String),
}

/// The record (or its tombstone) as a read found it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RecordRead {
    record: Option<DetachRecord>,
    version: Option<RecordVersion>,
    stamp: Option<Stamp>,
    attempts: Option<u64>,
    through: Option<String>,
}

impl RecordRead {
    fn of(found: &Detachment) -> Self {
        Self {
            record: found.record.clone(),
            version: found.version.clone(),
            stamp: found.record_stamp.clone(),
            attempts: found.attempts,
            through: found.through.clone(),
        }
    }
}

/// The marker's stamp and gaps as a read found them.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MarkerRead {
    stamp: Option<Stamp>,
    gaps: Vec<Gap>,
}

/// What this process knows of one lane's record.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RecordKnown {
    /// As last read.
    Read(RecordRead),
    /// Written by this process before any read here: only this process's.
    OnlyMine,
}

/// What this process knows of one lane's detach records.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Known {
    record: RecordKnown,
    /// `None`: nothing read yet, so any marker is adopted.
    marker: Option<MarkerRead>,
    /// The opening ([`Rereads::read_begins`](crate::Rereads::read_begins))
    /// this was learnt under.
    at: u64,
    /// The records may keep a gap: the last read found one, or an own write
    /// since may have left one.
    may_keep_gaps: bool,
}

impl Known {
    fn read(found: &Detachment, at: u64) -> Self {
        Self {
            record: RecordKnown::Read(RecordRead::of(found)),
            marker: Some(MarkerRead {
                stamp: found.marker_stamp.clone(),
                gaps: found.gaps.clone(),
            }),
            at,
            may_keep_gaps: !found.gaps.is_empty(),
        }
    }
}

/// [`Known`] for each of a sink engine's indexes, the writer token its
/// writes are stamped with, and the first foreign change any read found.
#[derive(Debug)]
pub(crate) struct KnownRecords {
    writer: String,
    writes: AtomicU64,
    known: Mutex<BTreeMap<IndexName, Known>>,
    /// The first read judged another process's change, wherever it was
    /// read: it stops the deployment.
    foreign: Mutex<Option<(IndexName, String)>>,
}

impl Default for KnownRecords {
    fn default() -> Self {
        Self {
            writer: writer_token(),
            writes: AtomicU64::new(0),
            known: Mutex::default(),
            foreign: Mutex::default(),
        }
    }
}

/// A token no other process start makes: std's per-process random hash
/// keys over the process id and the clock. Never persisted.
fn writer_token() -> String {
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    if let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(now.as_nanos());
    }
    format!("{:016x}", hasher.finish())
}

impl KnownRecords {
    fn lock(&self) -> MutexGuard<'_, BTreeMap<IndexName, Known>> {
        self.known.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The stamp of this process's next write.
    pub(crate) fn stamp(&self) -> Stamp {
        Stamp::new(
            self.writer.clone(),
            self.writes.fetch_add(1, Ordering::Relaxed) + 1,
        )
    }

    /// Whether `stamp` is one of this process's.
    pub(crate) fn is_mine(&self, stamp: Option<&Stamp>) -> bool {
        stamp.is_some_and(|stamp| stamp.writer == self.writer)
    }

    /// The first change of another process any read found, by index.
    pub(crate) fn foreign(&self) -> Option<(IndexName, String)> {
        self.foreign
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Judge `found`, a read of `index`'s records begun under opening
    /// `read_at` (see the module doc). `same_opening`: the read began under
    /// the opening this was learnt under, and that opening's stream still
    /// held the slot once the read returned. A read that is the same is
    /// what this process knows from now on.
    pub(crate) fn judge(
        &self,
        index: &IndexName,
        found: &Detachment,
        read_at: u64,
        same_opening: bool,
    ) -> Change {
        let mut known = self.lock();
        let judged = match known.get(index) {
            None => Change::Same,
            Some(was) if same_opening && was.at == read_at => Change::Same,
            Some(was) => match self.differs(was, found) {
                None => Change::Same,
                Some(differs) => Change::Foreign(format!("{index}: {differs}")),
            },
        };
        match &judged {
            Change::Same => {
                known.insert(index.clone(), Known::read(found, read_at));
            }
            Change::Foreign(what) => {
                self.foreign
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .get_or_insert_with(|| (index.clone(), what.clone()));
            }
        }
        judged
    }

    /// Why `found` isn't this process's or as last read, if it isn't.
    fn differs(&self, was: &Known, found: &Detachment) -> Option<String> {
        if !self.is_mine(found.record_stamp.as_ref()) {
            match &was.record {
                RecordKnown::OnlyMine => {
                    return Some(format!(
                        "the record is {:?}, written by {}, where this process wrote it",
                        found.record,
                        writer(found.record_stamp.as_ref())
                    ));
                }
                RecordKnown::Read(read) => {
                    let now = RecordRead::of(found);
                    if *read != now {
                        return Some(format!(
                            "the record was {:?} (version {}, by {}), now {:?} (version {}, by {})",
                            read.record,
                            version(read.version.as_ref()),
                            writer(read.stamp.as_ref()),
                            now.record,
                            version(now.version.as_ref()),
                            writer(now.stamp.as_ref()),
                        ));
                    }
                }
            }
        }
        if !self.is_mine(found.marker_stamp.as_ref())
            && let Some(read) = &was.marker
            && (read.stamp != found.marker_stamp || read.gaps != found.gaps)
        {
            return Some(format!(
                "the seed marker was written by {} with gaps {:?}, now by {} with gaps {:?}",
                writer(read.stamp.as_ref()),
                read.gaps,
                writer(found.marker_stamp.as_ref()),
                found.gaps
            ));
        }
        None
    }

    /// An own write of `index`'s record was sent under opening `at`. With
    /// nothing read here yet, the record is known as only this process's
    /// from now on; otherwise nothing changes: the next read shows the
    /// write's stamp, or what was last read.
    pub(crate) fn wrote_record(&self, index: &IndexName, at: u64) {
        self.lock().entry(index.clone()).or_insert(Known {
            record: RecordKnown::OnlyMine,
            marker: None,
            at,
            may_keep_gaps: false,
        });
    }

    /// An own write of `index`'s gaps may have left `gaps`: `landed` when its
    /// call returned them, else it may also have left those kept before.
    pub(crate) fn wrote_gaps(&self, index: &IndexName, gaps: &[Gap], landed: bool) {
        if let Some(known) = self.lock().get_mut(index) {
            known.may_keep_gaps = !gaps.is_empty() || (!landed && known.may_keep_gaps);
        }
    }

    /// The opening `index`'s records were last read under, if any.
    pub(crate) fn learnt_at(&self, index: &IndexName) -> Option<u64> {
        self.lock().get(index).map(|known| known.at)
    }

    /// Whether `index`'s records may keep a gap: the last read found one, or
    /// an own write since may have left one. Nothing known yet: none known.
    pub(crate) fn keeps_gaps(&self, index: &IndexName) -> bool {
        self.lock()
            .get(index)
            .is_some_and(|known| known.may_keep_gaps)
    }
}

/// Who a stamp names, for a message.
fn writer(stamp: Option<&Stamp>) -> String {
    stamp.map_or_else(
        || "nobody known (no stamp)".to_owned(),
        |stamp| format!("writer {} (write {})", stamp.writer, stamp.seq),
    )
}

fn version(version: Option<&RecordVersion>) -> &str {
    version.map_or("none", RecordVersion::as_str)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use sink::MarkerAt;

    fn index() -> IndexName {
        IndexName::try_new("users").unwrap()
    }

    fn other(seq: u64) -> Stamp {
        Stamp::new("another process", seq)
    }

    fn detached(by: Option<Stamp>, version: &str) -> Detachment {
        let mut found = Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new(version)),
        );
        found.through = Some("14".to_owned());
        found.record_stamp = by;
        found
    }

    fn gap(from: &str) -> Gap {
        Gap {
            from: Some(from.to_owned()),
            to: Some("9".to_owned()),
        }
    }

    fn with(mut found: Detachment, change: impl FnOnce(&mut Detachment)) -> Detachment {
        change(&mut found);
        found
    }

    #[test]
    fn stamps_name_one_writer_and_count_its_writes() {
        let known = KnownRecords::default();
        let (first, second) = (known.stamp(), known.stamp());
        assert_eq!(first.writer, second.writer);
        assert!(second.seq > first.seq);
        assert!(known.is_mine(Some(&first)));
        assert!(!known.is_mine(None));
        let elsewhere = KnownRecords::default();
        assert_ne!(elsewhere.stamp().writer, first.writer);
        assert!(!known.is_mine(Some(&elsewhere.stamp())));
    }

    #[test]
    fn nothing_known_is_adopted() {
        let known = KnownRecords::default();
        let found = detached(Some(other(1)), "1:1");
        assert_eq!(known.judge(&index(), &found, 1, false), Change::Same);
        assert_eq!(known.learnt_at(&index()), Some(1));
    }

    #[test]
    fn a_read_under_the_same_held_opening_is_adopted() {
        let known = KnownRecords::default();
        known.judge(&index(), &detached(None, "1:1"), 1, true);
        let changed = detached(Some(other(4)), "2:1");
        assert_eq!(known.judge(&index(), &changed, 1, true), Change::Same);
        let changed_again = detached(Some(other(5)), "3:1");
        assert!(matches!(
            known.judge(&index(), &changed_again, 2, true),
            Change::Foreign(_)
        ));
    }

    #[test]
    fn the_same_records_read_again_are_the_same() {
        let known = KnownRecords::default();
        let found = with(detached(Some(other(1)), "1:1"), |found| {
            found.gaps = vec![gap("3")];
            found.marker_stamp = Some(other(2));
        });
        known.judge(&index(), &found, 1, false);
        assert_eq!(known.judge(&index(), &found, 2, false), Change::Same);
    }

    /// Whatever another process changes, unstamped or stamped, is foreign
    /// to a running process: each field of the record against the version
    /// read, the marker's stamp and gaps.
    #[test]
    fn every_change_another_writer_makes_is_foreign() {
        let read = with(detached(None, "1:1"), |found| {
            found.gaps = vec![gap("3")];
        });
        let changes: Vec<Detachment> = vec![
            with(read.clone(), |f| {
                f.version = Some(RecordVersion::new("2:1"))
            }),
            with(read.clone(), |f| f.record_stamp = Some(other(1))),
            with(read.clone(), |f| {
                f.record = Some(DetachRecord::Rebuilding { live_only: false });
            }),
            with(read.clone(), |f| f.attempts = Some(3)),
            with(read.clone(), |f| f.through = Some("20".to_owned())),
            with(read.clone(), |f| {
                f.record = None;
                f.version = Some(RecordVersion::new("2:1"));
                f.record_stamp = Some(other(1));
            }),
            with(read.clone(), |f| f.gaps = vec![gap("3"), gap("5")]),
            with(read.clone(), |f| f.marker_stamp = Some(other(2))),
        ];
        for changed in changes {
            let known = KnownRecords::default();
            known.judge(&index(), &read, 1, false);
            assert!(
                matches!(
                    known.judge(&index(), &changed, 2, false),
                    Change::Foreign(_)
                ),
                "{changed:?}"
            );
            assert!(known.foreign().is_some());
        }
    }

    /// This process's own stamp is the same whatever the document holds:
    /// the write landed (a write of unknown outcome needs nothing kept).
    #[test]
    fn a_document_carrying_this_writer_is_the_same_whatever_it_holds() {
        let known = KnownRecords::default();
        known.judge(&index(), &detached(Some(other(1)), "1:1"), 1, false);
        let mine = known.stamp();
        let rewritten = with(detached(Some(mine.clone()), "5:1"), |f| {
            f.record = Some(DetachRecord::Rebuilding { live_only: true });
            f.attempts = Some(9);
            f.gaps = vec![gap("1")];
            f.marker_stamp = Some(mine.clone());
        });
        assert_eq!(known.judge(&index(), &rewritten, 2, false), Change::Same);
        let forgotten = with(Detachment::default(), |f| {
            f.version = Some(RecordVersion::new("6:1"));
            f.record_stamp = Some(known.stamp());
            f.marker_stamp = Some(mine);
            f.gaps = vec![gap("1")];
        });
        assert_eq!(known.judge(&index(), &forgotten, 3, false), Change::Same);
    }

    /// An own write whose call failed: not landed, the records read are
    /// exactly as before; landed, they carry its stamp; anything else is
    /// another process's.
    #[test]
    fn an_own_write_of_unknown_outcome_is_settled_by_the_next_read() {
        let before = detached(Some(other(1)), "1:1");
        for (after, same) in [
            (before.clone(), true),
            (detached(None, "2:1"), false),
            (detached(Some(other(2)), "2:1"), false),
        ] {
            let known = KnownRecords::default();
            known.judge(&index(), &before, 1, false);
            let _unknown_outcome = known.stamp();
            assert_eq!(
                known.judge(&index(), &after, 2, false) == Change::Same,
                same,
                "{after:?}"
            );
        }
        let known = KnownRecords::default();
        known.judge(&index(), &before, 1, false);
        let landed = known.stamp();
        assert_eq!(
            known.judge(&index(), &detached(Some(landed), "2:1"), 2, false),
            Change::Same
        );
    }

    /// Found by the world at 50,000 cases: a lane this process detached
    /// before any read of its records succeeded. Its own record is what it
    /// knows, so another process's reattach or detach is foreign; its own
    /// record read back is the same, and the marker is adopted.
    #[test]
    fn an_own_record_write_with_nothing_read_before_is_only_this_processs() {
        let known = KnownRecords::default();
        known.wrote_record(&index(), 1);
        let reattached = with(Detachment::default(), |f| {
            f.gaps = vec![gap("14")];
            f.record_stamp = Some(other(3));
        });
        assert!(matches!(
            known.judge(&index(), &reattached, 2, false),
            Change::Foreign(_)
        ));

        let known = KnownRecords::default();
        known.wrote_record(&index(), 1);
        assert!(matches!(
            known.judge(&index(), &detached(Some(other(3)), "1:1"), 2, false),
            Change::Foreign(_)
        ));

        let known = KnownRecords::default();
        known.wrote_record(&index(), 1);
        let ours = with(detached(Some(known.stamp()), "3:1"), |f| {
            f.marker_stamp = Some(other(9));
            f.gaps = vec![gap("2")];
        });
        assert_eq!(known.judge(&index(), &ours, 2, false), Change::Same);
    }

    /// An own record write over records already read changes nothing known.
    #[test]
    fn an_own_record_write_after_a_read_keeps_the_read() {
        let known = KnownRecords::default();
        let before = detached(Some(other(1)), "1:1");
        known.judge(&index(), &before, 1, false);
        known.wrote_record(&index(), 2);
        assert_eq!(known.learnt_at(&index()), Some(1));
        assert_eq!(known.judge(&index(), &before, 2, false), Change::Same);
    }

    #[test]
    fn gaps_are_kept_while_a_read_found_one_or_an_own_write_may_have_left_one() {
        let known = KnownRecords::default();
        assert!(!known.keeps_gaps(&index()));
        known.judge(&index(), &detached(None, "1:1"), 1, false);
        assert!(!known.keeps_gaps(&index()));
        known.wrote_gaps(&index(), &[gap("1")], false);
        assert!(known.keeps_gaps(&index()));
        known.wrote_gaps(&index(), &[], false);
        assert!(
            known.keeps_gaps(&index()),
            "the earlier write may have landed"
        );
        known.wrote_gaps(&index(), &[], true);
        assert!(!known.keeps_gaps(&index()));
        let with_gap = with(detached(None, "1:1"), |f| f.gaps = vec![gap("1")]);
        known.judge(&index(), &with_gap, 1, true);
        assert!(known.keeps_gaps(&index()));
    }
}
