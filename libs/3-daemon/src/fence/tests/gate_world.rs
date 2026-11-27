//! The O29 gate's property in the real world's shape, through the real sink
//! engines (the third re-check's item 3, widened by the fourth's S-2).
//!
//! Two sinks: `audit` (`backfill = false`: a detached lane is reattached)
//! and `primary` (a detached lane is rebuilt to its seed). Two indexes, so
//! four lanes. The engines write real items to a stand-in sink; the test
//! plays the source, the index's builder and every other process.
//!
//! Events:
//! - openings that succeed or fail, past what another process did
//!   meanwhile: it applied every lane, dropped one lane's changes (its detach
//!   record left), or dropped them and began a rebuild of it (a `rebuilding`
//!   record and a walk checkpoint left, the process gone);
//! - sink-run restarts, whose staging may fail to read some indexes' records;
//! - reads of the records, and `ensure_index`, held until the generator
//!   releases them (a read or a restage in flight across an opening);
//! - a target gone missing, which restages the lane;
//! - changes published on both lanes of an index;
//! - reattaches (`audit`) and reindexes (`primary`), the attach answered at
//!   once, the backfill only at a `Seed`;
//! - this process's stream ending, another process acting before its next
//!   opening begins (as at an opening);
//! - this process detaching a lane itself (its writes refused), its detach
//!   sometimes writing the record and then failing (what a clear undoes);
//! - `/status` reads; waits at both levels, on one lane or a whole sink;
//! - a new process: a fresh pipeline over the same destination, redelivered
//!   at its first stream from the position the slot confirmed (the source's
//!   answer for what the last process's progress said landed).
//!
//! Each process has its own capture, as a real one does: it starts closed
//! at the position the slot confirmed, stays closed after a failed opening,
//! and answers a lane that routed nothing (`Landed::Idle`) as Postgres does.
//!
//! **The oracle owes nothing to the code under test.** The world keeps its
//! own record of whether this process holds a stream, and of its streams.
//! - *Records.* No record of another process is erased (a seed, a delete),
//!   nor rewritten into anything but detached (a rebuild), while this
//!   process holds no stream (O29, the rule since the fifth re-check).
//! - *Writes, while it holds a stream.* The stand-in judges every write the
//!   engines make, a seed's marker included, as it is made: on a lane whose
//!   records were last read (a read that began) under an earlier stream, or
//!   onto a record that says detached (this process's, or another's written
//!   before the stream opened), it is a failure. With no stream held, plain
//!   writes behave as on `main`.
//! - *Waits.* A wait for `p` that passes on a lane needs every change at or
//!   before `p` of the lane's index held there: written by this process,
//!   applied by another (a jump with no drop on it), or read by a seed whose
//!   walk began after it. A change neither is excused only by a reattach
//!   attached after it, and only for waits past that attach. Drops are the
//!   ones the test itself made (another process's) or saw (published, never
//!   written); seeds and attaches are the ones the test answered; never the
//!   gaps or counts the engine keeps.
//! - *Liveness.* Once every record is readable, nothing held, the slot held
//!   and the runs settled past their backoffs, every lane that is not
//!   detached or rebuilding passes a wait for the newest position.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use engine::{SinkControl, SinkEngine};
use futures::FutureExt;
use kernel::{Envelope, GenericValue, IndexMapping};
use proptest::prelude::*;
use sink::{DetachRecord, Detachment, MarkerAt, RecordVersion, Sink, WriteReport};
use stream::{Batch, LaneItem, Request, Stream};
use stream_channel::ChannelStream;
use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinHandle;

use super::*;

const INDEXES: [&str; 2] = ["orders", "users"];
const SINKS: [&str; 2] = ["audit", "primary"];

fn lanes() -> Vec<LaneKey> {
    SINKS
        .iter()
        .flat_map(|s| INDEXES.iter().map(move |i| LaneKey::new(index(i), sink(s))))
        .collect()
}

