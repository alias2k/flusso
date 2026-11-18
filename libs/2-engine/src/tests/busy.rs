//! A document the destination refuses because it is busy (an item-level 429
//! or 503) is written later, never handed to `on_error`: the lane item that
//! carries it stays unacknowledged until it lands. Driven through the real
//! OpenSearch sink against a stand-in `_bulk` endpoint.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use kernel::{Envelope, IndexName, Position, SinkName};
use sink::{Sink, WriteReport};
use source::cdc::Continuity;
use stream::Stream;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use super::{Harness, MockSource, QuarantineObserver, sink_name, upsert, wait_for};
use crate::{FailurePolicies, FailurePolicy, Observer};

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

/// The OpenSearch sink with staging left out, so only `_bulk` reaches the stub.
#[derive(Debug)]
struct BulkOnly(sink_opensearch::OpensearchSink);

#[async_trait]
impl Sink for BulkOnly {
    async fn write(&self, envelopes: &[Envelope], caught_up: bool) -> sink::Result<WriteReport> {
        self.0.write(envelopes, caught_up).await
    }

    async fn is_seeded(&self, _: &IndexName) -> sink::Result<bool> {
        Ok(true)
    }
}

fn opensearch(url: &str) -> BulkOnly {
    let config = sink_opensearch::OpensearchConfig {
        url: kernel::Secret::Value(url.to_owned()),
        username: None,
        password: None,
        tls_verify: true,
        ca_file: None,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 5,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 0,
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    BulkOnly(
        sink_opensearch::OpensearchSink::from_config(
            &SinkName::try_new("primary").unwrap(),
            &config,
        )
        .unwrap(),
    )
}

#[tokio::test]
async fn a_batch_is_not_acked_until_its_busy_documents_are_written() {
    let stub = stub_bulk(|request, _| if request < 2 { 429 } else { 201 }).await;
    let requests = Arc::clone(&stub.requests);
    let observer = Arc::new(QuarantineObserver::default());
    let observer_dyn: Arc<dyn Observer> = Arc::clone(&observer) as Arc<dyn Observer>;
    let harness = Harness::new(
        MockSource::new(vec![upsert(1)]),
        vec![(sink_name("primary"), Arc::new(opensearch(&stub.url)))],
    )
    .map_sinks(move |engine| {
        engine
            .with_observer(Arc::clone(&observer_dyn))
            .with_failure_policies(FailurePolicies::new(FailurePolicy::Skip))
    });
    let progress = Arc::clone(&harness.progress);

    let run = tokio::spawn(harness.run(Continuity::Resumed));
    wait_for(|| requests.lock().unwrap().len() >= 2).await;
    assert_eq!(
        super::covered(&progress),
        None,
        "the document is still refused as busy: the batch has not landed"
    );

    let outcomes = run.await.unwrap();
    assert!(outcomes.iter().all(Result::is_ok), "{outcomes:?}");
    assert!(
        observer.quarantined.lock().unwrap().is_empty(),
        "a busy refusal never reaches on_error"
    );
    assert_eq!(requests.lock().unwrap().len(), 3);
    assert_eq!(super::covered(&progress), Some(Position::new(0)));
}

fn item(position: u64, ids: &[&str]) -> stream::LaneItem {
    let envelopes: Vec<Envelope> = ids
        .iter()
        .map(|id| {
            Envelope::upsert(
                super::users(),
                *id,
                kernel::GenericValue::Map(Default::default()),
                Some(Position::new(position)),
                chrono::Utc::now(),
            )
        })
        .collect();
    stream::LaneItem::Batch(stream::Batch {
        position: Some(Position::new(position)),
        changes: 1,
        envelopes: envelopes.into(),
        hold: Default::default(),
    })
}

/// With several writes in flight, an item whose document is refused as busy
/// holds back only the items sharing a document with it: a later item over
/// other documents is written meanwhile, a later item over the busy document
/// waits until it lands, and confirmation stays before the busy item.
/// Item 0 lands first, since a fresh engine writes its first item alone.
#[tokio::test]
async fn a_busy_item_holds_back_only_the_items_sharing_its_documents() {
    let refusals = std::sync::atomic::AtomicUsize::new(0);
    let stub = stub_bulk(move |_, id| {
        if id == "1" && refusals.fetch_add(1, std::sync::atomic::Ordering::SeqCst) < 2 {
            429
        } else {
            201
        }
    })
    .await;
    let requests = Arc::clone(&stub.requests);
    let name = sink_name("primary");
    let stream = Arc::new(stream_channel::ChannelStream::over(
        16,
        [super::users()],
        [name.clone()],
    ));
    let lane = stream
        .lane(&stream::LaneKey::new(super::users(), name.clone()))
        .unwrap()
        .producer;
    let progress = Arc::new(crate::Progress::of([super::users()], [name.clone()]));
    let publish = |position: u64, ids: &[&str]| {
        let item = item(position, ids);
        let lane = &lane;
        let progress = &progress;
        let name = &name;
        async move {
            lane.publish(item).await.unwrap();
            let position = Position::new(position);
            progress.published(
                &stream::LaneKey::new(super::users(), name.clone()),
                position,
                position,
            );
        }
    };
    let engine = crate::SinkEngine::new(
        name.clone(),
        Arc::new(opensearch(&stub.url)),
        Arc::clone(&stream) as Arc<dyn Stream>,
        vec![super::mapping()],
        Arc::clone(&progress),
    )
    .with_writes(4);
    let (_control, mut rx) = tokio::sync::mpsc::channel(1);
    let run = tokio::spawn(async move { engine.run(&mut rx).await });
    publish(0, &["0"]).await;
    wait_for(|| super::covered(&progress) == Some(Position::new(0))).await;

    publish(1, &["1", "3"]).await;
    publish(2, &["2"]).await;
    publish(3, &["1", "4"]).await;
    wait_for(|| requests.lock().unwrap().contains(&vec!["2".to_owned()])).await;
    assert_eq!(
        super::covered(&progress),
        Some(Position::new(0)),
        "1 is still refused as busy"
    );

    wait_for(|| stream.is_idle()).await;
    run.abort();
    let requests = requests.lock().unwrap().clone();
    let holding_one: Vec<Vec<String>> = requests
        .iter()
        .filter(|ids| ids.contains(&"1".to_owned()))
        .cloned()
        .collect();
    assert_eq!(
        holding_one,
        [vec!["1", "3"], vec!["1"], vec!["1"], vec!["1", "4"]],
        "item 3 is sent only once item 1 is written"
    );
    let at = |ids: &[&str]| {
        requests
            .iter()
            .position(|r| r.iter().map(String::as_str).eq(ids.iter().copied()))
            .unwrap()
    };
    assert!(at(&["2"]) < at(&["1", "4"]));
    assert_eq!(super::covered(&progress), Some(Position::new(3)));
}
