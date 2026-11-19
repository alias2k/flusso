//! `flusso run` stops when it is told to, and a second start says what holds
//! the first one's place (issue #371).
//!
//! Each test starts the real binary over a Postgres container, puts it in the
//! state a laptop wakes into (a half-open HTTP request, an open long-poll, a
//! dead parent), signals it, and times the exit.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(shutdown)'
//! ```

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    unused_crate_dependencies
)]

mod common;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use testcontainers_modules::testcontainers::ImageExt;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

/// How long closing the HTTP surfaces may take of a stop, and room for a slow
/// runner on top.
const SIDE_STEP: Duration = Duration::from_secs(5);
const MARGIN: Duration = Duration::from_secs(5);

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_half_open_http_request_is_left_after_its_bound_and_the_stop_still_exits_cleanly() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "half_open");
    let mut flusso = Flusso::start(&dir, &[]);

    let _client = half_open_request(flusso.public);
    let started = Instant::now();
    flusso.signal("TERM");
    let status = flusso.exit_within(SIDE_STEP + MARGIN);

    assert!(
        started.elapsed() >= SIDE_STEP - Duration::from_millis(500),
        "it waited for the request up to its bound: {:?}",
        started.elapsed()
    );
    let stderr = flusso.stderr();
    assert_eq!(
        status.code(),
        Some(0),
        "the data path stopped cleanly: {stderr}"
    );
    assert!(
        stderr.contains("an HTTP connection did not close in time"),
        "the log says what it gave up on: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn one_press_under_a_forwarding_runner_two_senders_at_once_drains_every_time() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "forwarded");
    for (round, gap) in [0, 1, 20, 300].iter().cycle().take(20).enumerate() {
        let mut flusso = Flusso::start(&dir, &[]);
        let _client = half_open_request(flusso.public);
        // The group signalled by one process, the forwarded copy by another.
        send(flusso.child.id(), "INT");
        std::thread::sleep(Duration::from_millis(*gap));
        send(flusso.child.id(), "INT");
        let status = flusso.exit_within(SIDE_STEP + MARGIN);
        assert_eq!(
            status.code(),
            Some(0),
            "round {round}, copy {gap} ms late: {}",
            flusso.stderr()
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn the_same_sender_twice_quickly_stops_at_once() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "same_sender");
    let mut flusso = Flusso::start(&dir, &[]);

    let _client = half_open_request(flusso.public);
    let started = Instant::now();
    let pid = flusso.child.id();
    let sent = Command::new("sh")
        .arg("-c")
        .arg(format!("kill -INT {pid}; sleep 0.1; kill -INT {pid}"))
        .status()
        .unwrap();
    assert!(sent.success());
    let status = flusso.exit_within(Duration::from_secs(3));

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        status.code(),
        Some(130),
        "128 + SIGINT: {}",
        flusso.stderr()
    );
    assert!(
        flusso.stderr().contains("forced: exiting at once"),
        "{}",
        flusso.stderr()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn three_quick_signals_from_different_senders_stop_at_once() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "three");
    let mut flusso = Flusso::start(&dir, &[]);

    let _client = half_open_request(flusso.public);
    for _ in 0..3 {
        send(flusso.child.id(), "TERM");
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = flusso.exit_within(Duration::from_secs(3));

    assert_eq!(
        status.code(),
        Some(143),
        "128 + SIGTERM: {}",
        flusso.stderr()
    );
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_run_started_with_sighup_ignored_outlives_its_terminal() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "nohup");
    let log = dir.join("nohup.log");
    let mut child = Command::new("sh")
        .current_dir(&dir)
        .env("NO_COLOR", "1")
        .arg("-c")
        .arg(r#"trap '' HUP; exec "$0" run --config flusso.toml --public-address 127.0.0.1:0 --private-address 127.0.0.1:0"#)
        .arg(env!("CARGO_BIN_EXE_flusso"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("following live changes")
    {
        assert!(Instant::now() < deadline, "never went live");
        std::thread::sleep(Duration::from_millis(100));
    }

    send(child.id(), "HUP");
    std::thread::sleep(Duration::from_secs(2));
    assert!(
        child.try_wait().unwrap().is_none(),
        "the hangup was ignored, as nohup asked"
    );
    send(child.id(), "TERM");
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "SIGTERM still stops it");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(status.code(), Some(0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_wait_open_at_the_end_of_the_drain_is_told_flusso_is_stopping() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "wait");
    let mut flusso = Flusso::start(&dir, &[]);

    let public = flusso.public;
    let waiter = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(public).unwrap();
        stream
            .write_all(
                b"GET /wait?position=FFFFFFFF/0&timeout_ms=60000 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let mut answer = String::new();
        stream.read_to_string(&mut answer).unwrap();
        answer
    });
    std::thread::sleep(Duration::from_millis(500));
    let started = Instant::now();
    flusso.signal("TERM");
    let status = flusso.exit_within(Duration::from_secs(10));

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "an open wait does not hold the stop: {:?}",
        started.elapsed()
    );
    assert_eq!(status.code(), Some(0), "{}", flusso.stderr());
    let answer = waiter.join().unwrap();
    assert!(
        answer.starts_with("HTTP/1.1 503")
            && answer.to_ascii_lowercase().contains("retry-after: 1")
            && answer.contains("\"reason\":\"stopping\""),
        "the waiter is told to ask again: {answer}"
    );
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn sighup_stops_like_sigterm() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "hup");
    let mut flusso = Flusso::start(&dir, &[]);

    flusso.signal("HUP");
    let status = flusso.exit_within(Duration::from_secs(4));

    assert_eq!(
        status.code(),
        Some(0),
        "a clean stop, not a kill: {status:?}"
    );
    assert!(flusso.stderr().contains("SIGHUP"), "{}", flusso.stderr());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn exit_with_parent_stops_a_run_whose_parent_died() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "orphan");

    let watched = Orphan::start(&dir, &["--exit-with-parent"]);
    let unwatched = Orphan::start(&dir, &["--slot", "s_other", "--publication", "p_other"]);
    watched.kill_parent();
    unwatched.kill_parent();

    let deadline = Instant::now() + Duration::from_secs(5);
    while watched.alive() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let watched_alive = watched.alive();
    let unwatched_alive = unwatched.alive();
    watched.kill();
    unwatched.kill();

    assert!(
        !watched_alive,
        "--exit-with-parent stops the orphan: {}",
        watched.stderr()
    );
    assert!(
        watched.stderr().contains("parent process") && watched.stderr().contains("exited"),
        "{}",
        watched.stderr()
    );
    assert!(
        unwatched_alive,
        "without the flag an orphan keeps running, like any daemon"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_second_start_names_the_flusso_that_holds_its_place() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let dir = project(&url, "twice");
    let mut first = Flusso::start(&dir, &[]);

    let same_address = Command::new(env!("CARGO_BIN_EXE_flusso"))
        .current_dir(&dir)
        .env("NO_COLOR", "1")
        .args([
            "run",
            "--config",
            "flusso.toml",
            "--public-address",
            &first.public.to_string(),
            "--private-address",
            "127.0.0.1:0",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&same_address.stderr);
    assert!(!same_address.status.success(), "{text}");
    assert!(
        text.contains(&format!(
            "it reports pid {}, a flusso process on this host",
            first.child.id()
        )) && text.contains("Address already in use"),
        "the address error names the running flusso and keeps the OS error: {text}"
    );

    let mut second = Flusso::spawn(&dir, &[]);
    let deadline = Instant::now() + Duration::from_secs(30);
    let named = loop {
        let stderr = second.stderr();
        if stderr.contains("replication slot \"s_twice\" is in use") {
            break stderr;
        }
        assert!(
            Instant::now() < deadline,
            "the second run never named the slot's holder: {stderr}"
        );
        std::thread::sleep(Duration::from_millis(200));
    };
    second.signal("TERM");
    second.exit_within(Duration::from_secs(10));
    first.signal("TERM");
    first.exit_within(Duration::from_secs(10));
    assert!(
        named.contains("application_name \"flusso\"") && named.contains("pg_terminate_backend"),
        "the slot error names the holder and the way out: {named}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn under_steady_writes_a_stop_lands_and_confirms_everything_it_read_and_a_restart_rewrites_none_of_it()
 {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let dir = project_opensearch(&url, &os_url, "steady");
    let mut flusso = Flusso::start(&dir, &[]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();

    let writing = Arc::new(AtomicBool::new(true));
    let writer = tokio::spawn(write_steadily(pool.clone(), Arc::clone(&writing)));
    tokio::time::sleep(Duration::from_secs(3)).await;
    let started = Instant::now();
    flusso.signal("TERM");
    tokio::time::sleep(Duration::from_secs(1)).await;
    writing.store(false, Ordering::SeqCst);
    let written = writer.await.unwrap();
    let status = flusso.exit_within(Duration::from_secs(30));
    let stopped_in = started.elapsed();
    eprintln!("graceful stop under steady writes took {stopped_in:?}");

    assert_eq!(status.code(), Some(0), "{}", flusso.stderr());
    let confirmed = confirmed_flush(&pool, "s_steady").await;
    let indexed = documents(&os_url, "t").await;
    let read: Vec<i32> = written
        .iter()
        .filter(|(_, lsn)| *lsn <= confirmed)
        .map(|(id, _)| *id)
        .collect();
    let missing: Vec<&i32> = read
        .iter()
        .filter(|id| !indexed.contains_key(*id))
        .collect();
    assert!(
        missing.is_empty(),
        "every row committed at or before the confirmed position {confirmed:X} is indexed; \
         missing {missing:?}"
    );
    assert!(!read.is_empty(), "flusso indexed rows before the stop");
    assert!(
        written.iter().any(|(_, lsn)| *lsn > confirmed),
        "rows written after flusso stopped reading are beyond the confirmed position"
    );
    assert!(
        indexed
            .keys()
            .all(|id| written.iter().any(|(w, _)| w == id)),
        "nothing indexed that was not written"
    );

    let mut restarted = Flusso::start(&dir, &[]);
    let deadline = Instant::now() + Duration::from_secs(60);
    let after = loop {
        let after = documents(&os_url, "t").await;
        if after.len() == written.len() {
            break after;
        }
        assert!(
            Instant::now() < deadline,
            "the restart never indexed the rest"
        );
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    restarted.signal("TERM");
    restarted.exit_within(Duration::from_secs(30));
    let rewritten: Vec<&i32> = indexed
        .iter()
        .filter(|(id, version)| after.get(*id) != Some(*version))
        .map(|(id, _)| id)
        .collect();
    assert!(
        rewritten.is_empty(),
        "the restart redelivered nothing already indexed; rewritten {rewritten:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_stop_with_opensearch_paused_gives_up_at_the_deadline_naming_the_sink_and_a_restart_converges()
 {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let (opensearch, os_url) = common::start_opensearch().await;
    let dir = project_opensearch(&url, &os_url, "paused");
    let mut flusso = Flusso::start(&dir, &["--shutdown-timeout-secs", "5"]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    insert(&pool, 1..=10).await;
    wait_for_count(&os_url, 10).await;

    docker(&["pause", opensearch.id()]);
    insert(&pool, 11..=20).await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let started = Instant::now();
    flusso.signal("TERM");
    std::thread::sleep(Duration::from_millis(1_500));
    flusso.signal("TERM");
    std::thread::sleep(Duration::from_millis(1_500));
    assert!(flusso.running(), "a slow repeat doesn't shorten the drain");
    let status = flusso.exit_within(Duration::from_secs(15));
    let stopped_in = started.elapsed();
    docker(&["unpause", opensearch.id()]);

    let stderr = flusso.stderr();
    assert!(
        stderr.contains("stop already requested: draining the pipeline (primary: ")
            && stderr.contains("s left. Send the signal twice quickly to exit now"),
        "the repeat is answered with where the stop is: {stderr}"
    );
    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(
        stopped_in >= Duration::from_secs(4) && stopped_in < Duration::from_secs(10),
        "{stopped_in:?}"
    );
    assert!(
        stderr.contains("shutdown deadline of 5s passed while draining the pipeline")
            && stderr.contains("primary: "),
        "the log names the step and the sink: {stderr}"
    );

    let mut restarted = Flusso::start(&dir, &[]);
    wait_for_count(&os_url, 20).await;
    restarted.signal("TERM");
    assert_eq!(
        restarted.exit_within(Duration::from_secs(30)).code(),
        Some(0)
    );
}

/// A stop with a lane parked on a write block: the stop probes it once more,
/// the block holds, so the drain can't finish; at the deadline flusso exits 1
/// naming the lane, its health and OpenSearch's reason. Once the block is
/// lifted, the next start writes what was left and converges.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_stop_with_a_write_blocked_lane_gives_up_at_the_deadline_naming_the_lane_and_a_restart_converges()
 {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let dir = project_opensearch(&url, &os_url, "blocked");
    let mut flusso = Flusso::start(&dir, &["--shutdown-timeout-secs", "5"]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    insert(&pool, 1..=10).await;
    wait_for_count(&os_url, 10).await;

    block_writes(&os_url, "t", true).await;
    insert(&pool, 11..=20).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    while lane_health(flusso.public, "t").await.as_deref() != Some("blocked") {
        assert!(
            Instant::now() < deadline,
            "the lane never parked as blocked"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    flusso.signal("TERM");
    let status = flusso.exit_within(Duration::from_secs(15));
    let stderr = flusso.stderr();
    assert_eq!(status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains("shutdown deadline of 5s passed while draining the pipeline")
            && stderr.contains("lane t blocked since")
            && stderr.contains("FORBIDDEN/8"),
        "the log names the parked lane and why: {stderr}"
    );
    assert_eq!(documents(&os_url, "t").await.len(), 10);

    block_writes(&os_url, "t", false).await;
    let mut restarted = Flusso::start(&dir, &[]);
    wait_for_count(&os_url, 20).await;
    restarted.signal("TERM");
    assert_eq!(
        restarted.exit_within(Duration::from_secs(30)).code(),
        Some(0)
    );
}

/// Set or lift `index.blocks.write` on `index`.
async fn block_writes(os_url: &str, index: &str, on: bool) {
    let value = if on {
        serde_json::Value::Bool(true)
    } else {
        serde_json::Value::Null
    };
    let response = reqwest::Client::new()
        .put(format!("{os_url}/{index}/_settings"))
        .json(&serde_json::json!({ "index.blocks.write": value }))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{index}/_settings");
}

/// `index`'s lane health into `primary`, as `/status` reports it.
async fn lane_health(public: SocketAddr, index: &str) -> Option<String> {
    let status: serde_json::Value = reqwest::get(format!("http://{public}/status"))
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    status["sinks"]["primary"]["lanes"][index]["health"]
        .as_str()
        .map(str::to_owned)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_position_reached_before_a_graceful_stop_is_answered_by_the_next_process() {
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let dir = project_opensearch(&url, &os_url, "fence");
    let mut flusso = Flusso::start(&dir, &[]);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    insert(&pool, 1..=1).await;
    let position: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(wait(flusso.public, &position, "searchable").await, 200);
    flusso.signal("TERM");
    assert_eq!(flusso.exit_within(Duration::from_secs(30)).code(), Some(0));

    let mut next = Flusso::start(&dir, &[]);
    for level in ["applied", "searchable"] {
        assert_eq!(
            wait(next.public, &position, level).await,
            200,
            "the next process answers {position} at {level}: {}",
            next.stderr()
        );
    }
    assert!(documents(&os_url, "t").await.contains_key(&1));
    next.signal("TERM");
    next.exit_within(Duration::from_secs(30));
}

/// Insert one row per transaction until `writing` turns false; each id with
/// the WAL position read right after its commit (at or past its commit's end).
async fn write_steadily(pool: sqlx::PgPool, writing: Arc<AtomicBool>) -> Vec<(i32, u64)> {
    let mut written = Vec::new();
    let mut id = 0;
    while writing.load(Ordering::SeqCst) {
        id += 1;
        sqlx::query("INSERT INTO t VALUES ($1, 'x')")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let lsn: String = sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
            .fetch_one(&pool)
            .await
            .unwrap();
        written.push((id, lsn_value(&lsn)));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    written
}

async fn insert(pool: &sqlx::PgPool, ids: std::ops::RangeInclusive<i32>) {
    for id in ids {
        sqlx::query("INSERT INTO t VALUES ($1, 'x')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }
}

fn lsn_value(text: &str) -> u64 {
    let (high, low) = text.split_once('/').unwrap();
    (u64::from_str_radix(high, 16).unwrap() << 32) | u64::from_str_radix(low, 16).unwrap()
}

async fn confirmed_flush(pool: &sqlx::PgPool, slot: &str) -> u64 {
    let lsn: String = sqlx::query_scalar(
        "SELECT confirmed_flush_lsn::text FROM pg_replication_slots WHERE slot_name = $1",
    )
    .bind(slot)
    .fetch_one(pool)
    .await
    .unwrap();
    lsn_value(&lsn)
}

/// Every document of `index` by its `id`, with its `_version`.
async fn documents(os_url: &str, index: &str) -> std::collections::BTreeMap<i32, u64> {
    let http = reqwest::Client::new();
    http.post(format!("{os_url}/{index}/_refresh"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = http
        .get(format!("{os_url}/{index}/_search?size=10000&version=true"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    body["hits"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| {
            (
                i32::try_from(hit["_source"]["id"].as_i64().unwrap()).unwrap(),
                hit["_version"].as_u64().unwrap(),
            )
        })
        .collect()
}

async fn wait_for_count(os_url: &str, count: usize) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while documents(os_url, "t").await.len() != count {
        assert!(Instant::now() < deadline, "never reached {count} documents");
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

/// `/wait` on `position` at `level`; the status code.
async fn wait(public: SocketAddr, position: &str, level: &str) -> u16 {
    reqwest::Client::new()
        .get(format!("http://{public}/wait"))
        .query(&[
            ("position", position),
            ("until", level),
            ("timeout_ms", "30000"),
        ])
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

fn docker(args: &[&str]) {
    let done = Command::new("docker").args(args).status().unwrap();
    assert!(done.success(), "docker {args:?}");
}

/// A project of one index over `t`, writing to OpenSearch at `os_url`.
fn project_opensearch(url: &str, os_url: &str, name: &str) -> PathBuf {
    let dir = project(url, name);
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_{name}\"\n\
             publication = \"p_{name}\"\n\n[sinks.primary]\ntype = \"opensearch\"\nurl = \"{os_url}\"\n\
             number_of_replicas = 0\n\n[[index]]\nname = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    dir
}

/// Stops at random points (mid-backfill, mid-live) under inserts, updates and
/// deletes, each followed by a restart; the index must end equal to the
/// table. Seeded: `FLUSSO_STOP_SEED=<seed>` replays a failing run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn random_stops_under_writes_converge_after_a_restart() {
    let seed = std::env::var("FLUSSO_STOP_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or_else(|| (unique() % u128::from(u32::MAX)) as u64);
    eprintln!("seed {seed}: replay with FLUSSO_STOP_SEED={seed}");
    let mut random = Lcg(seed);
    let (_postgres, url) = common::start_postgres(TABLE).await;
    let (_opensearch, os_url) = common::start_opensearch().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t SELECT g, 'row ' || g FROM generate_series(1, 100000) g")
        .execute(&pool)
        .await
        .unwrap();
    let dir = project_opensearch(&url, &os_url, "converge");

    for round in 0..3 {
        let mut flusso = Flusso::start(&dir, &[]);
        let writing = Arc::new(AtomicBool::new(true));
        let writer = tokio::spawn(write_randomly(
            pool.clone(),
            Arc::clone(&writing),
            random.next(),
        ));
        let delay = Duration::from_millis(random.next() % 4_000);
        tokio::time::sleep(delay).await;
        flusso.signal("TERM");
        tokio::time::sleep(Duration::from_millis(300)).await;
        writing.store(false, Ordering::SeqCst);
        writer.await.unwrap();
        let status = flusso.exit_within(Duration::from_secs(40));
        assert_eq!(
            status.code(),
            Some(0),
            "seed {seed}, round {round}, stopped {delay:?} after going live: {}",
            flusso.stderr()
        );
    }

    let mut last = Flusso::start(&dir, &[]);
    let wanted: std::collections::BTreeMap<i32, String> = sqlx::query_as("SELECT id, name FROM t")
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .collect();
    let deadline = Instant::now() + Duration::from_secs(120);
    let indexed = loop {
        let indexed = names(&os_url, "t").await;
        if indexed == wanted || Instant::now() > deadline {
            break indexed;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    };
    last.signal("TERM");
    last.exit_within(Duration::from_secs(40));
    let missing: Vec<_> = wanted
        .keys()
        .filter(|id| !indexed.contains_key(id))
        .take(5)
        .collect();
    let stale: Vec<_> = indexed
        .iter()
        .filter(|(id, name)| wanted.get(id).is_some_and(|wanted| wanted != *name))
        .take(5)
        .collect();
    let extra: Vec<_> = indexed
        .keys()
        .filter(|id| !wanted.contains_key(id))
        .take(5)
        .collect();
    assert!(
        missing.is_empty() && stale.is_empty() && extra.is_empty(),
        "seed {seed}: the index differs from the table: missing {missing:?}, stale {stale:?}, \
         extra {extra:?} ({} rows, {} documents)",
        wanted.len(),
        indexed.len()
    );
}

/// A small seeded generator, enough to replay a run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

/// Random inserts, updates and deletes on `t` until `writing` turns false.
async fn write_randomly(pool: sqlx::PgPool, writing: Arc<AtomicBool>, seed: u64) {
    let mut random = Lcg(seed);
    while writing.load(Ordering::SeqCst) {
        let id = i32::try_from(random.next() % 120_000).unwrap() + 1;
        let statement = match random.next() % 3 {
            0 => {
                "INSERT INTO t VALUES ($1, 'new ' || $1) ON CONFLICT (id) DO UPDATE SET name = 'again ' || $1"
            }
            1 => "UPDATE t SET name = 'updated ' || $1 || ' ' || clock_timestamp() WHERE id = $1",
            _ => "DELETE FROM t WHERE id = $1",
        };
        sqlx::query(statement)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// Every document of `index` by its `id`, with its `name`, read with a scroll.
async fn names(os_url: &str, index: &str) -> std::collections::BTreeMap<i32, String> {
    let http = reqwest::Client::new();
    http.post(format!("{os_url}/{index}/_refresh"))
        .send()
        .await
        .unwrap();
    let mut found = std::collections::BTreeMap::new();
    let mut page: serde_json::Value = http
        .post(format!("{os_url}/{index}/_search?scroll=1m"))
        .json(&serde_json::json!({ "size": 10_000, "_source": ["id", "name"] }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    loop {
        let hits = page["hits"]["hits"].as_array().cloned().unwrap_or_default();
        if hits.is_empty() {
            break;
        }
        for hit in hits {
            found.insert(
                i32::try_from(hit["_source"]["id"].as_i64().unwrap()).unwrap(),
                hit["_source"]["name"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned(),
            );
        }
        let scroll = page["_scroll_id"].as_str().unwrap().to_owned();
        page = http
            .post(format!("{os_url}/_search/scroll"))
            .json(&serde_json::json!({ "scroll": "1m", "scroll_id": scroll }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
    }
    found
}

const TABLE: &str = "CREATE TABLE t (id int PRIMARY KEY, name text)";

/// A project of one index over `t`, writing to stdout.
fn project(url: &str, name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("flusso-shutdown-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.schema.yml"),
        "version: 1\ntable: t\nprimary_key: id\nfields:\n  - integer: id\n    required: true\n  - keyword: name\n    required: false\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("flusso.toml"),
        format!(
            "[source]\ntype = \"postgres\"\nconnection_url = \"{url}\"\nslot = \"s_{name}\"\n\
             publication = \"p_{name}\"\n\n[sinks.out]\ntype = \"stdout\"\n\n[[index]]\n\
             name = \"t\"\nschema = \"t.schema.yml\"\nenabled = true\n"
        ),
    )
    .unwrap();
    dir
}

/// A `flusso run` child of the test, logging to a file.
struct Flusso {
    child: Child,
    log: PathBuf,
    public: SocketAddr,
}

impl Flusso {
    /// Start `flusso run` and wait until it follows live changes.
    fn start(dir: &Path, extra: &[&str]) -> Self {
        let mut flusso = Self::spawn(dir, extra);
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let stderr = flusso.stderr();
            if stderr.contains("following live changes") {
                flusso.public = listening(&stderr, "public").unwrap();
                return flusso;
            }
            assert!(flusso.running(), "flusso exited at start: {stderr}");
            assert!(
                Instant::now() < deadline,
                "flusso never went live: {stderr}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn spawn(dir: &Path, extra: &[&str]) -> Self {
        let log = dir.join(format!("stderr-{}.log", unique()));
        let child = Command::new(env!("CARGO_BIN_EXE_flusso"))
            .current_dir(dir)
            .env("NO_COLOR", "1")
            .args([
                "run",
                "--config",
                "flusso.toml",
                "--public-address",
                "127.0.0.1:0",
                "--private-address",
                "127.0.0.1:0",
            ])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(&log).unwrap())
            .spawn()
            .unwrap();
        Self {
            child,
            log,
            public: SocketAddr::from(([127, 0, 0, 1], 0)),
        }
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn running(&mut self) -> bool {
        self.child.try_wait().unwrap().is_none()
    }

    fn signal(&self, name: &str) {
        send(self.child.id(), name);
    }

    /// The exit status, or a failed test (after a SIGKILL) when it takes
    /// longer than `limit`.
    fn exit_within(&mut self, limit: Duration) -> ExitStatus {
        let deadline = Instant::now() + limit;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            if Instant::now() > deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!(
                    "flusso was still running {limit:?} after the stop: {}",
                    self.stderr()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Flusso {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// A `flusso run` started by a shell that the test then kills with SIGKILL,
/// the way a task runner dies without passing anything on.
struct Orphan {
    parent: Child,
    pid: u32,
    log: PathBuf,
}

impl Orphan {
    fn start(dir: &Path, extra: &[&str]) -> Self {
        let log = dir.join(format!("orphan-{}.log", unique()));
        let pid_file = dir.join(format!("orphan-{}.pid", unique()));
        let mut parent = Command::new("sh")
            .current_dir(dir)
            .env("NO_COLOR", "1")
            .arg("-c")
            .arg(r#"bin="$1"; log="$2"; pidfile="$3"; shift 3; "$bin" "$@" 2>"$log" >/dev/null & echo $! > "$pidfile"; wait"#)
            .arg("sh")
            .arg(env!("CARGO_BIN_EXE_flusso"))
            .arg(&log)
            .arg(&pid_file)
            .args([
                "run",
                "--config",
                "flusso.toml",
                "--public-address",
                "127.0.0.1:0",
                "--private-address",
                "127.0.0.1:0",
            ])
            .args(extra)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let pid = loop {
            let log_text = std::fs::read_to_string(&log).unwrap_or_default();
            if log_text.contains("following live changes")
                && let Ok(pid) = std::fs::read_to_string(&pid_file)
            {
                break pid.trim().parse().unwrap();
            }
            assert!(
                parent.try_wait().unwrap().is_none(),
                "the shell exited: {log_text}"
            );
            assert!(
                Instant::now() < deadline,
                "flusso never went live: {log_text}"
            );
            std::thread::sleep(Duration::from_millis(100));
        };
        Self { parent, pid, log }
    }

    fn kill_parent(&self) {
        send(self.parent.id(), "KILL");
    }

    fn alive(&self) -> bool {
        Command::new("kill")
            .args(["-0", &self.pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    fn stderr(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    fn kill(&self) {
        let _ = Command::new("kill")
            .args(["-KILL", &self.pid.to_string()])
            .stderr(Stdio::null())
            .status();
    }
}

impl Drop for Orphan {
    fn drop(&mut self) {
        self.kill();
        let _ = self.parent.kill();
        let _ = self.parent.wait();
    }
}

fn send(pid: u32, signal: &str) {
    let sent = Command::new("kill")
        .args([&format!("-{signal}"), &pid.to_string()])
        .status()
        .unwrap();
    assert!(sent.success(), "kill -{signal} {pid}");
}

/// A request whose headers never end: what a client that vanished mid-request
/// (a laptop gone to sleep) leaves on the server.
fn half_open_request(address: SocketAddr) -> TcpStream {
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .write_all(b"GET /status HTTP/1.1\r\nHost: x\r\n")
        .unwrap();
    std::thread::sleep(Duration::from_millis(200));
    stream
}

/// The address a surface logged it listens on.
fn listening(stderr: &str, surface: &str) -> Option<SocketAddr> {
    stderr
        .lines()
        .filter(|line| line.contains("HTTP surface listening"))
        .find(|line| line.contains(&format!("surface=\"{surface}\"")))?
        .split_whitespace()
        .find_map(|word| word.strip_prefix("addr="))?
        .parse()
        .ok()
}

fn unique() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// A backfill killed mid-way (`SIGKILL`, nothing drained) resumes where it
/// durably got (ADR 0012 stage 8b): killed twice, each time once a checkpoint
/// was recorded, with writes between the kill and the restart below and
/// above any point it could have reached (deletes and updates of low ids,
/// updates of high ones, new ids past the end). Each restarted run reads
/// fewer key-page rows than the table holds, counted by
/// `pg_stat_statements`, and the index ends equal to the table: a resume
/// that skipped a row would leave a hole.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_killed_backfill_resumes_and_reads_fewer_rows_than_a_full_one() {
    const ROWS: i64 = 1_000_000;
    // Slow enough for several checkpoints, ten seconds apart.
    const PACE: &[&str] = &["--writes", "1", "--batch-max-changes", "64"];
    let postgres = test_postgres::image()
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "shared_preload_libraries=pg_stat_statements",
        ])
        .start()
        .await
        .unwrap();
    let port = postgres.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let (_opensearch, os_url) = common::start_opensearch().await;
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .unwrap();
    for statement in [
        "CREATE EXTENSION pg_stat_statements",
        TABLE,
        "INSERT INTO t SELECT g, 'row ' || g FROM generate_series(1, 1000000) g",
    ] {
        sqlx::query(sqlx::AssertSqlSafe(statement.to_owned()))
            .execute(&pool)
            .await
            .unwrap();
    }
    let dir = project_opensearch(&url, &os_url, "resume");

    let mut read = Vec::new();
    for round in 0..3 {
        reset_statements(&pool).await;
        let mut flusso = Flusso::spawn(&dir, PACE);
        if round == 2 {
            wait_until_indexed(&pool, &os_url).await;
            read.push(key_page_rows(&pool).await);
            flusso.signal("TERM");
            flusso.exit_within(Duration::from_secs(40));
            break;
        }
        let public = flusso.public_address();
        let deadline = Instant::now() + Duration::from_secs(180);
        while checkpoints(public).await < 1 {
            assert!(
                !seeded(public).await,
                "round {round}: the backfill ended before a checkpoint: {}",
                flusso.stderr()
            );
            assert!(flusso.running(), "flusso exited: {}", flusso.stderr());
            assert!(
                Instant::now() < deadline,
                "no checkpoint: {}",
                flusso.stderr()
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        flusso.signal("KILL");
        let _ = flusso.exit_within(Duration::from_secs(10));
        read.push(key_page_rows(&pool).await);
        for statement in [
            "DELETE FROM t WHERE id BETWEEN 1 AND 500".to_owned(),
            "UPDATE t SET name = 'low ' || id || ' ' || clock_timestamp() WHERE id BETWEEN 1000 AND 1500"
                .to_owned(),
            "UPDATE t SET name = 'high ' || id || ' ' || clock_timestamp() WHERE id > 999500"
                .to_owned(),
            format!(
                "INSERT INTO t SELECT g, 'new ' || g FROM generate_series({}, {}) g",
                1_000_001 + 100 * round,
                1_000_100 + 100 * round
            ),
        ] {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&pool)
                .await
                .unwrap();
        }
    }
    eprintln!("key-page rows read per run: {read:?} ({ROWS} rows in the table at the start)");
    let resumed = &read[1..];
    for rows in resumed {
        assert!(
            *rows < ROWS,
            "a resumed run read {rows} key-page rows, as many as the table holds: {read:?}"
        );
    }
}

/// Wait until the index holds exactly the table, for at most three minutes.
async fn wait_until_indexed(pool: &sqlx::PgPool, os_url: &str) {
    let wanted: std::collections::BTreeMap<i32, String> = sqlx::query_as("SELECT id, name FROM t")
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .collect();
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        let indexed = names(os_url, "t").await;
        if indexed == wanted {
            return;
        }
        if Instant::now() > deadline {
            let missing: Vec<_> = wanted
                .keys()
                .filter(|id| !indexed.contains_key(id))
                .take(5)
                .collect();
            let stale: Vec<_> = indexed
                .iter()
                .filter(|(id, name)| wanted.get(id).is_some_and(|wanted| wanted != *name))
                .take(5)
                .collect();
            let extra: Vec<_> = indexed
                .keys()
                .filter(|id| !wanted.contains_key(id))
                .take(5)
                .collect();
            panic!(
                "the index differs from the table: missing {missing:?}, stale {stale:?}, extra {extra:?} \
                 ({} rows, {} documents)",
                wanted.len(),
                indexed.len()
            );
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn reset_statements(pool: &sqlx::PgPool) {
    sqlx::query("SELECT pg_stat_statements_reset()")
        .execute(pool)
        .await
        .unwrap();
}

/// Rows the backfill's key pages returned since the last reset.
async fn key_page_rows(pool: &sqlx::PgPool) -> i64 {
    sqlx::query_scalar(
        "SELECT coalesce(sum(rows), 0)::int8 FROM pg_stat_statements WHERE query LIKE '%\"within\"%'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Whether `t` is seeded on the sink, from `/status`.
async fn seeded(public: SocketAddr) -> bool {
    let Ok(response) = reqwest::get(format!("http://{public}/status")).await else {
        return false;
    };
    let Ok(status) = response.json::<serde_json::Value>().await else {
        return false;
    };
    status["sinks"]["primary"]["indexes"]["t"] == "seeded"
}

/// The `t` lane's backfill checkpoints this run, from `/status`.
async fn checkpoints(public: SocketAddr) -> u64 {
    let Ok(response) = reqwest::get(format!("http://{public}/status")).await else {
        return 0;
    };
    let Ok(status) = response.json::<serde_json::Value>().await else {
        return 0;
    };
    status["sinks"]["primary"]["lanes"]["t"]["backfill_checkpoints"]
        .as_u64()
        .unwrap_or(0)
}

impl Flusso {
    /// The public address, once the process logged it.
    fn public_address(&mut self) -> SocketAddr {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(address) = listening(&self.stderr(), "public") {
                return address;
            }
            assert!(self.running(), "flusso exited at start: {}", self.stderr());
            assert!(
                Instant::now() < deadline,
                "no public address: {}",
                self.stderr()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}
