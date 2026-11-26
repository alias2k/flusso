//! Writing a detach against a stand-in meta store: the kill matrix over its
//! writes, the conditional writes, and what a staging reads back.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use kernel::{ColumnName, GenericValue, RowKey};
use serde_json::{Value, json};
use sink::{Detach, DetachRecord, MarkerAt, Recorded, Sink};

use super::{ALIAS, DETACH, Stub, mapping};

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
}

impl Store {
    /// A seeded index on generation 3, with a stale partial generation 4.
    fn seeded() -> Self {
        let mut store = Self::default();
        store.put(
            ALIAS,
            json!({ "active_generation": 3, "seeded": true, "uuid": "u3" }),
        );
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
        let read = next.sink().detachment(&mapping()).await.unwrap();
        let store = store.lock().unwrap();
        assert!(
            store.generations.contains("users_00000001_3"),
            "after {done} writes the serving generation is kept"
        );
        match done {
            0 => {
                assert!(read.is_empty(), "nothing kept: {read:?}");
                assert_eq!(
                    store.doc(ALIAS).unwrap(),
                    json!({ "active_generation": 3, "seeded": true, "uuid": "u3" })
                );
            }
            1 => {
                assert_eq!(read.restored, Some(3), "{read:?}");
                assert_eq!(read.record, None);
                assert_eq!(
                    store.doc(ALIAS).unwrap(),
                    json!({ "active_generation": 3, "seeded": true, "uuid": "u3" }),
                    "the marker is put back as it was"
                );
            }
            _ => {
                assert_eq!(
                    read.record,
                    Some(DetachRecord::Detached { live_only: false }),
                    "after {done} writes"
                );
                assert_eq!(read.marker, Some(MarkerAt::Elsewhere { seeded: false }));
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
            "detaching_from": { "generation": 3, "seeded": true, "uuid": "u3" },
        })
    );
    let record = store.doc(DETACH).unwrap();
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
    let read = stub.sink().detachment(&mapping()).await.unwrap();
    assert_eq!(
        read.record,
        Some(DetachRecord::Detached { live_only: true })
    );
    assert_eq!(read.marker, Some(MarkerAt::Serving { seeded: true }));
    assert_eq!(
        store.lock().unwrap().doc(ALIAS).unwrap(),
        json!({ "active_generation": 3, "seeded": true, "uuid": "u3" })
    );
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

/// A record is deleted only at the version read; a later write wins and the
/// delete fails; a record already gone is no error.
#[tokio::test]
async fn forgetting_a_record_deletes_only_the_version_read() {
    let store = Arc::new(Mutex::new(Store::seeded()));
    let stub = serve(&store).await;
    let sink = stub.sink();
    sink.detach(&mapping(), &blocked()).await.unwrap();
    let read = sink.detachment(&mapping()).await.unwrap();
    let version = read.version.unwrap();

    let newer = store.lock().unwrap().doc(DETACH).unwrap();
    store.lock().unwrap().put(DETACH, newer);
    assert!(sink.forget_detachment(&mapping(), &version).await.is_err());
    assert!(
        store.lock().unwrap().doc(DETACH).is_some(),
        "the newer record stays"
    );

    let version = sink.detachment(&mapping()).await.unwrap().version.unwrap();
    sink.forget_detachment(&mapping(), &version).await.unwrap();
    assert!(store.lock().unwrap().doc(DETACH).is_none());
    sink.forget_detachment(&mapping(), &version).await.unwrap();
}
