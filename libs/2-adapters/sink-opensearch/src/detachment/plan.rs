//! What the OpenSearch sink does to its meta documents, as pure functions
//! over the documents' values (the review of the closing round, item 1): the
//! seed marker (`{hash_alias}`), the detach record (`detach:{hash_alias}`,
//! or its tombstone) and the generations behind the hash alias. Each
//! operation reads what is kept and answers a [`Plan`]: the writes to make,
//! in order, and the generations to delete. The sink's HTTP code reads the
//! documents, asks for the plan and sends it, each write conditional on the
//! version read; `meta::memory` (feature `testing`) runs the same plans over an
//! in-memory meta index for the tests of the engine and the daemon, so the
//! stand-ins can never decide differently from the sink.
//!
//! Stamps are not part of a plan: the writer of each document adds the
//! caller's, so no whole write can go out without one.
//!
//! Two rules hold for every plan (the review of the closing round, A and E):
//!
//! - **A gap is dropped only by a seed that completed** (the index then holds
//!   everything): a marker on serving keeps its gaps; a marker moved off it
//!   keeps them in `detaching_from`; every write that rewrites the marker
//!   whole carries them, and only [`whole_marker`], the seed's, has none.
//! - **No plan deletes the generation the hash alias serves**, whatever the
//!   record says it served: a seed since may have moved the alias.
//!
//! ```
//! use serde_json::json;
//! use sink_opensearch::meta::{Kept, Step, detach};
//!
//! let marker = json!({ "active_generation": 3, "seeded": true, "uuid": "u3" });
//! let kept = Kept { marker: Some(&marker), record: None, serving: Some(3), generations: &[3], now: "t" };
//! let plan = detach(&kept, &sink::Detach::new("blocked")).unwrap();
//! assert!(matches!(plan.steps.first(), Some(Step::Marker(_))));
//! ```

use serde_json::{Value, json};
use sink::{Detach, Detachment, RecordVersion};

use super::{
    FORGOTTEN, carried_gaps, gaps_value, generation_of, marker_at, read_gaps, read_record,
    read_stamp,
};

/// What a plan reads: the documents kept and the generations behind the hash
/// alias.
#[derive(Debug, Clone, Copy)]
pub struct Kept<'a> {
    /// The seed marker, as read.
    pub marker: Option<&'a Value>,
    /// The detach record, or its tombstone, as read.
    pub record: Option<&'a Value>,
    /// The generation the hash alias holds, which reads see.
    pub serving: Option<u64>,
    /// Every generation that exists.
    pub generations: &'a [u64],
    /// The wall clock now, as the record's `since` and `last_attempt` keep it.
    pub now: &'a str,
}

/// One write of a plan.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Delete this generation; a refusal fails the plan.
    Delete(u64),
    /// Write the marker whole, conditional on the marker read.
    Marker(Value),
    /// Write the record (or its tombstone) whole, conditional on the record
    /// read.
    Record(Value),
}

/// The writes an operation makes, in order, then the generations it deletes
/// best effort (one left behind is deleted by a later rebuild, R9).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub best_effort: Vec<u64>,
}

impl Kept<'_> {
    /// The record read, unless it is a tombstone.
    fn live_record(&self) -> Option<&Value> {
        self.record.filter(|record| !is_tombstone(record))
    }

    /// Every generation but `keep` and the one the alias serves: what a
    /// rebuild or a demotion deletes.
    fn unserved_but(&self, keep: Option<u64>) -> Vec<u64> {
        let deleted: Vec<u64> = self
            .generations
            .iter()
            .copied()
            .filter(|generation| Some(*generation) != keep && Some(*generation) != self.serving)
            .collect();
        debug_assert!(
            self.serving
                .is_none_or(|serving| !deleted.contains(&serving)),
            "a plan deletes the generation the alias serves"
        );
        deleted
    }

    /// One past every generation left once `deleted` are gone.
    fn next_after(&self, deleted: &[u64]) -> u64 {
        self.generations
            .iter()
            .filter(|generation| !deleted.contains(generation))
            .max()
            .map_or(1, |highest| highest + 1)
    }
}

/// Whether `record` is a forgotten record's tombstone.
pub fn is_tombstone(record: &Value) -> bool {
    record.get("state").and_then(Value::as_str) == Some(FORGOTTEN)
}

