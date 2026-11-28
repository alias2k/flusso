//! Writing a detach against a stand-in meta store: the kill matrix over its
//! writes, the conditional writes, and what a staging reads back.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use kernel::{ColumnName, GenericValue, RowKey};
use serde_json::{Value, json};
use sink::{Detach, DetachRecord, MarkerAt, Recorded, Records};

use super::{ALIAS, DETACH, Stub, mapping, stamp};

/// The seed marker of a seeded index on generation 3, with a checkpoint
/// left from its backfill: a put-back restores it byte for byte.
fn marker() -> Value {
    json!({ "active_generation": 3, "seeded": true, "uuid": "u3", "walked": { "generation": 3, "mark": "k" } })
}

/// `doc` as a write stamped [`stamp`] leaves it.
fn stamped(mut doc: Value) -> Value {
    doc["stamp"] = json!({ "writer": "w", "seq": 3 });
    doc
}

/// A meta index, the generations of [`ALIAS`] and its holder, answering the
/// requests a detach and a staging's read make.
#[derive(Debug, Default)]
struct Store {
    docs: BTreeMap<String, (Value, u64)>,
    next_seq: u64,
    generations: BTreeSet<String>,
    holder: Option<String>,
    /// Mutating requests answered so far.
    writes: usize,
    /// Answer every mutating request from this one on with a failure: the
    /// process died before it took effect.
    killed_at: Option<usize>,
    /// Runs before a `_create` of this id is answered, once: another writer
    /// gets there first.
    before_create: Option<(String, Value)>,
    /// Runs before an `_update` of this id is answered, once: another writer
    /// replaces the document first.
    before_update: Option<(String, Value)>,
    /// Every body written to a meta document: whole (`PUT`), or the `doc`
    /// of an `_update`.
    bodies: Vec<(String, Value)>,
}

impl Store {
    /// A seeded index on generation 3, with a stale partial generation 4.
    fn seeded() -> Self {
        let mut store = Self::default();
        store.put(ALIAS, marker());
        store.generations = ["users_00000001_3", "users_00000001_4"]
            .map(str::to_owned)
            .into();
        store.holder = Some("users_00000001_3".to_owned());
        store
    }

    fn put(&mut self, id: &str, source: Value) {
        self.next_seq += 1;
        self.docs.insert(id.to_owned(), (source, self.next_seq));
    }

    fn doc(&self, id: &str) -> Option<Value> {
        self.docs.get(id).map(|(source, _)| source.clone())
    }

    fn answer(&mut self, line: &str, body: &str) -> (u16, String) {
        let (method, target) = line.split_once(' ').unwrap();
        let (path, query) = target.split_once('?').unwrap_or((target, ""));
        if method != "GET" && !path.ends_with("/_mget") {
            self.writes += 1;
            if self.killed_at.is_some_and(|at| self.writes >= at) {
                return (500, json!({ "error": "the process is gone" }).to_string());
            }
        }
        let condition = query_seq(query);
        if let Some(id) = path
            .strip_prefix("/flusso_meta/_doc/")
            .or_else(|| path.strip_prefix("/flusso_meta/_create/"))
            .or_else(|| path.strip_prefix("/flusso_meta/_update/"))
            && method != "DELETE"
        {
            let body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
            let written = body.get("doc").cloned().unwrap_or(body);
            self.bodies.push((id.to_owned(), written));
        }
        match (method, path) {
            ("POST", "/flusso_meta/_mget") => {
                let ids: Value = serde_json::from_str(body).unwrap();
                let docs: Vec<Value> = ids["ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|id| {
                        let id = id.as_str().unwrap();
                        match self.docs.get(id) {
                            Some((source, seq)) => json!({ "_id": id, "found": true, "_seq_no": seq, "_primary_term": 1, "_source": source }),
                            None => json!({ "_id": id, "found": false }),
                        }
                    })
                    .collect();
                (200, json!({ "docs": docs }).to_string())
            }
            ("GET", path) if path.starts_with("/_alias/") => match &self.holder {
                Some(holder) => (
                    200,
                    json!({ holder: { "aliases": { ALIAS: {} } } }).to_string(),
                ),
                None => (
                    404,
                    json!({ "error": "alias missing", "status": 404 }).to_string(),
                ),
            },
            ("GET", path) if path.starts_with("/_cat/indices/") => {
                let rows: Vec<Value> = self
                    .generations
                    .iter()
                    .map(|index| json!({ "index": index }))
                    .collect();
                (200, Value::from(rows).to_string())
            }
            ("PUT", path) if path.starts_with("/flusso_meta/_create/") => {
                let id = path.trim_start_matches("/flusso_meta/_create/");
                if let Some((first, source)) =
                    self.before_create.take_if(|(at, _)| at.as_str() == id)
                {
                    self.put(&first, source);
                }
                if self.docs.contains_key(id) {
                    return (
                        409,
                        json!({ "error": { "type": "version_conflict_engine_exception" } })
                            .to_string(),
                    );
                }
                self.put(id, serde_json::from_str(body).unwrap());
                (201, "{}".to_owned())
            }
            ("PUT", path) if path.starts_with("/flusso_meta/_doc/") => {
                let id = path.trim_start_matches("/flusso_meta/_doc/");
                if let Some(seq) = condition
                    && self.docs.get(id).map(|(_, at)| *at) != Some(seq)
                {
                    return (
                        409,
                        json!({ "error": { "type": "version_conflict_engine_exception" } })
                            .to_string(),
                    );
                }
                self.put(id, serde_json::from_str(body).unwrap());
                (200, "{}".to_owned())
            }
            ("POST", path) if path.starts_with("/flusso_meta/_update/") => {
                let id = path.trim_start_matches("/flusso_meta/_update/");
                if let Some((first, source)) =
                    self.before_update.take_if(|(at, _)| at.as_str() == id)
                {
                    self.put(&first, source);
                }
                if let Some(seq) = condition
                    && self.docs.get(id).map(|(_, at)| *at) != Some(seq)
                {
                    return (
                        409,
                        json!({ "error": { "type": "version_conflict_engine_exception" } })
                            .to_string(),
                    );
                }
                let Some(mut source) = self.doc(id) else {
                    return (
                        404,
                        json!({ "error": { "type": "document_missing_exception" } }).to_string(),
                    );
                };
                let update: Value = serde_json::from_str(body).unwrap();
                for (key, value) in update["doc"].as_object().unwrap() {
                    source[key] = value.clone();
                }
                self.put(id, source);
                (200, "{}".to_owned())
            }
            ("DELETE", path) if path.starts_with("/flusso_meta/_doc/") => {
                let id = path.trim_start_matches("/flusso_meta/_doc/");
                match (self.docs.get(id).map(|(_, at)| *at), condition) {
                    (None, _) => (404, "{}".to_owned()),
                    (Some(at), Some(seq)) if at != seq => (
                        409,
                        json!({ "error": { "type": "version_conflict_engine_exception" } })
                            .to_string(),
                    ),
                    _ => {
                        self.docs.remove(id);
                        (200, "{}".to_owned())
                    }
                }
            }
            ("DELETE", path) => {
                let index = path.trim_start_matches('/');
                if self.generations.remove(index) {
                    (200, "{}".to_owned())
                } else {
                    (404, "{}".to_owned())
                }
            }
            _ => panic!("the stand-in doesn't know {line}"),
        }
    }
}

/// `if_seq_no` from a query, checked to come with the primary term the store
/// gives every document.
fn query_seq(query: &str) -> Option<u64> {
    let mut seq = None;
    let mut term = None;
    for pair in query.split('&') {
        match pair.split_once('=') {
            Some(("if_seq_no", value)) => seq = value.parse().ok(),
            Some(("if_primary_term", value)) => term = value.parse::<u64>().ok(),
            _ => {}
        }
    }
    if seq.is_some() {
        assert_eq!(
            term,
            Some(1),
            "a conditional write names the primary term read"
        );
    }
    seq
}