fn mapping(of: &str) -> IndexMapping {
    IndexMapping {
        index: index(of),
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// A gate the generator holds and releases: what passes it while held waits.
#[derive(Debug, Default)]
struct Hold(Mutex<Option<Arc<Semaphore>>>);

impl Hold {
    fn hold(&self) {
        self.0
            .lock()
            .unwrap()
            .get_or_insert_with(|| Arc::new(Semaphore::new(0)));
    }

    fn release(&self) {
        if let Some(gate) = self.0.lock().unwrap().take() {
            gate.close();
        }
    }

    async fn pass(&self) {
        let gate = self.0.lock().unwrap().clone();
        if let Some(gate) = gate {
            let _ = gate.acquire().await;
        }
    }
}

/// What only the world knows of this process, never read from the code:
/// whether it holds a stream (an opening it began got through, and the
/// stream hasn't ended), and how many of its streams opened so far.
#[derive(Debug, Default)]
struct Holding {
    holds: std::sync::atomic::AtomicBool,
    streams: std::sync::atomic::AtomicU64,
}

impl Holding {
    fn holds(&self) -> bool {
        self.holds.load(Ordering::SeqCst)
    }

    fn stream(&self) -> u64 {
        self.streams.load(Ordering::SeqCst)
    }
}

/// Who wrote a record, and under which stream.
#[derive(Debug, Clone)]
struct Kept {
    detachment: Detachment,
    /// This process wrote it.
    ours: bool,
    /// The stream held when it was written.
    under: u64,
}

/// One sink's destination: its records, what was written, and the failures
/// it saw as the writes were made.
#[derive(Debug)]
struct Store {
    /// Whether this process holds a stream, as the world tracks it.
    holding: Arc<Holding>,
    live_only: bool,
    kept: Mutex<BTreeMap<String, Kept>>,
    gaps: Mutex<BTreeMap<String, Vec<sink::Gap>>>,
    unreadable: Mutex<BTreeSet<String>>,
    unseeded: Mutex<BTreeSet<String>>,
    missing: Mutex<BTreeSet<String>>,
    /// Indexes whose writes are refused (a block): what parks a lane.
    blocked: Mutex<BTreeSet<String>>,
    /// Indexes whose next detach writes its record, then fails.
    detach_fails: Mutex<BTreeSet<String>>,
    /// Each index's target, as OpenSearch's generations: a rebuild, a
    /// reindex or a target gone makes a new one, which cuts a backfill asked
    /// for the old one (#428).
    generation: Mutex<BTreeMap<String, u64>>,
    /// Each index's walk checkpoint: its mark, and where the walk began.
    walked: Mutex<BTreeMap<String, (String, u64)>>,
    /// Where the walk of the backfill now completing began, per index.
    completing: Mutex<BTreeMap<String, u64>>,
    /// Each seed: the index, and where its walk began.
    seeds: Mutex<Vec<(String, u64)>>,
    /// Each read of an index's records that got through, with the stream
    /// held when it began.
    reads: Mutex<Vec<(String, u64)>>,
    /// Every change written, as `(index, position)`.
    written: Mutex<BTreeSet<(String, u64)>>,
    violations: Mutex<Vec<String>>,
    read_hold: Hold,
    ensure_hold: Hold,
}

impl Store {
    fn new(live_only: bool, holding: Arc<Holding>) -> Self {
        Self {
            holding,
            live_only,
            kept: Mutex::default(),
            gaps: Mutex::default(),
            unreadable: Mutex::default(),
            unseeded: Mutex::default(),
            missing: Mutex::default(),
            blocked: Mutex::default(),
            detach_fails: Mutex::default(),
            generation: Mutex::default(),
            walked: Mutex::default(),
            completing: Mutex::default(),
            seeds: Mutex::default(),
            reads: Mutex::default(),
            written: Mutex::default(),
            violations: Mutex::default(),
            read_hold: Hold::default(),
            ensure_hold: Hold::default(),
        }
    }

    /// The stream this process holds or last held, as the world counts.
    fn held(&self) -> u64 {
        self.holding.stream()
    }

    /// A new process runs: what the last one read counts for nothing, and
    /// every record is another process's to it.
    fn enter(&self) {
        self.reads.lock().unwrap().clear();
        for kept in self.kept.lock().unwrap().values_mut() {
            kept.ours = false;
        }
    }

    /// O29, the rule since the fifth re-check: while this process holds no
    /// stream, another process may hold the slot and rely on any record or
    /// gap of `of`, whoever wrote it (it read them under its own stream), so
    /// none is erased, rewritten or has the generations beside it deleted.
    /// `what` names the request (a seed, a delete, a rebuild, a reindex, a
    /// demotion, a detach). With neither there, nothing is relied on.
    fn touch(&self, of: &str, what: &str) {
        if self.holding.holds() {
            return;
        }
        let record = self
            .kept
            .lock()
            .unwrap()
            .get(of)
            .map(|kept| kept.detachment.record.clone());
        let gaps = self
            .gaps
            .lock()
            .unwrap()
            .get(of)
            .is_some_and(|gaps| !gaps.is_empty());
        if record.is_some() || gaps {
            self.violations.lock().unwrap().push(format!(
                "{what} of {of} over a record ({record:?}, gaps kept: {gaps}) with no stream held"
            ));
        }
    }

    /// Write `of`'s record, keeping the attempts the one before counted.
    fn keep(&self, of: &str, record: DetachRecord, marker: MarkerAt, ours: bool) {
        let mut detachment =
            Detachment::recorded(record, Some(marker), Some(RecordVersion::new("1:1")));
        detachment.attempts = self
            .kept
            .lock()
            .unwrap()
            .get(of)
            .and_then(|kept| kept.detachment.attempts);
        self.kept.lock().unwrap().insert(
            of.to_owned(),
            Kept {
                detachment,
                ours,
                under: self.held(),
            },
        );
    }

    fn next_generation(&self, of: &str) {
        *self
            .generation
            .lock()
            .unwrap()
            .entry(of.to_owned())
            .or_default() += 1;
    }

    /// Another process detached `of` and dropped its changes.
    fn dropped(&self, of: &str) {
        let (record, marker) = if self.live_only {
            (
                DetachRecord::Detached { live_only: true },
                MarkerAt::Serving { seeded: true },
            )
        } else {
            (
                DetachRecord::Detached { live_only: false },
                MarkerAt::Newer { seeded: false },
            )
        };
        self.keep(of, record, marker, false);
    }

    /// Another process began a rebuild of `of` whose walk began at `began`,
    /// then went away.
    fn rebuilt_by_another(&self, of: &str, began: u64) {
        self.keep(
            of,
            DetachRecord::Rebuilding { live_only: false },
            MarkerAt::Newer { seeded: false },
            false,
        );
        if let Some(kept) = self.kept.lock().unwrap().get_mut(of) {
            kept.detachment.attempts = Some(kept.detachment.attempts.unwrap_or(0) + 1);
        }
        self.next_generation(of);
        self.unseeded.lock().unwrap().insert(of.to_owned());
        self.walked
            .lock()
            .unwrap()
            .insert(of.to_owned(), (began.to_string(), began));
    }

    /// The stream held when `of`'s records were last read.
    fn read_for(&self, of: &str) -> u64 {
        self.reads
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, epoch)| *epoch)
            .max()
            .unwrap_or(0)
    }

    /// Judge a write of `of`'s change at `at` (`None`: its seed) as it is
    /// made, while this process holds a stream: plain writes with none held
    /// behave as on `main` (late writes of what the lanes hold), and what a
    /// seed would erase is judged by `touch`.
    fn judge(&self, of: &str, at: Option<u64>) {
        if !self.holding.holds() {
            return;
        }
        let held = self.held();
        let read = self.read_for(of);
        let at = at.map_or_else(|| "seed".to_owned(), |at| at.to_string());
        let mut violations = self.violations.lock().unwrap();
        if read < held {
            violations.push(format!(
                "{of}@{at} written with its records last read under stream {read}, while {held} is held"
            ));
        }
        // Another process's rebuild needs no rule of its own: read under the
        // stream held, a staging takes it over (the process-start attach),
        // and a read again puts it back to detached first (B-2).
        if let Some(kept) = self.kept.lock().unwrap().get(of)
            && matches!(kept.detachment.record, Some(DetachRecord::Detached { .. }))
            && (kept.ours || kept.under < held)
        {
            violations.push(format!(
                "{of}@{at} written onto a detach record (ours: {}, under {} of {held})",
                kept.ours, kept.under
            ));
        }
    }
}

