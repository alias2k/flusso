//! Detaching a lane (ADR 0012 stage 10, slice 2 step 2): asked for, it
//! happens at the lane's next refused probe or its next staging; the sink
//! records it first; then the lane's fence freezes, it leaves the slot's
//! minimum, and everything it holds or receives is acknowledged unwritten.
//! Staging brings a detached index back detached. All on paused time.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position};
use sink::{
    DetachRecord, Detachment, MarkerAt, RecordVersion, Recorded, Sink, SinkError, WriteReport,
};
use source::cdc::{Continuity, Landed};
use stream::{Batch, LaneItem, LaneKey, Stream};
use stream_channel::ChannelStream;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::sink_name;
use crate::sink_engine::Blame;
use crate::{LaneHealth, Level, Observer, Progress, SinkControl, SinkEngine};

fn index(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn users() -> IndexName {
    index("users")
}

fn orders() -> IndexName {
    index("orders")
}

fn lane(of: IndexName) -> LaneKey {
    LaneKey::new(of, sink_name("primary"))
}

fn mapping(of: IndexName) -> IndexMapping {
    IndexMapping {
        index: of,
        hash: kernel::ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// How the sink answers a detach.
#[derive(Debug, Clone, Copy)]
enum Record {
    Kept,
    NotKept,
    Fail,
    /// The record lands, and the call still fails (a timeout after
    /// OpenSearch applied it), and so does the read right after it: the
    /// outcome stays unknown until a later read.
    FailAfterWriting,
    /// The record lands exactly as asked, then the call fails.
    LandsAsAskedThenFails,
}

/// A sink that blocks, or misses, the indexes it is told to, records every
/// write and every detach, and answers staging's read with what it is given.
///
/// It keeps what it is asked to keep and nothing more: no marker moved by a
/// detach, no gaps carried, no put-back, no adapter rule on `through`. What
/// depends on how the OpenSearch sink writes its documents is the daemon's
/// world's (`libs/3-daemon/src/fence/tests/gate_world.rs`), whose stand-in
/// runs the sink's own plans (the review of the closing round, item 1).
#[derive(Debug, Default)]
struct Gate {
    blocked: Mutex<BTreeSet<String>>,
    missing: Mutex<BTreeSet<String>>,
    /// Each write: when, and the index of each document.
    writes: Mutex<Vec<(Instant, Vec<String>)>>,
    ensured: Mutex<Vec<String>>,
    detaches: Mutex<Vec<(String, sink::Detach)>>,
    /// How the next detaches are answered, in order; then `Kept`.
    records: Mutex<VecDeque<Record>>,
    /// Each detach takes this long.
    detach_takes: Mutex<Option<Duration>>,
    /// What staging reads of each index's records.
    kept: Mutex<BTreeMap<String, Detachment>>,
    forgotten: Mutex<Vec<String>>,
    /// Indexes `is_seeded` answers unseeded.
    unseeded: Mutex<BTreeSet<String>>,
    /// Every document written, as `index/id`, in write order.
    ids: Mutex<Vec<String>>,
    /// A write holding this id takes this long.
    slow: Mutex<Option<(String, Duration)>>,
    /// Each rebuild begun, by index.
    rebuilds: Mutex<Vec<String>>,
    /// Each demotion, by index, with whether live only.
    demoted: Mutex<Vec<(String, bool)>>,
    /// Each index marked seeded.
    seeded: Mutex<Vec<String>>,
    /// The gaps reattaches keep with each index's seed marker.
    marker_gaps: Mutex<BTreeMap<String, Vec<sink::Gap>>>,
    /// Each list of gaps written, by index, in order.
    gaps: Mutex<Vec<(String, Vec<sink::Gap>)>>,
    /// The seed meets a block on the serving generation (#479).
    retire_blocked: Mutex<bool>,
    /// Calls that fail from now on, by method name: the process died
    /// before them.
    dead: Mutex<BTreeSet<&'static str>>,
    /// While set, `ensure_index` waits for a permit here.
    ensure_gate: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    /// While set, a read of the records waits for a permit here.
    read_gate: Mutex<Option<Arc<tokio::sync::Semaphore>>>,
    /// Each index a reindex moved to a new generation.
    reindexed: Mutex<Vec<String>>,
    /// `forget_detachment` deletes the record, then fails (a timeout after
    /// the delete landed).
    forget_lands_then_fails: Mutex<bool>,
    /// The next reads of the records that fail.
    reads_fail: Mutex<usize>,
    /// Indexes whose records can't be read.
    unreadable: Mutex<BTreeSet<String>>,
    /// The rereads the engine counts reads in, to judge each write by.
    rereads: std::sync::OnceLock<Arc<crate::Rereads>>,
    /// Each document written on a lane not read under the current stream.
    unread_writes: Mutex<Vec<String>>,
    /// Each index's walk checkpoint, as a real sink keeps it with the marker.
    walked: Mutex<BTreeMap<String, String>>,
    /// Reads of the records that fail once `forget_detachment` landed then
    /// failed.
    reads_fail_after_forget: Mutex<usize>,
    /// The seed lands but its best-effort delete of a finished
    /// rebuild's record fails (the adapter's `forget_finished_rebuild` only
    /// warns): `Ok`, the record kept.
    seed_keeps_record: Mutex<bool>,
    /// `begin_rebuild` lands, then its call fails (a timeout).
    rebuild_lands_then_fails: Mutex<bool>,
    /// A read takes its answer, then waits on `read_gate`.
    read_first: Mutex<bool>,
    /// The next demotion fails before writing anything.
    demote_fails: Mutex<bool>,
    /// The last version this sink gave a record or a tombstone: every write
    /// gets a new one, as OpenSearch's sequence numbers do.
    versions: Mutex<u64>,
    /// Where a forget left a tombstone in place of a record: its version
    /// and stamp.
    tombstones: Mutex<BTreeMap<String, (RecordVersion, Option<sink::Stamp>)>>,
    /// The stamp of each index's marker's last stamped write.
    marker_stamps: Mutex<BTreeMap<String, sink::Stamp>>,
}

impl Gate {
    /// A version no write of this sink gave before (and never the `7:1` a
    /// test writes by hand).
    fn next_version(&self) -> RecordVersion {
        let mut versions = self.versions.lock().unwrap();
        *versions += 1;
        RecordVersion::new(format!("{}:9", 100 + *versions))
    }

    /// `record` stored for `index` as a write stamped `stamp` leaves it: a
    /// new version, the stamp kept, any tombstone replaced.
    fn store(&self, index: &str, mut record: Detachment, stamp: Option<&sink::Stamp>) {
        record.version = Some(self.next_version());
        record.record_stamp = stamp.cloned();
        self.tombstones.lock().unwrap().remove(index);
        self.kept.lock().unwrap().insert(index.to_owned(), record);
    }

    /// The marker of `index` written by `stamp`.
    fn stamp_marker(&self, index: &str, stamp: Option<&sink::Stamp>) {
        let mut stamps = self.marker_stamps.lock().unwrap();
        match stamp {
            Some(stamp) => stamps.insert(index.to_owned(), stamp.clone()),
            None => stamps.remove(index),
        };
    }

    /// `index`'s marker written whole by a seed: no gap.
    fn marker_written_whole(&self, index: &str, stamp: Option<&sink::Stamp>) {
        self.marker_gaps.lock().unwrap().remove(index);
        self.stamp_marker(index, stamp);
    }

    /// What a read of `index`'s records finds: the record or its
    /// tombstone, the marker's gaps and stamp.
    fn read_kept(&self, index: &str) -> Detachment {
        let mut kept = self
            .kept
            .lock()
            .unwrap()
            .get(index)
            .cloned()
            .unwrap_or_default();
        if kept.record.is_none()
            && let Some((version, stamp)) = self.tombstones.lock().unwrap().get(index)
        {
            kept.version = Some(version.clone());
            kept.record_stamp = stamp.clone();
        }
        kept.gaps = self
            .marker_gaps
            .lock()
            .unwrap()
            .get(index)
            .cloned()
            .unwrap_or_default();
        kept.marker_stamp = self.marker_stamps.lock().unwrap().get(index).cloned();
        kept
    }

    /// The record of `index` forgotten by `stamp`: a tombstone in its place.
    fn tombstone(&self, index: &str, stamp: Option<&sink::Stamp>) {
        self.kept.lock().unwrap().remove(index);
        let version = self.next_version();
        self.tombstones
            .lock()
            .unwrap()
            .insert(index.to_owned(), (version, stamp.cloned()));
    }

    fn dies_before(&self, call: &'static str) -> sink::Result<()> {
        if self.dead.lock().unwrap().contains(call) {
            return Err(SinkError::Write(format!("the process died before {call}")));
        }
        Ok(())
    }
}

impl Gate {
    fn block(&self, of: &IndexName, blocked: bool) {
        let mut set = self.blocked.lock().unwrap();
        if blocked {
            set.insert(of.to_string());
        } else {
            set.remove(of.as_ref());
        }
    }

    /// When each write holding `of` went out.
    fn writes_of(&self, of: &IndexName) -> Vec<Instant> {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, indexes)| indexes.iter().any(|at| at == of.as_ref()))
            .map(|(when, _)| *when)
            .collect()
    }

    fn detaches(&self) -> Vec<(String, sink::Detach)> {
        self.detaches.lock().unwrap().clone()
    }
}

#[async_trait]
impl Sink for Gate {
    async fn ensure_index(&self, mapping: &IndexMapping) -> sink::Result<sink::Ensured> {
        self.dies_before("ensure_index")?;
        let gate = self.ensure_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.acquire().await.unwrap().forget();
        }
        self.ensured.lock().unwrap().push(mapping.index.to_string());
        Ok(sink::Ensured::default())
    }

    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        let caught_up = if caught_up {
            sink::CaughtUp::All
        } else {
            sink::CaughtUp::No
        };
        self.write_parts(&[envelopes], caught_up).await
    }

    async fn write_parts(
        &self,
        parts: &[&[Envelope]],
        _: sink::CaughtUp<'_>,
    ) -> sink::Result<WriteReport> {
        let slow = self.slow.lock().unwrap().clone();
        if let Some((id, takes)) = slow
            && parts
                .iter()
                .flat_map(|part| part.iter())
                .any(|e| e.id == id)
        {
            tokio::time::sleep(takes).await;
        }
        let mut report = WriteReport::clean();
        let mut indexes = Vec::new();
        let mut named = BTreeSet::new();
        for envelope in parts.iter().flat_map(|part| part.iter()) {
            indexes.push(envelope.index.to_string());
            if self
                .rereads
                .get()
                .is_some_and(|rereads| !rereads.current(&lane(envelope.index.clone())))
            {
                self.unread_writes
                    .lock()
                    .unwrap()
                    .push(format!("{}/{}", envelope.index, envelope.id));
            }
            self.ids
                .lock()
                .unwrap()
                .push(format!("{}/{}", envelope.index, envelope.id));
            if !named.insert(envelope.index.clone()) {
                continue;
            }
            if self
                .blocked
                .lock()
                .unwrap()
                .contains(envelope.index.as_ref())
            {
                report = report.blocking(
                    envelope.index.clone(),
                    format!(
                        "index [{}] blocked by: [FORBIDDEN/8/index write (api)]",
                        envelope.index
                    ),
                );
            } else if self
                .missing
                .lock()
                .unwrap()
                .contains(envelope.index.as_ref())
            {
                report = report.missing_target(
                    envelope.index.clone(),
                    format!("no such index [{}]", envelope.index),
                );
            }
        }
        if !indexes.is_empty() {
            self.writes.lock().unwrap().push((Instant::now(), indexes));
        }
        Ok(report)
    }

    /// A reindex moved the index's writes to a new generation, which reads
    /// don't see until its seed.
    async fn is_rebuilding(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(self.reindexed.lock().unwrap().contains(&index.to_string()))
    }

    async fn is_seeded(&self, index: &IndexName) -> sink::Result<bool> {
        Ok(!self.unseeded.lock().unwrap().contains(index.as_ref()))
    }

    async fn record_walked(&self, index: &IndexName, mark: &str) -> sink::Result<()> {
        self.walked
            .lock()
            .unwrap()
            .insert(index.to_string(), mark.to_owned());
        Ok(())
    }

    async fn forget_walked(&self, index: &IndexName) -> sink::Result<()> {
        self.walked.lock().unwrap().remove(index.as_ref());
        Ok(())
    }

    async fn seeding(&self, index: &IndexName) -> sink::Result<sink::Seeding> {
        Ok(if self.unseeded.lock().unwrap().contains(index.as_ref()) {
            sink::Seeding::Unseeded {
                walked: self.walked.lock().unwrap().get(index.as_ref()).cloned(),
            }
        } else {
            sink::Seeding::Seeded
        })
    }

    fn records(&self) -> Option<&dyn sink::Records> {
        Some(self)
    }
}

#[async_trait]
impl sink::Records for Gate {
    async fn ensure_index_as(
        &self,
        mapping: &IndexMapping,
        _: &sink::Stamp,
    ) -> sink::Result<sink::Ensured> {
        sink::Sink::ensure_index(self, mapping).await
    }

    async fn detachment(
        &self,
        mapping: &IndexMapping,
        _: &sink::Stamp,
    ) -> sink::Result<Detachment> {
        if *self.read_first.lock().unwrap() {
            let kept = self.read_kept(mapping.index.as_ref());
            let gate = self.read_gate.lock().unwrap().clone();
            if let Some(gate) = gate {
                gate.acquire().await.unwrap().forget();
            }
            return Ok(kept);
        }
        let gate = self.read_gate.lock().unwrap().clone();
        if let Some(gate) = gate {
            gate.acquire().await.unwrap().forget();
        }
        {
            let mut fails = self.reads_fail.lock().unwrap();
            if *fails > 0 {
                *fails -= 1;
                return Err(SinkError::Write("the meta index timed out".to_owned()));
            }
        }
        if self
            .unreadable
            .lock()
            .unwrap()
            .contains(mapping.index.as_ref())
        {
            return Err(SinkError::Write(format!(
                "the records of {} can't be read",
                mapping.index
            )));
        }
        Ok(self.read_kept(mapping.index.as_ref()))
    }

    async fn detach(
        &self,
        mapping: &IndexMapping,
        detach: &sink::Detach,
    ) -> sink::Result<Recorded> {
        self.detaches
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), detach.clone()));
        let takes = *self.detach_takes.lock().unwrap();
        if let Some(takes) = takes {
            tokio::time::sleep(takes).await;
        }
        let answer = self.records.lock().unwrap().pop_front();
        match answer.unwrap_or(Record::Kept) {
            // Kept as a real sink keeps it: the next staging reads it
            // (review of step 3, S2).
            kind @ (Record::Kept | Record::LandsAsAskedThenFails) => {
                let attempts = self
                    .kept
                    .lock()
                    .unwrap()
                    .get(mapping.index.as_ref())
                    .and_then(|kept| kept.attempts);
                let marker = if detach.live_only {
                    MarkerAt::Serving { seeded: true }
                } else {
                    MarkerAt::Newer { seeded: false }
                };
                let mut record = Detachment::recorded(
                    DetachRecord::Detached {
                        live_only: detach.live_only,
                    },
                    Some(marker),
                    version(),
                );
                record.attempts = attempts;
                record.through = detach.through.clone();
                if let Some(stamp) = &detach.stamp {
                    self.stamp_marker(mapping.index.as_ref(), Some(stamp));
                }
                self.store(mapping.index.as_ref(), record, detach.stamp.as_ref());
                if matches!(kind, Record::LandsAsAskedThenFails) {
                    return Err(SinkError::Write(
                        "the record landed, the call timed out".to_owned(),
                    ));
                }
                Ok(Recorded::Kept)
            }
            Record::NotKept => Ok(Recorded::NotKept),
            Record::Fail => Err(SinkError::Write(
                "the meta index refused the record".to_owned(),
            )),
            Record::FailAfterWriting => {
                self.stamp_marker(mapping.index.as_ref(), detach.stamp.as_ref());
                self.store(
                    mapping.index.as_ref(),
                    Detachment::recorded(
                        DetachRecord::Detached { live_only: false },
                        Some(MarkerAt::Newer { seeded: false }),
                        version(),
                    ),
                    detach.stamp.as_ref(),
                );
                *self.reads_fail.lock().unwrap() += 1;
                Err(SinkError::Write("the record write timed out".to_owned()))
            }
        }
    }

    async fn forget_detachment(
        &self,
        mapping: &IndexMapping,
        version: &RecordVersion,
        stamp: &sink::Stamp,
    ) -> sink::Result<()> {
        self.dies_before("forget_detachment")?;
        let index = mapping.index.as_ref();
        // Conditional on the version read, as the adapter's tombstone is.
        if self.read_kept(index).version.as_ref() != Some(version) {
            return Err(SinkError::Write(format!(
                "{index}: version conflict, the record changed since {}",
                version.as_str()
            )));
        }
        if *self.forget_lands_then_fails.lock().unwrap() {
            self.tombstone(index, Some(stamp));
            *self.reads_fail.lock().unwrap() =
                std::mem::take(&mut *self.reads_fail_after_forget.lock().unwrap());
            return Err(SinkError::Write("the delete timed out".to_owned()));
        }
        self.forgotten
            .lock()
            .unwrap()
            .push(format!("{}@{}", mapping.index, version.as_str()));
        self.tombstone(index, Some(stamp));
        Ok(())
    }

    async fn record_gaps(
        &self,
        mapping: &IndexMapping,
        decide: &sink::DecideGaps<'_>,
        stamp: &sink::Stamp,
    ) -> sink::Result<Vec<sink::Gap>> {
        self.dies_before("record_gap")?;
        let kept = self
            .marker_gaps
            .lock()
            .unwrap()
            .get(mapping.index.as_ref())
            .cloned()
            .unwrap_or_default();
        let Some(gaps) = decide(&kept) else {
            return Ok(kept);
        };
        self.gaps
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), gaps.clone()));
        self.marker_gaps
            .lock()
            .unwrap()
            .insert(mapping.index.to_string(), gaps.clone());
        self.stamp_marker(mapping.index.as_ref(), Some(stamp));
        Ok(gaps)
    }

    async fn begin_rebuild(
        &self,
        mapping: &IndexMapping,
        stamp: &sink::Stamp,
    ) -> sink::Result<u64> {
        self.dies_before("begin_rebuild")?;
        let index = mapping.index.as_ref();
        let Some(mut record) = self.kept.lock().unwrap().get(index).cloned() else {
            return Err(SinkError::Write("no detach record".to_owned()));
        };
        let attempts = record.attempts.unwrap_or(0) + 1;
        record.record = Some(DetachRecord::Rebuilding { live_only: false });
        record.attempts = Some(attempts);
        record.marker = Some(MarkerAt::Newer { seeded: false });
        self.stamp_marker(index, Some(stamp));
        self.store(index, record, Some(stamp));
        self.rebuilds
            .lock()
            .unwrap()
            .push(mapping.index.to_string());
        if std::mem::take(&mut *self.rebuild_lands_then_fails.lock().unwrap()) {
            return Err(SinkError::Write(
                "the rebuild's record write timed out".to_owned(),
            ));
        }
        Ok(attempts)
    }

    async fn demote(
        &self,
        mapping: &IndexMapping,
        live_only: bool,
        stamp: &sink::Stamp,
    ) -> sink::Result<()> {
        if std::mem::take(&mut *self.demote_fails.lock().unwrap()) {
            return Err(SinkError::Write(
                "the demotion was refused (nothing written)".to_owned(),
            ));
        }
        let index = mapping.index.as_ref();
        // A demotion forgets the walk, as the real sink's does.
        self.walked.lock().unwrap().remove(index);
        let record = self.kept.lock().unwrap().get(index).cloned();
        if let Some(mut record) = record {
            record.record = Some(DetachRecord::Detached { live_only });
            // R14: live only, the marker is back on the generation reads see.
            record.marker = Some(if live_only {
                MarkerAt::Serving { seeded: true }
            } else {
                MarkerAt::Newer { seeded: false }
            });
            self.stamp_marker(index, Some(stamp));
            self.store(index, record, Some(stamp));
        }
        self.demoted
            .lock()
            .unwrap()
            .push((mapping.index.to_string(), live_only));
        Ok(())
    }

    async fn mark_seeded_as(&self, index: &IndexName, stamp: &sink::Stamp) -> sink::Result<()> {
        if *self.retire_blocked.lock().unwrap() {
            return Err(SinkError::RetireBlocked {
                new: format!("{index}_2"),
                old: format!("{index}_1"),
                block: "index write (api)".to_owned(),
                deletable: false,
            });
        }
        // The marker written whole: no gap, this seed's stamp.
        self.marker_written_whole(index.as_ref(), Some(stamp));
        // r4: the seed forgets a finished rebuild's record.
        if *self.seed_keeps_record.lock().unwrap() {
            if let Some(kept) = self.kept.lock().unwrap().get_mut(index.as_ref()) {
                kept.marker = Some(MarkerAt::Newer { seeded: true });
            }
        } else if self.kept.lock().unwrap().contains_key(index.as_ref()) {
            self.tombstone(index.as_ref(), Some(stamp));
        }
        self.seeded.lock().unwrap().push(index.to_string());
        Ok(())
    }

    async fn reindex_as(&self, mapping: &IndexMapping, stamp: &sink::Stamp) -> sink::Result<()> {
        self.reindexed
            .lock()
            .unwrap()
            .push(mapping.index.to_string());
        self.stamp_marker(mapping.index.as_ref(), Some(stamp));
        Ok(())
    }
}