async fn serve(store: &Arc<Mutex<Store>>) -> Stub {
    let store = Arc::clone(store);
    Stub::start(move |line, body| store.lock().unwrap().answer(line, body)).await
}

fn key(id: i64) -> RowKey {
    RowKey(vec![(
        ColumnName::try_new("id").unwrap(),
        GenericValue::BigInt(id),
    )])
}

fn blocked() -> Detach {
    Detach::new("blocked: index [users_00000001_3] blocked by: [FORBIDDEN/8/index write (api)]")
        .refused_keys(vec![key(7)])
        .stamp(stamp())
}

/// A detach killed before each of its writes, and the next staging's read of
/// what it left: before the marker moved, nothing is kept; after the marker
/// moved and before the record, the marker is put back as it was; from the
/// record on, the index reads detached, its marker unseeded elsewhere. The
/// serving generation is never touched.
#[tokio::test]
async fn a_detach_killed_after_any_step_comes_back_correct() {
    // Writes in order: the marker (d2b), the record (d2a), the stale
    // generation (d2c).
    for killed_at in 1..=4 {
        let store = Arc::new(Mutex::new(Store::seeded()));
        store.lock().unwrap().killed_at = Some(killed_at);
        let dying = serve(&store).await;
        let detached = dying.sink().detach(&mapping(), &blocked()).await;
        let done = killed_at - 1;
        assert_eq!(
            detached.is_ok(),
            done >= 2,
            "after {done} writes: {detached:?}"
        );
        if done == 3 {
            assert_eq!(detached.unwrap(), Recorded::Kept);
        }

        store.lock().unwrap().killed_at = None;
        let next = serve(&store).await;
        let read = next.sink().detachment(&mapping(), &stamp()).await.unwrap();
        let store = store.lock().unwrap();
        assert!(
            store.generations.contains("users_00000001_3"),
            "after {done} writes the serving generation is kept"
        );
        match done {
            0 => {
                assert!(read.is_empty(), "nothing kept: {read:?}");
                assert_eq!(store.doc(ALIAS).unwrap(), marker());
            }
            1 => {
                assert_eq!(read.restored, Some(3), "{read:?}");
                assert_eq!(read.record, None);
                assert_eq!(
                    store.doc(ALIAS).unwrap(),
                    stamped(marker()),
                    "the marker is put back as it was, stamped by the read"
                );
            }
            _ => {
                assert_eq!(
                    read.record,
                    Some(DetachRecord::Detached { live_only: false }),
                    "after {done} writes"
                );
                assert_eq!(read.marker, Some(MarkerAt::Newer { seeded: false }));
                assert!(read.version.is_some());
                assert_eq!(
                    read.restored, None,
                    "a marker beside its record stays moved"
                );
                assert_eq!(store.doc(ALIAS).unwrap()["active_generation"], 5);
                assert_eq!(
                    store.generations.contains("users_00000001_4"),
                    done == 2,
                    "the stale generation goes at d2c"
                );
            }
        }
    }
}

/// The marker moves past every existing generation, unseeded, with no
/// checkpoint and no uuid, keeping the marker as it was; the record names the
/// generation the hash alias holds, and the refused keys as text.
#[tokio::test]
async fn a_detach_moves_the_marker_first_then_records_serving() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    store.lock().unwrap().put(
        ALIAS,
        json!({ "active_generation": 3, "seeded": true, "uuid": "u3", "walked": { "generation": 3, "mark": "k" } }),
    );
    let stub = serve(&store).await;
    assert_eq!(
        stub.sink().detach(&mapping(), &blocked()).await.unwrap(),
        Recorded::Kept
    );
    let writes: Vec<String> = stub
        .lines()
        .into_iter()
        .filter(|line| !line.starts_with("GET") && !line.ends_with("_mget"))
        .collect();
    assert_eq!(
        writes,
        [
            "PUT /flusso_meta/_doc/users_00000001?if_seq_no=2&if_primary_term=1".to_owned(),
            format!("PUT /flusso_meta/_create/{DETACH}"),
            "DELETE /users_00000001_4".to_owned(),
        ]
    );
    let store = store.lock().unwrap();
    assert_eq!(
        store.doc(ALIAS).unwrap(),
        json!({
            "active_generation": 5, "seeded": false, "walked": null, "uuid": null,
            "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3", "walked": { "generation": 3, "mark": "k" }, "gaps": [] },
            "stamp": { "writer": "w", "seq": 3 },
        })
    );
    let record = store.doc(DETACH).unwrap();
    assert_eq!(record["stamp"], json!({ "writer": "w", "seq": 3 }));
    assert_eq!(record["state"], "detached");
    assert_eq!(record["live_only"], false);
    assert_eq!(record["serving"], 3);
    assert_eq!(record["attempts"], 0);
    assert!(record["why"].as_str().unwrap().starts_with("blocked: "));
    let keys: Vec<RowKey> = serde_json::from_str(record["refused_keys"].as_str().unwrap()).unwrap();
    assert_eq!(keys, vec![key(7)], "stored as text, read back whole");
    assert!(record["since"].as_str().is_some());
}

/// Run again (a detach whose record write failed, or a staging that finds the
/// marker on serving), a detach leaves a marker it already moved exactly as
/// it is, and keeps the record's `since` and attempts (H4).
#[tokio::test]
async fn a_rerun_detach_never_moves_the_marker_again() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    stub.sink().detach(&mapping(), &blocked()).await.unwrap();
    let (marker, record) = {
        let mut store = store.lock().unwrap();
        // A generation a later step created, past the marker's.
        store.generations.insert("users_00000001_6".to_owned());
        let mut record = store.doc(DETACH).unwrap();
        record["attempts"] = json!(2);
        store.put(DETACH, record.clone());
        (store.doc(ALIAS).unwrap(), record)
    };
    stub.sink()
        .detach(&mapping(), &Detach::new("refused: id 7"))
        .await
        .unwrap();
    let store = store.lock().unwrap();
    assert_eq!(
        store.doc(ALIAS).unwrap(),
        marker,
        "the marker stays where it was"
    );
    let again = store.doc(DETACH).unwrap();
    assert_eq!(again["since"], record["since"]);
    assert_eq!(again["attempts"], 2);
    assert_eq!(again["why"], "refused: id 7");
}

/// A sink that can't rebuild leaves the marker on the serving generation and
/// says so in its record; staging reads it detached, live only, with the
/// marker on serving.
#[tokio::test]
async fn a_live_only_detach_leaves_the_marker_on_serving() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    stub.sink()
        .detach(&mapping(), &blocked().live_only(true))
        .await
        .unwrap();
    let read = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(
        read.record,
        Some(DetachRecord::Detached { live_only: true })
    );
    assert_eq!(read.marker, Some(MarkerAt::Serving { seeded: true }));
    assert_eq!(store.lock().unwrap().doc(ALIAS).unwrap(), marker());
}