#[async_trait]
impl Sink for Store {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        self.ensure_hold.pass().await;
        let of = mapping.index.as_ref();
        let created = self.missing.lock().unwrap().remove(of);
        if created {
            self.next_generation(of);
        }
        let generation = self
            .generation
            .lock()
            .unwrap()
            .get(of)
            .copied()
            .unwrap_or(0);
        let target = Some(sink::TargetId::new(format!("{of}_{generation}")));
        Ok(if created {
            sink::Ensured::created(target)
        } else {
            sink::Ensured::existing(target)
        })
    }

    async fn write(&self, envelopes: &[Envelope], _: bool) -> sink::Result<WriteReport> {
        let mut report = WriteReport::clean();
        for envelope in envelopes {
            let of = envelope.index.as_ref();
            if self.missing.lock().unwrap().contains(of) {
                report =
                    report.missing_target(envelope.index.clone(), format!("no such index [{of}]"));
                continue;
            }
            if self.blocked.lock().unwrap().contains(of) {
                report = report.blocking(
                    envelope.index.clone(),
                    format!("index [{of}] blocked by: [FORBIDDEN/8/index write (api)]"),
                );
                continue;
            }
            let at: u64 = envelope.id.parse().unwrap();
            self.judge(of, Some(at));
            self.written.lock().unwrap().insert((of.to_owned(), at));
        }
        Ok(report)
    }

    async fn is_seeded(&self, of: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(of.as_ref()))
    }

    async fn seeding(&self, of: &IndexName) -> sink::Result<sink::Seeding> {
        Ok(if self.unseeded.lock().unwrap().contains(of.as_ref()) {
            sink::Seeding::Unseeded {
                walked: self
                    .walked
                    .lock()
                    .unwrap()
                    .get(of.as_ref())
                    .map(|(mark, _)| mark.clone()),
            }
        } else {
            sink::Seeding::Seeded
        })
    }

    async fn detachment(&self, mapping: &IndexMapping) -> sink::Result<Detachment> {
        let began = self.held();
        self.read_hold.pass().await;
        let of = mapping.index.as_ref();
        if self.unreadable.lock().unwrap().contains(of) {
            return Err(sink::SinkError::Write(
                "the records can't be read".to_owned(),
            ));
        }
        let mut kept = self
            .kept
            .lock()
            .unwrap()
            .get(of)
            .map(|kept| kept.detachment.clone())
            .unwrap_or_default();
        kept.gaps = self
            .gaps
            .lock()
            .unwrap()
            .get(of)
            .cloned()
            .unwrap_or_default();
        // As OpenSearch's marker names its generation's `index.uuid`: each
        // rebuild moves to a generation of its own.
        if kept.record.is_some() {
            let generation = self
                .generation
                .lock()
                .unwrap()
                .get(of)
                .copied()
                .unwrap_or(0);
            kept.marker_target = Some(sink::TargetId::new(format!("{of}_{generation}")));
        }
        self.reads.lock().unwrap().push((of.to_owned(), began));
        Ok(kept)
    }

    async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &sink::Detach,
    ) -> sink::Result<sink::Recorded> {
        self.touch(mapping.index.as_ref(), "a detach");
        let marker = if self.live_only {
            MarkerAt::Serving { seeded: true }
        } else {
            MarkerAt::Newer { seeded: false }
        };
        self.keep(
            mapping.index.as_ref(),
            DetachRecord::Detached {
                live_only: detach.live_only,
            },
            marker,
            true,
        );
        if self
            .detach_fails
            .lock()
            .unwrap()
            .remove(mapping.index.as_ref())
        {
            return Err(sink::SinkError::Write(
                "the record landed, the call timed out".to_owned(),
            ));
        }
        Ok(sink::Recorded::Kept)
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        _: &RecordVersion,
    ) -> sink::Result<()> {
        self.touch(mapping.index.as_ref(), "a delete");
        self.kept.lock().unwrap().remove(mapping.index.as_ref());
        Ok(())
    }

    async fn record_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &sink::DecideGaps<'_>,
    ) -> sink::Result<Vec<sink::Gap>> {
        let mut gaps = self.gaps.lock().unwrap();
        let kept = gaps
            .get(mapping.index.as_ref())
            .cloned()
            .unwrap_or_default();
        let Some(new) = decide(&kept) else {
            return Ok(kept);
        };
        gaps.insert(mapping.index.to_string(), new.clone());
        Ok(new)
    }

    async fn begin_rebuild(&self, mapping: &IndexMapping) -> sink::Result<u64> {
        let of = mapping.index.as_ref();
        self.touch(of, "a rebuild");
        let attempts = self
            .kept
            .lock()
            .unwrap()
            .get(of)
            .and_then(|kept| kept.detachment.attempts)
            .unwrap_or(0)
            + 1;
        self.keep(
            of,
            DetachRecord::Rebuilding { live_only: false },
            MarkerAt::Newer { seeded: false },
            true,
        );
        if let Some(kept) = self.kept.lock().unwrap().get_mut(of) {
            kept.detachment.attempts = Some(attempts);
        }
        self.next_generation(of);
        self.unseeded.lock().unwrap().insert(of.to_owned());
        self.walked.lock().unwrap().remove(of);
        Ok(attempts)
    }

    async fn demote(&self, mapping: &IndexMapping, live_only: bool) -> sink::Result<()> {
        let of = mapping.index.as_ref();
        self.touch(of, "a demotion");
        self.walked.lock().unwrap().remove(of);
        let held = self.held();
        if let Some(kept) = self.kept.lock().unwrap().get_mut(of) {
            kept.detachment.record = Some(DetachRecord::Detached { live_only });
            kept.ours = true;
            kept.under = held;
        }
        Ok(())
    }

    async fn forget_walked(&self, of: &IndexName) -> sink::Result<()> {
        self.walked.lock().unwrap().remove(of.as_ref());
        Ok(())
    }

    async fn reindex(&self, mapping: &IndexMapping) -> sink::Result<()> {
        let of = mapping.index.as_ref();
        self.touch(of, "a reindex");
        self.next_generation(of);
        self.unseeded.lock().unwrap().insert(of.to_owned());
        self.walked.lock().unwrap().remove(of);
        Ok(())
    }

    async fn mark_seeded(&self, of: &IndexName) -> sink::Result<()> {
        // The seed writes the marker whole: the record and the gaps go, which
        // stands for a read of them under the stream it was made in.
        let name = of.as_ref();
        self.touch(name, "a seed");
        self.judge(name, None);
        self.kept.lock().unwrap().remove(name);
        self.gaps.lock().unwrap().remove(name);
        self.unseeded.lock().unwrap().remove(name);
        self.walked.lock().unwrap().remove(name);
        let began = self.completing.lock().unwrap().remove(name).unwrap_or(0);
        self.seeds.lock().unwrap().push((name.to_owned(), began));
        let held = self.held();
        self.reads.lock().unwrap().push((name.to_owned(), held));
        Ok(())
    }
}

/// What the engines report of each lane: detached, rebuilding.
#[derive(Debug, Default)]
struct Tap {
    detached: Mutex<BTreeSet<LaneKey>>,
    rebuilding: Mutex<BTreeSet<LaneKey>>,
}

impl engine::Observer for Tap {
    fn on_lane_health(&self, lane: &LaneKey, health: &engine::LaneHealth) {
        let mut detached = self.detached.lock().unwrap();
        if health.is_detached() {
            detached.insert(lane.clone());
        } else {
            detached.remove(lane);
        }
    }

    fn on_lane_rebuild(&self, lane: &LaneKey, rebuilding: Option<u64>, _: Option<u64>) {
        let mut set = self.rebuilding.lock().unwrap();
        if rebuilding.is_some() {
            set.insert(lane.clone());
        } else {
            set.remove(lane);
        }
    }
}

#[derive(Debug, Clone)]
enum Event {
    /// The stream opens, starting `jump` past where the source stood. Another
    /// process held the slot meanwhile: it applied every lane, but `drop`'s,
    /// whose changes it dropped (its detach record left), and, with
    /// `rebuild` on a `primary` lane, then began a rebuild of it halfway.
    Open {
        ok: bool,
        jump: u64,
        drop: Option<usize>,
        rebuild: bool,
    },
    /// This process's stream ends (its walsender dropped), and another
    /// process holds the slot over `jump` before this one's next opening
    /// begins: it applied every lane, but `drop`'s (as `Open`'s).
    StreamEnds {
        jump: u64,
        drop: Option<usize>,
        rebuild: bool,
    },
    /// `lane`'s writes are refused (a block), and this process asks to
    /// detach it; with `fails`, its detach writes the record, then fails.
    Detach {
        lane: usize,
        fails: bool,
    },
    /// `lane`'s writes land again.
    Unblock {
        lane: usize,
    },
    /// A restart of `sink`'s run: staging, the indexes in `unreadable`
    /// failing their read (and keep failing until `Readable`).
    Restart {
        sink: usize,
        unreadable: u8,
    },
    Readable {
        sink: usize,
    },
    /// `sink`'s reads of its records wait until `Release`.
    HoldReads {
        sink: usize,
    },
    /// `sink`'s `ensure_index` waits until `Release`.
    HoldEnsure {
        sink: usize,
    },
    Release {
        sink: usize,
    },
    /// `lane`'s target is gone: its next write restages it.
    Missing {
        lane: usize,
    },
    /// A change of `index`, published on both its lanes while a stream is
    /// open.
    Apply {
        index: usize,
    },
    /// The backfill `lane` waits for completes.
    Seed {
        lane: usize,
    },
    Horizon {
        by: u64,
    },
    Status,
    /// This process ends and a new one starts over the same destination.
    NewProcess,
    /// A reindex of `index` on `primary`: a rebuild when detached.
    Reindex {
        index: usize,
    },
    /// A reattach of `index` on `audit`.
    Reattach {
        index: usize,
    },
    Wait {
        lane: usize,
        whole_sink: bool,
        searchable: bool,
        at: u64,
    },
    /// This process's stream ends and another process drops `lane` (with
    /// `rebuild`, begins its rebuild), then `then` happens with no stream
    /// held: every gated request's unheld case, drawn often (the sixth
    /// re-check, S-3). Expanded into the two before the run.
    Unheld {
        lane: usize,
        rebuild: bool,
        then: u8,
    },
    /// Another process holding the slot finishes the rebuild of `lane` it
    /// began: its seed covers every change so far, its record goes. Only
    /// while this process holds no stream.
    Finishes {
        lane: usize,
    },
    /// Another process holding the slot reattaches `lane` it detached: its
    /// record goes, everything dropped since the detach kept as one gap
    /// with the marker and given up. Only while this process holds no
    /// stream.
    Reattaches {
        lane: usize,
    },
    /// A wait on `lane` at the newest change.
    WaitNewest {
        lane: usize,
    },
    /// A wait on `lane` just inside the last range another process dropped
    /// on it.
    WaitDropped {
        lane: usize,
    },
    /// The seventh re-check's B-1, drawn often: this process's stream ends,
    /// another process rebuilds (or detaches) `lane`, this process's sink
    /// run stages with no stream held, the other process finishes (or
    /// reattaches), then this process opens, a change follows, and a new
    /// process waits for it. Expanded before the run.
    HolderFinishes {
        lane: usize,
        reattach: bool,
    },
    /// G8 drawn often: a backfill of `lane`'s index asked, the stream ends
    /// with another process dropping `lane`, the backfill completes with no
    /// stream held (owed), the other process keeps its drop as a gap, then
    /// this process opens and waits inside the gap. Expanded before the
    /// run.
    GapOverOwed {
        lane: usize,
    },
}

