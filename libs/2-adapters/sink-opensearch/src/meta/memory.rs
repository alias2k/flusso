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

use std::collections::BTreeMap;

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
    /// Each index's generations, each with the `index.uuid` OpenSearch gave
    /// it at its creation.
    generations: BTreeMap<String, BTreeMap<u64, String>>,
    /// The uuids made so far: each creation takes the next.
    made: u64,
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
            .map(|made| made.keys().copied().collect())
            .unwrap_or_default()
    }

    /// `generation` of `alias` created, under a uuid no index had before.
    fn create(&mut self, alias: &str, generation: u64) -> String {
        self.made += 1;
        let uuid = format!("uuid-{}", self.made);
        self.generations
            .entry(alias.to_owned())
            .or_default()
            .insert(generation, uuid.clone());
        uuid
    }

    /// The adapter's `record_uuid`: `uuid` recorded on the marker only while
    /// it still names `generation` and records no uuid (m-B); else nothing,
    /// and the next ensure records the live uuid of what the marker names.
    fn record_uuid(&mut self, alias: &str, generation: u64, uuid: &str) {
        let names = self.active(alias) == Some(generation);
        let recorded = self
            .doc(alias)
            .and_then(|marker| marker.get("uuid"))
            .is_some_and(|uuid| !uuid.is_null());
        if names && !recorded {
            self.update_marker(alias, "uuid", Value::from(uuid));
        }
    }

    /// A partial `_update` of one field of the marker: a new version, the
    /// stamp kept.
    fn update_marker(&mut self, alias: &str, key: &str, value: Value) {
        if let Some((doc, _)) = self.docs.get(alias).cloned() {
            let mut doc = doc;
            if let Some(fields) = doc.as_object_mut() {
                fields.insert(key.to_owned(), value);
            }
            self.seq += 1;
            self.docs.insert(alias.to_owned(), (doc, self.seq));
        }
    }

    /// Delete `generation` of `alias`; the hash alias goes with the
    /// generation it was on, as OpenSearch drops an index's aliases with it.
    fn remove(&mut self, alias: &str, generation: u64) {
        if let Some(made) = self.generations.get_mut(alias) {
            made.remove(&generation);
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

    /// `ensure_index` as the sink stages it (`ensure_generation`): the
    /// generation the marker names, or with no marker the one the alias holds,
    /// else one past every generation. An unseeded generation whose uuid is
    /// not the one the marker recorded is not the one flusso created: it is
    /// deleted and created again (a seeded one is kept). A generation that
    /// doesn't exist is created, its marker rewritten first (gaps and
    /// `detaching_from` carried) and its uuid recorded after; a marker naming
    /// a generation that's gone loses `seeded`. The hash alias moves to the
    /// generation when the marker says seeded, or when nothing serves yet.
    pub fn ensure(&mut self, alias: &str, stamp: &Stamp) -> Ensured {
        self.ensure_with(alias, stamp, |_| {})
    }

    /// [`ensure`](Self::ensure), with `between` run after the generation is
    /// created and before its uuid is recorded: another writer's write landing
    /// in that window, as it can on OpenSearch.
    pub fn ensure_with(
        &mut self,
        alias: &str,
        stamp: &Stamp,
        between: impl FnOnce(&mut Self),
    ) -> Ensured {
        let marker = self.doc(alias).cloned();
        let generations = self.generations_of(alias);
        let (generation, mut seeded, recorded) = match &marker {
            Some(marker) => (
                marker
                    .get("active_generation")
                    .and_then(Value::as_u64)
                    .unwrap_or(1),
                marker
                    .get("seeded")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                marker
                    .get("uuid")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            ),
            None => (
                self.serving
                    .get(alias)
                    .copied()
                    .unwrap_or_else(|| generations.iter().max().map_or(1, |highest| highest + 1)),
                false,
                None,
            ),
        };
        let mut live = self.uuid(alias, generation).map(str::to_owned);
        if let (Some(found), Some(made)) = (&live, &recorded)
            && found != made
            && !seeded
        {
            self.remove(alias, generation);
            live = None;
        }
        let exists = live.is_some();
        if !exists {
            seeded = false;
        }
        let first = self.serving.get(alias).is_none();
        if !exists {
            self.put(
                alias,
                meta::rewritten(marker.as_ref(), generation, None),
                Some(stamp),
            );
            let uuid = self.create(alias, generation);
            if first {
                self.serving.insert(alias.to_owned(), generation);
            }
            between(self);
            self.record_uuid(alias, generation, &uuid);
            live = Some(uuid);
        } else if marker.is_none() {
            self.put(
                alias,
                meta::rewritten(None, generation, live.as_deref()),
                Some(stamp),
            );
        } else if recorded.is_none()
            && let Some(uuid) = live.clone()
        {
            self.record_uuid(alias, generation, &uuid);
        }
        if seeded || first {
            self.serving.insert(alias.to_owned(), generation);
        }
        let target = live.map(TargetId::new);
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
        let uuid = self.uuid(alias, active).map(str::to_owned);
        let in_place = self.serving.get(alias) == Some(&active);
        let current = self.doc(alias).cloned();
        self.serving.insert(alias.to_owned(), active);
        if let Some(made) = self.generations.get_mut(alias) {
            made.retain(|generation, _| *generation == active);
        }
        self.put(
            alias,
            meta::seeded(current.as_ref(), active, uuid.as_deref(), in_place),
            Some(stamp),
        );
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

    /// `reindex`: a seeded index's marker, or one keeping a gap on the
    /// generation the alias serves (`meta::reindex_moves`), moves, written
    /// whole, to a generation past every one, unseeded, for the next ensure
    /// to create; its gaps and `detaching_from` kept.
    pub fn reindex(&mut self, alias: &str, stamp: &Stamp) {
        let serving = self.serving.get(alias).copied();
        if !self.is_seeded(alias) && !meta::reindex_moves(self.doc(alias), serving) {
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
        self.uuid(alias, generation).is_some()
    }

    /// The `index.uuid` of `generation` of `alias`, if it exists.
    pub fn uuid(&self, alias: &str, generation: u64) -> Option<&str> {
        self.generations
            .get(alias)?
            .get(&generation)
            .map(String::as_str)
    }

    /// The uuid of every generation of `alias` that exists now.
    pub fn uuids(&self, alias: &str) -> Vec<String> {
        self.generations
            .get(alias)
            .map(|made| made.values().cloned().collect())
            .unwrap_or_default()
    }

    /// The generation the hash alias serves, with its uuid: what reads see.
    pub fn read_by(&self, alias: &str) -> Option<(u64, String)> {
        let generation = self.serving(alias)?;
        Some((generation, self.uuid(alias, generation)?.to_owned()))
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use serde_json::json;
    use sink::Stamp;

    use super::*;

    fn stamp() -> Stamp {
        Stamp::new("w", 1)
    }

    /// An index seeded on generation 1, its marker then keeping a gap, and
    /// put back unseeded on it (a live-only demotion over a reindex's moved
    /// marker leaves it so).
    fn unseeded_on_serving_with_a_gap() -> MemoryMeta {
        let mut meta = MemoryMeta::default();
        meta.ensure("users", &stamp());
        meta.seed("users", &stamp(), true).unwrap();
        meta.update_marker("users", "seeded", Value::from(false));
        meta.update_marker("users", "gaps", json!([{ "from": "10", "to": "20" }]));
        meta
    }

    /// The review of round 14, B-1: a reindex of an index whose marker keeps
    /// a gap on the generation reads see moves it to a new generation, even
    /// unseeded: only a generation created after the gap may be seeded over
    /// it.
    #[test]
    fn a_reindex_over_a_gap_on_serving_stages_a_new_generation() {
        let mut meta = unseeded_on_serving_with_a_gap();
        meta.reindex("users", &stamp());
        assert_eq!(meta.active("users"), Some(2));
        assert_eq!(meta.serving("users"), Some(1));
        let gaps = meta::read(meta.documents("users").0, None, None).gaps;
        assert_eq!(gaps.len(), 1, "the gap stays until the seed");
    }

    /// The review of round 14, m-B: another process's live-only demotion
    /// lands between this one creating a generation and recording its uuid.
    /// The uuid must not land on the marker the demotion wrote (on the
    /// serving generation), or the next ensure reads that generation as not
    /// the one flusso created and deletes it.
    #[test]
    fn a_uuid_is_recorded_only_on_the_marker_naming_its_generation() {
        let mut meta = MemoryMeta::default();
        meta.ensure("users", &stamp());
        meta.seed("users", &stamp(), true).unwrap();
        let serving = meta.read_by("users").unwrap();
        // A reindex under way (generation 2 created), then a detach: the
        // demotion below puts the marker back on 1 with no uuid (F).
        meta.reindex("users", &stamp());
        meta.ensure("users", &stamp());
        meta.detach("users", &sink::Detach::new("blocked").stamp(stamp()), "now")
            .unwrap();
        let other = Stamp::new("another process", 1);
        meta.ensure_with("users", &stamp(), |meta| {
            meta.demote("users", true, &other, "now").unwrap();
        });
        let (marker, _) = meta.documents("users");
        let marker = marker.unwrap();
        assert_eq!(marker["active_generation"], 1, "{marker}");
        assert!(
            marker
                .get("uuid")
                .is_none_or(|uuid| uuid == serving.1.as_str()),
            "the created generation's uuid landed on the demoted marker: {marker}"
        );
        meta.ensure("users", &other);
        assert_eq!(meta.read_by("users"), Some(serving));
    }

    /// B-1: a seed of the generation the alias already served keeps the
    /// gaps; a seed of a generation created since drops them.
    #[test]
    fn only_a_seed_of_a_newer_generation_drops_the_gaps() {
        let mut in_place = unseeded_on_serving_with_a_gap();
        in_place.seed("users", &stamp(), true).unwrap();
        let gaps = meta::read(in_place.documents("users").0, None, None).gaps;
        assert_eq!(gaps.len(), 1, "a seed in place keeps the gap");
        let mut rebuilt = unseeded_on_serving_with_a_gap();
        rebuilt.reindex("users", &stamp());
        rebuilt.ensure("users", &stamp());
        rebuilt.seed("users", &stamp(), true).unwrap();
        assert_eq!(rebuilt.serving("users"), Some(2));
        let gaps = meta::read(rebuilt.documents("users").0, None, None).gaps;
        assert!(gaps.is_empty(), "{gaps:?}");
    }
}
