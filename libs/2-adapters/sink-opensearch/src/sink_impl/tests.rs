use std::sync::{Arc, Mutex};

use kernel::{IndexName, SinkName};
use serde_json::json;
use sink::Sink;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

use crate::{IndexTarget, OpensearchConfig, OpensearchSink, TextAnalysis};

/// A stand-in for OpenSearch answering each request with `answer(line,
/// count)`, `count` the number of earlier requests with the same line;
/// every request line is recorded.
async fn stub(
    answer: impl Fn(&str, usize) -> (u16, String) + Send + Sync + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&seen);
    let answer = Arc::new(answer);
    tokio::spawn(async move {
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            let recorded = Arc::clone(&recorded);
            let answer = Arc::clone(&answer);
            tokio::spawn(async move {
                let mut socket = tokio::io::BufStream::new(socket);
                while let Some(line) = read_request(&mut socket).await {
                    let count = {
                        let mut recorded = recorded.lock().unwrap();
                        let count = recorded.iter().filter(|seen| **seen == line).count();
                        recorded.push(line.clone());
                        count
                    };
                    let (status, body) = answer(&line, count);
                    let response = format!(
                        "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
                        body.len()
                    );
                    socket.write_all(response.as_bytes()).await.unwrap();
                    socket.flush().await.unwrap();
                }
            });
        }
    });
    (url, seen)
}

async fn read_request(socket: &mut tokio::io::BufStream<tokio::net::TcpStream>) -> Option<String> {
    let mut length = 0usize;
    let mut request_line = String::new();
    if socket.read_line(&mut request_line).await.ok()? == 0 {
        return None;
    }
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
    Some(
        request_line
            .trim_end()
            .trim_end_matches(" HTTP/1.1")
            .to_owned(),
    )
}

fn sink(url: &str) -> OpensearchSink {
    let config = OpensearchConfig {
        url: kernel::Secret::Value(url.to_owned()),
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

/// #479: a seed whose one-request alias swap is refused for another reason
/// (here an alias name OpenSearch rejects) reads the holders again and tries
/// the hash alias alone; that narrower attempt, refused by the old
/// generation's block, is the typed `RetireBlocked` too. The marker is never
/// written.
#[tokio::test]
async fn the_narrower_second_attempt_of_a_seed_is_retire_blocked_too() {
    let (url, seen) = stub(|line, count| match line {
        "GET /users_00000001_2/_settings/index.uuid" => (
            200,
            json!({ "users_00000001_2": { "settings": { "index": { "uuid": "u2" } } } })
                .to_string(),
        ),
        "GET /_alias/users_00000001,users" => (
            200,
            json!({ "users_00000001_1": { "aliases": { "users_00000001": {}, "users": {} } } })
                .to_string(),
        ),
        "GET /_cat/indices/users_00000001_*?h=index&format=json" => (
            200,
            json!([{ "index": "users_00000001_1" }, { "index": "users_00000001_2" }]).to_string(),
        ),
        "POST /_aliases" if count == 0 => (
            400,
            json!({ "error": { "type": "illegal_argument_exception",
                "reason": "an alias named [users] can't be added here" } })
            .to_string(),
        ),
        "POST /_aliases" => (
            403,
            json!({ "error": { "type": "cluster_block_exception",
                "reason": "index [users_00000001_1] blocked by: [FORBIDDEN/5/index read-only (api)];" } })
            .to_string(),
        ),
        _ => (200, "{}".to_owned()),
    })
    .await;
    let sink = sink(&url);
    sink.set_target(
        "users",
        IndexTarget {
            generation: "users_00000001_2".to_owned(),
            hash_alias: "users_00000001".to_owned(),
            seeded: false,
            single_shard: true,
            uuid: Some("u2".to_owned()),
        },
    );

    let refused = sink
        .mark_seeded(&IndexName::try_new("users").unwrap())
        .await;
    match refused {
        Err(sink::SinkError::RetireBlocked {
            new,
            old,
            deletable,
            ..
        }) => {
            assert_eq!(new, "users_00000001_2");
            assert_eq!(old, "users_00000001_1");
            assert!(!deletable);
        }
        other => panic!("expected RetireBlocked, got {other:?}"),
    }
    let seen = seen.lock().unwrap().clone();
    assert_eq!(
        seen.iter().filter(|line| *line == "POST /_aliases").count(),
        2,
        "both attempts were made: {seen:?}"
    );
    assert!(
        !seen.iter().any(|line| line.contains("flusso_meta")),
        "the marker is not written: {seen:?}"
    );
}

/// The review of round 11, S-4: a sink names its cluster by the
/// `cluster_uuid` `GET /` answers, so two URLs reaching one cluster (two
/// nodes, a hostname and its IP) name the same one; an answer without it,
/// or a refused request, is an error, never a guess.
#[tokio::test]
async fn a_sink_names_its_cluster_by_the_uuid_get_root_answers() {
    let (url, seen) = stub(|line, _| match line {
        "GET /" => (
            200,
            json!({ "name": "node-1", "cluster_name": "search", "cluster_uuid": "c1u2" })
                .to_string(),
        ),
        _ => (404, "{}".to_owned()),
    })
    .await;
    assert_eq!(sink(&url).cluster_uuid().await.unwrap(), "c1u2");
    assert_eq!(*seen.lock().unwrap(), ["GET /"]);

    let (url, _) = stub(|_, _| (200, json!({ "name": "node-1" }).to_string())).await;
    assert!(sink(&url).cluster_uuid().await.is_err());
    let (url, _) = stub(|_, _| (503, "{}".to_owned())).await;
    assert!(sink(&url).cluster_uuid().await.is_err());
}