impl Event {
    /// The events `self` stands for, in order.
    fn expand(self) -> Vec<Event> {
        let opens = Event::Open {
            ok: true,
            jump: 0,
            drop: None,
            rebuild: false,
        };
        match self {
            Event::HolderFinishes { lane, reattach } => {
                return vec![
                    Event::StreamEnds {
                        jump: 10,
                        drop: Some(lane),
                        rebuild: !reattach,
                    },
                    Event::Restart {
                        sink: lane / 2,
                        unreadable: 0,
                    },
                    if reattach {
                        Event::Reattaches { lane }
                    } else {
                        Event::Finishes { lane }
                    },
                    opens.clone(),
                    Event::Apply { index: lane % 2 },
                    Event::NewProcess,
                    opens,
                    Event::WaitNewest { lane },
                ];
            }
            Event::GapOverOwed { lane } => {
                return vec![
                    Event::Missing { lane },
                    Event::Apply { index: lane % 2 },
                    Event::StreamEnds {
                        jump: 10,
                        drop: Some(lane),
                        rebuild: false,
                    },
                    Event::Seed { lane },
                    Event::Reattaches { lane },
                    opens,
                    Event::Apply { index: lane % 2 },
                    Event::WaitDropped { lane },
                ];
            }
            _ => {}
        }
        let Event::Unheld {
            lane,
            rebuild,
            then,
        } = self
        else {
            return vec![self];
        };
        let ends = Event::StreamEnds {
            jump: 10,
            drop: Some(lane),
            rebuild,
        };
        let index = lane % 2;
        let then = match then {
            0 => Event::Seed { lane },
            1 => Event::Reattach { index },
            2 => Event::Reindex { index },
            3 => Event::Restart {
                sink: lane / 2,
                unreadable: 0,
            },
            4 => Event::Detach { lane, fails: false },
            _ => Event::Unblock { lane },
        };
        vec![ends, then]
    }
}

fn event() -> impl Strategy<Value = Event> {
    prop_oneof![
        3 => (any::<bool>(), 0u64..3, prop::option::of(0usize..4), any::<bool>()).prop_map(
            |(ok, jump, drop, rebuild)| Event::Open { ok, jump: jump * 10, drop, rebuild }
        ),
        // Another process took a `primary` lane over: dropped, then began its
        // rebuild (B-2's ground), given its own weight.
        1 => (2usize..4).prop_map(|lane| Event::Open { ok: true, jump: 10, drop: Some(lane), rebuild: true }),
        2 => (0u64..3, prop::option::of(0usize..4), any::<bool>()).prop_map(
            |(jump, drop, rebuild)| Event::StreamEnds { jump: jump * 10, drop, rebuild }
        ),
        1 => (0usize..4, any::<bool>()).prop_map(|(lane, fails)| Event::Detach { lane, fails }),
        1 => (0usize..4).prop_map(|lane| Event::Unblock { lane }),
        3 => (0usize..2, 0u8..4).prop_map(|(sink, unreadable)| Event::Restart { sink, unreadable }),
        2 => (0usize..2).prop_map(|sink| Event::Readable { sink }),
        1 => (0usize..2).prop_map(|sink| Event::HoldReads { sink }),
        1 => (0usize..2).prop_map(|sink| Event::HoldEnsure { sink }),
        2 => (0usize..2).prop_map(|sink| Event::Release { sink }),
        1 => (0usize..4).prop_map(|lane| Event::Missing { lane }),
        4 => (0usize..2).prop_map(|index| Event::Apply { index }),
        2 => (0usize..4).prop_map(|lane| Event::Seed { lane }),
        1 => (1u64..10).prop_map(|by| Event::Horizon { by }),
        1 => Just(Event::Status),
        2 => Just(Event::NewProcess),
        2 => (0usize..2).prop_map(|index| Event::Reindex { index }),
        1 => (0usize..2).prop_map(|index| Event::Reattach { index }),
        5 => (0usize..4, any::<bool>(), any::<bool>(), 0u64..140).prop_map(
            |(lane, whole_sink, searchable, at)| Event::Wait { lane, whole_sink, searchable, at }
        ),
        6 => (0usize..4, any::<bool>(), 0u8..6).prop_map(
            |(lane, rebuild, then)| Event::Unheld { lane, rebuild, then }
        ),
        1 => (0usize..4).prop_map(|lane| Event::Finishes { lane }),
        1 => (0usize..4).prop_map(|lane| Event::Reattaches { lane }),
        1 => (0usize..4).prop_map(|lane| Event::WaitDropped { lane }),
        4 => (0usize..4, any::<bool>()).prop_map(
            |(lane, reattach)| Event::HolderFinishes { lane, reattach }
        ),
        3 => (2usize..4).prop_map(|lane| Event::GapOverOwed { lane }),
    ]
}

type Run = (JoinHandle<engine::Result<()>>, mpsc::Sender<SinkControl>);

struct World {
    stores: Vec<Arc<Store>>,
    lanes: Vec<LaneKey>,
    /// Whether this process holds a stream, kept by the world alone.
    holding: Arc<Holding>,
    /// The running process's pipeline: its own capture, as a real process
    /// has, starting where the slot confirmed.
    source: Arc<NumberedSource>,
    progress: Arc<Progress>,
    rereads: Arc<Rereads>,
    fence: Fence,
    tap: Arc<Tap>,
    stream: Arc<ChannelStream>,
    engines: Vec<Arc<SinkEngine>>,
    runs: Vec<Option<Run>>,
    /// An opening of the running process got through: `/wait` answers.
    live: bool,
    /// What the slot redelivers to the running process at its first stream:
    /// every change past the position the last one confirmed, by index.
    replay: Vec<(usize, u64)>,
    horizon: u64,
    /// What the slot confirmed: the source's answer for what the running
    /// process's progress says landed, after each step; kept across
    /// processes.
    confirmed: u64,
    /// The test's own clock: one tick per thing it does, so it orders a
    /// change, a drop and a reattach without asking the code.
    tick: u64,
    /// The changes published, by index: their positions, and when.
    changes: [Vec<(u64, u64)>; 2],
    /// Per lane, the ranges another process dropped, `(from, to]`, and when.
    dropped: Vec<Vec<(u64, u64, u64)>>,
    /// Per lane, when each reattach was attached.
    attached: Vec<Vec<u64>>,
    /// Per lane, the backfill asked and not yet completed: its ticket and
    /// where its walk began.
    asked: Vec<Option<(stream::Ticket, u64)>>,
}

/// One process's pipeline over the stores.
struct Pipeline {
    source: Arc<NumberedSource>,
    progress: Arc<Progress>,
    rereads: Arc<Rereads>,
    fence: Fence,
    tap: Arc<Tap>,
    stream: Arc<ChannelStream>,
    engines: Vec<Arc<SinkEngine>>,
}