/// The documents read, as a [`Detachment`]: the record (a tombstone reads
/// as none, its stamp and version kept), where the marker stands against
/// it, the gaps, both stamps. `version` is the record's as read.
pub fn read(marker: Option<&Value>, record: Option<&Value>, version: Option<&str>) -> Detachment {
    let mut kept = Detachment::default();
    if let Some(found) = record {
        kept.record_stamp = read_stamp(found);
        kept.version = version.map(RecordVersion::new);
    }
    kept.marker_stamp = marker.and_then(read_stamp);
    if let Some(record) = record.filter(|record| !is_tombstone(record)) {
        let serving = record.get("serving").and_then(Value::as_u64);
        kept.marker = marker.and_then(|marker| marker_at(marker, serving));
        kept.attempts = record.get("attempts").and_then(Value::as_u64);
        kept.through = record
            .get("through")
            .and_then(Value::as_str)
            .map(str::to_owned);
        kept.record = Some(read_record(record));
    }
    kept.gaps = marker.map(read_gaps).unwrap_or_default();
    kept
}

/// The read's put-back (H5): a marker a detach moved (`detaching_from`)
/// with no record beside it goes back on the generation it came from, as it
/// was. `Ok(None)`: nothing to put back; `Err`: a `detaching_from` this
/// version can't read, put back nowhere (O24). The marker, and the
/// generation it is put back on.
pub fn put_back(kept: &Kept<'_>) -> Result<Option<(Value, u64)>, String> {
    if kept.live_record().is_some() {
        return Ok(None);
    }
    let Some(from) = kept
        .marker
        .and_then(|marker| marker.get("detaching_from"))
        .filter(|from| !from.is_null())
    else {
        return Ok(None);
    };
    let generation = generation_of(from)?;
    let mut doc = json!({
        "active_generation": generation,
        "seeded": from.get("seeded").and_then(Value::as_bool).unwrap_or(false),
    });
    if let Some(fields) = doc.as_object_mut() {
        if let Some(uuid) = from.get("uuid").and_then(Value::as_str) {
            fields.insert("uuid".to_owned(), Value::from(uuid));
        }
        if let Some(walked) = from.get("walked").filter(|walked| !walked.is_null()) {
            fields.insert("walked".to_owned(), walked.clone());
        }
        if let Some(gaps) = kept.marker.and_then(gaps_to_restore) {
            fields.insert("gaps".to_owned(), gaps);
        }
    }
    Ok(Some((doc, generation)))
}

/// A detach (R4): d2b, the marker moved past every generation unless the
/// detach is live only or a detach moved it already; d2a, the record; d2c,
/// every generation but the serving one, best effort.
pub fn detach(kept: &Kept<'_>, detach: &Detach) -> Result<Plan, String> {
    let mut plan = Plan::default();
    if !detach.live_only
        && let Some(marker) = kept.marker
        && marker.get("detaching_from").is_none_or(Value::is_null)
    {
        plan.steps.push(Step::Marker(json!({
            "active_generation": kept.next_after(&[]),
            "seeded": false,
            "walked": null,
            "uuid": null,
            "detaching_from": detaching_from(marker),
        })));
    }
    plan.steps.push(Step::Record(record_body(
        detach,
        kept.serving,
        kept.record,
        kept.now,
    )?));
    if let Some(serving) = kept.serving {
        plan.best_effort = kept
            .generations
            .iter()
            .copied()
            .filter(|generation| *generation != serving)
            .collect();
    }
    Ok(plan)
}

/// r1 of a rebuild: every generation but the record's `serving` (and the one
/// the alias serves) deleted, the marker moved past every generation left
/// (`detaching_from` kept, or made from the marker with its gaps), and the
/// record `rebuilding`, not live only, with one more attempt, which is
/// answered.
pub fn begin_rebuild(kept: &Kept<'_>) -> Result<(Plan, u64), String> {
    let Some(record) = kept.live_record() else {
        return Err("it has no detach record to rebuild from".to_owned());
    };
    let serving = record.get("serving").and_then(Value::as_u64);
    let deleted = kept.unserved_but(serving);
    let mut steps: Vec<Step> = deleted.iter().copied().map(Step::Delete).collect();
    steps.push(Step::Marker(moved_past(kept, &deleted)));
    let attempts = record.get("attempts").and_then(Value::as_u64).unwrap_or(0) + 1;
    let mut body = record.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.insert("state".to_owned(), Value::from("rebuilding"));
        // A rebuild is begun only where the sink can rebuild: whatever the
        // detach said, the record is no longer live only (E).
        fields.insert("live_only".to_owned(), Value::from(false));
        fields.insert("attempts".to_owned(), Value::from(attempts));
        fields.insert("last_attempt".to_owned(), Value::from(kept.now));
    }
    steps.push(Step::Record(body));
    Ok((
        Plan {
            steps,
            best_effort: Vec::new(),
        },
        attempts,
    ))
}

