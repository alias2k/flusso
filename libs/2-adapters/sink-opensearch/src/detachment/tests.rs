use std::sync::{Arc, Mutex};

use kernel::{ContentHash, IndexMapping, IndexName, SinkName};
use serde_json::{Value, json};
use sink::{DetachRecord, Detachment, Sink, StuckClock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use crate::{OpensearchConfig, OpensearchSink, TextAnalysis};

/// The hash alias of [`mapping`]'s index, and its two record ids.
const ALIAS: &str = "users_00000001";
const DETACH: &str = "detach:users_00000001";
const STUCK: &str = "stuck:users_00000001";

fn mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new("users").unwrap(),
        hash: ContentHash::new(1),
        fields: Vec::new(),
    }
}

/// One request as the stub saw it: `"<METHOD> <path>"` and its body.
type Seen = (String, String);

/// What the stub answers a request: its status and body.
type Answer = Box<dyn Fn(&str, &str) -> (u16, String) + Send + Sync>;

/// A stand-in for OpenSearch answering every request with `answer(line,
/// body)`, recording each.
struct Stub {
    url: String,
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl Stub {
    async fn start(answer: impl Fn(&str, &str) -> (u16, String) + Send + Sync + 'static) -> Self {
        let answer: Arc<Answer> = Arc::new(Box::new(answer));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::default();
        let recorded = Arc::clone(&seen);
        tokio::spawn(async move {
            loop {
                let (socket, _) = listener.accept().await.unwrap();
                let answer = Arc::clone(&answer);
                let recorded = Arc::clone(&recorded);
                tokio::spawn(async move {
                    let mut socket = tokio::io::BufStream::new(socket);
                    while let Some((line, body)) = read_request(&mut socket).await {
                        recorded.lock().unwrap().push((line.clone(), body.clone()));
                        let (status, answer) = answer(&line, &body);
                        let response = format!(
                            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{answer}",
                            answer.len()
                        );
                        socket.write_all(response.as_bytes()).await.unwrap();
                        socket.flush().await.unwrap();
                    }
                });
            }
        });
        Self { url, seen }
    }

    fn lines(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|(line, _)| line.clone())
            .collect()
    }

    fn body(&self, at: usize) -> Value {
        serde_json::from_str(&self.seen.lock().unwrap()[at].1).unwrap()
    }

    fn sink(&self) -> OpensearchSink {
        let config = OpensearchConfig {
            url: kernel::Secret::Value(self.url.clone()),
            username: None,
            password: None,
            tls_verify: true,
            ca_file: None,
            batch_size: 1000,
            max_bytes: 10 * 1024 * 1024,
            timeout_secs: 5,
            max_retries: 0,
            pipeline: None,
            number_of_shards: 1,
            number_of_replicas: 0,
            refresh_interval: "1s".to_owned(),
            text_analysis: TextAnalysis::Builtin,
            auto_subfields: true,
            index_settings: Default::default(),
        };
        OpensearchSink::from_config(&SinkName::try_new("primary").unwrap(), &config).unwrap()
    }
}

