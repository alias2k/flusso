//! An in-memory meta index that runs the sink's own plans ([`crate::meta`]),
//! for the stand-in sinks of the engine's and the daemon's tests (feature
//! `testing`). Every document write is versioned, every whole write takes
//! the caller's stamp, a forget leaves a tombstone, the gaps' write is a
//! partial update: as on OpenSearch, modelled once, here. The generations
//! behind each index and the one its hash alias serves are kept beside the
//! documents. Calls are atomic: the conditional writes a plan makes always
//! find what the plan read.
//!
//! ```
//! use sink_opensearch::meta::memory::MemoryMeta;
//!
//! let mut meta = MemoryMeta::default();
//! let stamp = sink::Stamp::new("w", 1);
//! meta.ensure("users", &stamp);
//! meta.seed("users", &stamp, true).unwrap();
//! assert!(meta.is_seeded("users"));
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use sink::{Detach, Detachment, Ensured, Gap, RecordVersion, Recorded, Stamp, TargetId};

use crate::detachment::stamp_value;
use crate::meta::{self, Kept, Plan, Step};

/// A meta index of several indexes, each named by an `alias` of the
/// caller's choosing.
#[derive(Debug, Clone, Default)]
pub struct MemoryMeta {
    docs: BTreeMap<String, (Value, u64)>,
    seq: u64,
    generations: BTreeMap<String, BTreeSet<u64>>,
    serving: BTreeMap<String, u64>,
    /// The next plan stops after this many of its steps, then fails: a
    /// process that died part way.
    stop_after: Option<usize>,
}

fn record_id(alias: &str) -> String {
    format!("detach:{alias}")
}

impl MemoryMeta {
    fn doc(&self, id: &str) -> Option<&Value> {
        self.docs.get(id).map(|(doc, _)| doc)
    }

    fn put(&mut self, id: &str, mut doc: Value, stamp: Option<&Stamp>) {
        if let Some(fields) = doc.as_object_mut() {
            fields.insert("stamp".to_owned(), stamp_value(stamp));
        }
        self.seq += 1;
        self.docs.insert(id.to_owned(), (doc, self.seq));
    }