/// Every lane health the observers heard, in order.
#[derive(Debug, Default)]
struct Heard {
    health: Mutex<Vec<(LaneKey, LaneHealth)>>,
    /// The deployment's progress, read as each detach is heard.
    progress: std::sync::OnceLock<Arc<Progress>>,
    /// For each detach heard, whether its lane had already left the
    /// minimum: never, or the fence would freeze where `Free` reaches (R5).
    left_first: Mutex<Vec<bool>>,
    /// Each index heard rebuilding.
    rebuilding: Mutex<Vec<IndexName>>,
    /// Each list of gaps heard, with how many lane healths were heard before.
    gaps: Mutex<Vec<(LaneKey, Vec<sink::Gap>, usize)>>,
    /// How many probes were heard. A run that probes without yielding never
    /// lets a paused-time test go on: past [`PROBES_WITHOUT_END`] it fails.
    probes: Mutex<u64>,
}

/// More probes than any test makes, but for one that spins.
const PROBES_WITHOUT_END: u64 = 10_000;

impl Heard {
    fn kinds(&self, of: &LaneKey) -> Vec<&'static str> {
        self.health
            .lock()
            .unwrap()
            .iter()
            .filter(|(at, _)| at == of)
            .map(|(_, health)| health.kind())
            .collect()
    }
}

impl Observer for Heard {
    fn on_lane_probe(&self, _: &LaneKey, outcome: crate::ProbeOutcome) {
        let mut probes = self.probes.lock().unwrap();
        *probes += 1;
        assert!(
            *probes < PROBES_WITHOUT_END,
            "{PROBES_WITHOUT_END} probes heard, the last {outcome:?}: the run probes without yielding"
        );
    }

    fn on_index_rebuilding(&self, _: &kernel::SinkName, index: &IndexName) {
        self.rebuilding.lock().unwrap().push(index.clone());
    }

    fn on_lane_gaps(&self, lane: &LaneKey, gaps: &[sink::Gap]) {
        // Every read tells its list; these tests follow the lists kept.
        if gaps.is_empty() {
            return;
        }
        let heard = self.health.lock().unwrap().len();
        self.gaps
            .lock()
            .unwrap()
            .push((lane.clone(), gaps.to_vec(), heard));
    }

    fn on_lane_health(&self, lane: &LaneKey, health: &LaneHealth) {
        if health.is_detached()
            && let Some(progress) = self.progress.get()
        {
            self.left_first
                .lock()
                .unwrap()
                .push(progress.is_detached(lane));
        }
        self.health
            .lock()
            .unwrap()
            .push((lane.clone(), health.clone()));
    }
}

fn item(of: &IndexName, id: i64, position: u64) -> LaneItem {
    LaneItem::Batch(Batch {
        key: Some(stream::ItemKey::new(position)),
        position: Some(Position::new(position)),
        envelopes: vec![Envelope::upsert(
            of.clone(),
            id.to_string(),
            GenericValue::Map(BTreeMap::new()),
            Some(Position::new(position)),
            chrono::Utc::now(),
        )]
        .into(),
        ..Default::default()
    })
}

/// A source whose answer for what landed is the `seq` it covers, as text,
/// once it is live. A lane that routed nothing stands where the stream
/// started (`start`, `Landed::Idle`'s contract), `nothing` before any.
#[derive(Debug, Default)]
struct Seqs {
    down: std::sync::atomic::AtomicBool,
    start: std::sync::atomic::AtomicU64,
}

impl crate::SourcePositions for Seqs {
    fn place(&self, landed: Landed) -> Option<String> {
        if self.down.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let start = self.start.load(std::sync::atomic::Ordering::SeqCst);
        Some(landed.covered_seq().map_or_else(
            || match start {
                0 => "nothing".to_owned(),
                start => start.to_string(),
            },
            |seq| seq.to_string(),
        ))
    }

    fn parses(&self, text: &str) -> bool {
        text == "nothing" || text.parse::<u64>().is_ok()
    }

    fn compare(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        let seq = |text: &str| match text {
            "nothing" => Some(0),
            text => text.parse::<u64>().ok(),
        };
        Some(seq(a)?.cmp(&seq(b)?))
    }
}

/// Two lanes, `orders` and `users`, of one sink `primary`.
struct Rig {
    stream: Arc<ChannelStream>,
    progress: Arc<Progress>,
    sink: Arc<Gate>,
    heard: Arc<Heard>,
    positions: Arc<Seqs>,
    rereads: Arc<crate::Rereads>,
}

impl Rig {
    fn new(capacity: usize) -> Self {
        Self::over(capacity, &["primary"])
    }

    /// Both indexes on each of `sinks`; `sink` is `primary`'s.
    fn over(capacity: usize, sinks: &[&str]) -> Self {
        let indexes = [orders(), users()];
        let sinks: Vec<_> = sinks.iter().map(|name| sink_name(name)).collect();
        let progress = Arc::new(Progress::of(indexes.clone(), sinks.clone()));
        let heard = Arc::new(Heard::default());
        let _ = heard.progress.set(Arc::clone(&progress));
        let rig = Self {
            stream: Arc::new(ChannelStream::over(
                capacity,
                indexes.clone(),
                sinks.clone(),
            )),
            progress,
            sink: Arc::new(Gate::default()),
            heard,
            positions: Arc::new(Seqs::default()),
            rereads: Arc::new(crate::Rereads::default()),
        };
        let _ = rig.sink.rereads.set(Arc::clone(&rig.rereads));
        rig
    }

    /// A live item of `of` at `position`, registered as the ingest engine
    /// would.
    async fn live(&self, of: &IndexName, id: i64, position: u64) {
        self.progress.published(
            &lane(of.clone()),
            stream::ItemKey::new(position),
            Position::new(position),
            Position::new(position),
        );
        self.stream
            .lane(&lane(of.clone()))
            .unwrap()
            .producer
            .publish(item(of, id, position))
            .await
            .unwrap();
    }

    fn engine(&self) -> Arc<SinkEngine> {
        self.engine_of("primary", Arc::clone(&self.sink))
    }

    /// The engine of sink `name`, writing to `sink`.
    fn engine_of(&self, name: &str, sink: Arc<Gate>) -> Arc<SinkEngine> {
        Arc::new(self.built(name, sink))
    }

    /// `primary`'s engine with `backfill = false`: the sink a reattach is for.
    fn live_only_engine(&self) -> Arc<SinkEngine> {
        Arc::new(
            self.built("primary", Arc::clone(&self.sink))
                .with_options(sink::SinkOptions { backfill: false }),
        )
    }

    fn built(&self, name: &str, sink: Arc<Gate>) -> SinkEngine {
        let engine = SinkEngine::new(
            sink_name(name),
            sink as Arc<dyn Sink>,
            Arc::clone(&self.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&self.progress),
        )
        .with_writes(4)
        .with_observer(Arc::clone(&self.heard) as Arc<dyn Observer>)
        .with_positions(Arc::clone(&self.positions) as Arc<dyn crate::SourcePositions>)
        .with_rereads(Arc::clone(&self.rereads));
        engine.set_blame(Blame::Nobody);
        engine
    }

    fn applied(&self, of: &IndexName) -> Option<Landed> {
        self.progress.lane(&lane(of.clone()), Level::Applied)
    }
}

/// A run of `engine`, and the sender of its controls.
fn spawn(
    engine: &Arc<SinkEngine>,
) -> (
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
) {
    let (send, mut control) = mpsc::channel::<SinkControl>(4);
    let engine = Arc::clone(engine);
    let task = tokio::spawn(async move { engine.run(&mut control).await });
    (task, send)
}

/// Wait, on paused time, until `condition` holds; a minute of it at most.
async fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !condition() {
        assert!(Instant::now() < deadline, "never: {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

fn through(position: u64) -> Option<Landed> {
    Some(Landed::Through(Position::new(position)))
}

/// `users` blocked and asked to detach, with an item parked on: the first
/// write parks it, and its first refused probe detaches it.
async fn detached_on_a_block(
    rig: &Rig,
) -> (Arc<SinkEngine>, tokio::task::JoinHandle<crate::Result<()>>) {
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    (engine, task)
}

/// Asked to detach, a parked lane detaches at its next refused probe: the
/// sink records it first, why and the item's keys, then the lane is detached,
/// its fence frozen (the observers hear it) and out of the slot's minimum;
/// the sibling lane keeps landing, and nothing of the detached index is
/// written again (R3–R5, R12).
#[tokio::test(start_paused = true)]
async fn a_refused_probe_detaches_a_lane_asked_to_detach() {
    let rig = Rig::new(64);
    let start = Instant::now();
    let (engine, task) = detached_on_a_block(&rig).await;

    let detaches = rig.sink.detaches();
    assert_eq!(detaches.len(), 1);
    let (index, detach) = &detaches[0];
    assert_eq!(index, "users");
    assert!(
        detach.why.starts_with("blocked: index [users] blocked by"),
        "{detach:?}"
    );
    assert!(!detach.live_only);
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "the first write and one refused probe"
    );
    assert!(
        rig.sink
            .writes_of(&users())
            .iter()
            .all(|at| *at < start + Duration::from_secs(2)),
        "the probe at 1 s, then the detach"
    );
    assert_eq!(
        rig.heard.kinds(&lane(users())),
        ["blocked", "blocked", "detached"],
        "parked, its probe refused, detached"
    );
    assert!(rig.progress.is_detached(&lane(users())));

    rig.live(&orders(), 1, 2).await;
    rig.live(&users(), 2, 3).await;
    eventually("orders lands beside the detached lane", || {
        rig.applied(&orders()) == through(2)
    })
    .await;
    assert_eq!(
        rig.progress.landed(),
        Landed::Through(Position::new(2)),
        "only orders answers: the detached lane holds nothing back"
    );
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "nothing of the detached index reaches write"
    );
    assert!(engine.lane_health(&lane(orders())).is_open());
    task.abort();
}

/// A detached lane is always taken from and each item acknowledged
/// unwritten, so a send blocked on a full lane completes (R6), at once: no
/// item waits for the run's next wake-up (review B1: one item per idle
/// tick).
#[tokio::test(start_paused = true)]
async fn a_detached_lane_is_drained() {
    let rig = Rig::new(2);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    // The producer is ahead of the parked lane, blocked on it, when it
    // detaches.
    let producer = rig.stream.lane(&lane(users())).unwrap().producer;
    let sending = tokio::spawn(async move {
        for position in 1..1001 {
            producer
                .publish(item(&users(), i64::try_from(position).unwrap(), position))
                .await
                .unwrap();
        }
    });
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let sent = tokio::time::timeout(Duration::from_millis(100), sending).await;
    assert!(
        sent.is_ok(),
        "a thousand items through a lane of two within 100 ms: each was taken at once"
    );
    assert_eq!(rig.sink.writes_of(&users()).len(), 2);
    task.abort();
}

/// The index's builder publishes each item to every lane of the index, the
/// detached one first: a healthy second sink of the same index is not slowed
/// by the detached lane (review B1).
#[tokio::test(start_paused = true)]
async fn a_detached_lane_slows_no_healthy_sink_of_its_index() {
    let rig = Rig::over(2, &["primary", "secondary"]);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    let healthy = rig.engine_of("secondary", Arc::new(Gate::default()));
    let (healthy_task, _control) = spawn(&healthy);
    let secondary = LaneKey::new(users(), sink_name("secondary"));
    let detached = rig.stream.lane(&lane(users())).unwrap().producer;
    let other = rig.stream.lane(&secondary).unwrap().producer;
    let progress = Arc::clone(&rig.progress);
    let registered = secondary.clone();
    let builder = tokio::spawn(async move {
        for position in 1..1001 {
            let id = i64::try_from(position).unwrap();
            detached
                .publish(item(&users(), id, position))
                .await
                .unwrap();
            progress.published(
                &registered,
                stream::ItemKey::new(position),
                Position::new(position),
                Position::new(position),
            );
            other.publish(item(&users(), id, position)).await.unwrap();
        }
    });
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let start = Instant::now();
    eventually("the healthy sink lands the last item", || {
        rig.progress.lane(&secondary, Level::Applied) == through(1000)
    })
    .await;
    assert!(
        start.elapsed() < Duration::from_millis(100),
        "a thousand items in {:?}",
        start.elapsed()
    );
    builder.await.unwrap();
    task.abort();
    healthy_task.abort();
}

/// A restaging lane has no probe: asked to detach, it detaches when its
/// staging comes due, and the staging never runs.
#[tokio::test(start_paused = true)]
async fn a_restaging_lane_asked_to_detach_detaches_instead_of_staging() {
    let rig = Rig::new(64);
    rig.sink.missing.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    let detaches = rig.sink.detaches();
    assert_eq!(detaches.len(), 1);
    assert!(
        detaches[0].1.why.starts_with("missing: "),
        "{:?}",
        detaches[0]
    );
    assert!(
        rig.sink.ensured.lock().unwrap().is_empty(),
        "the staging the detach replaced never ran"
    );
    assert_eq!(rig.heard.kinds(&lane(users())), ["restaging", "detached"]);
    task.abort();
}

/// A probe that lands drops the ask: the lane opens, and a later park and
/// refused probe leave it parked, with no detach.
#[tokio::test(start_paused = true)]
async fn a_landed_probe_drops_the_ask() {
    let rig = Rig::new(64);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(engine.lane_state(&lane(users())).detach_asked);
    rig.sink.block(&users(), false);
    eventually("the probe lands", || rig.applied(&users()) == through(1)).await;
    assert!(!engine.lane_state(&lane(users())).detach_asked, "dropped");

    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(rig.sink.detaches().is_empty());
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    task.abort();
}

/// A sink that keeps no records never detaches: the ask is dropped and the
/// lane stays parked, holding the slot (R4, O7).
#[tokio::test(start_paused = true)]
async fn a_sink_without_records_never_detaches() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::NotKept);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the sink was asked", || rig.sink.detaches().len() == 1).await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(rig.sink.detaches().len(), 1, "asked once only");
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(
        rig.applied(&users()),
        Some(Landed::Before(Position::new(1))),
        "still in the minimum"
    );
    assert!(!engine.lane_state(&lane(users())).detach_asked);

    // The parked item was neither acknowledged nor dropped while the sink
    // was asked: lifted, the block lets that very item land.
    let before = rig.sink.writes_of(&users()).len();
    rig.sink.block(&users(), false);
    eventually("the parked item lands", || {
        rig.applied(&users()) == through(1)
    })
    .await;
    assert!(
        rig.sink.writes_of(&users()).len() > before,
        "written, not dropped"
    );
    task.abort();
}

/// A record that fails leaves the lane parked, in the minimum, with its ask:
/// the next refused probe records again, and then it detaches.
#[tokio::test(start_paused = true)]
async fn a_failed_record_leaves_the_lane_parked_until_its_next_refused_probe() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::Fail);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the first record failed", || rig.sink.detaches().len() == 1).await;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked { .. }
    ));
    assert!(!rig.progress.is_detached(&lane(users())));
    eventually("the next refused probe detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert_eq!(rig.sink.detaches().len(), 2);
    assert!(rig.progress.is_detached(&lane(users())));
    task.abort();
}

/// A run that dies while the sink records the detach leaves the lane in the
/// minimum, parked, with its ask: the next run detaches it at its next
/// refused probe (the kill walk's d1, d2 inside a process).
#[tokio::test(start_paused = true)]
async fn a_run_dying_while_the_detach_is_recorded_asks_again() {
    let rig = Rig::new(64);
    *rig.sink.detach_takes.lock().unwrap() = Some(Duration::from_secs(30));
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record is under way", || rig.sink.detaches().len() == 1).await;
    task.abort();
    let _ = task.await;
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(
        rig.applied(&users()),
        Some(Landed::Before(Position::new(1))),
        "still in the minimum"
    );
    assert!(engine.lane_state(&lane(users())).detach_asked);

    *rig.sink.detach_takes.lock().unwrap() = None;
    let (task, _control) = spawn(&engine);
    eventually("the next run detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert_eq!(rig.sink.detaches().len(), 2);
    assert!(rig.progress.is_detached(&lane(users())));
    task.abort();
}

/// A run restarted after the detach keeps the lane detached: what the lane
/// delivers again, and what comes after, is acknowledged unwritten (H3).
#[tokio::test(start_paused = true)]
async fn a_restarted_run_never_writes_what_a_detached_lane_holds() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    rig.sink.block(&users(), false);
    rig.live(&users(), 2, 2).await;
    rig.live(&orders(), 1, 3).await;
    let (task, _control) = spawn(&engine);
    eventually("orders lands", || rig.applied(&orders()) == through(3)).await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        2,
        "nothing more of users"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();
}