/// A record another writer created between the read and the create is read
/// again and replaced on its version, never overwritten blind: its attempts
/// stay, and a record that wasn't `detached` starts a new `since` (R11).
#[tokio::test]
async fn a_conflicting_record_is_read_again_and_never_overwritten_blind() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    store.lock().unwrap().before_create = Some((
        DETACH.to_owned(),
        json!({ "state": "rebuilding", "live_only": false, "serving": 3, "attempts": 2, "since": "then" }),
    ));
    let stub = serve(&store).await;
    assert_eq!(
        stub.sink().detach(&mapping(), &blocked()).await.unwrap(),
        Recorded::Kept
    );
    let store = store.lock().unwrap();
    let record = store.doc(DETACH).unwrap();
    assert_eq!(record["state"], "detached");
    assert_eq!(record["attempts"], 2, "kept from the record read again");
    assert_ne!(
        record["since"], "then",
        "a rebuild detached again starts its clock"
    );
    assert!(
        stub.lines()
            .iter()
            .any(|line| line.starts_with(&format!("PUT /flusso_meta/_doc/{DETACH}?if_seq_no="))),
        "replaced on the version read again: {:?}",
        stub.lines()
    );
}

/// A marker moved by another writer between the read and the move is read
/// again: one that now carries `detaching_from` stays as it is.
#[tokio::test]
async fn a_marker_moved_meanwhile_is_read_again_and_left_alone() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let moved = json!({ "active_generation": 9, "seeded": false, "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3" } });
    {
        let store = Arc::clone(&store);
        let moved = moved.clone();
        // The first write of the marker finds it moved by someone else.
        let first = std::sync::atomic::AtomicBool::new(true);
        let stub = Stub::start(move |line, body| {
            let mut store = store.lock().unwrap();
            if line.starts_with("PUT /flusso_meta/_doc/users_00000001?")
                && first.swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                store.put(ALIAS, moved.clone());
            }
            store.answer(line, body)
        })
        .await;
        stub.sink().detach(&mapping(), &blocked()).await.unwrap();
    }
    let store = store.lock().unwrap();
    assert_eq!(store.doc(ALIAS).unwrap(), moved);
    assert_eq!(store.doc(DETACH).unwrap()["serving"], 3);
}

/// A sink that can't rebuild, detached while nothing holds the hash alias
/// (its lane restaging): the marker stays seeded where it was, and staging
/// can't read that as a rebuild that finished (review S2).
#[tokio::test]
async fn a_live_only_detach_with_nothing_serving_reads_as_no_known_serving() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    store.lock().unwrap().holder = None;
    let stub = serve(&store).await;
    stub.sink()
        .detach(&mapping(), &blocked().live_only(true))
        .await
        .unwrap();
    let read = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(
        read.record,
        Some(DetachRecord::Detached { live_only: true })
    );
    assert_eq!(read.marker, Some(MarkerAt::Elsewhere { seeded: true }));
}

/// With nothing serving reads, the record says so and no generation is
/// deleted.
#[tokio::test]
async fn with_nothing_serving_no_generation_is_deleted() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    store.lock().unwrap().holder = None;
    let stub = serve(&store).await;
    stub.sink().detach(&mapping(), &blocked()).await.unwrap();
    let store = store.lock().unwrap();
    assert_eq!(store.doc(DETACH).unwrap()["serving"], Value::Null);
    assert_eq!(store.generations.len(), 2);
}

/// A record is forgotten only at the version read, by a tombstone stamped
/// with who forgot it; a later write wins and the forget fails, and so does
/// one at a version the tombstone replaced. A read finds no record, and the
/// tombstone's stamp and version.
#[tokio::test]
async fn forgetting_a_record_leaves_a_tombstone_only_on_the_version_read() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    let sink = stub.sink();
    sink.detach(&mapping(), &blocked()).await.unwrap();
    let read = sink.detachment(&mapping(), &stamp()).await.unwrap();
    let version = read.version.unwrap();

    let newer = store.lock().unwrap().doc(DETACH).unwrap();
    store.lock().unwrap().put(DETACH, newer.clone());
    assert!(
        sink.forget_detachment(&mapping(), &version, &stamp())
            .await
            .is_err()
    );
    assert_eq!(
        store.lock().unwrap().doc(DETACH),
        Some(newer),
        "the newer record stays"
    );

    let version = sink
        .detachment(&mapping(), &stamp())
        .await
        .unwrap()
        .version
        .unwrap();
    let forgetter = sink::Stamp::new("f", 11);
    sink.forget_detachment(&mapping(), &version, &forgetter)
        .await
        .unwrap();
    assert_eq!(
        store.lock().unwrap().doc(DETACH),
        Some(json!({ "state": "forgotten", "stamp": { "writer": "f", "seq": 11 } }))
    );
    let read = sink.detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(read.record, None);
    assert_eq!(read.record_stamp, Some(forgetter.clone()));
    assert!(read.version.is_some() && read.version != Some(version.clone()));
    assert!(
        sink.forget_detachment(&mapping(), &version, &forgetter)
            .await
            .is_err(),
        "the version forgotten is gone: a forget asked again reads again"
    );
}

/// A detach over a tombstone replaces it on its version, not as a create,
/// and keeps nothing of it: no attempts, no `through`.
#[tokio::test]
async fn a_detach_over_a_tombstone_replaces_it_and_keeps_nothing_of_it() {
    let mut seeded = Store::seeded();
    seeded.put(
        DETACH,
        json!({ "state": "forgotten", "stamp": { "writer": "f", "seq": 1 }, "attempts": 4, "through": "0/99" }),
    );
    let at = seeded.next_seq;
    let store = Arc::new(Mutex::new(seeded));
    let stub = serve(&store).await;
    stub.sink().detach(&mapping(), &blocked()).await.unwrap();
    assert!(
        stub.lines().contains(&format!(
            "PUT /flusso_meta/_doc/{DETACH}?if_seq_no={at}&if_primary_term=1"
        )),
        "{:?}",
        stub.lines()
    );
    let record = store.lock().unwrap().doc(DETACH).unwrap();
    assert_eq!(record["state"], "detached");
    assert_eq!(record["attempts"], 0);
    assert_eq!(record["through"], Value::Null);
    assert_eq!(record["stamp"], json!({ "writer": "w", "seq": 3 }));
}

/// A rebuild, a demotion and the seed's forget find no record in a
/// tombstone: the rebuild refuses, the others write nothing.
#[tokio::test]
async fn a_tombstone_is_no_record_to_rebuild_demote_or_forget() {
    let mut seeded = detached_store();
    seeded.put(
        DETACH,
        json!({ "state": "forgotten", "stamp": { "writer": "f", "seq": 1 } }),
    );
    let store = Arc::new(Mutex::new(seeded));
    let stub = serve(&store).await;
    assert!(
        stub.sink()
            .begin_rebuild(&mapping(), &stamp())
            .await
            .is_err()
    );
    stub.sink()
        .demote(&mapping(), false, &stamp())
        .await
        .unwrap();
    stub.sink()
        .forget_finished_rebuild(ALIAS, Some(&stamp()))
        .await
        .unwrap();
    assert_eq!(store.lock().unwrap().writes, 0, "{:?}", stub.lines());
}

/// Every whole write of the seed marker carries the stamp it is given, and
/// `null` for a caller that keeps no records; the checkpoint's partial
/// update names no stamp, so `_update` keeps the one there; the uuid's
/// record is a conditional whole write carrying the caller's (m-B).
#[tokio::test]
async fn whole_marker_writes_carry_their_stamp_and_partial_ones_keep_it() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    let sink = stub.sink();
    sink.write_index_meta(ALIAS, 4, false, None, Some(&stamp()))
        .await
        .unwrap();
    assert_eq!(
        store.lock().unwrap().doc(ALIAS),
        Some(stamped(json!({ "active_generation": 4, "seeded": false })))
    );
    sink.update_walked(ALIAS, json!({ "mark": "k", "generation": 4 }))
        .await
        .unwrap();
    sink.record_uuid(ALIAS, 4, "u4", Some(&stamp()))
        .await
        .unwrap();
    assert_eq!(
        store.lock().unwrap().doc(ALIAS).unwrap()["stamp"],
        json!({ "writer": "w", "seq": 3 })
    );
    sink.write_index_meta(ALIAS, 5, false, None, None)
        .await
        .unwrap();
    assert_eq!(
        store.lock().unwrap().doc(ALIAS),
        Some(json!({ "active_generation": 5, "seeded": false, "stamp": null }))
    );
}