    fn generations_of(&self, alias: &str) -> Vec<u64> {
        self.generations
            .get(alias)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Delete `generation` of `alias`; the hash alias goes with the
    /// generation it was on, as OpenSearch drops an index's aliases with it.
    fn remove(&mut self, alias: &str, generation: u64) {
        if let Some(set) = self.generations.get_mut(alias) {
            set.remove(&generation);
        }
        if self.serving.get(alias) == Some(&generation) {
            self.serving.remove(alias);
        }
    }

    /// Run `with` over what a plan for `alias` reads.
    fn with_kept<T>(&self, alias: &str, now: &str, with: impl FnOnce(&Kept<'_>) -> T) -> T {
        let generations = self.generations_of(alias);
        let record = record_id(alias);
        let kept = Kept {
            marker: self.doc(alias),
            record: self.doc(&record),
            serving: self.serving.get(alias).copied(),
            generations: &generations,
            now,
        };
        with(&kept)
    }

    fn apply(&mut self, alias: &str, plan: &Plan, stamp: Option<&Stamp>) -> Result<(), String> {
        let stop_after = self.stop_after.take();
        for (done, step) in plan.steps.iter().enumerate() {
            if stop_after == Some(done) {
                return Err(format!(
                    "the process died after {done} of the plan's writes"
                ));
            }
            match step {
                Step::Delete(generation) => self.remove(alias, *generation),
                Step::Marker(doc) => self.put(alias, doc.clone(), stamp),
                Step::Record(doc) => self.put(&record_id(alias), doc.clone(), stamp),
            }
        }
        for generation in &plan.best_effort {
            self.remove(alias, *generation);
        }
        Ok(())
    }

    /// The next plan stops after `steps` of its writes and fails.
    pub fn die_after(&mut self, steps: usize) {
        self.stop_after = Some(steps);
    }

    /// What a read of `alias`'s records finds, as the sink reads it, with no
    /// put-back: for a test's own judgement.
    pub fn peek(&self, alias: &str) -> Detachment {
        let record = record_id(alias);
        let version = self.docs.get(&record).map(|(_, seq)| format!("{seq}:1"));
        meta::read(self.doc(alias), self.doc(&record), version.as_deref())
    }

    /// The sink's read of `alias`'s records, with its put-back of a marker a
    /// detach left half moved, stamped `stamp`.
    pub fn read(&mut self, alias: &str, stamp: &Stamp) -> Result<Detachment, String> {
        let mut kept = self.peek(alias);
        match self.with_kept(alias, "", meta::put_back) {
            Ok(None) => {}
            Ok(Some((doc, generation))) => {
                kept.gaps = meta::read(Some(&doc), None, None).gaps;
                self.put(alias, doc, Some(stamp));
                kept.restored = Some(generation);
                kept.marker_stamp = Some(stamp.clone());
            }
            Err(why) => kept.unreadable_marker = Some(why),
        }
        Ok(kept)
    }

    /// A detach, as the sink records it.
    pub fn detach(&mut self, alias: &str, detach: &Detach, now: &str) -> Result<Recorded, String> {
        let plan = self.with_kept(alias, now, |kept| meta::detach(kept, detach))?;
        self.apply(alias, &plan, detach.stamp.as_ref())?;
        Ok(Recorded::Kept)
    }

    /// r1 of a rebuild; the attempt.
    pub fn begin_rebuild(&mut self, alias: &str, stamp: &Stamp, now: &str) -> Result<u64, String> {
        let (plan, attempts) = self.with_kept(alias, now, meta::begin_rebuild)?;
        self.apply(alias, &plan, Some(stamp))?;
        Ok(attempts)
    }

    /// A rebuild put back to detached.
    pub fn demote(
        &mut self,
        alias: &str,
        live_only: bool,
        stamp: &Stamp,
        now: &str,
    ) -> Result<(), String> {
        let Some(plan) = self.with_kept(alias, now, |kept| meta::demote(kept, live_only)) else {
            return Ok(());
        };
        self.apply(alias, &plan, Some(stamp))
    }

    /// Forget the record read at `version`: a tombstone, or an error when it
    /// changed since.
    pub fn forget(
        &mut self,
        alias: &str,
        version: &RecordVersion,
        stamp: &Stamp,
    ) -> Result<(), String> {
        let record = record_id(alias);
        let current = self.docs.get(&record).map(|(_, seq)| format!("{seq}:1"));
        if current.as_deref() != Some(version.as_str()) {
            return Err(format!(
                "version conflict: the record of {alias} is at {current:?}, the forget asked for {}",
                version.as_str()
            ));
        }
        self.put(&record, meta::tombstone(), Some(stamp));
        Ok(())
    }

    /// The gaps kept with the marker, changed as `decide` says (a partial
    /// update of the gaps and the stamp).
    pub fn record_gaps(
        &mut self,
        alias: &str,
        decide: &sink::DecideGaps<'_>,
        stamp: &Stamp,
    ) -> Result<Vec<Gap>, String> {
        let Some((marker, _)) = self.docs.get(alias).cloned() else {
            return Err(format!("{alias} has no seed marker"));
        };
        let kept = meta::read(Some(&marker), None, None).gaps;
        let Some(gaps) = decide(&kept) else {
            return Ok(kept);
        };
        let mut updated = marker;
        if let (Some(fields), Some(update)) = (
            updated.as_object_mut(),
            meta::gaps_update(&gaps).as_object(),
        ) {
            for (key, value) in update {
                fields.insert(key.clone(), value.clone());
            }
        }
        self.put(alias, updated, Some(stamp));
        Ok(gaps)
    }

    /// `ensure_index` as the sink stages it: the generation the marker
    /// names, created (with its marker written whole) when it doesn't exist;
    /// the first generation takes the hash alias.
    pub fn ensure(&mut self, alias: &str, stamp: &Stamp) -> Ensured {
        let marker = self.doc(alias).cloned();
        let generations = self.generations_of(alias);
        let generation = match marker
            .as_ref()
            .and_then(|marker| marker.get("active_generation"))
            .and_then(Value::as_u64)
        {
            Some(generation) => generation,
            None => self
                .serving
                .get(alias)
                .copied()
                .unwrap_or_else(|| generations.iter().max().map_or(1, |highest| highest + 1)),
        };
        let exists = generations.contains(&generation);
        if !exists || marker.is_none() {
            self.put(
                alias,
                meta::rewritten(marker.as_ref(), generation, None),
                Some(stamp),
            );
        }
        if !exists {
            self.generations
                .entry(alias.to_owned())
                .or_default()
                .insert(generation);
            self.serving.entry(alias.to_owned()).or_insert(generation);
        }
        let target = Some(TargetId::new(format!("{alias}_{generation}")));
        if exists {
            Ensured::existing(target)
        } else {
            Ensured::created(target)
        }
    }

    /// `mark_seeded`: the hash alias moves to the active generation, every
    /// other goes, the marker is written whole, seeded; the finished
    /// rebuild's record is forgotten, unless `forget_fails` (the sink's
    /// best-effort forget failing).
    pub fn seed(&mut self, alias: &str, stamp: &Stamp, forget: bool) -> Result<(), String> {
        let Some(active) = self.active(alias) else {
            return Err(format!("{alias} has no active generation"));
        };
        if !self.generations_of(alias).contains(&active) {
            return Err(format!("{alias}'s active generation {active} is gone"));
        }
        self.serving.insert(alias.to_owned(), active);
        self.generations
            .insert(alias.to_owned(), BTreeSet::from([active]));
        self.put(alias, meta::whole_marker(active, true, None), Some(stamp));
        let record = record_id(alias);
        if forget
            && self
                .doc(&record)
                .is_some_and(|record| !meta::is_tombstone(record))
        {
            self.put(&record, meta::tombstone(), Some(stamp));
        }
        Ok(())
    }

    /// `reindex`: a seeded index's marker moves, written whole, to a
    /// generation past every one, unseeded, for the next ensure to create;
    /// its gaps and `detaching_from` kept.
    pub fn reindex(&mut self, alias: &str, stamp: &Stamp) {
        if !self.is_seeded(alias) {
            return;
        }
        let next = self
            .generations_of(alias)
            .iter()
            .max()
            .map_or(1, |highest| highest + 1);
        let current = self.doc(alias).cloned();
        self.put(
            alias,
            meta::rewritten(current.as_ref(), next, None),
            Some(stamp),
        );
    }

    /// The walk checkpoint, a partial update keeping the stamp.
    pub fn record_walked(&mut self, alias: &str, mark: Option<&str>) {
        let Some(active) = self.active(alias) else {
            return;
        };
        if let Some((marker, _)) = self.docs.get_mut(alias)
            && let Some(fields) = marker.as_object_mut()
        {
            let walked = mark.map_or(
                Value::Null,
                |mark| json!({ "mark": mark, "generation": active }),
            );
            fields.insert("walked".to_owned(), walked);
        }
    }

    /// The walk checkpoint of the active generation, if any.
    pub fn walked(&self, alias: &str) -> Option<String> {
        let marker = self.doc(alias)?;
        let walked = marker.get("walked")?;
        (walked.get("generation").and_then(Value::as_u64) == self.active(alias))
            .then(|| {
                walked
                    .get("mark")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .flatten()
    }

    /// The generation the marker names.
    pub fn active(&self, alias: &str) -> Option<u64> {
        self.doc(alias)?
            .get("active_generation")
            .and_then(Value::as_u64)
    }

    /// Whether the marker says seeded and its generation exists.
    pub fn is_seeded(&self, alias: &str) -> bool {
        let seeded = self
            .doc(alias)
            .and_then(|marker| marker.get("seeded"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        seeded
            && self
                .active(alias)
                .is_some_and(|active| self.exists(alias, active))
    }

    /// Whether `generation` of `alias` exists.
    pub fn exists(&self, alias: &str, generation: u64) -> bool {
        self.generations
            .get(alias)
            .is_some_and(|set| set.contains(&generation))
    }

    /// The generation the hash alias serves.
    pub fn serving(&self, alias: &str) -> Option<u64> {
        self.serving.get(alias).copied()
    }

    /// A generation gone from under the sink (deleted by hand).
    pub fn delete_generation(&mut self, alias: &str, generation: u64) {
        self.remove(alias, generation);
    }

    /// The marker and the record (or tombstone) as stored, for a test's
    /// judgement.
    pub fn documents(&self, alias: &str) -> (Option<&Value>, Option<&Value>) {
        (self.doc(alias), self.doc(&record_id(alias)))
    }
}