fn unreadable() -> DetachRecord {
    DetachRecord::Unreadable {
        reason: "a newer state".to_owned(),
    }
}

fn version() -> Option<RecordVersion> {
    Some(RecordVersion::new("7:1"))
}

/// `record` as another process's write leaves it: its own stamp, a new
/// version.
fn by_another_process(record: &mut Detachment) {
    record.record_stamp = Some(sink::Stamp::new("another process", 1));
    record.version = Some(RecordVersion::new("9:1"));
}

/// What a staging does with each record and marker pair (§4): detached
/// indexes are neither ensured nor backfilled and their lanes leave the
/// minimum; a record a rebuild outlived is deleted; a record whose marker is
/// on serving detaches again; anything this step doesn't act on is staged as
/// before.
#[tokio::test(start_paused = true)]
async fn staging_classifies_every_record_and_marker_pair() {
    let detached = |live_only| DetachRecord::Detached { live_only };
    let rebuilding = DetachRecord::Rebuilding { live_only: false };
    let elsewhere = |seeded| Some(MarkerAt::Elsewhere { seeded });
    let serving = |seeded| Some(MarkerAt::Serving { seeded });
    let newer = |seeded| Some(MarkerAt::Newer { seeded });
    // (what staging reads, ensured, detached after, detach recorded, forgotten)
    let cases: Vec<(&str, Detachment, bool, bool, bool, bool)> = vec![
        (
            "nothing kept",
            Detachment::default(),
            true,
            false,
            false,
            false,
        ),
        (
            "detached, marker unseeded elsewhere",
            Detachment::recorded(detached(false), newer(false), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "detached, a seeded marker on no known serving copy: not finished",
            Detachment::recorded(detached(false), elsewhere(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only, detached while restaging, nothing serving (review S2)",
            Detachment::recorded(detached(true), elsewhere(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only, a rebuild seeded a newer generation since: finished, by the marker \
             alone (the review of the closing round, E)",
            Detachment::recorded(detached(true), newer(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding from a live-only record, finished (E)",
            Detachment::recorded(
                DetachRecord::Rebuilding { live_only: true },
                newer(true),
                version(),
            ),
            true,
            false,
            false,
            true,
        ),
        (
            "detached with no marker",
            Detachment::recorded(detached(false), None, version()),
            false,
            true,
            false,
            false,
        ),
        (
            "live only, marker on serving",
            Detachment::recorded(detached(true), serving(true), version()),
            false,
            true,
            false,
            false,
        ),
        (
            "detached, marker back on serving",
            Detachment::recorded(detached(false), serving(true), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "detached, a rebuild seeded the marker's generation",
            Detachment::recorded(detached(false), newer(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding, finished",
            Detachment::recorded(rebuilding.clone(), newer(true), version()),
            true,
            false,
            false,
            true,
        ),
        (
            "rebuilding: staged as before until step 4",
            Detachment::recorded(rebuilding, elsewhere(false), version()),
            true,
            false,
            false,
            false,
        ),
        (
            "rebuilding, live only, marker on serving: detached again (O30)",
            Detachment::recorded(
                DetachRecord::Rebuilding { live_only: true },
                serving(true),
                version(),
            ),
            false,
            true,
            true,
            false,
        ),
        (
            "rebuilding, marker on serving: detached again (O30)",
            Detachment::recorded(
                DetachRecord::Rebuilding { live_only: false },
                serving(true),
                version(),
            ),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, marker on a newer unseeded generation: a rebuild, as before (O30)",
            Detachment::recorded(unreadable(), newer(false), version()),
            true,
            false,
            false,
            false,
        ),
        (
            "unreadable, marker on serving: detached again (O30)",
            Detachment::recorded(unreadable(), serving(true), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, serving unknown: detached again (O30)",
            Detachment::recorded(unreadable(), elsewhere(false), version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, no marker: detached again (O30)",
            Detachment::recorded(unreadable(), None, version()),
            false,
            true,
            true,
            false,
        ),
        (
            "unreadable, a newer seeded marker: detached again (O30)",
            Detachment::recorded(unreadable(), newer(true), version()),
            false,
            true,
            true,
            false,
        ),
    ];
    for (name, kept, ensured, is_detached, recorded, forgotten) in cases {
        let rig = Rig::new(64);
        rig.sink
            .kept
            .lock()
            .unwrap()
            .insert("users".to_owned(), kept);
        let engine = rig.engine();
        engine.stage(Continuity::Resumed).await.unwrap();
        assert_eq!(
            rig.sink
                .ensured
                .lock()
                .unwrap()
                .contains(&"users".to_owned()),
            ensured,
            "{name}: ensured"
        );
        assert_eq!(
            engine.lane_health(&lane(users())).is_detached(),
            is_detached,
            "{name}: health"
        );
        assert_eq!(
            rig.progress.is_detached(&lane(users())),
            is_detached,
            "{name}: progress"
        );
        assert_eq!(
            !rig.sink.detaches().is_empty(),
            recorded,
            "{name}: recorded again"
        );
        assert_eq!(
            *rig.sink.forgotten.lock().unwrap() == ["users@7:1"],
            forgotten,
            "{name}: forgotten"
        );
        assert!(
            rig.sink
                .ensured
                .lock()
                .unwrap()
                .contains(&"orders".to_owned()),
            "{name}: orders is staged as ever"
        );
        if is_detached {
            assert_eq!(rig.heard.kinds(&lane(users())), ["detached"], "{name}");
        }
    }
}

/// A lane staging brings back detached is detached in the next run too:
/// what its lane delivers is acknowledged unwritten.
#[tokio::test(start_paused = true)]
async fn a_lane_staged_detached_writes_nothing() {
    let rig = Rig::new(64);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Elsewhere { seeded: false }),
            version(),
        ),
    );
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.live(&users(), 1, 1).await;
    rig.live(&orders(), 1, 2).await;
    let (task, _control) = spawn(&engine);
    eventually("orders lands", || rig.applied(&orders()) == through(2)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(rig.sink.writes_of(&users()).is_empty());
    assert_eq!(rig.progress.landed(), Landed::Through(Position::new(2)));
    task.abort();
}

/// The fence hears a detach before the lane leaves the minimum, in a run and
/// at staging alike: heard after, it would freeze at what `Free` reaches,
/// the source's horizon, and vouch for changes the index never got (R5,
/// review S4).
#[tokio::test(start_paused = true)]
async fn the_fence_hears_a_detach_before_the_lane_leaves_the_minimum() {
    let rig = Rig::new(64);
    let (_engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    assert_eq!(*rig.heard.left_first.lock().unwrap(), [false], "in a run");

    let rig = Rig::new(64);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            version(),
        ),
    );
    rig.engine().stage(Continuity::Resumed).await.unwrap();
    assert!(rig.progress.is_detached(&lane(users())));
    assert_eq!(*rig.heard.left_first.lock().unwrap(), [false], "at staging");
}

/// A record that landed although its call failed (a timeout after the
/// write) is deleted before the lane's next probe, so a lane whose probe
/// then lands is not detached by the next staging (§5, review S1).
#[tokio::test(start_paused = true)]
async fn a_record_that_landed_although_its_call_failed_goes_before_the_lane_opens() {
    let rig = Rig::new(64);
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record was written and the call failed", || {
        rig.sink.detaches().len() == 1
    })
    .await;
    rig.sink.block(&users(), false);
    eventually("the probe lands", || rig.applied(&users()) == through(1)).await;
    assert_eq!(rig.sink.forgotten.lock().unwrap().len(), 1);
    assert!(rig.sink.kept.lock().unwrap().is_empty());
    assert!(engine.lane_health(&lane(users())).is_open());
    task.abort();
    let _ = task.await;

    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(
        engine.lane_health(&lane(users())).is_open(),
        "the next staging finds no record"
    );
    assert!(!rig.progress.is_detached(&lane(users())));
}

/// A record that says detached beside a marker on serving is detached again
/// at staging; a sink that answers it keeps no record fails the staging
/// step, never staging the index attached over the gap (review m2).
#[tokio::test(start_paused = true)]
async fn a_detach_again_the_sink_keeps_no_record_of_fails_the_staging_step() {
    let rig = Rig::new(64);
    rig.sink.records.lock().unwrap().push_back(Record::NotKept);
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Serving { seeded: true }),
            version(),
        ),
    );
    let engine = rig.engine();
    let error = engine.stage(Continuity::Fresh).await.unwrap_err();
    assert!(error.to_string().contains("keeps no record"), "{error}");
    assert!(
        !rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    assert!(!rig.progress.is_detached(&lane(users())));
}

/// The detach records an earlier process left, for `users`.
fn detached_record() -> Detachment {
    Detachment::recorded(
        DetachRecord::Detached { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    )
}

/// Items of `users` published past a lane of two, within a second: what
/// only a run that takes the lane as detached lets through.
async fn drains_within_a_second(rig: &Rig) -> bool {
    let producer = rig.stream.lane(&lane(users())).unwrap().producer;
    tokio::time::timeout(Duration::from_secs(1), async {
        for position in 100..140 {
            producer
                .publish(item(&users(), i64::try_from(position).unwrap(), position))
                .await
                .unwrap();
        }
    })
    .await
    .is_ok()
}

/// A lane restaged during a run whose staging finds the index detached is
/// taken as detached by the run: drained, never reopened (`adopt_detached`).
#[tokio::test(start_paused = true)]
async fn a_restage_during_a_run_that_finds_the_index_detached_detaches_the_lane() {
    let rig = Rig::new(2);
    rig.sink.missing.lock().unwrap().insert("users".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    let engine = rig.engine();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("the restage finds it detached", || {
        rig.progress.is_detached(&lane(users()))
    })
    .await;
    assert!(
        drains_within_a_second(&rig).await,
        "the run drains the lane"
    );
    assert_eq!(
        rig.sink.writes_of(&users()).len(),
        1,
        "only the first write"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();
}

/// A reindex whose staging finds the index detached detaches the lane in
/// the run: nothing ensured, nothing written (`adopt_detached`).
#[tokio::test(start_paused = true)]
async fn a_reindex_whose_staging_finds_the_index_detached_detaches_the_lane() {
    let rig = Rig::new(2);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users lands", || rig.applied(&users()) == through(1)).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the reindex finds it detached", || {
        rig.progress.is_detached(&lane(users()))
    })
    .await;
    assert!(
        drains_within_a_second(&rig).await,
        "the run drains the lane"
    );
    assert_eq!(rig.sink.writes_of(&users()).len(), 1);
    assert!(rig.sink.ensured.lock().unwrap().is_empty());
    task.abort();
}

/// A lane the process holds detached is asked no backfill by a later
/// staging that finds no record (removed by hand): `Progress` decides inside
/// a process, and the fence keeps answering `detached` (review m5).
#[tokio::test(start_paused = true)]
async fn a_lane_held_detached_is_asked_no_backfill_by_a_later_staging() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    engine.stage(Continuity::Resumed).await.unwrap();
    let mut requests = rig.stream.requests().unwrap().consumer;
    let asked = tokio::time::timeout(Duration::from_millis(1), requests.recv()).await;
    assert!(asked.is_err(), "no backfill asked: {asked:?}");
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(rig.progress.is_detached(&lane(users())));
}

/// The attach request the sink engine sent for `users`: its attempt.
async fn attach_asked(rig: &Rig) -> u64 {
    let mut requests = rig.stream.requests().unwrap().consumer;
    let delivery = tokio::time::timeout(Duration::from_secs(5), requests.recv())
        .await
        .expect("an attach is asked for")
        .unwrap()
        .unwrap();
    let (request, handle) = delivery.into_parts();
    handle.ack().await.unwrap();
    let stream::Request::Attach { index, attempt, .. } = request else {
        panic!("an attach request, got {request:?}");
    };
    assert_eq!(index, users());
    attempt
}

/// `users` detached on a block, the block lifted, and an attach asked for:
/// the engine and the attempt it waits for.
async fn attaching(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
    u64,
) {
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Attach { index: users() })
        .await
        .unwrap();
    let attempt = attach_asked(rig).await;
    (engine, task, control, attempt)
}

async fn publish(rig: &Rig, item: LaneItem) {
    rig.stream
        .lane(&lane(users()))
        .unwrap()
        .producer
        .publish(item)
        .await
        .unwrap();
}

fn written(rig: &Rig) -> Vec<String> {
    rig.sink
        .ids
        .lock()
        .unwrap()
        .iter()
        .filter(|id| id.starts_with("users/"))
        .cloned()
        .collect()
}

/// R6, R8: what reaches the lane before its `Attached` (built while it was
/// detached) is acknowledged unwritten; the `Attached` of the attempt asked
/// for opens the lane, and what follows is written.
#[tokio::test(start_paused = true)]
async fn items_before_attached_are_discarded_and_after_it_written() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    let before = written(&rig);
    publish(&rig, item(&users(), 10, 10)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("the item after the attach is written", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    assert!(engine.lane_health(&lane(users())).is_open());
    let after: Vec<String> = written(&rig).into_iter().skip(before.len()).collect();
    assert_eq!(
        after,
        ["users/11"],
        "the item before the attach is not written"
    );
    task.abort();
}

/// An `Attached` of another attempt than the one asked for (an older one)
/// is discarded: the lane stays detached until its own.
#[tokio::test(start_paused = true)]
async fn an_older_attempts_attached_is_discarded() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    publish(
        &rig,
        LaneItem::Attached {
            attempt: attempt - 1,
        },
    )
    .await;
    publish(&rig, item(&users(), 10, 10)).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(!written(&rig).contains(&"users/10".to_owned()));
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("its own attach opens the lane", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    task.abort();
}

/// A second `Attached` of the same attempt (published again by a run that
/// died between the attach and the first) discards nothing (H2).
#[tokio::test(start_paused = true)]
async fn a_second_attached_of_the_same_attempt_discards_nothing() {
    let rig = Rig::new(64);
    let (_engine, task, _control, attempt) = attaching(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 12, 12)).await;
    eventually("both items are written", || {
        let written = written(&rig);
        written.contains(&"users/11".to_owned()) && written.contains(&"users/12".to_owned())
    })
    .await;
    task.abort();
}

/// A sink run restarted after the `Attached` took effect never writes an
/// item delivered before it: those were acknowledged in line before the
/// lane opened, so nothing earlier comes back; what was after it and not
/// acknowledged is delivered again and written (H3).
#[tokio::test(start_paused = true)]
async fn a_run_restart_after_attached_never_writes_a_redelivered_earlier_item() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = attaching(&rig).await;
    *rig.sink.slow.lock().unwrap() = Some(("11".to_owned(), Duration::from_secs(30)));
    publish(&rig, item(&users(), 10, 10)).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    publish(&rig, item(&users(), 11, 11)).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    task.abort();
    let _ = task.await;
    *rig.sink.slow.lock().unwrap() = None;
    assert!(
        !written(&rig).contains(&"users/11".to_owned()),
        "its write was cut by the restart"
    );
    let (task, _control) = spawn(&engine);
    eventually(
        "the item after the attach is written, delivered again",
        || written(&rig).contains(&"users/11".to_owned()),
    )
    .await;
    assert!(
        !written(&rig).contains(&"users/10".to_owned()),
        "{:?}",
        written(&rig)
    );
    assert!(engine.lane_health(&lane(users())).is_open());
    task.abort();
}

/// The next request the sink engine sent.
async fn next_request(rig: &Rig) -> stream::Request {
    let mut requests = rig.stream.requests().unwrap().consumer;
    let delivery = tokio::time::timeout(Duration::from_secs(10), requests.recv())
        .await
        .expect("a request is sent")
        .unwrap()
        .unwrap();
    let (request, handle) = delivery.into_parts();
    handle.ack().await.unwrap();
    request
}

/// `users` detached on a block, the block lifted, and a reindex asked:
/// the rebuild begins and the engine asks for the attach; its attempt.
async fn rebuilding_on_reindex(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
    u64,
) {
    let (engine, task) = detached_on_a_block(rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(rig).await else {
        panic!("the rebuild asks for its attach");
    };
    (engine, task, control, attempt)
}

/// R13 and r1–r4: a reindex of a detached index rebuilds it. The sink begins
/// the rebuild (the record says `rebuilding` before the attach is asked:
/// S2), the generation is ensured, the attach is asked; once `Attached` is
/// taken the lane is open, rebuilding, and asks its backfill; the backfill's
/// seed finishes it: the record is gone and the lane is current.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_detached_index_rebuilds_it_to_the_seed() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    assert_eq!(*rig.sink.rebuilds.lock().unwrap(), ["users"]);
    assert!(
        matches!(
            rig.sink
                .kept
                .lock()
                .unwrap()
                .get("users")
                .and_then(|kept| kept.record.clone()),
            Some(DetachRecord::Rebuilding { .. })
        ),
        "rebuilding is recorded before the attach is asked"
    );
    assert!(
        rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "until Attached"
    );

    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { index, ticket, .. } = next_request(&rig).await else {
        panic!("the rebuild asks its backfill once attached");
    };
    assert_eq!(index, users());
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));

    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the rebuild is seeded", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    eventually("the lane is current", || {
        engine.lane_state(&lane(users())).rebuilding.is_none()
    })
    .await;
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "record gone"
    );
    task.abort();
}

/// Review of step 3, S2: a lane attached by its rebuild stays attached when
/// the daemon stages the sink again before restarting its run: the record
/// says `rebuilding`, read through the real staging path, never `detached`.
#[tokio::test(start_paused = true)]
async fn an_attached_lane_stays_attached_across_a_sink_run_restart() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    // What the index's builder does before it publishes `Attached`.
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    task.abort();
    let _ = task.await;
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert!(!rig.progress.is_detached(&lane(users())));
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 11, 11).await;
    eventually("the lane writes after the restart", || {
        written(&rig).contains(&"users/11".to_owned())
    })
    .await;
    task.abort();
}

/// Review of step 3, m5: an attach with no backfill (the test path, as
/// `reattach`) clears the cut backfill entry the detach left, so a later
/// reindex of the index is staged, not taken for one still outstanding.
#[tokio::test(start_paused = true)]
async fn a_reindex_after_an_attach_is_staged() {
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let _backfill = next_request(&rig).await;
    rig.sink.block(&users(), true);
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Attach { index: users() })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("an attach");
    };
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    // The record goes with the attach, as `reattach` deletes it before.
    rig.sink.kept.lock().unwrap().remove("users");
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the reindex is staged", || {
        rig.sink.ensured.lock().unwrap().len() > ensured
    })
    .await;
    task.abort();
}