/// A store with `users` detached as a detach leaves it: the marker moved to
/// generation 5 keeping where it was, the record `detached`, serving 3, and
/// a partial generation 5 a failed attempt left.
fn detached_store() -> Store {
    let mut store = Store::seeded();
    store.put(
        ALIAS,
        json!({ "active_generation": 5, "seeded": false, "walked": { "generation": 5, "mark": "old" },
                "uuid": "u5", "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3" } }),
    );
    store.put(
        DETACH,
        json!({ "state": "detached", "live_only": false, "serving": 3, "attempts": 1, "since": "then" }),
    );
    store.generations = ["users_00000001_3", "users_00000001_5"]
        .map(str::to_owned)
        .into();
    store
}

/// r1: every generation but serving goes first, the marker moves past every
/// generation (no checkpoint, no uuid, `detaching_from` kept), then the
/// record says `rebuilding` with one more attempt, which is returned.
#[tokio::test]
async fn a_rebuild_begins_by_clearing_partials_moving_the_marker_and_counting_an_attempt() {
    let store = Arc::new(Mutex::new(detached_store()));
    let stub = serve(&store).await;
    assert_eq!(
        stub.sink()
            .begin_rebuild(&mapping(), &stamp())
            .await
            .unwrap(),
        2
    );
    let store = store.lock().unwrap();
    assert_eq!(store.generations, ["users_00000001_3".to_owned()].into());
    assert_eq!(
        store.doc(ALIAS).unwrap(),
        json!({ "active_generation": 4, "seeded": false, "walked": null, "uuid": null,
                "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3" },
                "stamp": { "writer": "w", "seq": 3 } }),
        "past every generation left: the partial is gone, its checkpoint with it"
    );
    let record = store.doc(DETACH).unwrap();
    assert_eq!(record["stamp"], json!({ "writer": "w", "seq": 3 }));
    assert_eq!(record["state"], "rebuilding");
    assert_eq!(record["attempts"], 2);
    assert_eq!(record["serving"], 3);
    assert!(record["last_attempt"].as_str().is_some());
}

/// A rebuild killed before each of its writes: the record says `rebuilding`
/// only once the partial is gone and the marker moved, so a restart finding
/// `rebuilding` rebuilds into an empty generation; before that it finds
/// `detached` and starts over (the kill walk's r1).
#[tokio::test]
async fn a_rebuild_killed_after_any_step_comes_back_correct() {
    // Writes in order: the partial's delete, the marker, the record.
    for killed_at in 1..=4 {
        let store = Arc::new(Mutex::new(detached_store()));
        store.lock().unwrap().killed_at = Some(killed_at);
        let dying = serve(&store).await;
        let begun = dying.sink().begin_rebuild(&mapping(), &stamp()).await;
        let done = killed_at - 1;
        assert_eq!(begun.is_ok(), done == 3, "after {done} writes: {begun:?}");
        store.lock().unwrap().killed_at = None;
        let next = serve(&store).await;
        let read = next.sink().detachment(&mapping(), &stamp()).await.unwrap();
        let store = store.lock().unwrap();
        assert!(
            store.generations.contains("users_00000001_3"),
            "serving kept"
        );
        match done {
            0 => assert_eq!(
                read.record,
                Some(DetachRecord::Detached { live_only: false })
            ),
            1 | 2 => {
                assert_eq!(
                    read.record,
                    Some(DetachRecord::Detached { live_only: false })
                );
                assert!(!store.generations.contains("users_00000001_5"));
            }
            _ => {
                assert_eq!(
                    read.record,
                    Some(DetachRecord::Rebuilding { live_only: false })
                );
                assert_eq!(read.attempts, Some(2));
                assert_eq!(read.marker, Some(MarkerAt::Newer { seeded: false }));
            }
        }
    }
}

/// O26: a rebuild put back to detached drops the partial and its checkpoint
/// and moves the marker past every generation; R14 (live only) puts the
/// marker back on serving as it was.
#[tokio::test]
async fn a_demotion_puts_a_rebuild_back_to_detached() {
    for live_only in [false, true] {
        let store = Arc::new(Mutex::new(detached_store()));
        let stub = serve(&store).await;
        stub.sink()
            .begin_rebuild(&mapping(), &stamp())
            .await
            .unwrap();
        store
            .lock()
            .unwrap()
            .generations
            .insert("users_00000001_4".to_owned());
        stub.sink()
            .demote(&mapping(), live_only, &stamp())
            .await
            .unwrap();
        let read = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
        let store = store.lock().unwrap();
        assert_eq!(
            read.record,
            Some(DetachRecord::Detached { live_only }),
            "{live_only}"
        );
        assert!(
            !store.generations.contains("users_00000001_4"),
            "{live_only}: partial gone"
        );
        if live_only {
            assert_eq!(read.marker, Some(MarkerAt::Serving { seeded: true }));
            assert_eq!(
                store.doc(ALIAS).unwrap(),
                stamped(json!({ "active_generation": 3, "seeded": true, "uuid": "u3" }))
            );
        } else {
            assert_eq!(read.marker, Some(MarkerAt::Newer { seeded: false }));
            assert_eq!(store.doc(ALIAS).unwrap()["walked"], Value::Null);
        }
        assert_eq!(read.marker_stamp, Some(stamp()), "{live_only}");
        assert_eq!(read.record_stamp, Some(stamp()), "{live_only}");
    }
}

/// r4's last write: the seed forgets the finished rebuild's record on the
/// version it reads, by a tombstone stamped as the seed; with none (or the
/// tombstone), nothing is written.
#[tokio::test]
async fn the_seed_forgets_the_finished_rebuilds_record() {
    let store = Arc::new(Mutex::new(detached_store()));
    let stub = serve(&store).await;
    stub.sink()
        .forget_finished_rebuild(ALIAS, Some(&stamp()))
        .await
        .unwrap();
    assert_eq!(
        store.lock().unwrap().doc(DETACH),
        Some(json!({ "state": "forgotten", "stamp": { "writer": "w", "seq": 3 } }))
    );
    let writes = store.lock().unwrap().writes;
    stub.sink()
        .forget_finished_rebuild(ALIAS, Some(&stamp()))
        .await
        .unwrap();
    assert_eq!(store.lock().unwrap().writes, writes);
}