/// A rebuild put back to detached (O26, R14): every generation but the
/// record's `serving` (and the one the alias serves) deleted, the marker
/// moved past every generation, or, `live_only`, back on the generation the
/// alias serves as `detaching_from` says it was, its gaps with it; the record
/// `detached`. `None` with no record: nothing to demote.
pub fn demote(kept: &Kept<'_>, live_only: bool) -> Option<Plan> {
    let record = kept.live_record()?;
    let recorded = record.get("serving").and_then(Value::as_u64);
    let deleted = recorded
        .map(|recorded| kept.unserved_but(Some(recorded)))
        .unwrap_or_default();
    let mut steps: Vec<Step> = deleted.iter().copied().map(Step::Delete).collect();
    if live_only {
        // Back on what reads see: the alias's generation, or the record's
        // when no alias was read.
        if let Some(serving) = kept.serving.or(recorded) {
            steps.push(Step::Marker(restored(kept, serving)));
        }
    } else {
        steps.push(Step::Marker(moved_past(kept, &deleted)));
    }
    let mut body = record.clone();
    if let Some(fields) = body.as_object_mut() {
        fields.insert("state".to_owned(), Value::from("detached"));
        if live_only {
            fields.insert("live_only".to_owned(), Value::from(true));
        }
    }
    steps.push(Step::Record(body));
    Some(Plan {
        steps,
        best_effort: Vec::new(),
    })
}

/// A record's tombstone, written over it on the version read.
pub fn tombstone() -> Value {
    json!({ "state": FORGOTTEN })
}

/// The marker written whole: `generation`, seeded or not, its `uuid` once
/// the generation exists.
pub fn whole_marker(generation: u64, seeded: bool, uuid: Option<&str>) -> Value {
    let mut doc = json!({ "active_generation": generation, "seeded": seeded });
    if let (Some(uuid), Some(fields)) = (uuid, doc.as_object_mut()) {
        fields.insert("uuid".to_owned(), Value::from(uuid));
    }
    doc
}

/// The marker written whole on `generation`, unseeded, for a write that is
/// not a seed (a generation created, a reindex): it keeps the gaps and the
/// `detaching_from` of `current`, the marker read (A).
pub fn rewritten(current: Option<&Value>, generation: u64, uuid: Option<&str>) -> Value {
    let mut doc = whole_marker(generation, false, uuid);
    if let (Some(current), Some(fields)) = (current, doc.as_object_mut()) {
        for key in ["gaps", "gap", "detaching_from"] {
            if let Some(value) = current.get(key).filter(|value| !value.is_null()) {
                fields.insert(key.to_owned(), value.clone());
            }
        }
    }
    doc
}

/// The partial update of the marker's gaps (an earlier single `gap` nulled).
pub fn gaps_update(gaps: &[sink::Gap]) -> Value {
    json!({ "gaps": gaps_value(gaps), "gap": null })
}

/// The marker moved past every generation left once `deleted` are gone,
/// unseeded, with no checkpoint and no uuid, keeping its `detaching_from`,
/// or, over a marker no detach moved, making it (the gaps go with it, A).
fn moved_past(kept: &Kept<'_>, deleted: &[u64]) -> Value {
    let detaching_from = match kept.marker {
        Some(marker) => match marker.get("detaching_from").filter(|from| !from.is_null()) {
            Some(from) => from.clone(),
            None => detaching_from(marker),
        },
        None => Value::Null,
    };
    json!({
        "active_generation": kept.next_after(deleted),
        "seeded": false,
        "walked": null,
        "uuid": null,
        "detaching_from": detaching_from,
    })
}

/// The marker back on `serving` as `detaching_from` says it was (R14); with
/// none, as the marker itself is when it stands there already (its uuid kept,
/// m-d). The gaps come back with it, from either (A).
fn restored(kept: &Kept<'_>, serving: u64) -> Value {
    let from = kept
        .marker
        .and_then(|marker| marker.get("detaching_from"))
        .filter(|from| !from.is_null())
        .or_else(|| {
            kept.marker.filter(|marker| {
                marker.get("active_generation").and_then(Value::as_u64) == Some(serving)
            })
        });
    let mut doc = json!({
        "active_generation": serving,
        "seeded": from.and_then(|from| from.get("seeded")).and_then(Value::as_bool).unwrap_or(true),
    });
    if let Some(fields) = doc.as_object_mut() {
        if let Some(uuid) = from
            .and_then(|from| from.get("uuid"))
            .and_then(Value::as_str)
        {
            fields.insert("uuid".to_owned(), Value::from(uuid));
        }
        if let Some(gaps) = kept.marker.and_then(gaps_to_restore) {
            fields.insert("gaps".to_owned(), gaps);
        }
    }
    doc
}