/// O20(c): a reindex of a rebuilding index waiting for its attach, or open,
/// is ignored; one of a rebuild that parked starts it again: detached
/// again, then a second rebuild begins.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_rebuild_is_ignored_unless_it_parked() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "waiting for Attached: ignored"
    );

    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "open and rebuilding: ignored"
    );

    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    eventually("the rebuilding lane parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the rebuild starts again", || {
        rig.sink.rebuilds.lock().unwrap().len() == 2
    })
    .await;
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "detached again, attaching"
    );
    let stream::Request::Attach { attempt: again, .. } = next_request(&rig).await else {
        panic!("a new attach");
    };
    assert!(again > attempt);
    task.abort();
}

/// O20's restart of a parked rebuild detaches it first. That detach landing
/// but failing (found by the world at 50,000 cases, after the review of the
/// cut) left a record that says detached while the lane went on writing onto
/// it. Read back while the slot is held, the detach is seen to have landed:
/// taken as done, the restart goes on, and the rebuild begins again.
#[tokio::test(start_paused = true)]
async fn a_rebuild_restart_whose_detach_failed_writes_nothing_onto_its_record() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    eventually("the rebuilding lane parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::LandsAsAskedThenFails);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.sink.block(&users(), false);
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        2,
        "the restart went on: a second rebuild"
    );
    let record = rig
        .sink
        .kept
        .lock()
        .unwrap()
        .get("users")
        .and_then(|kept| kept.record.clone());
    assert!(
        matches!(record, Some(DetachRecord::Rebuilding { .. })),
        "the record says rebuilding, not detached: {record:?}"
    );
    assert!(!task.is_finished(), "{:?}", task.await);
    task.abort();
}

/// The review of round 10, S-4: the same restart whose detach landed and
/// failed, with the read back failing too, so its outcome stays unknown.
/// Nothing of the lane is written onto what the detach may have left until
/// a read settles it (`731c3791`'s `detach_unsure`); here no read does.
#[tokio::test(start_paused = true)]
async fn a_rebuild_restart_whose_detach_is_unknown_writes_nothing_until_a_read() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    eventually("the rebuilding lane parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the restart's detach was sent", || {
        !rig.sink.detaches().is_empty()
    })
    .await;
    let detached_at = Instant::now();
    *rig.sink.reads_fail.lock().unwrap() = 1_000_000;
    rig.sink.block(&users(), false);
    tokio::time::sleep(Duration::from_secs(30)).await;
    let late: Vec<Instant> = rig
        .sink
        .writes_of(&users())
        .into_iter()
        .filter(|at| *at >= detached_at)
        .collect();
    assert!(
        late.is_empty(),
        "written onto a detach of unknown outcome: {late:?}"
    );
    assert!(!task.is_finished(), "{:?}", task.await);
    task.abort();
}

/// The process-start attach of a `rebuilding` record: the lane starts
/// attached and rebuilding, its index ensured and its backfill asked; under
/// `--skip-backfill` it is demoted (O26), and on a sink with `backfill =
/// false` demoted live only (R14): detached.
#[tokio::test(start_paused = true)]
async fn a_start_attaches_a_rebuild_or_demotes_it() {
    let rebuilding = || {
        let mut record = Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            version(),
        );
        record.attempts = Some(3);
        record
    };
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), rebuilding());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(3));
    assert!(
        rig.sink
            .ensured
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    );
    let stream::Request::Backfill { index, .. } = next_request(&rig).await else {
        panic!("its backfill");
    };
    assert_eq!(index, users());

    for (skip_backfill, backfill) in [(true, true), (false, false)] {
        let rig = Rig::new(64);
        rig.sink
            .kept
            .lock()
            .unwrap()
            .insert("users".to_owned(), rebuilding());
        let engine = SinkEngine::new(
            sink_name("primary"),
            Arc::clone(&rig.sink) as Arc<dyn Sink>,
            Arc::clone(&rig.stream) as Arc<dyn Stream>,
            vec![mapping(orders()), mapping(users())],
            Arc::clone(&rig.progress),
        )
        .skip_backfill(skip_backfill)
        .with_options(sink::SinkOptions { backfill });
        engine.stage(Continuity::Resumed).await.unwrap();
        assert!(
            engine.lane_health(&lane(users())).is_detached(),
            "{skip_backfill}"
        );
        assert!(rig.progress.is_detached(&lane(users())));
        assert_eq!(
            *rig.sink.demoted.lock().unwrap(),
            [("users".to_owned(), !backfill)]
        );
    }
}

/// A reattach of `index` whose verdict nobody hears.
fn reattach(index: IndexName) -> SinkControl {
    SinkControl::Reattach {
        index,
        reply: crate::Reply::ignored(),
    }
}

fn gap(from: Option<&str>, to: Option<&str>) -> sink::Gap {
    sink::Gap {
        from: from.map(str::to_owned),
        to: to.map(str::to_owned),
    }
}

/// `primary` with `backfill = false`, staged and running: `users/1` lands,
/// then `users` is blocked and asked to detach, and detaches on `users/2`;
/// the block is lifted.
async fn detached_live_only(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    mpsc::Sender<SinkControl>,
) {
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    eventually("users/1 lands", || rig.applied(&users()) == through(1)).await;
    rig.sink.block(&users(), true);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 2, 2).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    (engine, task, control)
}

/// O16, §6a, S2 and S3: `reattach --accept-gap` on a live-only lane. The
/// detach recorded where reads stood (`from`); the reattach ensures the
/// index, keeps the gap with `to` unknown, deletes the record (the commit
/// point), and asks for the attach. At `Attached` the fence hears the whole
/// gap before the lane opens, the sink keeps it, and the lane is written as
/// usual.
#[tokio::test(start_paused = true)]
async fn a_reattach_keeps_the_gap_deletes_the_record_and_attaches_the_lane() {
    let rig = Rig::new(64);
    let (engine, task, control) = detached_live_only(&rig).await;
    let detaches = rig.sink.detaches();
    let (_, detach) = detaches.last().unwrap();
    assert!(detach.live_only);
    assert_eq!(detach.through.as_deref(), Some("1"), "users/2 never landed");
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Detached { through: Some(ref at), .. } if at == "1"
    ));

    let ensured = rig.sink.ensured.lock().unwrap().len();
    control.send(reattach(users())).await.unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("the reattach asks for its attach");
    };
    assert!(rig.sink.ensured.lock().unwrap().len() > ensured);
    assert_eq!(
        *rig.sink.gaps.lock().unwrap(),
        [("users".to_owned(), vec![gap(Some("1"), None)])],
        "kept before the record goes"
    );
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "the record is gone: no staging detaches the lane again"
    );
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "until Attached"
    );
    assert!(rig.heard.gaps.lock().unwrap().is_empty());

    // What the index's builder does: the lane answers through the newest
    // change routed while it was detached, then `Attached` is published.
    rig.progress.routed(Position::new(3));
    assert_eq!(
        rig.progress.attach(&lane(users()), attempt),
        crate::Attach::Attached
    );
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    let heard = rig.heard.gaps.lock().unwrap().clone();
    assert_eq!(heard.len(), 1);
    let (at, heard_gap, healths) = heard.first().cloned().unwrap();
    assert_eq!(
        (at, heard_gap),
        (lane(users()), vec![gap(Some("1"), Some("3"))])
    );
    assert_eq!(
        rig.heard.kinds(&lane(users())).last(),
        Some(&"open"),
        "the lane opened"
    );
    assert!(
        rig.heard
            .health
            .lock()
            .unwrap()
            .get(healths)
            .is_some_and(|(_, health)| health.is_open()),
        "the gap is heard before the lane opens"
    );
    eventually("the whole gap is kept", || {
        rig.sink.gaps.lock().unwrap().last()
            == Some(&("users".to_owned(), vec![gap(Some("1"), Some("3"))]))
    })
    .await;
    rig.live(&users(), 4, 4).await;
    eventually("written as usual", || {
        written(&rig).contains(&"users/4".to_owned())
    })
    .await;
    task.abort();
}

/// O16, §6a: a reattach is refused, and changes nothing, on a sink that can
/// rebuild (a reindex does), on a lane that is not detached, and when the
/// records don't say detached live only with the marker on the seeded
/// generation reads see.
#[tokio::test(start_paused = true)]
async fn a_reattach_is_refused_where_it_does_not_apply() {
    let quiet = |rig: &Rig| {
        rig.sink.gaps.lock().unwrap().is_empty() && rig.sink.forgotten.lock().unwrap().is_empty()
    };

    // A sink that can rebuild.
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "a sink that can rebuild");
    assert!(engine.lane_health(&lane(users())).is_detached());
    task.abort();

    // A lane not detached.
    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "a lane not detached");
    task.abort();

    // Records that don't allow it: the marker moved off the generation
    // reads see.
    let rig = Rig::new(64);
    let (engine, task, control) = detached_live_only(&rig).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Newer { seeded: false });
    control.send(reattach(users())).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(quiet(&rig), "records that don't allow it");
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(engine.lane_state(&lane(users())).attaching.is_none());
    task.abort();
}

/// S3's crash window: a reattach that died between deleting its record and
/// the attach left a gap whose end is unknown. A start before the slot is
/// held stages the lane attached and hears the open gap; once the slot is
/// held the read again completes it where the lane stands, before anything
/// of it is written, and keeps it.
#[tokio::test(start_paused = true)]
async fn a_reattach_cut_short_before_its_attach_completes_its_gap_once_the_slot_is_held() {
    let rig = Rig::new(64);
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), None)]);
    rig.positions
        .down
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .iter()
            .map(|(at, gap, _)| (at.clone(), gap.clone()))
            .collect::<Vec<_>>(),
        [(lane(users()), vec![gap(Some("1"), None)])]
    );
    assert!(rig.sink.gaps.lock().unwrap().is_empty());

    rig.positions
        .down
        .store(false, std::sync::atomic::Ordering::SeqCst);
    // The stream starts at or past the gap's start (`Landed::Idle`).
    rig.positions
        .start
        .store(1, std::sync::atomic::Ordering::SeqCst);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    assert!(!read_all(&rig));
    let (task, _control) = spawn(&engine);
    eventually("the gap is completed and kept", || {
        rig.sink.gaps.lock().unwrap().as_slice()
            == [("users".to_owned(), vec![gap(Some("1"), Some("1"))])]
    })
    .await;
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .last()
            .map(|(_, gap, _)| gap.clone()),
        Some(vec![gap(Some("1"), Some("1"))])
    );
    eventually("the records count as read", || read_all(&rig)).await;
    task.abort();
}

/// The cut of 2026-11-27: a lane another process detached after this one
/// read its records is not taken as detached: the read once the slot is
/// held finds the change and the deployment stops, nothing of the lane
/// written and its read uncounted, so confirmation never passes it.
#[tokio::test(start_paused = true)]
async fn a_detach_another_process_recorded_since_this_one_read_stops_the_deployment() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    stops(task).await;
    assert!(written(&rig).is_empty(), "nothing of it is written");
    assert!(!rig.rereads.current(&lane(users())), "its read uncounted");
    assert_eq!(
        rig.sink.kept.lock().unwrap().get("users"),
        Some(&detached_record())
    );
}

/// O29: a staging after the slot opened reads every record: it is the read
/// again, and the run reads nothing more.
#[tokio::test(start_paused = true)]
async fn a_staging_once_the_slot_is_held_counts_as_the_read() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let engine = rig.engine();
    assert!(!read_all(&rig));
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(read_all(&rig));
}

/// Review of step 4, the gap list: a lane reattached, detached again and
/// reattached again keeps both gaps exactly, the earlier as it was and the
/// new one added; positions between them are vouched as usual.
#[tokio::test(start_paused = true)]
async fn a_second_reattach_keeps_both_gaps_exactly() {
    let rig = Rig::new(64);
    let earlier = gap(Some("0"), Some("0"));
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![earlier.clone()]);
    let (_engine, task, control) = detached_live_only(&rig).await;
    control.send(reattach(users())).await.unwrap();
    let _attach = next_request(&rig).await;
    assert_eq!(
        rig.sink.gaps.lock().unwrap().last(),
        Some(&("users".to_owned(), vec![earlier, gap(Some("1"), None)]))
    );
    task.abort();
}

/// Review of step 4, m4: once the slot is held, a gap end the source can't
/// read is settled and kept: an unreadable `to` becomes where the lane stands
/// (everything after it reaches the lane), an unreadable `from` unknown (the
/// gap reaches back from its end, all in the past). Nothing refuses forever.
#[tokio::test(start_paused = true)]
async fn a_gap_end_the_source_cannot_read_is_settled_at_a_start() {
    let rig = Rig::new(64);
    rig.sink.marker_gaps.lock().unwrap().insert(
        "users".to_owned(),
        vec![
            gap(Some("garbled"), Some("2")),
            gap(Some("3"), Some("garbled")),
        ],
    );
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    // The stream starts at or past what the gaps hold (`Landed::Idle`).
    rig.positions
        .start
        .store(3, std::sync::atomic::Ordering::SeqCst);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    let settled = vec![gap(None, Some("2")), gap(Some("3"), Some("3"))];
    eventually("settled once the slot is held", || {
        *rig.sink.gaps.lock().unwrap() == [("users".to_owned(), settled.clone())]
    })
    .await;
    task.abort();
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .last()
            .map(|(_, gaps, _)| gaps.clone()),
        Some(settled)
    );
}

/// Review of step 4, S1: a rebuild an operator forced under
/// `--skip-backfill` survives its sink's re-staging inside the process (a
/// run restart after a transient failure): O26 demotes only at a process's
/// start, where no backfill would finish it.
#[tokio::test(start_paused = true)]
async fn a_rebuild_forced_under_skip_backfill_survives_a_sink_run_restart() {
    let rig = Rig::new(64);
    rig.sink.block(&users(), true);
    let engine = Arc::new(
        rig.built("primary", Arc::clone(&rig.sink))
            .skip_backfill(true),
    );
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.sink.block(&users(), false);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("the forced rebuild asks for its attach");
    };
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    task.abort();
    let _ = task.await;
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(
        rig.sink.demoted.lock().unwrap().is_empty(),
        "no demotion inside the process"
    );
    assert!(engine.lane_health(&lane(users())).is_open());
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
}

/// Review of step 4, S2: a rebuild whose seed meets a block on the serving
/// generation (#479) waits with its complete generation, parked `blocked`.
/// A reindex then would delete that generation and meet the same block, the
/// cost O28 rejected: it is ignored, and the rebuild stays parked as it was.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_a_rebuild_waiting_on_its_seeds_retire_block_is_refused() {
    let rig = Rig::new(64);
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!("the rebuild asks its backfill");
    };
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the seed parks on the retire block", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked {
                reason: crate::ParkReason::SeedBlocked { .. },
                ..
            }
        )
    })
    .await;
    let detaches = rig.sink.detaches().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(
        rig.sink.rebuilds.lock().unwrap().len(),
        1,
        "no second rebuild"
    );
    assert_eq!(rig.sink.detaches().len(), detaches, "not detached again");
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    assert!(matches!(
        engine.lane_health(&lane(users())),
        LaneHealth::Parked {
            reason: crate::ParkReason::SeedBlocked { .. },
            ..
        }
    ));
    task.abort();
}

/// Review of step 4, S3: the reattach killed before each of its writes, and
/// what the next start finds. Until the record is deleted the lane comes
/// back detached (the fence frozen at the detach); from the delete on it
/// comes back attached with the gap, never without it, so the fence never
/// vouches inside it. The gap must be kept before the record goes.
#[tokio::test(start_paused = true)]
async fn a_reattach_killed_before_any_step_comes_back_with_its_gap_or_detached() {
    // The step the process died before, and whether the next start finds
    // the lane detached (else attached, carrying the open gap). The last
    // row dies after `Attached`, before the completed gap is saved.
    for (died_before, detached) in [
        ("ensure_index", true),
        ("record_gap", true),
        ("forget_detachment", true),
        ("attached", false),
        ("saved", false),
    ] {
        let rig = Rig::new(64);
        let (engine, task, control) = detached_live_only(&rig).await;
        if !matches!(died_before, "attached" | "saved") {
            rig.sink.dead.lock().unwrap().insert(died_before);
        }
        control.send(reattach(users())).await.unwrap();
        if matches!(died_before, "attached" | "saved") {
            let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
                panic!("an attach");
            };
            if died_before == "saved" {
                rig.sink.dead.lock().unwrap().insert("record_gap");
                rig.progress.routed(Position::new(3));
                rig.progress.attach(&lane(users()), attempt);
                publish(&rig, LaneItem::Attached { attempt }).await;
                eventually("the lane opens", || {
                    engine.lane_health(&lane(users())).is_open()
                })
                .await;
            }
        } else {
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        task.abort();
        let _ = task.await;

        // The next start, before the slot is held, over what the sink kept.
        let mut next = Rig::new(64);
        next.sink = Arc::clone(&rig.sink);
        next.sink.dead.lock().unwrap().clear();
        next.positions
            .down
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let engine = next.live_only_engine();
        engine.stage(Continuity::Resumed).await.unwrap();
        let heard: Vec<sink::Gap> = next
            .heard
            .gaps
            .lock()
            .unwrap()
            .iter()
            .flat_map(|(_, gaps, _)| gaps.clone())
            .collect();
        if detached {
            // The fence freezes where the detach recorded reads stood: it
            // vouches up to 1, nothing past it.
            assert!(
                matches!(
                    engine.lane_health(&lane(users())),
                    LaneHealth::Detached { through: Some(ref at), .. } if at == "1"
                ),
                "{died_before}: the record still says detached, frozen at 1"
            );
        } else {
            // The fence refuses every position past 1 until a read while
            // the slot is held settles the gap's end.
            assert!(
                engine.lane_health(&lane(users())).is_open(),
                "{died_before}: no record, the lane attached"
            );
            assert_eq!(
                heard,
                [gap(Some("1"), None)],
                "{died_before}: the fence hears the open gap"
            );
        }
    }
}

/// Send a reattach of `users` on `control` and wait for its verdict.
async fn verdict(control: &mpsc::Sender<SinkControl>) -> crate::ReattachVerdict {
    let (reply, verdict) = crate::Reply::new();
    control
        .send(SinkControl::Reattach {
            index: users(),
            reply,
        })
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), verdict)
        .await
        .expect("a verdict")
        .expect("answered")
}

/// Review of step 4, S4: the operator hears the truth. A reattach answers
/// `Started` once the record is gone; a refusal, with why and what works
/// instead; a sink failing a step before the record's delete, `Failed`.
#[tokio::test(start_paused = true)]
async fn a_reattach_answers_its_verdict() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    let (task, control) = spawn(&engine);
    let crate::ReattachVerdict::Refused(why) = verdict(&control).await else {
        panic!("a sink that can rebuild refuses");
    };
    assert!(why.contains("flusso reindex users --sink primary"), "{why}");
    task.abort();

    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    assert_eq!(
        verdict(&control).await,
        crate::ReattachVerdict::Refused("the lane is not detached".to_owned())
    );
    task.abort();

    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    rig.sink.dead.lock().unwrap().insert("ensure_index");
    assert!(matches!(
        verdict(&control).await,
        crate::ReattachVerdict::Failed(_)
    ));
    rig.sink.dead.lock().unwrap().clear();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Newer { seeded: false });
    let crate::ReattachVerdict::Refused(why) = verdict(&control).await else {
        panic!("records that don't allow it refuse");
    };
    assert!(why.contains("restart flusso"), "{why}");
    rig.sink
        .kept
        .lock()
        .unwrap()
        .get_mut("users")
        .unwrap()
        .marker = Some(MarkerAt::Serving { seeded: true });
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    task.abort();
}