/// O16, S3: a detach keeps where reads stood (`through`); a reattach keeps
/// its gap with the seed marker, which outlives the record, and changes
/// nothing else of the marker; a staging reads both back.
#[tokio::test]
async fn a_reattach_keeps_its_gap_with_the_seed_marker() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    let sink = stub.sink();
    sink.detach(
        &mapping(),
        &blocked().live_only(true).through(Some("0/10".to_owned())),
    )
    .await
    .unwrap();
    let read = sink.detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(read.through.as_deref(), Some("0/10"));
    assert!(read.gaps.is_empty());

    let open = sink::Gap {
        from: read.through.clone(),
        to: None,
    };
    sink.record_gaps(&mapping(), &|_| Some(vec![open.clone()]), &stamp())
        .await
        .unwrap();
    sink.forget_detachment(&mapping(), read.version.as_ref().unwrap(), &stamp())
        .await
        .unwrap();
    let read = sink.detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(read.record, None, "the record is forgotten");
    assert_eq!(read.gaps, [open], "the gap stays with the marker");

    let first = sink::Gap {
        from: Some("0/10".to_owned()),
        to: Some("0/20".to_owned()),
    };
    let second = sink::Gap {
        from: Some("0/30".to_owned()),
        to: Some("0/40".to_owned()),
    };
    let both = [first, second];
    sink.record_gaps(&mapping(), &|_| Some(both.to_vec()), &stamp())
        .await
        .unwrap();
    assert_eq!(
        sink.detachment(&mapping(), &stamp()).await.unwrap().gaps,
        both
    );
    let mut kept = store.lock().unwrap().doc(ALIAS).unwrap();
    assert_eq!(
        kept["gaps"],
        json!([{ "from": "0/10", "to": "0/20" }, { "from": "0/30", "to": "0/40" }])
    );
    let fields = kept.as_object_mut().unwrap();
    fields.remove("gaps");
    fields.remove("gap");
    assert_eq!(
        fields.remove("stamp"),
        Some(json!({ "writer": "w", "seq": 3 })),
        "the gaps' write is stamped"
    );
    assert_eq!(kept, marker(), "nothing else of the marker changed");
}

/// The single `gap` an earlier build of step 4 kept is read as a list of one.
#[tokio::test]
async fn a_single_gap_an_earlier_build_kept_reads_as_a_list_of_one() {
    let mut seeded = Store::seeded();
    let mut kept = marker();
    kept["gap"] = json!({ "from": "0/10", "to": "0/20" });
    seeded.put(ALIAS, kept);
    let store = Arc::new(Mutex::new(seeded));
    let stub = serve(&store).await;
    assert_eq!(
        stub.sink()
            .detachment(&mapping(), &stamp())
            .await
            .unwrap()
            .gaps,
        [sink::Gap {
            from: Some("0/10".to_owned()),
            to: Some("0/20".to_owned()),
        }]
    );
}

/// Review of the step-4 fixes, S-a: the gaps' write is conditional on the
/// marker read. Another writer changing the marker first makes the sink read
/// again and decide again over what that writer kept, never write blind.
#[tokio::test]
async fn a_gap_write_meeting_another_writer_reads_again_and_decides_again() {
    let mut seeded = Store::seeded();
    let mut theirs = marker();
    theirs["gaps"] = json!([{ "from": "0/10", "to": "0/40" }]);
    seeded.before_update = Some((ALIAS.to_owned(), theirs));
    let store = Arc::new(Mutex::new(seeded));
    let stub = serve(&store).await;
    let seen = Mutex::new(Vec::new());
    let kept = stub
        .sink()
        .record_gaps(
            &mapping(),
            &|kept| {
                seen.lock().unwrap().push(kept.to_vec());
                // Settle an open end only: one known already stays.
                let open = kept.iter().any(|gap| gap.to.is_none());
                open.then(|| {
                    vec![sink::Gap {
                        from: Some("0/10".to_owned()),
                        to: Some("0/20".to_owned()),
                    }]
                })
                .or_else(|| {
                    kept.is_empty().then(|| {
                        vec![sink::Gap {
                            from: Some("0/10".to_owned()),
                            to: None,
                        }]
                    })
                })
            },
            &stamp(),
        )
        .await
        .unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.len(), 2, "read again after the conflict: {seen:?}");
    assert_eq!(
        kept,
        [sink::Gap {
            from: Some("0/10".to_owned()),
            to: Some("0/40".to_owned()),
        }],
        "the other writer's settled gap stands"
    );
    assert_eq!(
        store.lock().unwrap().doc(ALIAS).unwrap()["gaps"],
        json!([{ "from": "0/10", "to": "0/40" }])
    );
}

/// Review of the step-4 fixes, m-c: a `gaps` entry that can't be read is
/// never "no gap": it reads as a gap with both ends unknown, which refuses
/// every position until a read while the slot is held settles it.
#[tokio::test]
async fn a_gap_entry_that_cannot_be_read_is_a_gap_with_unknown_ends() {
    let mut seeded = Store::seeded();
    let mut kept = marker();
    kept["gaps"] = json!([{ "from": "0/10", "to": "0/20" }, "garbled"]);
    seeded.put(ALIAS, kept);
    let store = Arc::new(Mutex::new(seeded));
    let stub = serve(&store).await;
    assert_eq!(
        stub.sink()
            .detachment(&mapping(), &stamp())
            .await
            .unwrap()
            .gaps,
        [
            sink::Gap {
                from: Some("0/10".to_owned()),
                to: Some("0/20".to_owned()),
            },
            sink::Gap {
                from: None,
                to: None,
            },
        ]
    );
}

/// The review of round 11, S-1: a seeded marker keeps a reattach's gap
/// (written while the sink was `backfill = false`). The sink, now able to
/// rebuild, detaches: d2b moves the marker whole, keeping its gaps in
/// `detaching_from`. When the marker comes back on the serving generation,
/// which still lacks the gap's changes, the gap comes back with it: (a) the
/// detach died before its record and a read puts the marker back; (b) a
/// start on `backfill = false` demotes the record live only (R14).
#[tokio::test]
async fn a_gap_survives_a_detach_whose_marker_comes_back_on_serving() {
    let gaps = json!([{ "from": "0/10", "to": "0/20" }]);
    let with_gap = || {
        let mut store = Store::seeded();
        let mut kept = marker();
        kept["gaps"] = gaps.clone();
        store.put(ALIAS, kept);
        store
    };

    let store = Arc::new(Mutex::new(with_gap()));
    store.lock().unwrap().killed_at = Some(2);
    let dying = serve(&store).await;
    let _ = dying.sink().detach(&mapping(), &blocked()).await;
    store.lock().unwrap().killed_at = None;
    let next = serve(&store).await;
    let putting_back = next.sink().detachment(&mapping(), &stamp()).await.unwrap();
    let after_put_back = next.sink().detachment(&mapping(), &stamp()).await.unwrap();

    let store_b = Arc::new(Mutex::new(with_gap()));
    let stub = serve(&store_b).await;
    stub.sink().detach(&mapping(), &blocked()).await.unwrap();
    stub.sink()
        .demote(&mapping(), true, &stamp())
        .await
        .unwrap();
    let after_demotion = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();

    let gap = sink::Gap {
        from: Some("0/10".to_owned()),
        to: Some("0/20".to_owned()),
    };
    // The read that puts the marker back tells its gaps too, or the fence
    // hears "no gap" until the next read (the review of the closing round).
    assert!(putting_back.restored.is_some(), "{putting_back:?}");
    assert_eq!(putting_back.gaps, [gap.clone()]);
    assert_eq!(after_put_back.gaps, [gap.clone()]);
    assert_eq!(after_demotion.gaps, [gap]);
    assert_eq!(store.lock().unwrap().doc(ALIAS).unwrap()["gaps"], gaps);
    assert_eq!(store_b.lock().unwrap().doc(ALIAS).unwrap()["gaps"], gaps);
    assert_eq!(
        (after_put_back.gaps.len(), after_demotion.gaps.len()),
        (1, 1),
        "the gap is lost: put back {:?}, demoted {:?}",
        store.lock().unwrap().doc(ALIAS),
        store_b.lock().unwrap().doc(ALIAS),
    );
}

