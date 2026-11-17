use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use kernel::{Envelope, GenericValue, IndexName, Position, SinkName};
use sink::Sink;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::request_error;
use crate::{OpensearchConfig, OpensearchSink, TextAnalysis};

/// A real connection failure: a request to a local port nothing listens on.
async fn refused(url_for_port: impl Fn(u16) -> String) -> reqwest::Error {
    let port = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.local_addr().unwrap().port()
    };
    reqwest::Client::new()
        .get(url_for_port(port))
        .send()
        .await
        .expect_err("nothing listens on the port")
}

fn deepest_cause(error: &reqwest::Error) -> String {
    let mut cause: &dyn std::error::Error = error;
    while let Some(next) = cause.source() {
        cause = next;
    }
    cause.to_string()
}

#[tokio::test]
async fn the_source_chain_is_rendered_after_the_context() {
    let error = refused(|port| format!("http://127.0.0.1:{port}/_cat/indices")).await;
    let deepest = deepest_cause(&error);
    assert!(!error.to_string().contains(&deepest), "{error}");

    let message = request_error("listing indices failed", error).to_string();

    assert!(
        message
            .contains("listing indices failed: error sending request for url (http://127.0.0.1:"),
        "{message}"
    );
    assert!(message.ends_with(&deepest), "{message}");
}

#[tokio::test]
async fn credentials_in_the_url_never_reach_the_message() {
    let sent = refused(|port| format!("http://flusso-user:s3cret-pass@127.0.0.1:{port}/")).await;
    let message = request_error("index check failed", sent).to_string();
    assert!(!message.contains("s3cret-pass"), "{message}");
    assert!(!message.contains("flusso-user"), "{message}");

    let with_userinfo = refused(|port| format!("http://127.0.0.1:{port}/"))
        .await
        .with_url(
            "http://flusso-user:s3cret-pass@127.0.0.1:9200/"
                .parse()
                .unwrap(),
        );
    assert!(
        with_userinfo.to_string().contains("s3cret-pass"),
        "{with_userinfo}"
    );

    let message = request_error("index check failed", with_userinfo).to_string();

    assert!(!message.contains("s3cret-pass"), "{message}");
    assert!(!message.contains("flusso-user"), "{message}");
    assert!(message.contains("(http://127.0.0.1:9200/)"), "{message}");
}

/// A stand-in for OpenSearch's `_bulk`: answers each action with the status
/// `answer(request, id)` picks (`request` counts from 0) and records the ids
/// each request carried.
struct StubBulk {
    url: String,
    requests: Arc<Mutex<Vec<Vec<String>>>>,
}

async fn stub_bulk(answer: impl Fn(usize, &str) -> u16 + Send + Sync + 'static) -> StubBulk {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let requests: Arc<Mutex<Vec<Vec<String>>>> = Arc::default();
    let answer = Arc::new(answer);
    let recorded = Arc::clone(&requests);
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let answer = Arc::clone(&answer);
            let recorded = Arc::clone(&recorded);
            tokio::spawn(async move {
                let mut socket = tokio::io::BufStream::new(socket);
                while let Some(body) = read_request(&mut socket).await {
                    let ids = bulk_ids(&body);
                    let request = {
                        let mut recorded = recorded.lock().unwrap();
                        recorded.push(ids.clone());
                        recorded.len() - 1
                    };
                    let items: Vec<serde_json::Value> = ids
                        .iter()
                        .map(|id| bulk_item(id, answer(request, id)))
                        .collect();
                    let errors = items.iter().any(|item| item["index"]["status"] != 201);
                    let body = serde_json::json!({ "took": 1, "errors": errors, "items": items })
                        .to_string();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.flush().await.unwrap();
                }
            });
        }
    });
    StubBulk { url, requests }
}

/// The body of the next request on `socket`, or `None` once the client closed it.
async fn read_request(socket: &mut tokio::io::BufStream<tokio::net::TcpStream>) -> Option<String> {
    let mut length = 0usize;
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
    Some(String::from_utf8(body).unwrap())
}

/// The `_id` of every action in an NDJSON bulk body, skipping source lines.
fn bulk_ids(body: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut lines = body.lines();
    while let Some(line) = lines.next() {
        let action: serde_json::Value = serde_json::from_str(line).unwrap();
        if let Some(index) = action.get("index") {
            ids.push(index["_id"].as_str().unwrap().to_owned());
            lines.next();
        } else if let Some(delete) = action.get("delete") {
            ids.push(delete["_id"].as_str().unwrap().to_owned());
        }
    }
    ids
}