/// Review of step 4, S4(b): a `detached` record written while the sink could
/// rebuild, found by a start on a sink now `backfill = false`, is put back
/// to detached live only (R14) with its marker on the generation reads see:
/// a reattach then applies.
#[tokio::test(start_paused = true)]
async fn a_detached_record_on_a_sink_switched_to_backfill_false_is_put_back_live_only() {
    let rig = Rig::new(64);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(
        *rig.sink.demoted.lock().unwrap(),
        [("users".to_owned(), true)]
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    let (task, control) = spawn(&engine);
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    task.abort();
}

/// Review of step 4, S5: the slot can open again while a turn awaits (here a
/// reindex's staging of another index), and another process may have
/// detached a lane meanwhile: the read before anything the turn takes is
/// written finds it, and the deployment stops.
#[tokio::test(start_paused = true)]
async fn a_slot_reopened_while_a_reindex_stages_is_read_before_the_items_it_brought() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.ensure_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "the reindex waits in ensure_index"
    );
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.live(&users(), 1, 1).await;
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(1);
    stops(task).await;
    assert!(written(&rig).is_empty(), "nothing of it is written");
}

/// Review of step 4, m1: a rebuild resumed at a start serves `rebuilding`
/// (O2), not `backfilling`: reads see the old generation. The fence hears
/// the rebuild before the backfill is asked.
#[tokio::test(start_paused = true)]
async fn a_rebuild_resumed_at_a_start_serves_rebuilding() {
    let rig = Rig::new(64);
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(*rig.heard.rebuilding.lock().unwrap(), [users()]);
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
}

/// The cut: a rebuild another process began on a lane this one holds open
/// stops the deployment at the read once the slot is held; the rebuild is
/// left to its process, nothing of the lane written.
#[tokio::test(start_paused = true)]
async fn a_rebuild_another_process_began_since_this_one_read_stops_the_deployment() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Rebuilding { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            version(),
        ),
    );
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    stops(task).await;
    assert!(written(&rig).is_empty(), "nothing of it is written");
    assert!(
        rig.sink.demoted.lock().unwrap().is_empty(),
        "left to its process"
    );
}

/// Review of step 4, m2 (§6a item 2): a rebuild that can't begin is owed:
/// the lane stays detached, its reason says why, and the rebuild is tried
/// again on the staging backoff until it begins.
#[tokio::test(start_paused = true)]
async fn a_rebuild_that_cannot_begin_is_owed_and_tried_again() {
    let rig = Rig::new(64);
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    rig.sink.dead.lock().unwrap().insert("begin_rebuild");
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the reason says why", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Detached { ref reason, .. } if reason.contains("the rebuild could not begin")
        )
    })
    .await;
    assert!(rig.sink.rebuilds.lock().unwrap().is_empty());
    rig.sink.dead.lock().unwrap().clear();
    let stream::Request::Attach { .. } = next_request(&rig).await else {
        panic!("the owed rebuild begins and asks for its attach");
    };
    assert_eq!(*rig.sink.rebuilds.lock().unwrap(), ["users"]);
    task.abort();
}

/// Review of the step-4 fixes, S-a: a staging needs no slot, so it never
/// settles a gap's unknown end, even with the source answering (here from a
/// stream that ended): the fence hears the gap open, and only a read while
/// this process's stream holds the slot settles it.
#[tokio::test(start_paused = true)]
async fn a_staging_while_the_stream_is_closed_never_settles_a_gap_end() {
    let rig = Rig::new(64);
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), None)]);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.sink.gaps.lock().unwrap().is_empty(), "nothing written");
    // The stream opened and ended again before the run reads its records:
    // the read is due, and the source answers from the ended stream.
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.rereads.closed();
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .last()
            .map(|(_, gaps, _)| gaps.clone()),
        Some(vec![gap(Some("1"), None)])
    );
    let (task, _control) = spawn(&engine);
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        rig.sink.gaps.lock().unwrap().is_empty(),
        "nor by the run while the slot isn't held: {:?}",
        rig.sink.gaps.lock().unwrap()
    );
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("settled once the slot is held", || {
        rig.sink.gaps.lock().unwrap().as_slice()
            == [("users".to_owned(), vec![gap(Some("1"), Some("nothing"))])]
    })
    .await;
    task.abort();
}

/// Review of the step-4 fixes, S-b: a reattach whose record delete failed
/// left its gap open; asked again, it reuses that gap (one per detach) and
/// completes it at the attach, so no open gap refuses later positions.
#[tokio::test(start_paused = true)]
async fn a_failed_forget_then_a_reattach_keeps_one_gap_and_completes_it() {
    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    rig.sink.dead.lock().unwrap().insert("forget_detachment");
    assert!(matches!(
        verdict(&control).await,
        crate::ReattachVerdict::Failed(_)
    ));
    rig.sink.dead.lock().unwrap().clear();
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("an attach");
    };
    rig.progress.routed(Position::new(3));
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the gap is completed", || {
        rig.sink.marker_gaps.lock().unwrap().get("users") == Some(&vec![gap(Some("1"), Some("3"))])
    })
    .await;
    assert_eq!(
        rig.heard
            .gaps
            .lock()
            .unwrap()
            .last()
            .map(|(_, gaps, _)| gaps.clone()),
        Some(vec![gap(Some("1"), Some("3"))])
    );
    task.abort();
}

/// Review of the step-4 fixes, S-b: a record delete whose call failed after
/// it landed is read again: the reattach goes on, and the operator hears
/// `Started`, not that nothing changed.
#[tokio::test(start_paused = true)]
async fn a_record_delete_that_landed_although_its_call_failed_goes_on() {
    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    *rig.sink.forget_lands_then_fails.lock().unwrap() = true;
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    let _attach = next_request(&rig).await;
    task.abort();
}

/// Review of the second fix round, S-a': another writer settles the
/// reattach's open gap at a stale place (it believed it held the slot)
/// before the attach. The attach takes its gap by its start, open or not,
/// and moves the end to where it attached: an end only ever moves later.
#[tokio::test(start_paused = true)]
async fn a_gap_settled_by_another_writer_before_the_attach_keeps_the_attach_end() {
    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("an attach");
    };
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), Some("2"))]);
    rig.progress.routed(Position::new(3));
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the attach end is kept", || {
        rig.sink.marker_gaps.lock().unwrap().get("users") == Some(&vec![gap(Some("1"), Some("3"))])
    })
    .await;
    task.abort();
}

/// Review of the second fix round: a failed opening counts no read and
/// holds no slot, so a read after it doesn't count and an open gap stays
/// open.
#[tokio::test(start_paused = true)]
async fn a_failed_opening_counts_no_read_and_settles_nothing() {
    let rig = Rig::new(64);
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), None)]);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    assert!(!rig.rereads.holding());
    rig.rereads.read(&lane(users()), rig.rereads.read_begins());
    assert!(!read_all(&rig));
    let (task, _control) = spawn(&engine);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(rig.sink.gaps.lock().unwrap().is_empty(), "nothing settled");
    task.abort();
}

/// Review of the second fix round: a sink run restarted mid-stream re-stages
/// over a gap a reattach left open (it died between the record's delete and
/// saving the end). With the slot held, its run settles the gap at its next
/// turn, without waiting for a new opening, so `/wait` doesn't refuse every
/// later position meanwhile.
#[tokio::test(start_paused = true)]
async fn a_restaging_while_the_slot_is_held_settles_the_gap_without_a_new_opening() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), None)]);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(read_all(&rig), "the staging is the read");
    assert!(
        rig.sink.gaps.lock().unwrap().is_empty(),
        "a staging never settles"
    );
    let (task, _control) = spawn(&engine);
    eventually("settled by the run", || {
        rig.sink.gaps.lock().unwrap().as_slice()
            == [("users".to_owned(), vec![gap(Some("1"), Some("nothing"))])]
    })
    .await;
    task.abort();
}

/// Review of the second fix round, m-f: a gap kept with both ends unknown (a
/// corrupt entry) before a reattach is settled after the attach, at the
/// run's next turn while the slot is held, instead of refusing every later
/// position until the next opening.
#[tokio::test(start_paused = true)]
async fn a_corrupt_gap_kept_before_a_reattach_is_settled_after_it() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (_engine, task, control) = detached_live_only(&rig).await;
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(None, None)]);
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
        panic!("an attach");
    };
    rig.progress.routed(Position::new(3));
    rig.progress.attach(&lane(users()), attempt);
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the corrupt gap is settled too", || {
        rig.sink.marker_gaps.lock().unwrap().get("users")
            == Some(&vec![gap(None, Some("3")), gap(Some("1"), Some("3"))])
    })
    .await;
    task.abort();
}

/// The review of the cut, B-2: a reattach whose delete failed, then asked
/// again under a later opening, never stops a lone process, whichever way
/// the delete went. Landed (unseen, the read after it failing too), the
/// record is gone with this reattach's open gap: the reattach goes on to
/// the attach. Not landed, the record stays: the reattach deletes it.
#[tokio::test(start_paused = true)]
async fn a_reattach_whose_delete_failed_finishes_either_way_without_a_stop() {
    for landed in [true, false] {
        let rig = Rig::new(64);
        rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
        let (engine, task, control) = detached_live_only(&rig).await;
        if landed {
            *rig.sink.forget_lands_then_fails.lock().unwrap() = true;
            // The reattach's own read succeeds; the reads after the failed
            // delete (the read back, the reattach's own) don't.
            *rig.sink.reads_fail_after_forget.lock().unwrap() = 2;
        } else {
            rig.sink.dead.lock().unwrap().insert("forget_detachment");
        }
        let crate::ReattachVerdict::Failed(_) = verdict(&control).await else {
            panic!("landed {landed}: the delete failed: Failed");
        };
        assert_eq!(
            rig.sink.kept.lock().unwrap().get("users").is_none(),
            landed,
            "landed {landed}"
        );
        *rig.sink.forget_lands_then_fails.lock().unwrap() = false;
        rig.sink.dead.lock().unwrap().clear();
        rig.rereads.closed();
        rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
        assert_eq!(
            verdict(&control).await,
            crate::ReattachVerdict::Started,
            "landed {landed}"
        );
        assert!(rig.sink.kept.lock().unwrap().get("users").is_none());
        let stream::Request::Attach { attempt, .. } = next_request(&rig).await else {
            panic!("landed {landed}: the attach is asked");
        };
        publish(&rig, LaneItem::Attached { attempt }).await;
        eventually("the lane is attached", || {
            engine.lane_health(&lane(users())).is_open()
        })
        .await;
        tokio::time::sleep(Duration::from_secs(5)).await;
        assert!(!task.is_finished(), "landed {landed}: {:?}", task.await);
        task.abort();
    }
}

/// The third re-check, item 2: an unread lane vouches for nothing, so after
/// an opening its sink reads promptly even on a quiet stream (no change, no
/// write): the opening wakes the run, which reads within the read's own
/// time, not at its next idle tick or write.
#[tokio::test(start_paused = true)]
async fn an_opening_on_a_quiet_stream_is_read_at_once() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, _control) = spawn(&engine);
    tokio::time::sleep(Duration::from_millis(250)).await;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let opened = Instant::now();
    while !read_all(&rig) {
        assert!(
            opened.elapsed() < Duration::from_millis(50),
            "not read within 50 ms of the opening"
        );
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    task.abort();
}

/// The third re-check, item 1 (B1'''): a staging that couldn't read one
/// index's records leaves it restaging (#471) and counts no read of it: its
/// lane stays unread (it vouches for nothing) while the lane read goes on.
#[tokio::test(start_paused = true)]
async fn a_staging_that_left_an_index_unread_counts_no_read_of_it() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink
        .unreadable
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.rereads.current(&lane(users())), "read");
    assert!(!rig.rereads.current(&lane(orders())), "never read");

    // During a stop too.
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink
        .unreadable
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let stop = crate::Stop::new();
    stop.request();
    let engine = rig.built("primary", Arc::clone(&rig.sink)).with_stop(stop);
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(
        !rig.rereads.current(&lane(orders())),
        "never read, stopping"
    );
}

/// The third re-check, item 1: the run's read again counts each lane it
/// read; one whose records can't be read stays unread (the run fails, and
/// its restart stages again), never counted with its siblings.
#[tokio::test(start_paused = true)]
async fn a_read_again_that_missed_a_lane_counts_no_read_of_it() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.sink
        .unreadable
        .lock()
        .unwrap()
        .insert("users".to_owned());
    let (task, _control) = spawn(&engine);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!rig.rereads.current(&lane(users())), "its read failed");
    task.abort();
}

/// Whether every lane of `primary` counts as read.
fn read_all(rig: &Rig) -> bool {
    [orders(), users()]
        .into_iter()
        .all(|index| rig.rereads.current(&lane(index)))
}

/// `orders` left restaging by a staging that couldn't read it, `users` read;
/// the restage's step then reads `orders`' records and waits in
/// `ensure_index` until `gate` is released.
async fn restage_held_in_ensure(
    rig: &Rig,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    Arc<tokio::sync::Semaphore>,
) {
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink
        .unreadable
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(!rig.rereads.current(&lane(orders())));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.ensure_gate.lock().unwrap() = Some(Arc::clone(&gate));
    rig.sink.unreadable.lock().unwrap().clear();
    let ensured = rig.sink.ensured.lock().unwrap().len();
    let (task, _control) = spawn(&engine);
    tokio::time::sleep(Duration::from_secs(60)).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "held in ensure_index"
    );
    assert!(!engine.lane_health(&lane(orders())).is_open());
    (engine, task, gate)
}

/// The fourth re-check, B-1: a restage whose read counted for the stream
/// before an opening reopens its lane only once its records were read again
/// under the current one: it is not written before, and it vouches once read
/// (no next opening needed).
#[tokio::test(start_paused = true)]
async fn a_restage_that_spans_an_opening_reads_again_before_its_lane_opens() {
    let rig = Rig::new(64);
    let (engine, task, gate) = restage_held_in_ensure(&rig).await;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(
        rig.rereads.current(&lane(users())),
        "the run read users again"
    );
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(16);
    eventually("orders reopens", || {
        engine.lane_health(&lane(orders())).is_open()
    })
    .await;
    eventually("orders is read under the current stream", || {
        rig.rereads.current(&lane(orders()))
    })
    .await;
    rig.live(&orders(), 7, 7).await;
    eventually("then written", || {
        rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned())
    })
    .await;
    assert!(rig.sink.unread_writes.lock().unwrap().is_empty());
    task.abort();
}

/// `orders`' reindex (a `Rebuild` step when `detached`, else a plain
/// reindex's), held in `ensure_index` while the slot opens again.
async fn reindex_held_across_an_opening(
    rig: &Rig,
    detached: bool,
) -> (
    Arc<SinkEngine>,
    tokio::task::JoinHandle<crate::Result<()>>,
    Arc<tokio::sync::Semaphore>,
) {
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    if detached {
        rig.sink
            .kept
            .lock()
            .unwrap()
            .insert("orders".to_owned(), detached_record());
    }
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(engine.lane_health(&lane(orders())).is_detached(), detached);
    let (task, control) = spawn(&engine);
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.ensure_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control
        .send(SinkControl::Reindex {
            indexes: vec![orders()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "held in ensure_index"
    );
    if detached {
        assert_eq!(*rig.sink.rebuilds.lock().unwrap(), ["orders".to_owned()]);
    }
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    (engine, task, gate)
}

/// B-1's reindex shape: a reindex whose step read `orders` before an
/// opening reopens its lane only once read again under the current stream.
#[tokio::test(start_paused = true)]
async fn a_reindex_that_spans_an_opening_reads_again_before_its_lane_opens() {
    let rig = Rig::new(64);
    let (engine, task, gate) = reindex_held_across_an_opening(&rig, false).await;
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(16);
    rig.live(&orders(), 7, 7).await;
    eventually("orders is read under the current stream", || {
        rig.rereads.current(&lane(orders()))
    })
    .await;
    eventually("then written", || {
        rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned())
    })
    .await;
    assert!(engine.lane_health(&lane(orders())).is_open());
    assert!(
        rig.sink.unread_writes.lock().unwrap().is_empty(),
        "written before its records were read under the current stream"
    );
    task.abort();
}

/// B-1's reindex shape, writer half: another process detached `orders`
/// while the reindex was in flight across an opening: the deployment stops,
/// nothing written.
#[tokio::test(start_paused = true)]
async fn a_reindex_across_an_opening_stops_on_a_lane_another_process_detached() {
    let rig = Rig::new(64);
    let (_engine, task, gate) = reindex_held_across_an_opening(&rig, false).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("orders".to_owned(), detached_record());
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(16);
    rig.live(&orders(), 7, 7).await;
    stops(task).await;
    assert!(
        !rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned()),
        "written into an index the records say is detached"
    );
}

/// The attach the rebuild of `orders` asked for, answered by the builder.
async fn attach_orders(rig: &Rig) {
    let attempt = loop {
        if let stream::Request::Attach { index, attempt, .. } = next_request(rig).await
            && index == orders()
        {
            break attempt;
        }
    };
    rig.stream
        .lane(&lane(orders()))
        .unwrap()
        .producer
        .publish(LaneItem::Attached { attempt })
        .await
        .unwrap();
}

/// B-1's rebuild shape: a detached `orders` rebuilt by a reindex whose
/// step spans an opening is written once attached, never before its
/// records were read under the current stream.
#[tokio::test(start_paused = true)]
async fn a_rebuild_that_spans_an_opening_reads_again_before_its_lane_opens() {
    let rig = Rig::new(64);
    let (engine, task, gate) = reindex_held_across_an_opening(&rig, true).await;
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(16);
    attach_orders(&rig).await;
    rig.live(&orders(), 7, 7).await;
    eventually("written once attached", || {
        rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned())
    })
    .await;
    assert!(engine.lane_health(&lane(orders())).is_open());
    assert!(
        rig.sink.unread_writes.lock().unwrap().is_empty(),
        "written before its records were read under the current stream"
    );
    task.abort();
}