/// The gaps a marker keeps, wherever it keeps them: in `detaching_from` when
/// a detach moved it, else its own. `None` for none.
fn gaps_to_restore(marker: &Value) -> Option<Value> {
    carried_gaps(marker.get("detaching_from").filter(|from| !from.is_null()))
        .or_else(|| carried_gaps(Some(marker)))
}

/// What a marker moved off the serving generation keeps of itself, to come
/// back as it was: generation, seeded, uuid, checkpoint, gaps.
fn detaching_from(marker: &Value) -> Value {
    json!({
        "generation": marker.get("active_generation"),
        "seeded": marker.get("seeded").and_then(Value::as_bool).unwrap_or(false),
        "uuid": marker.get("uuid"),
        "walked": marker.get("walked"),
        "gaps": gaps_value(&read_gaps(marker)),
    })
}

/// The detach record to write: a fresh one, or `read` brought to `detached`,
/// keeping its attempts and its `through`, and its `since` when it was
/// already detached. A tombstone read keeps nothing.
pub(crate) fn record_body(
    detach: &Detach,
    serving: Option<u64>,
    read: Option<&Value>,
    now: &str,
) -> Result<Value, String> {
    let read = read.filter(|found| !is_tombstone(found));
    let kept = |key: &str| {
        read.and_then(|found| found.get(key))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let was_detached = read
        .and_then(|found| found.get("state"))
        .and_then(Value::as_str)
        == Some("detached");
    let since = if was_detached && !kept("since").is_null() {
        kept("since")
    } else {
        Value::from(now)
    };
    let refused_keys = serde_json::to_string(&detach.refused_keys)
        .map_err(|e| format!("the refused keys can't be written: {e}"))?;
    // A record already there, detached or rebuilding, keeps where its first
    // detach began: the serving generation lacks every change from there, so
    // a reattach's gap starts there (round 12's sweep; a rebuild given up is
    // no seed, this round's).
    let through = match &detach.through {
        Some(through) if read.is_none() => Value::from(through.as_str()),
        _ => kept("through"),
    };
    Ok(json!({
        "state": "detached",
        "live_only": detach.live_only,
        "serving": serving,
        "why": detach.why,
        "refused_keys": refused_keys,
        "attempts": kept("attempts").as_u64().unwrap_or(0),
        "last_attempt": kept("last_attempt"),
        "since": since,
        "through": through,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    const GAPS: &str = r#"[{"from": "0/10", "to": "0/20"}]"#;

    fn gaps() -> Value {
        serde_json::from_str(GAPS).unwrap()
    }

    fn kept<'a>(
        marker: &'a Value,
        record: Option<&'a Value>,
        serving: u64,
        generations: &'a [u64],
    ) -> Kept<'a> {
        Kept {
            marker: Some(marker),
            record,
            serving: Some(serving),
            generations,
            now: "now",
        }
    }

    fn record(state: &str, live_only: bool, serving: u64) -> Value {
        json!({ "state": state, "live_only": live_only, "serving": serving, "attempts": 0 })
    }

    fn markers(plan: &Plan) -> Vec<&Value> {
        plan.steps
            .iter()
            .filter_map(|step| match step {
                Step::Marker(doc) => Some(doc),
                _ => None,
            })
            .collect()
    }

    fn records(plan: &Plan) -> Vec<&Value> {
        plan.steps
            .iter()
            .filter_map(|step| match step {
                Step::Record(doc) => Some(doc),
                _ => None,
            })
            .collect()
    }

    /// Every gap the marker `doc` keeps, wherever it keeps them.
    fn kept_gaps(doc: &Value) -> Vec<sink::Gap> {
        let mut found = read_gaps(doc);
        if let Some(from) = doc.get("detaching_from").filter(|from| !from.is_null()) {
            found.extend(read_gaps(from));
        }
        found
    }

    /// The review of the closing round, A: every marker a plan writes keeps
    /// the gaps of the marker read, on serving or in `detaching_from`; only
    /// the seed's [`whole_marker`] has none.
    #[test]
    fn only_the_seed_drops_the_gaps() {
        let on_serving =
            json!({ "active_generation": 3, "seeded": true, "uuid": "u3", "gaps": gaps() });
        let moved = json!({
            "active_generation": 4, "seeded": false,
            "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3", "gaps": gaps() },
        });
        let want = read_gaps(&json!({ "gaps": gaps() }));
        for (name, marker) in [("on serving", &on_serving), ("moved", &moved)] {
            for live_only in [false, true] {
                let detached = record("detached", live_only, 3);
                let rebuilding = record("rebuilding", live_only, 3);
                let generations = [3, 4];
                let plans = [
                    (
                        "detach",
                        detach(
                            &kept(marker, None, 3, &generations),
                            &Detach::new("x").live_only(live_only),
                        )
                        .unwrap(),
                    ),
                    (
                        "rebuild",
                        begin_rebuild(&kept(marker, Some(&detached), 3, &generations))
                            .unwrap()
                            .0,
                    ),
                    (
                        "demotion",
                        demote(&kept(marker, Some(&rebuilding), 3, &generations), live_only)
                            .unwrap(),
                    ),
                ];
                for (what, plan) in plans {
                    for doc in markers(&plan) {
                        assert_eq!(
                            kept_gaps(doc),
                            want,
                            "{what} over a marker {name}, live only {live_only}: {doc}"
                        );
                    }
                }
            }
            assert_eq!(kept_gaps(&rewritten(Some(marker), 5, None)), want, "{name}");
        }
        let (put, _) = put_back(&kept(&moved, None, 3, &[3, 4])).unwrap().unwrap();
        assert_eq!(read_gaps(&put), want);
        assert!(kept_gaps(&whole_marker(5, true, None)).is_empty());
    }

    /// This round's sweep: a detach over a `rebuilding` record (its lane
    /// parked and detached again) keeps where the first detach began; the
    /// serving generation lacks every change from there.
    #[test]
    fn a_detach_over_a_rebuilding_record_keeps_where_the_first_began() {
        let marker = json!({ "active_generation": 4, "seeded": false, "detaching_from": { "generation": 3, "seeded": true } });
        let mut rebuilding = record("rebuilding", false, 3);
        rebuilding["through"] = json!("14");
        let plan = detach(
            &kept(&marker, Some(&rebuilding), 3, &[3, 4]),
            &Detach::new("x").through(Some("34".to_owned())),
        )
        .unwrap();
        assert_eq!(records(&plan)[0]["through"], "14");
        let fresh = detach(
            &kept(&marker, None, 3, &[3, 4]),
            &Detach::new("x").through(Some("34".to_owned())),
        )
        .unwrap();
        assert_eq!(records(&fresh)[0]["through"], "34");
    }

    /// The review of the closing round, E: a rebuild begun from a live-only
    /// record is not live only.
    #[test]
    fn a_rebuild_begun_from_a_live_only_record_is_not_live_only() {
        let marker = json!({ "active_generation": 3, "seeded": true });
        let detached = record("detached", true, 3);
        let (plan, _) = begin_rebuild(&kept(&marker, Some(&detached), 3, &[3])).unwrap();
        let written = records(&plan);
        assert_eq!(written.len(), 1);
        assert_eq!(written[0]["state"], "rebuilding");
        assert_eq!(written[0]["live_only"], false);
    }

    /// The review of the closing round, E: a record naming a generation the
    /// alias left (a seed moved it, its forget failed) never has a rebuild
    /// or a demotion delete the generation the alias serves now.
    #[test]
    fn no_plan_deletes_the_generation_the_alias_serves() {
        let marker = json!({ "active_generation": 4, "seeded": true });
        let generations = [4];
        for live_only in [false, true] {
            let rebuilding = record("rebuilding", live_only, 3);
            let begun = begin_rebuild(&kept(&marker, Some(&rebuilding), 4, &generations))
                .unwrap()
                .0;
            let demoted = demote(
                &kept(&marker, Some(&rebuilding), 4, &generations),
                live_only,
            )
            .unwrap();
            for plan in [&begun, &demoted] {
                assert!(
                    !plan.steps.contains(&Step::Delete(4)),
                    "live only {live_only}: {plan:?}"
                );
            }
            if live_only {
                assert_eq!(markers(&demoted)[0]["active_generation"], 4);
            }
        }
    }

    /// The review of the closing round, m-d: a live-only demotion over a
    /// marker on the serving generation with no `detaching_from` keeps its
    /// uuid and its seeded state.
    #[test]
    fn a_live_only_demotion_keeps_the_uuid_of_a_marker_on_serving() {
        let marker = json!({ "active_generation": 3, "seeded": true, "uuid": "u3" });
        let rebuilding = record("rebuilding", true, 3);
        let plan = demote(&kept(&marker, Some(&rebuilding), 3, &[3]), true).unwrap();
        let written = markers(&plan);
        assert_eq!(written[0]["uuid"], "u3");
        assert_eq!(written[0]["seeded"], true);
    }
}