impl Pipeline {
    /// A process's pipeline, its capture starting where the slot `confirmed`.
    fn new(confirmed: u64, stores: &[Arc<Store>]) -> Self {
        let source = Arc::new(NumberedSource::starting_at(confirmed));
        let source = &source;
        let lanes = lanes();
        let progress = Arc::new(Progress::new(lanes.iter().cloned()));
        let rereads = Arc::new(Rereads::default());
        rereads.will_open();
        progress.hold_while_unread(Arc::clone(&rereads));
        let fence = Fence::new(
            Arc::clone(source) as Arc<dyn ChangeCapture>,
            Arc::clone(&progress),
            lanes.iter().cloned(),
        )
        .with_rereads(Arc::clone(&rereads));
        let tap = Arc::new(Tap::default());
        let observer = Arc::new(engine::FanOut::new(vec![
            Arc::new(FenceObserver::new(fence.clone())) as Arc<dyn engine::Observer>,
            Arc::clone(&tap) as Arc<dyn engine::Observer>,
        ]));
        let stream = Arc::new(ChannelStream::over(
            256,
            INDEXES.map(index),
            SINKS.map(sink),
        ));
        for store in stores {
            store.enter();
        }
        let engines = SINKS
            .iter()
            .zip(stores)
            .map(|(name, store)| {
                Arc::new(
                    SinkEngine::new(
                        sink(name),
                        Arc::clone(store) as Arc<dyn Sink>,
                        Arc::clone(&stream) as Arc<dyn Stream>,
                        INDEXES.iter().map(|i| mapping(i)).collect(),
                        Arc::clone(&progress),
                    )
                    .with_options(sink::SinkOptions {
                        backfill: *name != "audit",
                    })
                    .with_observer(Arc::clone(&observer) as Arc<dyn engine::Observer>)
                    .with_rereads(Arc::clone(&rereads))
                    .with_positions(Arc::new(engine::CapturePositions(
                        Arc::clone(source) as Arc<dyn ChangeCapture>
                    ))),
                )
            })
            .collect();
        Self {
            source: Arc::clone(source),
            progress,
            rereads,
            fence,
            tap,
            stream,
            engines,
        }
    }
}

impl World {
    fn new() -> Self {
        let holding = Arc::new(Holding::default());
        let stores: Vec<Arc<Store>> = SINKS
            .iter()
            .map(|name| Arc::new(Store::new(*name == "audit", Arc::clone(&holding))))
            .collect();
        let pipeline = Pipeline::new(0, &stores);
        Self {
            stores,
            lanes: lanes(),
            holding,
            source: pipeline.source,
            progress: pipeline.progress,
            rereads: pipeline.rereads,
            fence: pipeline.fence,
            tap: pipeline.tap,
            stream: pipeline.stream,
            engines: pipeline.engines,
            runs: vec![None, None],
            live: false,
            replay: Vec::new(),
            horizon: 0,
            confirmed: 0,
            tick: 0,
            changes: [Vec::new(), Vec::new()],
            dropped: vec![Vec::new(); 4],
            attached: vec![Vec::new(); 4],
            asked: vec![None; 4],
        }
    }

    /// Hand the slot what the running process's progress says landed, as
    /// its ingest engine does after each commit.
    fn confirm(&mut self) {
        if !self.holding.holds() {
            return;
        }
        let landed = self.progress.landed();
        self.source.confirm(landed);
        if let Some(SourcePosition(at)) = self.source.reached(landed) {
            self.confirmed = self.confirmed.max(at);
        }
    }

    /// This process ends and a new one starts over the same stores. The new
    /// process is redelivered every change past what the slot confirmed, at
    /// its first stream; what a detached lane dropped before it is gone.
    async fn new_process(&mut self) {
        for at in 0..2 {
            self.stop(at).await;
        }
        let confirmed = self.confirmed;
        let mut replay: Vec<(usize, u64)> = (0..2)
            .flat_map(|index| {
                self.changes[index]
                    .iter()
                    .filter(|(at, _)| *at > confirmed)
                    .map(move |(at, _)| (index, *at))
            })
            .collect();
        replay.sort_by_key(|(_, at)| *at);
        self.holding.holds.store(false, Ordering::SeqCst);
        let pipeline = Pipeline::new(confirmed, &self.stores);
        self.source = pipeline.source;
        self.progress = pipeline.progress;
        self.rereads = pipeline.rereads;
        self.fence = pipeline.fence;
        self.tap = pipeline.tap;
        self.stream = pipeline.stream;
        self.engines = pipeline.engines;
        self.live = false;
        // A gap accepted before any stream keeps its end unsettled in the
        // store: the new process settles it at its own first stream.
        self.replay = replay;
        self.asked = vec![None; 4];
        for at in 0..2 {
            self.restart(at).await;
        }
    }

    fn sink_of(lane: usize) -> usize {
        lane / 2
    }