/// B-1's rebuild shape, writer half: the process that held the slot before
/// the opening took this rebuild as another's and put it back to detached.
/// The attach opens nothing: the records are read first, and say detached.
#[tokio::test(start_paused = true)]
async fn a_rebuild_across_an_opening_never_writes_a_lane_another_process_demoted() {
    let rig = Rig::new(64);
    let (engine, task, gate) = reindex_held_across_an_opening(&rig, true).await;
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("orders".to_owned(), detached_record());
    *rig.sink.ensure_gate.lock().unwrap() = None;
    gate.add_permits(16);
    // Each attach asked of `orders` is answered, as a builder would.
    let mut requests = rig.stream.requests().unwrap().consumer;
    let producer = rig.stream.lane(&lane(orders())).unwrap().producer;
    let builder = tokio::spawn(async move {
        while let Ok(Some(delivery)) = requests.recv().await {
            let (request, handle) = delivery.into_parts();
            handle.ack().await.unwrap();
            if let stream::Request::Attach { index, attempt, .. } = request
                && index == orders()
            {
                producer
                    .publish(LaneItem::Attached { attempt })
                    .await
                    .unwrap();
            }
        }
    });
    rig.live(&orders(), 7, 7).await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    builder.abort();
    assert!(
        !rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned()),
        "written into an index the records say is detached"
    );
    assert!(rig.sink.unread_writes.lock().unwrap().is_empty());
    assert!(engine.lane_health(&lane(orders())).is_detached());
    task.abort();
}

/// The cut, for the fourth re-check's B-2: a rebuild another process began
/// (its record `rebuilding`, a walk checkpoint kept) is neither demoted nor
/// written over; the deployment stops, and the rebuild and its checkpoint
/// stay as its process left them for the next start to decide.
#[tokio::test(start_paused = true)]
async fn a_rebuild_another_process_began_is_left_as_it_is_when_the_deployment_stops() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink
        .walked
        .lock()
        .unwrap()
        .insert("users".to_owned(), "k".to_owned());
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    rig.live(&users(), 1, 1).await;
    stops(task).await;
    assert!(rig.sink.demoted.lock().unwrap().is_empty());
    assert!(written(&rig).is_empty());
    assert_eq!(
        rig.sink
            .walked
            .lock()
            .unwrap()
            .get("users")
            .map(String::as_str),
        Some("k"),
        "the checkpoint stays"
    );
}

/// The fourth re-check, S-1: a read is tied to the stream it began under.
/// A staging whose read of the records is held open while the slot opens
/// again counts no read for the new stream, though it returns after the
/// opening.
#[tokio::test(start_paused = true)]
async fn a_step_whose_read_spans_an_opening_counts_no_read_for_it() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.read_gate.lock().unwrap() = Some(Arc::clone(&gate));
    let engine = rig.engine();
    let staging = tokio::spawn({
        let engine = Arc::clone(&engine);
        async move { engine.stage(Continuity::Resumed).await }
    });
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!staging.is_finished(), "the reads are held");
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    *rig.sink.read_gate.lock().unwrap() = None;
    gate.add_permits(16);
    staging.await.unwrap().unwrap();
    for of in [orders(), users()] {
        assert!(
            !rig.rereads.current(&lane(of.clone())),
            "{of}: a read begun before the opening counts for it"
        );
    }
}

/// The fifth re-check: after a failed opening (another process may hold the
/// slot) what the lanes hold is written as on `main`, but no seed is made;
/// since the sixth, the seed is owed and lands once an opening gets through,
/// with no new walk.
#[tokio::test(start_paused = true)]
async fn a_failed_opening_seeds_nothing_until_an_opening_gets_through() {
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!("a backfill is asked at staging");
    };
    eventually("read under the stream", || read_all(&rig)).await;
    rig.rereads.closed();
    assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    rig.live(&users(), 2, 2).await;
    eventually("written as on main", || {
        written(&rig).contains(&"users/2".to_owned())
    })
    .await;
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "seeded with no slot held"
    );
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("the owed seed lands", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    task.abort();
}

/// Whose rebuild a `rebuilding` record is, its attempts say. A lane
/// rebuilding here under attempt 1 whose record, read again once the slot is
/// held, counts attempt 2 (another process put this one back and began its
/// own) stops the deployment, even with no read of the records before this
/// process's own rebuild: what it wrote is what it knows (found by the world
/// at 50,000 cases). Nothing is demoted.
#[tokio::test(start_paused = true)]
async fn a_rebuild_another_process_began_since_ours_stops_the_deployment() {
    let rig = Rig::new(64);
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the rebuild is attached", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    if let Some(record) = rig.sink.kept.lock().unwrap().get_mut("users") {
        record.attempts = Some(2);
        by_another_process(record);
    }
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    stops(task).await;
    assert!(
        rig.sink.demoted.lock().unwrap().is_empty(),
        "nothing demoted"
    );
}

/// An own record write is followed by a read while the slot is still held,
/// which learns the version it left: another process that later deletes it
/// and writes an identical record (its own rebuild under the same attempt
/// count) is told apart by the version, and the deployment stops.
#[tokio::test(start_paused = true)]
async fn an_identical_record_written_again_by_another_process_after_our_rebuild_stops() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the rebuild is attached", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    rig.rereads.closed();
    if let Some(record) = rig.sink.kept.lock().unwrap().get_mut("users") {
        by_another_process(record);
    }
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    stops(task).await;
}

/// The cut: a lane held detached while its rebuild waits to attach, whose
/// record another process put back to detached, stops the next staging: the
/// attach it asked for opens nothing.
#[tokio::test(start_paused = true)]
async fn a_staging_finding_its_rebuild_put_back_by_another_process_stops() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, _attempt) = rebuilding_on_reindex(&rig).await;
    task.abort();
    let _ = task.await;
    rig.rereads.closed();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    assert!(stopped(&engine.stage(Continuity::Resumed).await));
    assert!(!written(&rig).contains(&"users/99".to_owned()));
}

/// The fifth re-check, B-1: the stream ended (this process no longer holds
/// the slot) and no opening has begun. Another process detaches `users`.
/// What the lane holds is written as on `main`, but the seed it reaches
/// erases no record: with no slot held the index stays unseeded.
#[tokio::test(start_paused = true)]
async fn a_stream_that_ended_seeds_nothing_over_a_detach_written_since() {
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    let stream::Request::Backfill { index, ticket, .. } = next_request(&rig).await else {
        panic!("a backfill is asked at staging");
    };
    assert_eq!(index, users());
    eventually("read under the stream", || read_all(&rig)).await;
    rig.live(&users(), 1, 1).await;
    eventually("written while held", || {
        written(&rig).contains(&"users/1".to_owned())
    })
    .await;
    rig.rereads.closed();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    rig.live(&users(), 2, 2).await;
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the lane's backlog is written, as on main", || {
        written(&rig).contains(&"users/2".to_owned())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned();
    let seeded = rig.sink.seeded.lock().unwrap().clone();
    task.abort();
    assert!(
        kept.is_some() && seeded.is_empty(),
        "with no slot held the seed erased the record: record after {kept:?}, seeded {seeded:?}"
    );
}

/// The fifth re-check, S-1: one process, nothing detached. The stream ends
/// and the restart's opening fails (the old walsender still holds the slot).
/// A stop arrives: the sinks still land what their lanes hold, as on `main`,
/// so the drain ends instead of sitting out its deadline.
#[tokio::test(start_paused = true)]
async fn a_stop_while_an_opening_failed_still_drains_the_lanes() {
    let rig = Rig::new(64);
    let stop = crate::Stop::new();
    let engine = Arc::new(
        rig.built("primary", Arc::clone(&rig.sink))
            .with_stop(stop.clone()),
    );
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    eventually("read under the stream", || read_all(&rig)).await;
    rig.live(&users(), 1, 1).await;
    eventually("written", || written(&rig).contains(&"users/1".to_owned())).await;
    rig.sink
        .slow
        .lock()
        .unwrap()
        .replace(("2".to_owned(), Duration::from_secs(2)));
    for at in 2..10 {
        rig.live(&users(), i64::try_from(at).unwrap(), at).await;
    }
    rig.rereads.closed();
    assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    stop.request();
    let started = Instant::now();
    let mut drained = false;
    while Instant::now() < started + Duration::from_secs(30) {
        if rig.stream.is_idle() {
            drained = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let written = written(&rig);
    task.abort();
    assert!(
        drained,
        "the lanes never drained within 30 s of the stop: written {written:?}"
    );
}

/// `users`' detach writes its record but the call fails (the lane parks,
/// unsure), then this process stops holding the slot as `lose` does, and
/// another process writes its own detach record of `users`.
async fn a_failed_detach_then_another_process_detaches(
    rig: &Rig,
    lose: impl std::future::Future<Output = ()>,
) -> tokio::task::JoinHandle<crate::Result<()>> {
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record was written and the call failed", || {
        rig.sink.detaches().len() == 1
    })
    .await;
    lose.await;
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new("9:9")),
        ),
    );
    task
}

/// The review of the cut, B-2: a detach whose call fails after its record
/// landed is read back while the slot is still held; the record found is the
/// one it wrote, so the detach is taken as done: the lane detaches under its
/// own record, writes nothing more of the index, and nothing stops.
#[tokio::test(start_paused = true)]
async fn a_detach_that_landed_unseen_is_taken_as_done_from_its_read_back() {
    let rig = Rig::new(64);
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::LandsAsAskedThenFails);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches under its own record", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    assert_eq!(rig.sink.detaches().len(), 1, "no second detach");
    rig.sink.block(&users(), false);
    rig.live(&users(), 2, 2).await;
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(
        !written(&rig).contains(&"users/2".to_owned()),
        "nothing written onto it"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(!task.is_finished(), "{:?}", task.await);
    task.abort();
}

/// Found by the world at 50,000 cases: a lane whose detach landed but
/// failed, read again at the next opening before its clear ran. The record
/// carries this process's own stamp: its detach landed, and the read again
/// takes the lane as detached, as staging does (the review of round 10,
/// m-8). The clear then has nothing left to do: the record stays, the lane
/// stays detached once its block lifts.
#[tokio::test(start_paused = true)]
async fn a_failed_detach_read_again_before_its_clear_is_taken_as_landed() {
    let rig = Rig::new(64);
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record was written and the call failed", || {
        rig.sink.detaches().len() == 1
    })
    .await;
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.block(&users(), false);
    tokio::time::sleep(Duration::from_secs(60)).await;
    let health = engine.lane_health(&lane(users()));
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned();
    assert!(!task.is_finished(), "{:?}", task.await);
    task.abort();
    assert!(
        health.is_detached() && kept.is_some(),
        "health {health:?}, record {kept:?}"
    );
    assert!(rig.sink.forgotten.lock().unwrap().is_empty());
    assert!(
        !engine.lane_state(&lane(users())).detach_unsure,
        "settled by the read: the clear has nothing left to do"
    );
}

/// The fifth re-check (second report), B-2: a parked lane's clear deletes a
/// `Detached` record it reads. After a failed opening another process may
/// hold the slot and rely on that record: the clear waits until this process
/// holds the slot again.
#[tokio::test(start_paused = true)]
async fn a_clear_waits_while_no_stream_is_held() {
    let rig = Rig::new(64);
    let task = a_failed_detach_then_another_process_detaches(&rig, async {
        rig.rereads.closed();
        assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    })
    .await;
    tokio::time::sleep(Duration::from_secs(120)).await;
    let forgotten = rig.sink.forgotten.lock().unwrap().clone();
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned();
    task.abort();
    assert!(
        kept.is_some() && !forgotten.contains(&"users@9:9".to_owned()),
        "no stream held, yet: forgotten {forgotten:?}, record after {kept:?}"
    );
}

/// ... and the same once the stream ended, with no opening begun.
#[tokio::test(start_paused = true)]
async fn a_clear_waits_after_the_stream_ended() {
    let rig = Rig::new(64);
    let task = a_failed_detach_then_another_process_detaches(&rig, async {
        rig.rereads.closed();
    })
    .await;
    tokio::time::sleep(Duration::from_secs(120)).await;
    let forgotten = rig.sink.forgotten.lock().unwrap().clone();
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned();
    task.abort();
    assert!(
        kept.is_some() && !forgotten.contains(&"users@9:9".to_owned()),
        "no stream held, yet: forgotten {forgotten:?}, record after {kept:?}"
    );
}

/// The fifth re-check's narrowing: a reattach deletes the detach record,
/// which may be another process's once this process stopped holding the
/// slot. It is refused, with the reason, until the slot is held again; then
/// it starts as usual.
#[tokio::test(start_paused = true)]
async fn a_reattach_is_refused_while_no_stream_is_held() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (_engine, task, control) = detached_live_only(&rig).await;
    rig.rereads.closed();
    assert_eq!(
        verdict(&control).await,
        crate::ReattachVerdict::Refused(
            "this process doesn't hold the slot (its stream ended or its opening failed), \
             so the record may be another process's; ask again once it holds the slot"
                .to_owned()
        )
    );
    assert!(rig.sink.kept.lock().unwrap().get("users").is_some());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    assert_eq!(verdict(&control).await, crate::ReattachVerdict::Started);
    task.abort();
}

/// The fifth re-check's narrowing: a reindex of a detached index begins a
/// rebuild, rewriting the detach record, which may be another process's once
/// this process stopped holding the slot. The reindex is owed until the slot
/// is held again, then the rebuild begins; the run neither hangs nor fails.
#[tokio::test(start_paused = true)]
async fn a_reindex_waits_for_the_slot_before_it_rewrites_a_record() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    rig.rereads.closed();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(120)).await;
    assert!(
        rig.sink.rebuilds.lock().unwrap().is_empty(),
        "a rebuild began with no slot held"
    );
    assert!(!task.is_finished(), "the run goes on");
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("the rebuild begins once the slot is held", || {
        *rig.sink.rebuilds.lock().unwrap() == ["users"]
    })
    .await;
    task.abort();
}

/// ... and a reindex of an open index (a new generation) is owed too, its
/// lane restaging with the reason, until the slot is held; the sibling lane
/// writes on.
#[tokio::test(start_paused = true)]
async fn a_reindex_of_an_open_index_is_owed_while_no_stream_is_held() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    eventually("read under the stream", || read_all(&rig)).await;
    rig.rereads.closed();
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("owed, with why", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Restaging { ref reason, .. }
                if reason.contains("the reindex waits: ")
                    && reason.contains("doesn't hold the slot")
        )
    })
    .await;
    rig.live(&orders(), 1, 1).await;
    eventually("the sibling writes on", || {
        rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/1".to_owned())
    })
    .await;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("staged once the slot is held", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    task.abort();
}

/// The fifth re-check, B-3: a staging of a lane held detached finds another
/// process's rebuild (its record `rebuilding`, a walk checkpoint kept). It
/// puts it back to detached before it counts the read, as the read again
/// does, or the slot would pass what the lane dropped and a later start
/// would resume that walk over it.
#[tokio::test(start_paused = true)]
async fn a_staging_of_a_lane_held_detached_demotes_anothers_rebuild() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink
        .walked
        .lock()
        .unwrap()
        .insert("users".to_owned(), "k".to_owned());
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(
        *rig.sink.demoted.lock().unwrap(),
        [("users".to_owned(), false)],
        "another process's rebuild put back to detached"
    );
    assert!(rig.sink.walked.lock().unwrap().get("users").is_none());
    assert!(engine.lane_health(&lane(users())).is_detached());
}

/// The fifth re-check (the staging half of `acf2328a`): a lane rebuilding
/// here under attempt 1 whose record, read at a staging of a running
/// process, counts attempt 2 is another process's: put back to detached,
/// never taken as this one's.
#[tokio::test(start_paused = true)]
async fn a_staging_demotes_a_rebuild_begun_since_ours() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    eventually("the rebuild is attached", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    task.abort();
    let _ = task.await;
    if let Some(record) = rig.sink.kept.lock().unwrap().get_mut("users") {
        record.attempts = Some(2);
    }
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(
        *rig.sink.demoted.lock().unwrap(),
        [("users".to_owned(), false)]
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
}

/// Found by the world property (the fifth re-check's round): a process
/// whose ingest engine opens through `rereads` but hasn't yet (its
/// `prepare` failing while another process holds the slot) holds no slot.
/// A reattach then is refused, as once a stream ended.
#[tokio::test(start_paused = true)]
async fn a_process_whose_stream_never_opened_deletes_no_record() {
    let rig = Rig::new(64);
    let (_engine, task, control) = detached_live_only(&rig).await;
    // Marked as opening only now: a detach itself waits for the slot (S-2).
    rig.rereads.will_open();
    assert!(matches!(
        verdict(&control).await,
        crate::ReattachVerdict::Refused(why) if why.contains("doesn't hold the slot")
    ));
    assert!(rig.sink.kept.lock().unwrap().get("users").is_some());
    task.abort();
}

/// The fifth re-check (second report, Part 2, M14): a read again that can't
/// read a detached lane's records leaves it unread and detached; it never
/// fails the sink's run, and the other lane writes on.
#[tokio::test(start_paused = true)]
async fn a_failed_read_of_a_detached_lane_never_fails_the_run() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink
        .unreadable
        .lock()
        .unwrap()
        .insert("users".to_owned());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.live(&orders(), 7, 7).await;
    eventually("the other lane writes on", || {
        rig.sink
            .ids
            .lock()
            .unwrap()
            .contains(&"orders/7".to_owned())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(!task.is_finished(), "the run goes on");
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(!rig.rereads.current(&lane(users())), "left unread");
    task.abort();
}

/// A gate held until released: what passes it meanwhile waits.
fn held_gate(slot: &Mutex<Option<Arc<tokio::sync::Semaphore>>>) -> Arc<tokio::sync::Semaphore> {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *slot.lock().unwrap() = Some(Arc::clone(&gate));
    gate
}

fn release(slot: &Mutex<Option<Arc<tokio::sync::Semaphore>>>, gate: &tokio::sync::Semaphore) {
    *slot.lock().unwrap() = None;
    gate.add_permits(64);
}

/// The sixth re-check, B-1: the slot is checked immediately before each
/// request that may erase a record, not once ahead of the awaits before it.
/// A reattach that passed its check with the slot held waits in
/// `ensure_index` while the stream ends: its delete is refused, and the
/// record stays.
#[tokio::test(start_paused = true)]
async fn a_reattach_whose_stream_ends_mid_way_deletes_no_record() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (_engine, task, control) = detached_live_only(&rig).await;
    let gate = held_gate(&rig.sink.ensure_gate);
    let (reply, verdict) = crate::Reply::new();
    control
        .send(SinkControl::Reattach {
            index: users(),
            reply,
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.rereads.closed();
    release(&rig.sink.ensure_gate, &gate);
    let verdict = tokio::time::timeout(Duration::from_secs(10), verdict)
        .await
        .expect("a verdict")
        .expect("answered");
    assert!(
        matches!(verdict, crate::ReattachVerdict::Refused(ref why) if why.contains("doesn't hold the slot")),
        "{verdict:?}"
    );
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_some(),
        "the record stays"
    );
    task.abort();
}

/// ... a clear whose read of the records returns after the stream ended
/// deletes nothing, though another process wrote its record meanwhile.
#[tokio::test(start_paused = true)]
async fn a_clear_whose_stream_ends_after_its_read_deletes_nothing() {
    let rig = Rig::new(64);
    rig.sink
        .records
        .lock()
        .unwrap()
        .push_back(Record::FailAfterWriting);
    rig.sink.block(&users(), true);
    let engine = rig.engine();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the record was written and the call failed", || {
        rig.sink.detaches().len() == 1
    })
    .await;
    let gate = held_gate(&rig.sink.read_gate);
    tokio::time::sleep(Duration::from_secs(120)).await;
    rig.rereads.closed();
    rig.sink.kept.lock().unwrap().insert(
        "users".to_owned(),
        Detachment::recorded(
            DetachRecord::Detached { live_only: false },
            Some(MarkerAt::Newer { seeded: false }),
            Some(RecordVersion::new("9:9")),
        ),
    );
    release(&rig.sink.read_gate, &gate);
    tokio::time::sleep(Duration::from_secs(10)).await;
    let forgotten = rig.sink.forgotten.lock().unwrap().clone();
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned();
    task.abort();
    assert!(
        kept.is_some() && !forgotten.contains(&"users@9:9".to_owned()),
        "no stream held at the delete, yet: forgotten {forgotten:?}, record after {kept:?}"
    );
}