/// Item 7 of round 12's brief: every body the adapter writes to a record or
/// a marker, whole or as an `_update` of the gaps, carries the stamp of the
/// call that wrote it. Each record-keeping call is driven with a stamp of its
/// own; the stand-in keeps every body written.
#[tokio::test]
async fn every_record_or_marker_body_carries_the_callers_stamp() {
    async fn written_by(
        store: Store,
        call: impl AsyncFnOnce(&crate::OpensearchSink, &sink::Stamp),
    ) -> Vec<(String, Value)> {
        let store = Arc::new(Mutex::new(store));
        let stub = serve(&store).await;
        let stamp = sink::Stamp::new("caller", 17);
        call(&stub.sink(), &stamp).await;
        let bodies = std::mem::take(&mut store.lock().unwrap().bodies);
        assert!(!bodies.is_empty(), "nothing written");
        for (id, body) in &bodies {
            assert_eq!(
                body["stamp"],
                json!({ "writer": "caller", "seq": 17 }),
                "{id}: {body}"
            );
        }
        bodies
    }
    let mut half_moved = Store::seeded();
    half_moved.put(
        ALIAS,
        json!({ "active_generation": 4, "seeded": false,
                "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3" } }),
    );
    let with_gap = || {
        let mut store = Store::seeded();
        let mut kept = marker();
        kept["gaps"] = json!([{ "from": "0/1", "to": "0/2" }]);
        store.put(ALIAS, kept);
        store
    };
    let detached = |store: Store| async move {
        let store = Arc::new(Mutex::new(store));
        let stub = serve(&store).await;
        stub.sink().detach(&mapping(), &blocked()).await.unwrap();
        let mut taken = std::mem::take(&mut *store.lock().unwrap());
        taken.bodies.clear();
        taken
    };
    let calls: Vec<(&str, Vec<(String, Value)>)> = vec![
        (
            "detach",
            written_by(with_gap(), async |sink, stamp| {
                sink.detach(&mapping(), &blocked().stamp(stamp.clone()))
                    .await
                    .unwrap();
            })
            .await,
        ),
        (
            "live-only detach",
            written_by(with_gap(), async |sink, stamp| {
                sink.detach(&mapping(), &blocked().live_only(true).stamp(stamp.clone()))
                    .await
                    .unwrap();
            })
            .await,
        ),
        (
            "the read's put-back",
            written_by(half_moved, async |sink, stamp| {
                assert!(
                    sink.detachment(&mapping(), stamp)
                        .await
                        .unwrap()
                        .restored
                        .is_some()
                );
            })
            .await,
        ),
        (
            "rebuild",
            written_by(detached_store(), async |sink, stamp| {
                sink.begin_rebuild(&mapping(), stamp).await.unwrap();
            })
            .await,
        ),
        (
            "demotion",
            written_by(detached_store(), async |sink, stamp| {
                sink.demote(&mapping(), false, stamp).await.unwrap();
            })
            .await,
        ),
        (
            "live-only demotion",
            written_by(detached(with_gap()).await, async |sink, stamp| {
                sink.demote(&mapping(), true, stamp).await.unwrap();
            })
            .await,
        ),
        (
            "forget",
            written_by(detached_store(), async |sink, stamp| {
                let read = sink.detachment(&mapping(), stamp).await.unwrap();
                sink.forget_detachment(&mapping(), read.version.as_ref().unwrap(), stamp)
                    .await
                    .unwrap();
            })
            .await,
        ),
        (
            "the seed's forget",
            written_by(detached_store(), async |sink, stamp| {
                sink.forget_finished_rebuild(ALIAS, Some(stamp))
                    .await
                    .unwrap();
            })
            .await,
        ),
        (
            "gaps",
            written_by(Store::seeded(), async |sink, stamp| {
                sink.record_gaps(
                    &mapping(),
                    &|_| {
                        Some(vec![sink::Gap {
                            from: None,
                            to: None,
                        }])
                    },
                    stamp,
                )
                .await
                .unwrap();
            })
            .await,
        ),
        (
            "a marker written whole",
            written_by(Store::seeded(), async |sink, stamp| {
                sink.write_index_meta(ALIAS, 4, false, None, Some(stamp))
                    .await
                    .unwrap();
            })
            .await,
        ),
    ];
    for (call, bodies) in calls {
        assert!(
            bodies.iter().any(|(id, _)| id == ALIAS || id == DETACH),
            "{call}: {bodies:?}"
        );
    }
}

/// The review of round 12's sweep: a detach recorded over a record that
/// already says detached keeps where that detach began (its `through`), as
/// it keeps its `since`: the lane has dropped changes from there, and a
/// reattach's gap must start there, not where reads stand at the later
/// detach. Over a `rebuilding` record too (the closing round's sweep): the
/// rebuild given up seeded nothing, so the serving generation still lacks
/// every change from there.
#[tokio::test]
async fn a_detach_over_a_detached_record_keeps_where_it_began() {
    for (state, kept_through) in [("detached", true), ("rebuilding", true)] {
        let mut seeded = Store::seeded();
        seeded.put(
            DETACH,
            json!({ "state": state, "live_only": true, "serving": 3, "attempts": 0,
                    "since": "then", "through": "0/10" }),
        );
        let store = Arc::new(Mutex::new(seeded));
        let stub = serve(&store).await;
        stub.sink()
            .detach(
                &mapping(),
                &blocked().live_only(true).through(Some("0/39".to_owned())),
            )
            .await
            .unwrap();
        let record = store.lock().unwrap().doc(DETACH).unwrap();
        let expected = if kept_through { "0/10" } else { "0/39" };
        assert_eq!(record["through"], expected, "{state}: {record}");
    }
}

/// The review of the closing round, A: a gap kept on the serving marker, a
/// live-only detach (marker left on serving, no `detaching_from`), then
/// `backfill` turned on: a rebuild begins over a marker with no
/// `detaching_from`, then `backfill` turned off again before it seeds: the
/// live-only demotion puts the marker back on serving. The serving
/// generation still lacks the gap's changes, so the gap comes back.
#[tokio::test]
async fn a_gap_survives_a_rebuild_begun_over_a_live_only_detach() {
    let gaps = json!([{ "from": "0/10", "to": "0/20" }]);
    let mut store = Store::seeded();
    let mut kept = marker();
    kept["gaps"] = gaps.clone();
    store.put(ALIAS, kept);
    let store = Arc::new(Mutex::new(store));
    let stub = serve(&store).await;
    stub.sink()
        .detach(&mapping(), &blocked().live_only(true).stamp(stamp()))
        .await
        .unwrap();
    let before = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(
        before.gaps.len(),
        1,
        "after the live-only detach: {before:?}"
    );
    stub.sink()
        .begin_rebuild(&mapping(), &stamp())
        .await
        .unwrap();
    let moved = store.lock().unwrap().doc(ALIAS);
    stub.sink()
        .demote(&mapping(), true, &stamp())
        .await
        .unwrap();
    let after = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
    assert_eq!(
        after.gaps.len(),
        1,
        "the gap is lost: marker after begin_rebuild {moved:?}, after the demotion {:?}; read {after:?}",
        store.lock().unwrap().doc(ALIAS)
    );
}