    fn index_of(lane: usize) -> &'static str {
        INDEXES[lane % 2]
    }

    fn lane_of(&self, key: &LaneKey) -> Option<usize> {
        self.lanes.iter().position(|lane| lane == key)
    }

    async fn stop(&mut self, at: usize) {
        if let Some((task, _)) = self.runs[at].take() {
            task.abort();
            let _ = task.await;
        }
    }

    /// Stop `sink`'s run, then stage and run it again, on its own task: a
    /// staging may wait on a held read. As the daemon does, a failed staging
    /// is tried again after a second, and the run starts only once one got
    /// through.
    async fn restart(&mut self, at: usize) {
        self.stop(at).await;
        let (send, mut control) = mpsc::channel(8);
        let engine = Arc::clone(&self.engines[at]);
        let task = tokio::spawn(async move {
            while engine.stage(Continuity::Resumed).await.is_err() {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            engine.run(&mut control).await
        });
        self.runs[at] = Some((task, send));
    }

    fn control(&self, at: usize, message: SinkControl) {
        if let Some((_, send)) = &self.runs[at] {
            let _ = send.try_send(message);
        }
    }

    /// Let the runs take what is ready, playing each index's builder: an
    /// attach is answered at once, a backfill only at its `Seed`.
    async fn settle(&mut self) {
        let mut requests = self.stream.requests().unwrap().consumer;
        for _ in 0..4 {
            tokio::time::sleep(Duration::from_millis(20)).await;
            while let Some(Ok(Some(delivery))) = requests.recv().now_or_never() {
                let (request, handle) = delivery.into_parts();
                match request {
                    Request::Attach {
                        index,
                        sink,
                        attempt,
                    } => {
                        let key = LaneKey::new(index, sink);
                        let _ = self.progress.attach(&key, attempt);
                        // The gap the operator accepted holds whatever the lane
                        // missed before this.
                        if let Some(lane) = self.lane_of(&key)
                            && self.stores[Self::sink_of(lane)].live_only
                        {
                            self.tick += 1;
                            self.attached[lane].push(self.tick);
                        }
                        if let Ok(lane) = self.stream.lane(&key) {
                            let _ = lane
                                .producer
                                .publish(LaneItem::Attached { attempt })
                                .now_or_never();
                        }
                    }
                    Request::Backfill {
                        index,
                        sink,
                        after,
                        ticket,
                    } => {
                        let key = LaneKey::new(index, sink);
                        if let Some(lane) = self.lane_of(&key) {
                            // A walk resumed from a checkpoint began where
                            // that walk began (its mark says); a fresh one
                            // begins now.
                            let began = match after {
                                Some(mark) => mark.as_str().parse().unwrap(),
                                None => self.horizon,
                            };
                            self.asked[lane] = Some((ticket, began));
                        }
                    }
                    _ => {}
                }
                let _ = handle.ack().await;
            }
            // A run that ended on a failed read is restarted by the daemon.
            for at in 0..2 {
                if self.runs[at]
                    .as_ref()
                    .is_some_and(|(task, _)| task.is_finished())
                {
                    self.restart(at).await;
                }
            }
        }
    }

    fn seed(&mut self, lane: usize) {
        let Some((ticket, began)) = self.asked[lane].take() else {
            return;
        };
        let of = Self::index_of(lane);
        self.stores[Self::sink_of(lane)]
            .completing
            .lock()
            .unwrap()
            .insert(of.to_owned(), began);
        if let Ok(at) = self.stream.lane(&self.lanes[lane]) {
            let _ = at
                .producer
                .publish(LaneItem::SnapshotComplete {
                    index: index(of),
                    ticket,
                })
                .now_or_never();
        }
    }

    /// Another process held the slot over `(from, to]`: it applied every
    /// lane but the ones a record keeps it off and `drop`, and dropped those;
    /// with `rebuild` it began rebuilding `drop` halfway.
    fn another_process(&mut self, from: u64, to: u64, drop: Option<usize>, rebuild: bool) {
        self.tick += 1;
        let now = self.tick;
        for lane in 0..4 {
            let store = &self.stores[Self::sink_of(lane)];
            let of = Self::index_of(lane);
            if drop == Some(lane) {
                if rebuild && !store.live_only {
                    let half = from + (to - from) / 2;
                    self.dropped[lane].push((from, half, now));
                    store.rebuilt_by_another(of, half);
                } else {
                    self.dropped[lane].push((from, to, now));
                    store.dropped(of);
                }
                continue;
            }
            let kept = store.kept.lock().unwrap().get(of).cloned();
            match kept.and_then(|kept| kept.detachment.record) {
                // A rebuild it finds not its own is put back to detached
                // before anything is dropped (B-2).
                Some(DetachRecord::Rebuilding { .. }) => {
                    store.walked.lock().unwrap().remove(of);
                    store.dropped(of);
                    self.dropped[lane].push((from, to, now));
                }
                Some(DetachRecord::Detached { .. }) => self.dropped[lane].push((from, to, now)),
                _ => {}
            }
        }
    }

    /// Another process finishes the rebuild of `lane` it began: its seed
    /// moves the marker (the rows read now, so every change so far is held)
    /// and deletes the record and the gaps. Only with no stream held here.
    fn finishes(&mut self, lane: usize) {
        if self.holding.holds() {
            return;
        }
        let store = &self.stores[Self::sink_of(lane)];
        let of = Self::index_of(lane);
        let rebuilding = store.kept.lock().unwrap().get(of).is_some_and(|kept| {
            matches!(
                kept.detachment.record,
                Some(DetachRecord::Rebuilding { .. })
            )
        });
        if !rebuilding {
            return;
        }
        store.kept.lock().unwrap().remove(of);
        store.gaps.lock().unwrap().remove(of);
        store.unseeded.lock().unwrap().remove(of);
        store.walked.lock().unwrap().remove(of);
        store
            .seeds
            .lock()
            .unwrap()
            .push((of.to_owned(), self.horizon));
    }

    /// Another process reattaches `lane`, which it detached: the record
    /// goes, the range it last dropped is kept as a gap with the marker and
    /// given up from now. Only with no stream held here.
    fn reattaches(&mut self, lane: usize) {
        if self.holding.holds() {
            return;
        }
        let store = &self.stores[Self::sink_of(lane)];
        let of = Self::index_of(lane);
        let detached = store.kept.lock().unwrap().get(of).is_some_and(|kept| {
            matches!(kept.detachment.record, Some(DetachRecord::Detached { .. }))
        });
        // A reattach's gap starts where reads stood at the detach (the
        // record's `through`): every range dropped since the last reattach.
        let since = self.attached[lane].last().copied().unwrap_or(0);
        let Some(from) = self.dropped[lane]
            .iter()
            .filter(|(_, _, when)| *when > since)
            .map(|(from, _, _)| *from)
            .min()
        else {
            return;
        };
        if !detached {
            return;
        }
        store.kept.lock().unwrap().remove(of);
        store
            .gaps
            .lock()
            .unwrap()
            .entry(of.to_owned())
            .or_default()
            .push(sink::Gap {
                from: Some(from.to_string()),
                to: Some(self.horizon.to_string()),
            });
        self.tick += 1;
        self.attached[lane].push(self.tick);
    }

    /// A change of `index`, published only while the process holds a stream:
    /// none reaches a lane otherwise.
    fn apply(&mut self, at: usize) {
        if !self.holding.holds() {
            return;
        }
        self.horizon += 5;
        self.tick += 1;
        self.changes[at].push((self.horizon, self.tick));
        self.publish(at, self.horizon);
    }

    /// Publish the change of `index` at `at` on both its lanes, as the
    /// running process's ingest engine would.
    fn publish(&self, at: usize, position: u64) {
        let change = self.source.emit(position);
        for sink_at in 0..2 {
            let key = &self.lanes[sink_at * 2 + at];
            self.progress
                .published(key, stream::ItemKey::new(change.seq), change, change);
        }
        self.progress.routed(change);
        for sink_at in 0..2 {
            let key = &self.lanes[sink_at * 2 + at];
            let item = LaneItem::Batch(Batch {
                key: Some(stream::ItemKey::new(change.seq)),
                position: Some(change),
                envelopes: vec![Envelope::upsert(
                    index(INDEXES[at]),
                    position.to_string(),
                    GenericValue::Map(BTreeMap::new()),
                    Some(change),
                    chrono::Utc::now(),
                )]
                .into(),
                ..Batch::default()
            });
            if let Ok(lane) = self.stream.lane(key) {
                let _ = lane.producer.publish(item).now_or_never();
            }
        }
    }

    /// Whether `lane` holds every change at or before `p`, as only the test
    /// knows it; why not, when it doesn't.
    ///
    /// A change this process never wrote is held by a seed whose walk began
    /// after it, or given up by a reattach attached after it (the gap the
    /// operator accepted), though never for a wait at that very change. What
    /// another process dropped is held only by such a seed: a wait inside the
    /// range it dropped never passes without one, a reattach or not; past
    /// it, a reattach after the drop gives it up.
    fn holds(&self, lane: usize, p: u64) -> std::result::Result<(), String> {
        let store = &self.stores[Self::sink_of(lane)];
        let of = Self::index_of(lane);
        let seeded = |at: u64| {
            store
                .seeds
                .lock()
                .unwrap()
                .iter()
                .any(|(index, began)| index == of && *began >= at)
        };
        let given_up = |when: u64| self.attached[lane].iter().any(|attach| *attach > when);
        let not_held = |what: String| {
            Err(format!(
                "{what} (seeds {:?}, written {:?}, reattached at {:?})",
                store.seeds.lock().unwrap(),
                store.written.lock().unwrap(),
                self.attached[lane]
            ))
        };
        for (at, when) in self.changes[lane % 2].iter().filter(|(at, _)| *at <= p) {
            let written = store
                .written
                .lock()
                .unwrap()
                .contains(&(of.to_owned(), *at));
            if written || seeded(*at) || (given_up(*when) && *at < p) {
                continue;
            }
            return not_held(format!("the change at {at} is not held"));
        }
        for (from, to, when) in &self.dropped[lane] {
            if *from >= p || seeded((*to).min(p)) {
                continue;
            }
            if p <= *to {
                return not_held(format!(
                    "{p} is inside ({from}, {to}], which another process dropped"
                ));
            }
            if !given_up(*when) {
                return not_held(format!(
                    "({from}, {to}], which another process dropped, is not held"
                ));
            }
        }
        Ok(())
    }

    fn violations(&self) -> Vec<String> {
        self.stores
            .iter()
            .flat_map(|store| store.violations.lock().unwrap().clone())
            .collect()
    }

    /// This process opens its stream at `start`; a failed opening leaves its
    /// capture closed, as a failed `live()` leaves no stream.
    async fn open(&mut self, ok: bool, start: u64) -> std::result::Result<(), ()> {
        self.holding.holds.store(false, Ordering::SeqCst);
        let source = Arc::clone(&self.source);
        let opened = self
            .rereads
            .open(async move {
                if ok {
                    source.closed.store(false, Ordering::SeqCst);
                    source.idle_to(start);
                    Ok(())
                } else {
                    source.close();
                    Err(())
                }
            })
            .await;
        self.horizon = self.horizon.max(start);
        if opened.is_ok() {
            self.holding.streams.fetch_add(1, Ordering::SeqCst);
            self.holding.holds.store(true, Ordering::SeqCst);
        }
        if opened.is_ok() && !self.live {
            self.live = true;
            for (index, at) in std::mem::take(&mut self.replay) {
                self.publish(index, at);
            }
        }
        opened
    }

    fn request(&self, lane: usize, whole_sink: bool, searchable: bool, at: u64) -> FenceRequest {
        let key = &self.lanes[lane];
        FenceRequest {
            position: at.to_string(),
            level: if searchable {
                FenceLevel::Searchable
            } else {
                FenceLevel::Applied
            },
            sink: Some(key.sink.clone()),
            indexes: if whole_sink {
                Vec::new()
            } else {
                vec![key.index.clone()]
            },
            timeout: Duration::from_millis(10),
        }
    }
}