/// ... a reindex whose staging reads the records while the stream ends
/// moves the index to no new generation.
#[tokio::test(start_paused = true)]
async fn a_reindex_whose_stream_ends_while_staging_rewrites_nothing() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    eventually("read under the stream", || read_all(&rig)).await;
    let gate = held_gate(&rig.sink.read_gate);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.rereads.closed();
    release(&rig.sink.read_gate, &gate);
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        rig.sink.reindexed.lock().unwrap().is_empty(),
        "a reindex with no stream held"
    );
    task.abort();
}

/// Every request `users`' backfill asks, as the ingest engine would see it.
fn backfill_asks(
    rig: &Rig,
) -> (
    Arc<Mutex<Vec<stream::Request>>>,
    tokio::task::JoinHandle<()>,
) {
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut requests = rig.stream.requests().unwrap().consumer;
    let into = Arc::clone(&asked);
    let task = tokio::spawn(async move {
        while let Ok(Some(delivery)) = requests.recv().await {
            let (request, handle) = delivery.into_parts();
            handle.ack().await.unwrap();
            into.lock().unwrap().push(request);
        }
    });
    (asked, task)
}

/// `users` is backfilling (its backfill asked at staging); the slot is held
/// and the lanes read. Its ticket.
async fn backfilling_under_a_held_stream(
    rig: &Rig,
    engine: &Arc<SinkEngine>,
) -> (stream::Ticket, tokio::task::JoinHandle<crate::Result<()>>) {
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(engine);
    let stream::Request::Backfill { ticket, .. } = next_request(rig).await else {
        panic!("a backfill is asked at staging");
    };
    eventually("read under the stream", || read_all(rig)).await;
    (ticket, task)
}

/// The sixth re-check, B-2: a seed met while this process holds no slot, on
/// a lane parked when the next read runs, still lands once the lane opens:
/// owed per index, whatever the lane's state at the read.
#[tokio::test(start_paused = true)]
async fn a_seed_met_without_the_slot_on_a_lane_parked_at_the_read_lands_once_it_opens() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    rig.sink.block(&users(), true);
    rig.live(&users(), 3, 3).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let health = engine.lane_health(&lane(users()));
    assert!(
        matches!(health, LaneHealth::Parked { .. }),
        "parked before the read: {health:?}"
    );
    let (asked, asks) = backfill_asks(&rig);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        rig.rereads.current(&lane(users())),
        "read again under the new stream"
    );
    rig.sink.block(&users(), false);
    eventually("the parked lane opens and writes", || {
        written(&rig).contains(&"users/3".to_owned())
    })
    .await;
    eventually("the seed lands, or its backfill is asked again", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
            || !asked.lock().unwrap().is_empty()
    })
    .await;
    task.abort();
    asks.abort();
}

/// The sixth re-check, S-1: one process, nothing detached. A backfill's
/// `SnapshotComplete` is met while the stream reconnects: its documents are
/// written already, only the seed waits. Once the slot is held and the
/// records read again show nothing written over the index, the seed lands:
/// the backfill is not walked again.
#[tokio::test(start_paused = true)]
async fn a_seed_met_without_the_slot_lands_once_held_without_a_new_walk() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "seeded with no slot held"
    );
    let (asked, asks) = backfill_asks(&rig);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("the owed seed lands", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        asked.lock().unwrap().is_empty(),
        "walked again: {:?}",
        asked.lock().unwrap()
    );
    task.abort();
    asks.abort();
}

/// ... and when another process kept a gap over the index meanwhile, the
/// read once the slot is held finds it: the deployment stops, nothing
/// seeded over the gap.
#[tokio::test(start_paused = true)]
async fn a_seed_owed_over_a_gap_written_meanwhile_stops_the_deployment() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(5)).await;
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), Some("2"))]);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    stops(task).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "seeded over the gap"
    );
}

/// The sixth re-check's S-2, under the cut: this process's run restarts
/// while its stream is down and stages over a rebuild another process
/// began meanwhile. The staging finds the change and the deployment stops:
/// the rebuild and its checkpoint are left as they are, nothing demoted.
#[tokio::test(start_paused = true)]
async fn a_staging_without_the_slot_over_anothers_rebuild_stops_and_leaves_it() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .walked
        .lock()
        .unwrap()
        .insert("users".to_owned(), "k".to_owned());
    assert!(stopped(
        &staged_over_without_the_slot(&rig, &engine, record).await
    ));
    assert!(rig.sink.demoted.lock().unwrap().is_empty());
    assert_eq!(
        rig.sink
            .walked
            .lock()
            .unwrap()
            .get("users")
            .map(String::as_str),
        Some("k"),
        "the other process's checkpoint stays"
    );
}

/// The sixth re-check, S-2, at a process start: the daemon builds the ingest
/// engine (`will_open`) before the first staging, so that staging holds no
/// slot. A live-only sink demotes a rebuild it finds (a backfilling one takes
/// it over, which writes no record); one another process may be walking is
/// not demoted there: the lane stands in, awaiting the slot (nothing of it
/// written or dropped, so its fence vouches no further), and the first
/// opening's read again demotes it once the slot is held.
#[tokio::test(start_paused = true)]
async fn a_start_staging_before_the_stream_opens_demotes_nothing() {
    let rig = Rig::new(64);
    rig.rereads.will_open();
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: true },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink
        .walked
        .lock()
        .unwrap()
        .insert("users".to_owned(), "k".to_owned());
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert!(rig.sink.demoted.lock().unwrap().is_empty());
    assert!(rig.sink.walked.lock().unwrap().contains_key("users"));
    assert!(
        engine.lane_health(&lane(users())).is_detached(),
        "{:?}",
        engine.lane_health(&lane(users()))
    );
    let (task, _control) = spawn(&engine);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("demoted once the slot is held", || {
        *rig.sink.demoted.lock().unwrap() == [("users".to_owned(), true)]
    })
    .await;
    task.abort();
}

/// The sixth re-check, S-2: a lane's detach deletes every generation but the
/// serving one, so with no slot held it waits. The stream ended; an asked
/// detach of a refusing lane records nothing and the lane stays as it was.
/// Once an opening gets through, the lane detaches.
#[tokio::test(start_paused = true)]
async fn a_detach_with_no_slot_held_waits_for_the_slot() {
    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    eventually("read under the stream", || read_all(&rig)).await;
    rig.live(&users(), 1, 1).await;
    eventually("users/1 lands", || rig.applied(&users()) == through(1)).await;
    rig.rereads.closed();
    rig.sink.block(&users(), true);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 2, 2).await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(
        rig.sink.detaches().is_empty(),
        "no detach sent without the slot"
    );
    assert!(!engine.lane_health(&lane(users())).is_detached());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("detached once the slot is held", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    task.abort();
}

/// The sixth re-check, Part 5: an owed rebuild isn't tried while the slot
/// is another's (`retry_rebuilds`' gate). Tried, each refusal would double
/// the staging backoff; kept back, it begins as soon as an opening gets
/// through, however long the slot was away.
#[tokio::test(start_paused = true)]
async fn an_owed_rebuild_begins_at_once_when_the_slot_comes_back() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    rig.sink.dead.lock().unwrap().insert("begin_rebuild");
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the rebuild is owed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Detached { ref reason, .. } if reason.contains("the rebuild could not begin")
        )
    })
    .await;
    rig.rereads.closed();
    rig.sink.dead.lock().unwrap().clear();
    tokio::time::sleep(Duration::from_secs(300)).await;
    assert!(rig.sink.rebuilds.lock().unwrap().is_empty());
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(*rig.sink.rebuilds.lock().unwrap(), ["users"]);
    task.abort();
}

/// The seventh re-check, B-2: a seed refused for want of the slot is not a
/// probe result. A lane parked on its seed (#479's blocked old generation)
/// while no slot is held keeps its seed out of the grant until an opening:
/// no spin, the sink's other lanes are written meanwhile, and the seed lands
/// at the first probe after the opening.
#[tokio::test(start_paused = true)]
async fn a_parked_seed_without_the_slot_waits_without_spinning() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("parked on its seed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    rig.rereads.closed();
    *rig.sink.retire_blocked.lock().unwrap() = false;
    let before = *rig.heard.probes.lock().unwrap();
    for n in 0..200u64 {
        rig.live(&orders(), i64::try_from(n).unwrap() + 10, n + 10)
            .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let probes = *rig.heard.probes.lock().unwrap() - before;
    assert!(probes <= 2, "{probes} probes while no slot is held");
    eventually("every orders item written", || {
        let ids = rig.sink.ids.lock().unwrap();
        (10..210).all(|n| ids.contains(&format!("orders/{n}")))
    })
    .await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "nothing seeded unheld"
    );
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("the seed lands once held", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    task.abort();
}

/// Stage a run of `engine` that read its records under a stream, then end
/// that stream and stage again over `record`, another process's, with no
/// slot held: the staging's result.
async fn staged_over_without_the_slot(
    rig: &Rig,
    engine: &Arc<SinkEngine>,
    record: Detachment,
) -> crate::Result<()> {
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(engine);
    eventually("read under the stream", || read_all(rig)).await;
    task.abort();
    let _ = task.await;
    rig.rereads.closed();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    engine.stage(Continuity::Resumed).await
}

/// The seventh re-check's B-1 (probe P1), under the cut: a record another
/// process wrote while this one held no slot stops the deployment at the
/// next read, before anything of the lane is taken as detached or dropped,
/// whatever that process does with it afterwards.
#[tokio::test(start_paused = true)]
async fn a_record_another_process_wrote_without_this_ones_slot_stops_the_staging() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    assert!(stopped(
        &staged_over_without_the_slot(&rig, &engine, record).await
    ));
    assert!(!engine.lane_health(&lane(users())).is_detached());
    assert!(rig.sink.detaches().is_empty(), "nothing recorded");
    assert!(rig.sink.demoted.lock().unwrap().is_empty());
}

/// The same on a sink with `backfill = false`.
#[tokio::test(start_paused = true)]
async fn a_record_another_process_wrote_without_this_ones_slot_stops_a_live_only_staging() {
    let rig = Rig::new(64);
    let engine = rig.live_only_engine();
    assert!(stopped(
        &staged_over_without_the_slot(&rig, &engine, detached_record()).await
    ));
    assert!(!engine.lane_health(&lane(users())).is_detached());
    assert!(rig.sink.detaches().is_empty(), "nothing recorded");
}

/// Round 4's m-3, under the cut: a lane this process detached, whose record
/// another process then removed (a reattach, or a rebuild it finished),
/// stops the deployment at the read once this process holds the slot again:
/// nothing recorded over it, and nothing this process dropped since the
/// opening confirmed (its read uncounted).
#[tokio::test(start_paused = true)]
async fn a_lane_held_detached_whose_record_another_process_removed_stops_the_deployment() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    task.abort();
    let _ = task.await;
    assert_eq!(rig.sink.detaches().len(), 1);
    rig.rereads.closed();
    rig.sink.kept.lock().unwrap().remove("users");
    let (task, _control) = spawn(&engine);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    stops(task).await;
    assert_eq!(rig.sink.detaches().len(), 1, "nothing recorded again");
    assert!(!rig.rereads.current(&lane(users())));
}

/// The eighth re-check, B-2, under the cut: a lane held detached whose
/// record another process removed (its reattach) is never reattached here
/// as if the missing record were this process's lost delete. Asked to
/// reattach while this process holds no slot, the deployment stops, and
/// nothing of the index is ensured or attached.
#[tokio::test(start_paused = true)]
async fn a_reattach_asked_over_a_record_another_process_removed_stops_the_deployment() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, control) = detached_live_only(&rig).await;
    rig.rereads.closed();
    rig.sink.kept.lock().unwrap().remove("users");
    let ensured = rig.sink.ensured.lock().unwrap().len();
    control.send(reattach(users())).await.unwrap();
    stops(task).await;
    assert_eq!(
        rig.sink.ensured.lock().unwrap().len(),
        ensured,
        "nothing of the index ensured"
    );
    assert!(engine.lane_health(&lane(users())).is_detached());
    assert!(rig.sink.gaps.lock().unwrap().is_empty(), "no gap kept");
}

/// The review of the cut, m-2: once a run ended cleanly, erasing writes may
/// go on as they do at a stop on `main` (`Rereads::finished`), but no slot is
/// held any more: a read then is judged, never adopted as if under a held
/// opening, so another process's change since stops it.
#[tokio::test(start_paused = true)]
async fn a_read_after_a_clean_end_judges_another_process_s_change() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    eventually("read under the stream", || read_all(&rig)).await;
    task.abort();
    let _ = task.await;
    rig.rereads.closed();
    rig.rereads.finished();
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), detached_record());
    assert!(stopped(&engine.stage(Continuity::Resumed).await));
}

/// Whether `result`, a run's or a staging's, is the deployment's stop for
/// another process's change to the records (the cut of 2026-11-27).
fn stopped<T: std::fmt::Debug>(result: &crate::Result<T>) -> bool {
    matches!(result, Err(crate::EngineError::ForeignChange { .. }))
}

/// `task`, a run, ends as the deployment's stop for another process's
/// change to the records.
async fn stops(task: tokio::task::JoinHandle<crate::Result<()>>) {
    let ended = tokio::time::timeout(Duration::from_secs(60), task).await;
    let Ok(Ok(result)) = ended else {
        panic!("the run never ended as a stop: {ended:?}");
    };
    assert!(stopped(&result), "{result:?}");
}

/// Found by the world in the seventh re-check, under the cut: a gap another
/// process kept over an index while this process held no slot stops the
/// deployment at the read once the slot is held, so the backfill asked
/// before it never seeds over it.
#[tokio::test(start_paused = true)]
async fn a_gap_written_since_a_backfill_was_asked_stops_the_deployment() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("users".to_owned(), vec![gap(Some("1"), Some("2"))]);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    stops(task).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "the backfill seeded over the gap"
    );
}

/// Found by the world in the seventh re-check, under the cut: a rebuild this
/// process took over, whose record another process removed (it finished
/// it), stops the deployment at the next read: no later rebuild under the
/// same attempt count can be taken for this one's.
#[tokio::test(start_paused = true)]
async fn a_rebuild_whose_record_another_process_removed_stops_the_deployment() {
    let rig = Rig::new(64);
    rig.rereads.will_open();
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    let (task, _control) = spawn(&engine);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("read under the stream", || read_all(&rig)).await;
    rig.rereads.closed();
    rig.sink.kept.lock().unwrap().remove("users");
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    stops(task).await;
    assert!(rig.sink.demoted.lock().unwrap().is_empty());
}

/// The seventh re-check, m-2: an owed seed whose index's target changed
/// before the reopen (a write found it gone and restaged it) never lands:
/// the restage cut its backfill and asked a fresh one, which drops it. Once
/// held, nothing seeds until the fresh backfill completes.
#[tokio::test(start_paused = true)]
async fn an_owed_seed_whose_target_changed_meanwhile_never_lands() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let (asked, asks) = backfill_asks(&rig);
    rig.sink.missing.lock().unwrap().insert("users".to_owned());
    rig.live(&users(), 9, 9).await;
    eventually("a fresh backfill is asked", || {
        asked.lock().unwrap().iter().any(|request| {
            matches!(request, stream::Request::Backfill { index, after: None, .. } if *index == users())
        })
    })
    .await;
    rig.sink.missing.lock().unwrap().clear();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "an owed seed of a cut backfill landed"
    );
    task.abort();
    asks.abort();
}

/// The seventh re-check, m-3: an owed seed refused because the old
/// generation is blocked (#479) parks its lane `SeedBlocked`, as a seed's
/// completion does, so `/status` says why the index stays backfilling; the
/// owed seed is the probe, and lands once the block lifts.
#[tokio::test(start_paused = true)]
async fn an_owed_seed_meeting_a_blocked_old_generation_parks_its_lane() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("parked on the owed seed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked {
                reason: crate::ParkReason::SeedBlocked { .. },
                ..
            }
        )
    })
    .await;
    *rig.sink.retire_blocked.lock().unwrap() = false;
    eventually("the owed seed lands", || {
        rig.sink
            .seeded
            .lock()
            .unwrap()
            .contains(&"users".to_owned())
    })
    .await;
    eventually("the lane opens", || {
        engine.lane_health(&lane(users())).is_open()
    })
    .await;
    task.abort();
}

/// The eighth re-check, S-1: a rebuild taken over at a process start
/// resumes its walk from its checkpoint. Only a gap kept over the index
/// sends it back to the first key; its own `rebuilding` record doesn't.
#[tokio::test(start_paused = true)]
async fn a_rebuild_taken_over_at_a_start_resumes_its_walk() {
    let rig = Rig::new(64);
    let mut record = Detachment::recorded(
        DetachRecord::Rebuilding { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.attempts = Some(1);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.sink
        .walked
        .lock()
        .unwrap()
        .insert("users".to_owned(), "k500".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    assert_eq!(engine.lane_state(&lane(users())).rebuilding, Some(1));
    let stream::Request::Backfill { after, .. } = next_request(&rig).await else {
        panic!("a backfill is asked");
    };
    assert!(after.is_some(), "walked from the first key");
}

/// The eighth re-check, S-2: a lane held detached is asked no backfill at
/// the next opening, though the backfill outstanding at its detach was cut.
#[tokio::test(start_paused = true)]
async fn a_detached_lane_is_asked_no_backfill_at_the_next_opening() {
    let rig = Rig::new(64);
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, control) = spawn(&engine);
    let stream::Request::Backfill { .. } = next_request(&rig).await else {
        panic!("a backfill is asked at staging");
    };
    eventually("read under the stream", || read_all(&rig)).await;
    rig.sink.block(&users(), true);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the lane detaches", || {
        engine.lane_health(&lane(users())).is_detached()
    })
    .await;
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let asked = tokio::time::timeout(Duration::from_secs(2), next_request(&rig)).await;
    assert!(
        asked.is_err(),
        "a detached lane was asked a backfill: {:?}",
        asked.map(|request| format!("{request:?}"))
    );
    task.abort();
}

