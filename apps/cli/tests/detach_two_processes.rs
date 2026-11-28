//! Two real `flusso run` processes over one slot, live (ADR 0012 stage 10,
//! slice 3's checklist, O29 and the cut): one holds the slot, the other
//! staged and waits for it. Each writes to OpenSearch through its own proxy,
//! so a test tells who wrote what.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(detach_two_processes)'
//! ```

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stderr,
    unused_crate_dependencies
)]

mod common;
#[path = "common/live.rs"]
mod live;

use std::time::{Duration, Instant};

use live::{Flusso, OsProxy, Project};

/// O29 with two real processes, as the cut (2026-11-27) settled it. A holds
/// the slot; B staged and waits. A detaches the lane (its own `Sink::detach`,
/// triggered by a record written by hand that A adopts under its held
/// opening), drops changes, confirms past them, does not stop, and exits. B
/// then opens: its lane answers `starting` (never vouched) until B reads its
/// records under that stream, finds A's change and stops (exit 1, naming the
/// index and the sink), having written nothing of the lane. B's restart
/// takes the lane as detached and writes nothing of it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_process_waiting_for_the_slot_stops_at_the_detach_another_made_and_writes_nothing_of_it()
{
    let (_pg, pg_url) = live::postgres(&[]).await;
    let (_os, os_url) = common::start_opensearch().await;
    let proxy_a = OsProxy::start(&os_url).await;
    let proxy_b = OsProxy::start(&os_url).await;
    let pool = live::pool(&pg_url).await;
    live::insert(&pool, 1..=20, "before").await;
    let a_project = Project::new("o29", &pg_url, &proxy_a.url, "");
    let b_project = Project::new("o29", &pg_url, &proxy_b.url, "");

    let mut a = Flusso::start(&a_project, &[]).await;
    a.until_seeded().await;
    let alias = live::hash_alias(&os_url).await;
    let a_writer = writer(&live::meta_doc(&os_url, &alias).await.unwrap());
    let mut b = Flusso::spawn(&b_project, &[]);
    b.until_log("is in use by Postgres backend", Duration::from_secs(60))
        .await;

    // A's side, as the setup claims: the lane detached by A itself, under
    // A's stamp, changes dropped and confirmed past, A still running.
    live::detach_while_running(&mut a, &proxy_a, &os_url, &pool, 1000).await;
    let record = live::meta_doc(&os_url, &format!("detach:{alias}"))
        .await
        .unwrap();
    assert_eq!(record["state"], "detached", "{record}");
    assert_eq!(
        writer(&record),
        a_writer,
        "A wrote the record itself: {record}"
    );
    let dropped = live::commit(&pool, "UPDATE t SET name = 'dropped' WHERE id <= 10").await;
    live::eventually(
        "A confirms past the dropped changes",
        Duration::from_secs(30),
        || {
            let pool = pool.clone();
            let dropped = dropped.clone();
            async move { live::confirmed_flush(&pool, "s_o29").await >= live::lsn_value(&dropped) }
        },
    )
    .await;
    assert!(a.running(), "A did not stop: {}", a.stderr());
    let held = live::documents(&os_url, &alias).await;
    assert!(
        held.values().all(|name| name.as_deref() == Some("before")),
        "A dropped them: {held:?}"
    );
    a.stop();

    // B: `starting` until its read, then the stop; never vouched.
    let mut answers = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while b.running() {
        let (code, body) = live::wait_at(b.public, &dropped, "applied", 200).await;
        let reason = body["reason"].as_str().unwrap_or_default().to_owned();
        if code != 0 && answers.last() != Some(&(code, reason.clone())) {
            answers.push((code, reason));
        }
        assert!(
            Instant::now() < deadline,
            "B never stopped: {answers:?}\n{}",
            b.stderr()
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(
        answers,
        vec![(503, "starting".to_owned())],
        "{}",
        b.stderr()
    );
    let status = b.exited(Duration::from_secs(5)).await;
    assert_eq!(status.code(), Some(1), "{}", b.stderr());
    let log = b.stderr();
    assert!(
        log.contains("another flusso process changed the index's detach record or seed marker")
            && log.contains("sink `primary`, index `t`"),
        "{log}"
    );
    let lane_writes = |proxy: &OsProxy| {
        proxy.count(|seen| seen.bulk_to(&format!("{alias}_1")) || seen.bulk_to(&alias))
    };
    assert_eq!(lane_writes(&proxy_b), 0, "B wrote nothing of the lane");

    // B's restart decides from the records: detached, nothing written.
    let mut b = Flusso::start(&b_project, &[]).await;
    b.until_lane("detached").await;
    live::commit(&pool, "UPDATE t SET name = 'after' WHERE id > 10").await;
    let (code, body) = b.wait(&dropped, "applied").await;
    assert_eq!(
        (code, body["reason"].as_str()),
        (503, Some("detached")),
        "{body}"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(b.running(), "{}", b.stderr());
    assert_eq!(lane_writes(&proxy_b), 0, "B wrote nothing of the lane");
    assert_eq!(live::documents(&os_url, &alias).await, held);
    b.stop();
}

/// The writer a meta document's stamp names.
fn writer(doc: &serde_json::Value) -> String {
    doc["stamp"]["writer"].as_str().unwrap().to_owned()
}

/// O29 with a reattach in between, on a sink that can't rebuild. A holds the
/// slot with the lane detached and reattaches it (`--accept-gap`) while B
/// waits. If B read the records before the reattach, B stops at its read once
/// it holds the slot (exit 1, naming the index), and its restart reads the
/// gap and refuses inside it. If B first reads after the reattach, it reads
/// the gap and refuses inside it, and goes on.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_reattach_made_while_another_process_waits_is_read_as_a_gap_or_stops_it() {
    for read_before in [true, false] {
        let (_pg, pg_url) = live::postgres(&[]).await;
        let (_os, os_url) = common::start_opensearch().await;
        let proxy_a = OsProxy::start(&os_url).await;
        let proxy_b = OsProxy::start(&os_url).await;
        let pool = live::pool(&pg_url).await;
        let a_project = Project::new("between", &pg_url, &proxy_a.url, "backfill = false");
        let b_project = Project::new("between", &pg_url, &proxy_b.url, "backfill = false");
        let mut a = Flusso::start(&a_project, &[]).await;
        a.until_lane("open").await;
        live::commit(
            &pool,
            "INSERT INTO t SELECT g, 'live' FROM generate_series(1, 10) g",
        )
        .await;
        live::detach_while_running(&mut a, &proxy_a, &os_url, &pool, 100).await;
        let inside = live::commit(&pool, "UPDATE t SET name = 'in the gap' WHERE id <= 5").await;

        let mut b = None;
        if read_before {
            let mut started = Flusso::spawn(&b_project, &[]);
            started
                .until_log("is in use by Postgres backend", Duration::from_secs(60))
                .await;
            b = Some(started);
        }
        let (ok, out, err) = a.admin(&["reattach", "t", "--sink", "primary", "--accept-gap"]);
        assert!(ok, "reattach: {out} {err}");
        a.until_lane("open").await;
        let after = live::commit(&pool, "INSERT INTO t VALUES (200, 'after the reattach')").await;
        let (code, body) = a.wait(&after, "searchable").await;
        assert_eq!(code, 200, "{body}");
        let mut b = match b {
            Some(b) => b,
            None => {
                let mut started = Flusso::spawn(&b_project, &[]);
                started
                    .until_log("is in use by Postgres backend", Duration::from_secs(60))
                    .await;
                started
            }
        };
        a.stop();

        if read_before {
            let status = b.exited(Duration::from_secs(60)).await;
            assert_eq!(status.code(), Some(1), "{}", b.stderr());
            let log = b.stderr();
            assert!(
                log.contains(
                    "another flusso process changed the index's detach record or seed marker"
                ) && log.contains("index `t`"),
                "{log}"
            );
            b = Flusso::start(&b_project, &[]).await;
        } else {
            b.until_log("following live changes", Duration::from_secs(60))
                .await;
        }
        b.until_lane("open").await;
        let (code, body) = b.wait(&inside, "applied").await;
        assert_eq!(
            (code, body["reason"].as_str()),
            (409, Some("gap")),
            "read before: {read_before}: {body}"
        );
        let (code, body) = b.wait(&after, "searchable").await;
        assert_eq!(code, 200, "read before: {read_before}: {body}");
        assert!(b.running(), "{}", b.stderr());
        b.stop();
    }
}