async fn run(events: Vec<Event>) -> std::result::Result<(), TestCaseError> {
    let events: Vec<Event> = events.into_iter().flat_map(Event::expand).collect();
    let mut world = World::new();
    for at in 0..2 {
        world.restart(at).await;
    }
    for event in events {
        let event = match event {
            Event::WaitNewest { lane } => Event::Wait {
                lane,
                whole_sink: false,
                searchable: false,
                at: world.horizon,
            },
            Event::WaitDropped { lane } => Event::Wait {
                lane,
                whole_sink: false,
                searchable: false,
                at: world.dropped[lane].last().map_or(0, |(from, ..)| from + 1),
            },
            event => event,
        };
        match event.clone() {
            Event::Open {
                ok,
                jump,
                drop,
                rebuild,
            } => {
                let start = world.horizon + jump;
                if jump > 0 {
                    world.another_process(world.horizon, start, drop, rebuild);
                }
                let opened = world.open(ok, start).await;
                prop_assert_eq!(opened.is_ok(), ok);
            }
            Event::StreamEnds {
                jump,
                drop,
                rebuild,
            } => {
                world.rereads.closed();
                world.holding.holds.store(false, Ordering::SeqCst);
                world.source.close();
                let start = world.horizon + jump;
                if jump > 0 {
                    world.another_process(world.horizon, start, drop, rebuild);
                }
                world.horizon = start;
            }
            Event::Detach { lane, fails } => {
                let store = &world.stores[World::sink_of(lane)];
                let of = World::index_of(lane).to_owned();
                store.blocked.lock().unwrap().insert(of.clone());
                if fails {
                    store.detach_fails.lock().unwrap().insert(of);
                }
                world.control(
                    World::sink_of(lane),
                    SinkControl::Detach {
                        index: index(World::index_of(lane)),
                    },
                );
            }
            Event::Unblock { lane } => {
                world.stores[World::sink_of(lane)]
                    .blocked
                    .lock()
                    .unwrap()
                    .remove(World::index_of(lane));
            }
            Event::Restart {
                sink: at,
                unreadable,
            } => {
                *world.stores[at].unreadable.lock().unwrap() = INDEXES
                    .iter()
                    .enumerate()
                    .filter(|(bit, _)| unreadable & (1 << bit) != 0)
                    .map(|(_, name)| (*name).to_owned())
                    .collect();
                world.restart(at).await;
            }
            Event::Readable { sink: at } => world.stores[at].unreadable.lock().unwrap().clear(),
            Event::HoldReads { sink: at } => world.stores[at].read_hold.hold(),
            Event::HoldEnsure { sink: at } => world.stores[at].ensure_hold.hold(),
            Event::Release { sink: at } => {
                world.stores[at].read_hold.release();
                world.stores[at].ensure_hold.release();
            }
            Event::Missing { lane } => {
                world.stores[World::sink_of(lane)]
                    .missing
                    .lock()
                    .unwrap()
                    .insert(World::index_of(lane).to_owned());
            }
            Event::Apply { index: at } => world.apply(at),
            Event::Seed { lane } => world.seed(lane),
            Event::Horizon { by } => {
                world.horizon += by;
                if world.holding.holds() {
                    world.source.idle_to(world.horizon);
                }
            }
            Event::Status => {
                let _ = world.fence.positions();
            }
            Event::NewProcess => world.new_process().await,
            Event::Reindex { index: at } => world.control(
                1,
                SinkControl::Reindex {
                    indexes: vec![index(INDEXES[at])],
                },
            ),
            Event::Reattach { index: at } => world.control(
                0,
                SinkControl::Reattach {
                    index: index(INDEXES[at]),
                    reply: engine::Reply::ignored(),
                },
            ),
            // `/wait` answers `503` until the process's source is live.
            Event::Unheld { .. } | Event::HolderFinishes { .. } | Event::GapOverOwed { .. } => {
                unreachable!("expanded before the run")
            }
            Event::WaitNewest { .. } | Event::WaitDropped { .. } => {
                unreachable!("made a wait above")
            }
            Event::Finishes { lane } => world.finishes(lane),
            Event::Reattaches { lane } => world.reattaches(lane),
            Event::Wait { .. } if !world.live => {}
            Event::Wait {
                lane,
                whole_sink,
                searchable,
                at,
            } => {
                let selected: Vec<usize> = if whole_sink {
                    vec![World::sink_of(lane) * 2, World::sink_of(lane) * 2 + 1]
                } else {
                    vec![lane]
                };
                let request = world.request(lane, whole_sink, searchable, at);
                let answer = world.fence.wait(&request).await;
                // O2: while the store says a rebuild is under way on a lane
                // the engine rebuilds, reads see the old generation: a lane
                // not vouched for says `rebuilding`, never `backfilling`.
                if let Ok(FenceOutcome::Unserved { lanes, .. }) = &answer {
                    for (key, why) in lanes {
                        let Some(lane) = world.lane_of(key) else {
                            continue;
                        };
                        let rebuilds = world.stores[World::sink_of(lane)]
                            .kept
                            .lock()
                            .unwrap()
                            .get(World::index_of(lane))
                            .is_some_and(|kept| {
                                matches!(
                                    kept.detachment.record,
                                    Some(DetachRecord::Rebuilding { .. })
                                )
                            });
                        if rebuilds
                            && world.tap.rebuilding.lock().unwrap().contains(key)
                            && !world.tap.detached.lock().unwrap().contains(key)
                        {
                            prop_assert_eq!(
                                why,
                                &Unserved::Rebuilding,
                                "{} rebuilds, reads see the old generation",
                                key
                            );
                        }
                    }
                }
                if let Ok(FenceOutcome::Reached { .. }) = answer {
                    for lane in selected {
                        let held = world.holds(lane, at);
                        prop_assert!(
                            held.is_ok(),
                            "the fence passed a wait for {at} ({}) on {}: {}; detached {:?} \
                             rebuilding {:?} health {:?}",
                            if searchable { "searchable" } else { "applied" },
                            world.lanes[lane],
                            held.unwrap_err(),
                            world.tap.detached.lock().unwrap(),
                            world.tap.rebuilding.lock().unwrap(),
                            world.engines[World::sink_of(lane)].lane_health(&world.lanes[lane]),
                        );
                    }
                }
            }
        }
        world.settle().await;
        world.confirm();
        let violations = world.violations();
        prop_assert!(violations.is_empty(), "after {event:?}: {violations:?}");
    }

    // Liveness: every record readable, nothing held, the slot held, the runs
    // settled past their backoffs (a reindex owed or a rebuild under way
    // finishes: the staging backoff doubles to a minute), every backfill
    // completed.
    for store in &world.stores {
        store.unreadable.lock().unwrap().clear();
        store.blocked.lock().unwrap().clear();
        store.read_hold.release();
        store.ensure_hold.release();
    }
    let start = world.horizon;
    world.open(true, start).await.unwrap();
    // Then a change on every index, so no wait below falls in a gap a
    // reattach accepted meanwhile; and settled again.
    for round in 0..50 {
        if round == 40 {
            for at in 0..2 {
                world.apply(at);
            }
        }
        world.settle().await;
        world.confirm();
        for lane in 0..4 {
            world.seed(lane);
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    world.settle().await;
    let violations = world.violations();
    prop_assert!(violations.is_empty(), "at the end: {violations:?}");
    let newest = world.horizon;
    for lane in 0..4 {
        let key = world.lanes[lane].clone();
        let answer = world
            .fence
            .wait(&world.request(lane, false, false, newest))
            .await;
        let reached = matches!(answer, Ok(FenceOutcome::Reached { .. }));
        if reached {
            let held = world.holds(lane, newest);
            prop_assert!(
                held.is_ok(),
                "{key} vouched for {newest}: {}",
                held.unwrap_err()
            );
        }
        if world.tap.detached.lock().unwrap().contains(&key)
            || world.tap.rebuilding.lock().unwrap().contains(&key)
        {
            continue;
        }
        prop_assert!(
            reached,
            "{key}: readable, the slot held and settled, yet a wait for {newest} answered \
             {answer:?}; health {:?}; rereads {:?}; reads {:?}; run finished {:?}",
            world.engines[World::sink_of(lane)].lane_health(&key),
            world.rereads,
            world.stores[World::sink_of(lane)].reads.lock().unwrap(),
            world.runs[World::sink_of(lane)]
                .as_ref()
                .map(|(task, _)| task.is_finished()),
        );
    }
    for at in 0..2 {
        world.stop(at).await;
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: super::proptest_cases(1024), ..ProptestConfig::default() })]

    #[test]
    fn every_lane_vouches_only_for_what_it_holds(
        events in prop::collection::vec(event(), 1..40)
    ) {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .start_paused(true)
            .build()
            .unwrap()
            .block_on(run(events))?;
    }
}