async fn read_request(
    socket: &mut tokio::io::BufStream<tokio::net::TcpStream>,
) -> Option<(String, String)> {
    let mut length = 0usize;
    let mut request_line = String::new();
    if socket.read_line(&mut request_line).await.ok()? == 0 {
        return None;
    }
    let request_line = request_line
        .trim_end()
        .trim_end_matches(" HTTP/1.1")
        .to_owned();
    loop {
        let mut line = String::new();
        if socket.read_line(&mut line).await.ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("content-length")
        {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    socket.read_exact(&mut body).await.ok()?;
    Some((request_line, String::from_utf8(body).unwrap()))
}

/// An `_mget` answer: each of `docs` in the order the three ids are asked.
fn mget(docs: [Value; 3]) -> (u16, String) {
    (200, json!({ "docs": docs }).to_string())
}

fn missing(id: &str) -> Value {
    json!({ "_index": "flusso_meta", "_id": id, "found": false })
}

fn found(id: &str, source: Value) -> Value {
    json!({
        "_index": "flusso_meta", "_id": id, "_version": 1,
        "_seq_no": 7, "_primary_term": 2, "found": true, "_source": source,
    })
}

fn index_not_found(id: &str) -> Value {
    json!({ "_index": "flusso_meta", "_id": id, "error": {
        "root_cause": [{ "type": "index_not_found_exception", "reason": "no such index [flusso_meta]" }],
        "type": "index_not_found_exception", "reason": "no such index [flusso_meta]",
    } })
}

fn seeded_marker() -> Value {
    found(
        ALIAS,
        json!({ "active_generation": 1, "seeded": true, "uuid": "u1" }),
    )
}

async fn read(
    answer: impl Fn(&str, &str) -> (u16, String) + Send + Sync + 'static,
) -> (Stub, sink::Result<Detachment>) {
    let stub = Stub::start(answer).await;
    let read = stub.sink().detachment(&mapping()).await;
    (stub, read)
}

/// One `_mget` of the seed marker and both records, in that order, from the
/// meta index: the only request when nothing is kept.
#[tokio::test]
async fn one_request_reads_the_marker_and_both_records() {
    let (stub, read) = read(|_, _| mget([seeded_marker(), missing(DETACH), missing(STUCK)])).await;
    assert_eq!(read.unwrap(), Detachment::default());
    assert_eq!(stub.lines(), ["POST /flusso_meta/_mget"]);
    assert_eq!(stub.body(0), json!({ "ids": [ALIAS, DETACH, STUCK] }));
}

/// Only "nothing kept" reads as empty: the meta index missing, answered as a
/// `404` or document by document, or no document of either id.
#[tokio::test]
async fn a_missing_meta_index_or_documents_read_as_nothing_kept() {
    let answers: [fn() -> (u16, String); 3] = [
        || {
            (
                404,
                json!({ "error": { "type": "index_not_found_exception", "reason": "no such index [flusso_meta]" }, "status": 404 })
                    .to_string(),
            )
        },
        || {
            mget([
                index_not_found(ALIAS),
                index_not_found(DETACH),
                index_not_found(STUCK),
            ])
        },
        || mget([missing(ALIAS), missing(DETACH), missing(STUCK)]),
    ];
    for answer in answers {
        let (_stub, read) = read(move |_, _| answer()).await;
        assert_eq!(read.unwrap(), Detachment::default());
    }
}

/// Any other answer fails the read, naming what OpenSearch said: it is never
/// taken for "no record" (H6).
#[tokio::test]
async fn any_other_answer_fails_the_read() {
    let answers: [(fn() -> (u16, String), &str); 10] = [
        (|| (404, json!({ "error": "not found" }).to_string()), "404"),
        (
            || (404, "<html><body>Not Found</body></html>".to_owned()),
            "404",
        ),
        (
            || mget([json!({}), missing(DETACH), missing(STUCK)]),
            "neither found nor an error",
        ),
        (
            || {
                mget([
                    seeded_marker(),
                    json!({ "_id": DETACH, "found": "no" }),
                    missing(STUCK),
                ])
            },
            "neither found nor an error",
        ),
        (
            || {
                mget([
                    seeded_marker(),
                    missing("detach:orders_00000001"),
                    missing(STUCK),
                ])
            },
            "detach:orders_00000001",
        ),
        (
            || (503, json!({ "error": "unavailable" }).to_string()),
            "503",
        ),
        (
            || (429, json!({ "error": "too many requests" }).to_string()),
            "429",
        ),
        (|| (500, "not json".to_owned()), "500"),
        (
            || {
                mget([
                    seeded_marker(),
                    json!({ "_id": DETACH, "error": { "type": "no_shard_available_action_exception", "reason": "no shard" } }),
                    missing(STUCK),
                ])
            },
            "no_shard_available_action_exception",
        ),
        (|| (200, json!({ "docs": [] }).to_string()), "3 documents"),
    ];
    for (answer, named) in answers {
        let (_stub, read) = read(move |_, _| answer()).await;
        let error = read.expect_err(named).to_string();
        assert!(error.contains(named), "{named}: {error}");
    }
}

#[tokio::test]
async fn an_unreachable_meta_index_fails_the_read() {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    let stub = Stub {
        url: format!("http://127.0.0.1:{port}"),
        seen: Arc::default(),
    };
    assert!(stub.sink().detachment(&mapping()).await.is_err());
}

/// Each record state this version knows, with `live_only`.
#[tokio::test]
async fn a_record_is_read_with_its_state() {
    for (state, live_only, expected) in [
        (
            "detached",
            false,
            DetachRecord::Detached { live_only: false },
        ),
        ("detached", true, DetachRecord::Detached { live_only: true }),
        (
            "rebuilding",
            false,
            DetachRecord::Rebuilding { live_only: false },
        ),
    ] {
        let (stub, read) = read(move |_, _| {
            mget([
                seeded_marker(),
                found(
                    DETACH,
                    json!({ "state": state, "live_only": live_only, "serving": 1, "attempts": 2 }),
                ),
                missing(STUCK),
            ])
        })
        .await;
        let read = read.unwrap();
        assert_eq!(read.record, Some(expected), "{state}");
        assert_eq!(read.restored, None);
        assert_eq!(stub.lines().len(), 1, "a read writes nothing");
    }
}

/// A record of a state this version doesn't know, or malformed, is read as
/// unreadable with why: never an error, never "no record" (O24).
#[tokio::test]
async fn a_record_of_another_state_or_malformed_is_unreadable() {
    for (source, why) in [
        (json!({ "state": "frozen" }), "frozen"),
        (json!({ "live_only": true }), "state"),
        (
            json!({ "state": "detached", "live_only": "yes" }),
            "live_only",
        ),
        (json!("not an object"), "object"),
    ] {
        let (_stub, read) = read(move |_, _| {
            mget([
                seeded_marker(),
                found(DETACH, source.clone()),
                missing(STUCK),
            ])
        })
        .await;
        match read.unwrap().record {
            Some(DetachRecord::Unreadable { reason }) => {
                assert!(reason.contains(why), "{why}: {reason}");
            }
            other => panic!("{why}: read as {other:?}"),
        }
    }
}

/// A stuck clock is read as written; slice 1 acts on nothing of it.
#[tokio::test]
async fn a_stuck_clock_is_read() {
    let (_stub, read) = read(|_, _| {
        mget([
            seeded_marker(),
            missing(DETACH),
            found(
                STUCK,
                json!({ "since": "2026-11-24T08:00:00Z", "seen_at": "2026-11-24T08:10:00Z" }),
            ),
        ])
    })
    .await;
    let read = read.unwrap();
    assert_eq!(
        read.stuck,
        Some(StuckClock::new(
            Some("2026-11-24T08:00:00Z".to_owned()),
            Some("2026-11-24T08:10:00Z".to_owned())
        ))
    );
    assert_eq!(read.record, None);
}

fn half_detached_marker() -> Value {
    found(
        ALIAS,
        json!({
            "active_generation": 2, "seeded": false, "walked": null, "uuid": null,
            "detaching_from": { "generation": 1, "seeded": true, "uuid": "u1" },
        }),
    )
}

/// A marker a detach moved (`detaching_from`) with no record beside it: the
/// detach died between its two writes. The marker is put back on the
/// generation it came from, as it was, conditionally on the marker read, so
/// a write meanwhile wins (H5).
#[tokio::test]
async fn a_half_detached_marker_with_no_record_is_put_back() {
    let (stub, read) = read(|line, _| {
        if line.starts_with("POST /flusso_meta/_mget") {
            mget([half_detached_marker(), missing(DETACH), missing(STUCK)])
        } else {
            (200, json!({ "result": "updated" }).to_string())
        }
    })
    .await;
    let read = read.unwrap();
    assert_eq!(read.restored, Some(1));
    assert_eq!(read.record, None);
    assert_eq!(
        stub.lines(),
        [
            "POST /flusso_meta/_mget".to_owned(),
            format!("PUT /flusso_meta/_doc/{ALIAS}?if_seq_no=7&if_primary_term=2"),
        ]
    );
    assert_eq!(
        stub.body(1),
        json!({ "active_generation": 1, "seeded": true, "uuid": "u1" })
    );
}

/// A write between the read and the put-back wins: the conflict fails the
/// read, and the step is retried with a fresh read.
#[tokio::test]
async fn a_put_back_that_conflicts_fails_the_read() {
    let (_stub, read) = read(|line, _| {
        if line.starts_with("POST /flusso_meta/_mget") {
            mget([half_detached_marker(), missing(DETACH), missing(STUCK)])
        } else {
            (
                409,
                json!({ "error": { "type": "version_conflict_engine_exception" } }).to_string(),
            )
        }
    })
    .await;
    let error = read.unwrap_err().to_string();
    assert!(error.contains("409"), "{error}");
}

/// Beside a record, the marker is the record's business (slice 2): nothing is
/// put back.
#[tokio::test]
async fn a_half_detached_marker_beside_a_record_is_left_alone() {
    let (stub, read) = read(|_, _| {
        mget([
            half_detached_marker(),
            found(
                DETACH,
                json!({ "state": "detached", "live_only": false, "serving": 1 }),
            ),
            missing(STUCK),
        ])
    })
    .await;
    let read = read.unwrap();
    assert_eq!(read.restored, None);
    assert_eq!(
        read.record,
        Some(DetachRecord::Detached { live_only: false })
    );
    assert_eq!(stub.lines().len(), 1);
}

/// A `detaching_from` this version can't read (no generation, or not an
/// object) is put back nowhere and fails nothing: it is reported as
/// unreadable and the index is staged as before (O24), which rebuilds into
/// the generation the marker names. Failing the read would restage the index
/// until someone edits the marker.
#[tokio::test]
async fn a_malformed_detaching_from_is_reported_and_put_back_nowhere() {
    for detaching_from in [json!({ "seeded": true }), json!(3), json!("generation 1")] {
        let (stub, read) = read(move |_, _| {
            mget([
                found(
                    ALIAS,
                    json!({ "active_generation": 2, "seeded": false, "detaching_from": detaching_from.clone() }),
                ),
                missing(DETACH),
                missing(STUCK),
            ])
        })
        .await;
        let read = read.unwrap();
        assert_eq!(read.restored, None);
        assert!(
            read.unreadable_marker
                .as_deref()
                .is_some_and(|why| why.contains("detaching_from")),
            "{read:?}"
        );
        assert_eq!(stub.lines().len(), 1, "nothing written");
    }
}

/// A prefixed deployment reads its own meta index and ids.
#[tokio::test]
async fn a_prefixed_sink_reads_its_own_meta_index() {
    let stub = Stub::start(|_, _| {
        mget([
            missing("dev_users_00000001"),
            missing("detach:dev_users_00000001"),
            missing("stuck:dev_users_00000001"),
        ])
    })
    .await;
    let sink = stub.sink().with_index_prefix("dev_");
    sink.detachment(&mapping()).await.unwrap();
    assert_eq!(stub.lines(), ["POST /dev_flusso_meta/_mget"]);
    assert_eq!(
        stub.body(0),
        json!({ "ids": ["dev_users_00000001", "detach:dev_users_00000001", "stuck:dev_users_00000001"] })
    );
}