fn bulk_item(id: &str, status: u16) -> serde_json::Value {
    let error = match status {
        201 => {
            return serde_json::json!({ "index": { "_index": "users", "_id": id, "status": 201 } });
        }
        429 => (
            "rejected_execution_exception",
            "rejected execution of org.opensearch.action.support.replication.TransportWriteAction on QueueResizingOpenSearchThreadPoolExecutor[name = node/write, queue capacity = 10000]",
        ),
        503 => (
            "unavailable_shards_exception",
            "primary shard is not active",
        ),
        _ => ("mapper_parsing_exception", "failed to parse field [age]"),
    };
    serde_json::json!({ "index": {
        "_index": "users", "_id": id, "status": status,
        "error": { "type": error.0, "reason": error.1 },
    } })
}

fn sink(url: &str, max_retries: u32) -> OpensearchSink {
    let config = OpensearchConfig {
        url: kernel::Secret::Value(url.to_owned()),
        username: None,
        password: None,
        tls_verify: true,
        ca_file: None,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 5,
        max_retries,
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

fn upsert(id: &str) -> Envelope {
    let document = GenericValue::Map(BTreeMap::from([(
        "name".to_owned(),
        GenericValue::String("x".to_owned()),
    )]));
    Envelope::upsert(
        IndexName::try_new("users").unwrap(),
        id,
        document,
        Some(Position(0)),
        chrono::Utc::now(),
    )
}

fn upserts(ids: &[&str]) -> Vec<Envelope> {
    ids.iter().map(|id| upsert(id)).collect()
}

/// A full write queue refuses some items of a request; the write resends those
/// alone until they are written, and reports the one real rejection once.
#[tokio::test]
async fn items_refused_as_busy_are_resent_until_written() {
    let stub = stub_bulk(|request, id| match (request, id) {
        (_, "3") => 400,
        (0, "2") => 429,
        (0 | 1, "4") => 503,
        _ => 201,
    })
    .await;
    let sink = sink(&stub.url, 3);
    let report = sink
        .write(&upserts(&["1", "2", "3", "4"]), false)
        .await
        .unwrap();

    let rejected: Vec<(&str, &str)> = report
        .rejected
        .iter()
        .map(|doc| (doc.index.as_str(), doc.id.as_str()))
        .collect();
    assert_eq!(rejected, [("users", "3")], "{:?}", report.rejected);
    assert_eq!(
        *stub.requests.lock().unwrap(),
        [vec!["1", "2", "3", "4"], vec!["2", "4"], vec!["4"],],
        "each retry resends only what was refused as busy"
    );
}

/// Items still refused as busy once the retries are spent fail the write: the
/// batch stays unacknowledged and comes back, never handed to `on_error`.
#[tokio::test]
async fn items_still_busy_after_the_retries_fail_the_write() {
    let stub = stub_bulk(|_, id| if id == "2" { 429 } else { 201 }).await;
    let sink = sink(&stub.url, 1);

    let error = sink
        .write(&upserts(&["1", "2"]), false)
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("1 document"), "{error}");
    assert!(error.contains("429"), "{error}");
    assert_eq!(*stub.requests.lock().unwrap(), [vec!["1", "2"], vec!["2"]],);
}

#[test]
fn a_document_written_twice_in_one_request_is_caught() {
    use crate::bulk::BulkAction;
    let index = |id: &str| BulkAction::Index {
        index: "users_1".to_owned(),
        id: id.to_owned(),
        doc: "{}".to_owned(),
        require_alias: false,
    };
    let delete = |index: &str, id: &str| BulkAction::Delete {
        index: index.to_owned(),
        id: id.to_owned(),
    };

    assert!(super::one_write_per_document(&[
        index("1"),
        index("2"),
        delete("orders_1", "1")
    ]));
    assert!(!super::one_write_per_document(&[
        index("1"),
        delete("users_1", "1")
    ]));
}

/// A write that fails before sending sends nothing, and keeps nothing: the
/// lane redelivers the batch whole to the restarted engine, which writes it
/// again on the same sink, so each document reaches OpenSearch once.
#[tokio::test]
async fn a_batch_written_again_after_a_failed_write_is_sent_once() {
    let stub = stub_bulk(|_, _| 201).await;
    let sink = sink(&stub.url, 0);
    let mut no_document = upsert("2");
    no_document.document = None;

    sink.write(&[upsert("1"), no_document], false)
        .await
        .unwrap_err();
    sink.write(&upserts(&["1", "2"]), false).await.unwrap();

    assert_eq!(*stub.requests.lock().unwrap(), [vec!["1", "2"]]);
}