/// The review of the closing round, A: as the engine's `rebuild` does it,
/// `begin_rebuild` then `ensure_index_as` (which creates the generation the
/// marker names and rewrites the marker whole, `rewrite_marker`), then a
/// live-only demotion.
/// With the marker moved by d2b first (S-1's own case, plus a reindex) and
/// with a live-only detach first.
#[tokio::test]
async fn a_gap_survives_a_begun_rebuild_put_back_live_only() {
    for live_only_detach in [false, true] {
        let gaps = json!([{ "from": "0/10", "to": "0/20" }]);
        let mut store = Store::seeded();
        let mut kept = marker();
        kept["gaps"] = gaps.clone();
        store.put(ALIAS, kept);
        let store = Arc::new(Mutex::new(store));
        let stub = serve(&store).await;
        stub.sink()
            .detach(
                &mapping(),
                &blocked().live_only(live_only_detach).stamp(stamp()),
            )
            .await
            .unwrap();
        stub.sink()
            .begin_rebuild(&mapping(), &stamp())
            .await
            .unwrap();
        // What `ensure_generation` writes when the generation the marker
        // names does not exist yet (`sink_impl.rs`, `if !exists`).
        let next = store.lock().unwrap().doc(ALIAS).unwrap()["active_generation"]
            .as_u64()
            .unwrap();
        let ensured = stub
            .sink()
            .rewrite_marker(ALIAS, next, None, Some(&stamp()))
            .await;
        let after_ensure = store.lock().unwrap().doc(ALIAS);
        stub.sink()
            .demote(&mapping(), true, &stamp())
            .await
            .unwrap();
        let after = stub.sink().detachment(&mapping(), &stamp()).await.unwrap();
        assert_eq!(
            after.gaps.len(),
            1,
            "live_only detach {live_only_detach}: ensure {ensured:?}, marker after it {after_ensure:?}, after the demotion {:?}",
            store.lock().unwrap().doc(ALIAS)
        );
    }
}

/// The review of round 13, F: a reindex staged (the marker on generation 4,
/// unseeded, its uuid recorded), a rebuildable detach moves it, the sink is
/// made `backfill = false`, and the start's live-only demotion (D) puts the
/// marker back on the alias's generation 3 carrying generation 4's uuid and
/// seeded state. The next `ensure_index` (a reattach makes one) then reads
/// generation 3 as "not the one flusso created" and deleted it, the
/// generation every read sees. The marker now goes back with no uuid.
#[tokio::test]
async fn a_live_only_demotion_after_a_detach_during_a_reindex_keeps_the_serving_generation() {
    let mut store = Store::seeded();
    store.put(
        ALIAS,
        json!({ "active_generation": 4, "seeded": false, "uuid": "u4" }),
    );
    let store = Arc::new(Mutex::new(store));
    let routed = Arc::clone(&store);
    let stub = Stub::start(move |line, body| {
        let (method, target) = line.split_once(' ').unwrap();
        let path = target.split_once('?').map_or(target, |(path, _)| path);
        if method == "GET" && path.starts_with("/flusso_meta/_doc/") {
            let id = path.trim_start_matches("/flusso_meta/_doc/");
            return match routed.lock().unwrap().doc(id) {
                Some(source) => (
                    200,
                    json!({ "_id": id, "found": true, "_source": source }).to_string(),
                ),
                None => (404, json!({ "found": false }).to_string()),
            };
        }
        if method == "GET" && path.ends_with("/_settings/index.uuid") {
            let index = path.trim_start_matches('/').split('/').next().unwrap();
            let uuid = format!("u{}", index.rsplit('_').next().unwrap());
            return if routed.lock().unwrap().generations.contains(index) {
                (
                    200,
                    json!({ index: { "settings": { "index": { "uuid": uuid } } } }).to_string(),
                )
            } else {
                (404, "{}".to_owned())
            };
        }
        let known = path.starts_with("/flusso_meta/")
            || path.starts_with("/_alias/")
            || path.starts_with("/_cat/indices/")
            || method == "DELETE";
        if known {
            routed.lock().unwrap().answer(line, body)
        } else {
            (
                500,
                json!({ "error": "not modelled by the probe" }).to_string(),
            )
        }
    })
    .await;
    stub.sink()
        .detach(&mapping(), &blocked().live_only(false))
        .await
        .unwrap();
    stub.sink()
        .demote(&mapping(), true, &stamp())
        .await
        .unwrap();
    let marker = store.lock().unwrap().doc(ALIAS).unwrap();
    let ensured = sink::Records::ensure_index_as(&stub.sink(), &mapping(), &stamp()).await;
    let deletes: Vec<String> = stub
        .lines()
        .into_iter()
        .filter(|line| line.starts_with("DELETE /users_00000001_3"))
        .collect();
    assert!(
        deletes.is_empty(),
        "marker after the demotion {marker}; ensure {ensured:?}; the serving generation deleted: {deletes:?}; generations left {:?}",
        store.lock().unwrap().generations
    );
}

/// The review of round 13, H: a marker rewrite (before a generation create
/// or a reindex) reads the meta index; just created by a write elsewhere,
/// its shard may still be recovering, and the read is refused. The rewrite
/// waits for the meta index's primary, then reads again and writes.
#[tokio::test]
async fn a_marker_rewrite_waits_for_a_meta_index_still_recovering() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let routed = Arc::clone(&store);
    let refused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let once = Arc::clone(&refused);
    let stub = Stub::start(move |line, body| {
        if line.contains("/_mget") && !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
            let error = json!({ "type": "no_shard_available_action_exception", "reason": "No shard available for [MultiGetShardRequest]" });
            return (
                200,
                json!({ "docs": [
                    { "_id": ALIAS, "error": error },
                    { "_id": DETACH, "error": error },
                    { "_id": "stuck:users_00000001", "error": error },
                ] })
                .to_string(),
            );
        }
        if line.starts_with("GET /_cluster/health/flusso_meta") {
            return (200, json!({ "status": "yellow", "timed_out": false }).to_string());
        }
        routed.lock().unwrap().answer(line, body)
    })
    .await;
    stub.sink()
        .rewrite_marker(ALIAS, 4, None, Some(&stamp()))
        .await
        .unwrap();
    assert!(
        stub.lines()
            .iter()
            .any(|line| line.starts_with("GET /_cluster/health/flusso_meta")),
        "{:?}",
        stub.lines()
    );
    assert_eq!(
        store.lock().unwrap().doc(ALIAS).unwrap()["active_generation"],
        4
    );
}

/// H's wait is bounded: a meta index whose primary doesn't come up in time
/// fails the rewrite, which the sink engine stages again on its backoff.
#[tokio::test]
async fn a_marker_rewrite_fails_when_the_meta_index_never_gets_its_primary() {
    let stub = Stub::start(move |line, _| {
        if line.contains("/_mget") {
            return (
                503,
                json!({ "error": { "type": "no_shard_available_action_exception" }, "status": 503 })
                    .to_string(),
            );
        }
        if line.starts_with("GET /_cluster/health/flusso_meta") {
            return (
                408,
                json!({ "status": "red", "timed_out": true }).to_string(),
            );
        }
        (500, "{}".to_owned())
    })
    .await;
    let error = stub
        .sink()
        .rewrite_marker(ALIAS, 4, None, Some(&stamp()))
        .await
        .unwrap_err();
    assert!(
        matches!(error, sink::SinkError::Read(_)),
        "a read: {error:?}"
    );
    assert!(error.to_string().contains("primary shard"), "{error}");
}

/// The review of round 13, m-4: a live-only detach moves no marker, so the
/// generations are only its best-effort deletes: a failed listing doesn't
/// fail it. A rebuildable detach moves the marker past every generation and
/// still needs the listing.
#[tokio::test]
async fn a_failed_listing_fails_only_a_detach_that_moves_the_marker() {
    for live_only in [true, false] {
        let store = Arc::new(Mutex::new(Store::seeded()));
        let routed = Arc::clone(&store);
        let stub = Stub::start(move |line, body| {
            if line.contains("/_cat/indices/") {
                return (500, json!({ "error": "listing refused" }).to_string());
            }
            routed.lock().unwrap().answer(line, body)
        })
        .await;
        let detached = stub
            .sink()
            .detach(&mapping(), &blocked().live_only(live_only).stamp(stamp()))
            .await;
        assert_eq!(
            detached.is_ok(),
            live_only,
            "live only {live_only}: {detached:?}"
        );
        let record = store.lock().unwrap().doc(DETACH);
        assert_eq!(
            record.is_some(),
            live_only,
            "live only {live_only}: {record:?}"
        );
    }
}