/// The eighth re-check, S-3 (a): a stop while no slot is held drains a lane
/// parked on a seed that waits for the slot, as `main` does: the
/// completion is acknowledged unseeded, and the next start backfills.
#[tokio::test(start_paused = true)]
async fn a_stop_drains_a_lane_parked_on_a_seed_waiting_for_the_slot() {
    let rig = Rig::new(64);
    let stop = crate::Stop::new();
    let engine = Arc::new(
        rig.built("primary", Arc::clone(&rig.sink))
            .with_stop(stop.clone()),
    );
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("parked on its seed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    rig.rereads.closed();
    *rig.sink.retire_blocked.lock().unwrap() = false;
    tokio::time::sleep(Duration::from_secs(5)).await;
    stop.request();
    eventually("drained", || rig.stream.is_idle()).await;
    assert!(rig.sink.seeded.lock().unwrap().is_empty(), "seeded unheld");
    task.abort();
}

/// The eighth re-check, S-3 (b): a stop with the slot held drains a lane
/// parked on an owed seed that its blocked old generation refuses (#479):
/// the owed seed is dropped unseeded, and what is behind it is written.
#[tokio::test(start_paused = true)]
async fn a_stop_drains_a_lane_parked_on_a_blocked_owed_seed() {
    let rig = Rig::new(64);
    let stop = crate::Stop::new();
    let engine = Arc::new(
        rig.built("primary", Arc::clone(&rig.sink))
            .with_stop(stop.clone()),
    );
    let (ticket, task) = backfilling_under_a_held_stream(&rig, &engine).await;
    rig.rereads.closed();
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    *rig.sink.retire_blocked.lock().unwrap() = true;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    eventually("parked on the owed seed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked {
                reason: crate::ParkReason::SeedBlocked { .. },
                ..
            }
        )
    })
    .await;
    rig.live(&users(), 7, 7).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    stop.request();
    eventually("drained", || rig.stream.is_idle()).await;
    assert!(written(&rig).contains(&"users/7".to_owned()));
    task.abort();
}

/// The review of the cut, B-1: a rebuild's seed lands but the adapter's
/// best-effort delete of the finished record fails (`mark_seeded` returns
/// `Ok`, as the OpenSearch sink does). The seed's read back finds the
/// record and deletes it at once, as a staging would; a lone process's next
/// opening doesn't stop it.
#[tokio::test(start_paused = true)]
async fn a_seed_whose_record_delete_failed_deletes_it_and_never_stops_one_process() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    *rig.sink.seed_keeps_record.lock().unwrap() = true;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!("backfill");
    };
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the lane is current", || {
        engine.lane_state(&lane(users())).rebuilding.is_none()
    })
    .await;
    eventually("the finished record is deleted", || {
        rig.sink.kept.lock().unwrap().get("users").is_none()
    })
    .await;
    // A routine reconnect of the replication stream.
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "one process stopped: {:?}", task.await);
    task.abort();
}

/// The review of round 13, m-7: the record the seed's failed delete left is
/// one this version no longer writes, `rebuilding` and live only (an older
/// flusso's rebuild of a live-only record). The seed's read back finds it
/// beside the seeded newer marker and forgets it at once, without waiting
/// for a staging.
#[tokio::test(start_paused = true)]
async fn a_seed_forgets_a_live_only_rebuilding_record_its_delete_left() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    if let Some(kept) = rig.sink.kept.lock().unwrap().get_mut("users") {
        kept.record = Some(DetachRecord::Rebuilding { live_only: true });
    }
    *rig.sink.seed_keeps_record.lock().unwrap() = true;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!("backfill");
    };
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("the lane is current", || {
        engine.lane_state(&lane(users())).rebuilding.is_none()
    })
    .await;
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "the seed's read back left {:?}",
        rig.sink.kept.lock().unwrap().get("users")
    );
    task.abort();
}

/// The review of the cut, B-2: `begin_rebuild` lands, its call fails (the
/// response lost, e.g. this host's network dropped, which also ends the
/// replication stream). A lone process's next opening doesn't stop it.
#[tokio::test(start_paused = true)]
async fn a_rebuild_that_landed_unseen_never_stops_one_process() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    *rig.sink.rebuild_lands_then_fails.lock().unwrap() = true;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("the rebuild landed", || {
        !rig.sink.rebuilds.lock().unwrap().is_empty()
    })
    .await;
    // The stream ends with the same network drop, and comes back.
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "one process stopped: {:?}", task.await);
    task.abort();
}

/// The review of the cut, S-2 (the eighth re-check's B-1). An owed seed
/// of `orders`; an opening returns and the read again of `orders` takes its
/// answer, then is slow to return; the stream ends, another process keeps a
/// gap, the next opening returns; the read returns the old answer (counted
/// for the earlier opening). The seed must not land over the gap.
#[tokio::test(start_paused = true)]
async fn an_owed_seed_never_lands_on_a_read_answered_under_an_earlier_stream() {
    let rig = Rig::new(64);
    let engine = rig.engine();
    rig.sink
        .unseeded
        .lock()
        .unwrap()
        .insert("orders".to_owned());
    engine.stage(Continuity::Resumed).await.unwrap();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (task, _control) = spawn(&engine);
    let stream::Request::Backfill { ticket, index, .. } = next_request(&rig).await else {
        panic!("a backfill is asked at staging");
    };
    assert_eq!(index, orders());
    eventually("read under the stream", || read_all(&rig)).await;
    rig.rereads.closed();
    assert!(rig.rereads.open(async { Err::<(), ()>(()) }).await.is_err());
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: orders(),
            ticket,
        },
    )
    .await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(rig.sink.seeded.lock().unwrap().is_empty(), "owed");
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    *rig.sink.read_first.lock().unwrap() = true;
    *rig.sink.read_gate.lock().unwrap() = Some(Arc::clone(&gate));
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    rig.rereads.closed();
    rig.sink
        .marker_gaps
        .lock()
        .unwrap()
        .insert("orders".to_owned(), vec![gap(Some("1"), Some("2"))]);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    *rig.sink.read_first.lock().unwrap() = false;
    *rig.sink.read_gate.lock().unwrap() = None;
    gate.add_permits(1000);
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(
        rig.sink.seeded.lock().unwrap().is_empty(),
        "the owed seed landed over the gap another process kept"
    );
    task.abort();
}

/// The review of the cut: a fresh process (empty `known`) stages over what
/// B-1's and B-2's lost outcomes leave and doesn't stop.
#[tokio::test(start_paused = true)]
async fn a_fresh_process_converges_over_lost_outcomes() {
    // B-1's leftover: a finished rebuild's record with the marker seeded on the newer generation.
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, _control, attempt) = rebuilding_on_reindex(&rig).await;
    *rig.sink.seed_keeps_record.lock().unwrap() = true;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let stream::Request::Backfill { ticket, .. } = next_request(&rig).await else {
        panic!()
    };
    publish(
        &rig,
        LaneItem::SnapshotComplete {
            index: users(),
            ticket,
        },
    )
    .await;
    eventually("current", || {
        engine.lane_state(&lane(users())).rebuilding.is_none()
    })
    .await;
    task.abort();
    let _ = task.await;
    let fresh = rig.engine();
    fresh
        .stage(Continuity::Resumed)
        .await
        .expect("a fresh process stages");
    let (task, _c) = spawn(&fresh);
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "{:?}", task.await);
    assert!(
        rig.sink.kept.lock().unwrap().get("users").is_none(),
        "the finished record deleted"
    );
    assert!(fresh.lane_health(&lane(users())).is_open());
    task.abort();

    // B-2's leftover: a rebuild record written, its call lost.
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    *rig.sink.rebuild_lands_then_fails.lock().unwrap() = true;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    eventually("landed", || !rig.sink.rebuilds.lock().unwrap().is_empty()).await;
    task.abort();
    let _ = task.await;
    let fresh = rig.engine();
    fresh
        .stage(Continuity::Resumed)
        .await
        .expect("a fresh process stages");
    let (task, _c) = spawn(&fresh);
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "{:?}", task.await);
    task.abort();
}

/// The review of round 10, B-1: O20's restart detaches a parked rebuild with
/// no `through`; the adapter keeps the record's. The detach returns `Ok`
/// while the stream ends during the call, so no read back. One process, no
/// other: the next opening doesn't stop it.
#[tokio::test(start_paused = true)]
async fn a_rebuild_restart_detach_with_no_through_never_stops_one_process() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task, control, attempt) = rebuilding_on_reindex(&rig).await;
    publish(&rig, LaneItem::Attached { attempt }).await;
    let _backfill = next_request(&rig).await;
    rig.sink.block(&users(), true);
    rig.live(&users(), 2, 2).await;
    eventually("the rebuilding lane parks", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Parked { .. }
        )
    })
    .await;
    *rig.sink.detach_takes.lock().unwrap() = Some(Duration::from_secs(5));
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    // The stream ends while the detach is in flight.
    rig.rereads.closed();
    tokio::time::sleep(Duration::from_secs(10)).await;
    *rig.sink.detach_takes.lock().unwrap() = None;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "one process stopped: {:?}", task.await);
    task.abort();
}

/// The review of round 10, S-2: on a `backfill = false` sink, staging finds a record
/// written while the sink could rebuild and demotes it to live only first.
/// The demotion fails having written nothing. Its read back finds the record
/// `Detached { live_only: false }`, as before the call: that is not the
/// demotion having landed.
#[tokio::test(start_paused = true)]
async fn a_failed_live_only_demotion_is_not_taken_as_done() {
    let rig = Rig::new(64);
    let mut record = Detachment::recorded(
        DetachRecord::Detached { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.through = Some("3".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    *rig.sink.demote_fails.lock().unwrap() = true;
    let engine = rig.live_only_engine();
    let _ = engine.stage(Continuity::Resumed).await;
    let kept = rig.sink.kept.lock().unwrap().get("users").cloned().unwrap();
    let health = engine.lane_health(&lane(users()));
    assert!(
        !(health.is_detached() && kept.record == Some(DetachRecord::Detached { live_only: false })),
        "the failed demotion was taken as done: the lane is detached under a record that still says it can rebuild"
    );
}

/// The review of round 10, S-1: an owed rebuild retried for hours while the
/// sink refuses it, each try's read back failing too. Nothing of those tries
/// landed: the records are as first read, and a reconnect doesn't stop one
/// process.
#[tokio::test(start_paused = true)]
async fn many_failed_rebuild_tries_never_stop_one_process() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let (engine, task) = detached_on_a_block(&rig).await;
    rig.sink.block(&users(), false);
    task.abort();
    let _ = task.await;
    rig.sink.dead.lock().unwrap().insert("begin_rebuild");
    *rig.sink.reads_fail.lock().unwrap() = 1_000_000;
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Reindex {
            indexes: vec![users()],
        })
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_secs(2 * 3600)).await;
    rig.sink.dead.lock().unwrap().clear();
    *rig.sink.reads_fail.lock().unwrap() = 0;
    rig.rereads.closed();
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    tokio::time::sleep(Duration::from_secs(30)).await;
    assert!(!task.is_finished(), "one process stopped: {:?}", task.await);
    task.abort();
}

/// The review of round 10, B-2, held to the letter (the review of round 11,
/// S-5): this process's detach of `users` fails before it writes; while it
/// doesn't hold the slot, another process detaches the lane from the same
/// place, its record stamped by its own writer. Taken for this process's
/// own, the clear would forget it and the lane would open over the other's
/// drop. It is foreign: the deployment stops, and nothing is forgotten.
#[tokio::test(start_paused = true)]
async fn anothers_detach_from_the_same_place_is_never_taken_for_ours() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.block(&users(), true);
    rig.sink.records.lock().unwrap().push_back(Record::Fail);
    *rig.sink.detach_takes.lock().unwrap() = Some(Duration::from_secs(5));
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    control
        .send(SinkControl::Detach { index: users() })
        .await
        .unwrap();
    rig.live(&users(), 1, 1).await;
    eventually("the detach is sent", || !rig.sink.detaches().is_empty()).await;
    rig.rereads.closed();
    let through = rig.sink.detaches()[0].1.through.clone();
    let mut theirs = Detachment::recorded(
        DetachRecord::Detached { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        Some(RecordVersion::new("42:1")),
    );
    theirs.through = through;
    theirs.record_stamp = Some(sink::Stamp::new("another process", 1));
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), theirs);
    tokio::time::sleep(Duration::from_secs(10)).await;
    *rig.sink.detach_takes.lock().unwrap() = None;
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.block(&users(), false);
    tokio::time::sleep(Duration::from_secs(60)).await;
    let forgotten = rig.sink.forgotten.lock().unwrap().clone();
    let health = engine.lane_health(&lane(users()));
    assert!(forgotten.is_empty(), "{forgotten:?}");
    assert!(
        task.is_finished(),
        "rule 2: another process's same-place detach must stop the deployment; lane {health:?}"
    );
    assert!(stopped(&task.await.unwrap()));
}

/// The review of round 11, S-5: the failed-detach clear forgets only a
/// record this process wrote. Over a record another writer stamped, adopted
/// at the staging (nothing was known), the clear leaves it as it is.
#[tokio::test(start_paused = true)]
async fn the_clear_forgets_no_record_another_writer_stamped() {
    let rig = Rig::new(64);
    let mut theirs = Detachment::recorded(
        DetachRecord::Detached { live_only: false },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    by_another_process(&mut theirs);
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), theirs.clone());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    engine.clear_failed_detach(&users()).await.unwrap();
    assert!(rig.sink.forgotten.lock().unwrap().is_empty());
    assert_eq!(
        rig.sink
            .kept
            .lock()
            .unwrap()
            .get("users")
            .map(|kept| kept.record.clone()),
        Some(theirs.record)
    );
}

/// The review of the closing round, D: a live-only record beside a marker an
/// earlier detach moved (the world's
/// `a_live_only_detach_over_a_moved_marker_leaves_it_moved` state) on a
/// `backfill = false` sink. A start puts the record back live only, the
/// marker on the generation reads see, and the reattach goes through: what
/// the refusal before it tells the operator to do.
#[tokio::test(start_paused = true)]
async fn a_live_only_record_beside_a_moved_marker_reattaches_after_a_start() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    let mut record = Detachment::recorded(
        DetachRecord::Detached { live_only: true },
        Some(MarkerAt::Newer { seeded: false }),
        version(),
    );
    record.through = Some("1".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    let (reply, verdict) = crate::Reply::new();
    control
        .send(SinkControl::Reattach {
            index: users(),
            reply,
        })
        .await
        .unwrap();
    let verdict = tokio::time::timeout(Duration::from_secs(10), verdict)
        .await
        .expect("a verdict")
        .expect("answered");
    task.abort();
    let _ = task.await;
    let demoted = rig.sink.demoted.lock().unwrap().clone();
    assert_eq!(demoted, [("users".to_owned(), true)]);
    assert!(
        format!("{verdict:?}").contains("Started"),
        "after a start: {verdict:?}"
    );
}

/// The way out of the review of the closing round: a `backfill = false` sink
/// never seeds its index, so a live-only record beside the marker on the
/// generation reads see, unseeded, is its normal detached state. The
/// reattach takes it.
#[tokio::test(start_paused = true)]
async fn a_live_only_record_beside_an_unseeded_marker_on_serving_reattaches() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let mut record = Detachment::recorded(
        DetachRecord::Detached { live_only: true },
        Some(MarkerAt::Serving { seeded: false }),
        version(),
    );
    record.through = Some("1".to_owned());
    rig.sink
        .kept
        .lock()
        .unwrap()
        .insert("users".to_owned(), record);
    let engine = rig.live_only_engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, control) = spawn(&engine);
    let (reply, verdict) = crate::Reply::new();
    control
        .send(SinkControl::Reattach {
            index: users(),
            reply,
        })
        .await
        .unwrap();
    let verdict = tokio::time::timeout(Duration::from_secs(10), verdict)
        .await
        .expect("a verdict")
        .expect("answered");
    task.abort();
    let _ = task.await;
    assert!(format!("{verdict:?}").contains("Started"), "{verdict:?}");
}

/// The review of round 14, B-1: on a sink that can backfill, an index whose
/// marker keeps a gap on the generation reads see is never backfilled in
/// place (rows deleted in the gap keep their documents there, and the seed
/// would drop the gap): it is rebuilt into a new generation first, as a
/// reindex does, and its backfill asked only then.
#[tokio::test(start_paused = true)]
async fn a_backfill_over_a_gap_rebuilds_into_a_new_generation_first() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.sink.marker_gaps.lock().unwrap().insert(
        "users".to_owned(),
        vec![sink::Gap {
            from: Some("1".to_owned()),
            to: Some("2".to_owned()),
        }],
    );
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, _control) = spawn(&engine);
    let stream::Request::Backfill { index, .. } = next_request(&rig).await else {
        panic!("a backfill");
    };
    assert_eq!(index, users());
    assert_eq!(
        *rig.sink.reindexed.lock().unwrap(),
        ["users".to_owned()],
        "the backfill was asked before a new generation was staged"
    );
    task.abort();
}

/// B-1's control: an index that keeps no gap (a fresh index, or one whose
/// generation was deleted and is reseeded) is backfilled in place, as on
/// `main`.
#[tokio::test(start_paused = true)]
async fn an_unseeded_index_with_no_gap_is_backfilled_in_place() {
    let rig = Rig::new(64);
    rig.rereads.open(async { Ok::<(), ()>(()) }).await.unwrap();
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, _control) = spawn(&engine);
    let stream::Request::Backfill { index, .. } = next_request(&rig).await else {
        panic!("a backfill");
    };
    assert_eq!(index, users());
    assert!(rig.sink.reindexed.lock().unwrap().is_empty());
    task.abort();
}

/// B-1's rebuild waits for the slot like any reindex: until then the lane
/// restages, and its reason says a rebuild over the gap is under way.
#[tokio::test(start_paused = true)]
async fn a_rebuild_over_a_gap_waiting_for_the_slot_says_why() {
    let rig = Rig::new(64);
    rig.rereads.will_open();
    rig.sink.unseeded.lock().unwrap().insert("users".to_owned());
    rig.sink.marker_gaps.lock().unwrap().insert(
        "users".to_owned(),
        vec![sink::Gap {
            from: Some("1".to_owned()),
            to: Some("2".to_owned()),
        }],
    );
    let engine = rig.engine();
    engine.stage(Continuity::Resumed).await.unwrap();
    let (task, _control) = spawn(&engine);
    eventually("the rebuild is owed", || {
        matches!(
            engine.lane_health(&lane(users())),
            LaneHealth::Restaging { .. }
        )
    })
    .await;
    let health = engine.lane_health(&lane(users()));
    assert!(
        format!("{health:?}").contains("rebuilt into a new generation"),
        "{health:?}"
    );
    assert!(rig.sink.reindexed.lock().unwrap().is_empty());
    task.abort();
}