/// Run `events` in the world, as the property does.
fn replay(events: Vec<Event>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .start_paused(true)
        .build()
        .unwrap()
        .block_on(run(events))
        .unwrap();
}

/// The sixth re-check, B-1, the reviewer's first seed, as its shrunk events:
/// the seed in the regression file replays another list once the strategy
/// changes. A reattach waits in `ensure_index` while the stream ends; its
/// delete of another process's record is refused.
#[test]
fn a_reattach_waiting_in_ensure_as_the_stream_ends_deletes_no_record() {
    replay(vec![
        Event::StreamEnds {
            jump: 10,
            drop: Some(1),
            rebuild: false,
        },
        Event::Open {
            ok: true,
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::HoldEnsure { sink: 0 },
        Event::Reattach { index: 1 },
        Event::StreamEnds {
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::Release { sink: 0 },
    ]);
}

/// The sixth re-check, B-1, the reviewer's second seed: the same, the
/// reattach waiting in its read of the records instead.
#[test]
fn a_reattach_waiting_in_its_read_as_the_stream_ends_deletes_no_record() {
    replay(vec![
        Event::StreamEnds {
            jump: 10,
            drop: Some(1),
            rebuild: false,
        },
        Event::Open {
            ok: true,
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::HoldReads { sink: 0 },
        Event::Reattach { index: 1 },
        Event::StreamEnds {
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::Release { sink: 0 },
    ]);
}

/// An earlier round's seed, as its shrunk events: another process's rebuild
/// found by a new process, then a reindex.
#[test]
fn a_reindex_after_a_new_process_found_anothers_rebuild() {
    replay(vec![
        Event::Open {
            ok: true,
            jump: 10,
            drop: Some(2),
            rebuild: true,
        },
        Event::NewProcess,
        Event::Reindex { index: 0 },
    ]);
}

/// Found by the world in the seventh re-check (S-3's new events), as its
/// shrunk events: a backfill asked, another process's drop kept as a gap
/// while this process holds no slot, the completion granted as an ordinary
/// seed after the reopen. The gap cuts the backfill: nothing seeds over it.
#[test]
fn a_gap_written_while_unheld_cuts_the_backfill_asked_before_it() {
    replay(vec![
        Event::Open {
            ok: true,
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::Reindex { index: 0 },
        Event::GapOverOwed { lane: 2 },
    ]);
}

/// Found by the world in the seventh re-check: a rebuild taken over at a
/// start whose record another process removed, then another process's new
/// rebuild under the same attempt count: demoted, never taken for this
/// process's own.
#[test]
fn a_later_rebuild_under_the_same_attempts_is_not_taken_for_ours() {
    replay(vec![
        Event::Unheld {
            lane: 3,
            rebuild: true,
            then: 0,
        },
        Event::GapOverOwed { lane: 3 },
        Event::Open {
            ok: true,
            jump: 10,
            drop: Some(3),
            rebuild: true,
        },
        Event::StreamEnds {
            jump: 0,
            drop: None,
            rebuild: false,
        },
    ]);
}

/// The seventh re-check's B-1 as the world draws it: another process
/// finishes the rebuild this process met with no slot held, then this
/// process opens, a change follows, and a new process waits for it.
#[test]
fn a_rebuild_the_holder_finished_drops_nothing_unrecorded() {
    replay(
        Event::HolderFinishes {
            lane: 3,
            reattach: false,
        }
        .expand(),
    );
}

/// B-1's other half as the world draws it: another process reattaches a
/// lane this process holds detached under that process's record.
#[test]
fn a_lane_the_holder_reattached_drops_nothing_unrecorded() {
    replay(
        Event::HolderFinishes {
            lane: 0,
            reattach: true,
        }
        .expand(),
    );
}

/// Found by the world in the seventh re-check: this process's rebuild
/// begins while it holds the slot, then waits in `ensure_index` while its
/// stream ends and another process drops the lane and reattaches it. Read
/// once held, the lane is recorded detached again and the rebuild dropped,
/// in the run as in the engine: its attach opens nothing, and another
/// process's later rebuild under the same attempt count is not taken for it.
#[test]
fn a_rebuild_whose_record_was_replaced_meanwhile_is_dropped_by_the_run() {
    replay(vec![
        Event::HolderFinishes {
            lane: 2,
            reattach: true,
        },
        Event::Unheld {
            lane: 0,
            rebuild: false,
            then: 0,
        },
        Event::Reindex { index: 0 },
        Event::GapOverOwed { lane: 2 },
        Event::HoldEnsure { sink: 1 },
        Event::Open {
            ok: true,
            jump: 0,
            drop: None,
            rebuild: false,
        },
        Event::GapOverOwed { lane: 2 },
        Event::Release { sink: 1 },
        Event::Open {
            ok: true,
            jump: 10,
            drop: Some(2),
            rebuild: true,
        },
        Event::Readable { sink: 0 },
    ]);
}

/// Found by the world in the seventh re-check: this process takes over a
/// rebuild at its first staging (no slot held yet); with no read here in
/// between, the record is removed and another process begins a new rebuild,
/// its attempts restarting at the same count. Read once held, the new
/// rebuild's generation is not the one this process fills: it is demoted,
/// never taken for this process's own.
#[test]
fn a_rebuild_begun_again_under_the_same_attempts_is_told_apart_by_its_generation() {
    replay(vec![
        Event::Reindex { index: 1 },
        Event::Unheld {
            lane: 0,
            rebuild: false,
            then: 0,
        },
        Event::Unheld {
            lane: 3,
            rebuild: true,
            then: 3,
        },
        Event::GapOverOwed { lane: 3 },
        Event::Unheld {
            lane: 3,
            rebuild: true,
            then: 1,
        },
    ]);
}

/// Found by the world at 20,000 cases in the seventh re-check: a backfill
/// cut by a gap the read found while what the records asked waited for the
/// slot was never asked again, and its index stayed backfilling.
#[test]
fn a_backfill_cut_while_the_slot_is_away_is_asked_again() {
    replay(vec![
        Event::Unheld {
            lane: 3,
            rebuild: true,
            then: 0,
        },
        Event::Finishes { lane: 3 },
        Event::GapOverOwed { lane: 3 },
    ]);
}