/// The review of round 14, m-B, through the real requests: between staging
/// reading the marker and recording the generation's uuid (after creating
/// generation 4, or finding it with no uuid recorded), another process's
/// live-only demotion lands: the marker back on serving generation 3,
/// unseeded, no uuid. Generation 4's uuid must not be recorded on that
/// marker: the next `ensure_index` would read generation 3 as not the one
/// flusso created.
#[tokio::test]
async fn a_uuid_is_recorded_only_on_the_marker_naming_its_generation() {
    for created in [true, false] {
        let mut store = Store::seeded();
        store.put(ALIAS, json!({ "active_generation": 4, "seeded": false }));
        if created {
            store.generations.remove("users_00000001_4");
        }
        let store = Arc::new(Mutex::new(store));
        let routed = Arc::clone(&store);
        let demoted = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let once = Arc::clone(&demoted);
        let demote = move |store: &mut Store| {
            if !once.swap(true, std::sync::atomic::Ordering::SeqCst) {
                store.put(ALIAS, json!({ "active_generation": 3, "seeded": false }));
            }
        };
        let stub = Stub::start(move |line, body| {
            let (method, target) = line.split_once(' ').unwrap();
            let path = target.split_once('?').map_or(target, |(path, _)| path);
            let mut store = routed.lock().unwrap();
            if method == "GET" && path.starts_with("/flusso_meta/_doc/") {
                let id = path.trim_start_matches("/flusso_meta/_doc/");
                return match store.doc(id) {
                    Some(source) => (
                        200,
                        json!({ "_id": id, "found": true, "_source": source }).to_string(),
                    ),
                    None => (404, json!({ "found": false }).to_string()),
                };
            }
            if method == "GET" && path.ends_with("/_settings/index.uuid") {
                let index = path.trim_start_matches('/').split('/').next().unwrap();
                let uuid = format!("u{}", index.rsplit('_').next().unwrap());
                if !store.generations.contains(index) {
                    return (404, "{}".to_owned());
                }
                if !created && index.ends_with("_4") {
                    demote(&mut store);
                }
                return (
                    200,
                    json!({ index: { "settings": { "index": { "uuid": uuid } } } }).to_string(),
                );
            }
            if method == "PUT" && path == "/users_00000001_4" {
                store.generations.insert("users_00000001_4".to_owned());
                demote(&mut store);
                return (200, json!({ "acknowledged": true }).to_string());
            }
            if method == "GET" && path.ends_with("/_settings/index.number_of_shards") {
                return (404, "{}".to_owned());
            }
            if path.starts_with("/_aliases") {
                return (200, json!({ "acknowledged": true }).to_string());
            }
            store.answer(line, body)
        })
        .await;
        let ensured = sink::Records::ensure_index_as(&stub.sink(), &mapping(), &stamp()).await;
        let marker = store.lock().unwrap().doc(ALIAS).unwrap();
        assert_eq!(
            marker["active_generation"],
            3,
            "created {created}: {marker}; {ensured:?}; {:?}",
            stub.lines()
        );
        assert_ne!(
            marker.get("uuid"),
            Some(&json!("u4")),
            "created {created}: generation 4's uuid landed on the marker naming 3: {marker}"
        );
    }
}

/// The review of round 14, m-C: a generation the alias serves is refreshed
/// once at a start whatever its seeded flag: a live-only demotion leaves the
/// serving generation unseeded, and with `refresh_interval = "-1"` what an
/// earlier run wrote there stays unsearchable until the next caught-up flush
/// without it. An unseeded generation the alias doesn't serve (a rebuild's)
/// isn't.
#[tokio::test]
async fn a_start_refreshes_the_generation_reads_see_whatever_its_seeded_flag() {
    for (marker, refreshed) in [
        (
            json!({ "active_generation": 3, "seeded": false, "uuid": "u3" }),
            true,
        ),
        (
            json!({ "active_generation": 4, "seeded": false, "uuid": "u4" }),
            false,
        ),
    ] {
        let mut store = Store::seeded();
        store.generations.insert("users_00000001_4".to_owned());
        store.put(ALIAS, marker.clone());
        let store = Arc::new(Mutex::new(store));
        let routed = Arc::clone(&store);
        let stub = Stub::start(move |line, body| {
            let (method, target) = line.split_once(' ').unwrap();
            let path = target.split_once('?').map_or(target, |(path, _)| path);
            let store = routed.lock().unwrap();
            if method == "GET" && path.starts_with("/flusso_meta/_doc/") {
                let id = path.trim_start_matches("/flusso_meta/_doc/");
                return match store.doc(id) {
                    Some(source) => (
                        200,
                        json!({ "_id": id, "found": true, "_source": source }).to_string(),
                    ),
                    None => (404, json!({ "found": false }).to_string()),
                };
            }
            if method == "GET" && path.ends_with("/_settings/index.uuid") {
                let index = path.trim_start_matches('/').split('/').next().unwrap();
                let uuid = format!("u{}", index.rsplit('_').next().unwrap());
                return (
                    200,
                    json!({ index: { "settings": { "index": { "uuid": uuid } } } }).to_string(),
                );
            }
            if method == "POST" && path.ends_with("/_refresh") {
                return (200, json!({ "_shards": { "failed": 0 } }).to_string());
            }
            if method == "GET" && path.ends_with("/_settings/index.number_of_shards") {
                return (404, "{}".to_owned());
            }
            if path.starts_with("/_aliases") {
                return (200, json!({ "acknowledged": true }).to_string());
            }
            drop(store);
            routed.lock().unwrap().answer(line, body)
        })
        .await;
        sink::Records::ensure_index_as(&stub.sink(), &mapping(), &stamp())
            .await
            .unwrap();
        let generation = marker["active_generation"].as_u64().unwrap();
        let sent = stub
            .lines()
            .iter()
            .any(|line| line.starts_with(&format!("POST /users_00000001_{generation}/_refresh")));
        assert_eq!(sent, refreshed, "{marker}: {:?}", stub.lines());
    }
}

/// The review of round 14, m-D: only a shard not ready yet makes a marker
/// rewrite wait; any other refused read fails it at once, with no cluster
/// health asked.
#[tokio::test]
async fn a_marker_rewrite_waits_only_for_a_shard_not_ready() {
    let stub = Stub::start(move |line, _| {
        if line.contains("/_mget") {
            return (
                500,
                json!({ "error": { "type": "illegal_argument_exception" }, "status": 500 })
                    .to_string(),
            );
        }
        (
            200,
            json!({ "status": "yellow", "timed_out": false }).to_string(),
        )
    })
    .await;
    let refused = stub
        .sink()
        .rewrite_marker(ALIAS, 4, None, Some(&stamp()))
        .await;
    assert!(refused.is_err(), "{refused:?}");
    assert!(
        !stub
            .lines()
            .iter()
            .any(|line| line.contains("/_cluster/health")),
        "{:?}",
        stub.lines()
    );
}

/// m-D: the wait OpenSearch is asked for stays under the client's timeout,
/// so OpenSearch answers first.
#[test]
fn the_meta_primary_wait_stays_under_the_client_timeout() {
    use crate::detachment::record::meta_primary_wait;
    use std::time::Duration;
    for (timeout, wait) in [
        (Duration::from_secs(30), Duration::from_secs(24)),
        (Duration::from_secs(2), Duration::from_secs(1)),
        (Duration::from_secs(1), Duration::from_millis(500)),
        (Duration::from_secs(300), Duration::from_secs(30)),
    ] {
        assert_eq!(meta_primary_wait(timeout), wait, "{timeout:?}");
    }
}
