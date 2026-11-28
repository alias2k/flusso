//! A detached index rebuilt, live (ADR 0012 stage 10, slice 3's checklist):
//! the real `flusso run` over Postgres and OpenSearch, a detach started from
//! a record written by hand (nothing else detaches in this slice), then a
//! `reindex`.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(detach_rebuild)'
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

use std::time::Duration;

use live::{Act, Flusso, OsProxy, Project};

/// Rebuild to seed. A lane detached by its records is rebuilt by `reindex`:
/// while the new generation fills, `/wait` answers `rebuilding` past the
/// freeze and vouches up to it; after the seed, reads see every change made
/// while detached, and the record is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_detached_index_is_rebuilt_to_its_seed_and_reads_then_see_every_change() {
    let (_pg, pg_url) = live::postgres(&[]).await;
    let (_os, os_url) = common::start_opensearch().await;
    let proxy = OsProxy::start(&os_url).await;
    let pool = live::pool(&pg_url).await;
    live::insert(&pool, 1..=50, "before").await;
    let project = Project::new("rebuild", &pg_url, &proxy.url, "");

    let mut flusso = Flusso::start(&project, &[]).await;
    flusso.until_seeded().await;
    let alias = live::hash_alias(&os_url).await;
    assert_eq!(
        live::generations(&os_url, &alias).await,
        vec![format!("{alias}_1")]
    );
    let applied = live::commit(&pool, "UPDATE t SET name = 'applied' WHERE id = 1").await;
    let (code, body) = flusso.wait(&applied, "searchable").await;
    assert_eq!(code, 200, "{body}");

    let detaching = live::detach_while_running(&mut flusso, &proxy, &os_url, &pool, 1000).await;
    let record = live::meta_doc(&os_url, &format!("detach:{alias}"))
        .await
        .unwrap();
    assert_eq!(record["state"], "detached", "{record}");
    assert!(
        record["stamp"]["writer"].is_string(),
        "the process wrote it: {record}"
    );

    live::commit(&pool, "UPDATE t SET name = 'while detached' WHERE id <= 10").await;
    live::commit(&pool, "DELETE FROM t WHERE id BETWEEN 11 AND 15").await;
    let after = live::commit(
        &pool,
        "INSERT INTO t SELECT g, 'new' FROM generate_series(51, 60) g",
    )
    .await;
    for position in [&detaching, &after] {
        let (code, body) = flusso.wait(position, "applied").await;
        assert_eq!(code, 503, "nothing past the freeze is vouched for: {body}");
        assert_eq!(body["reason"], "detached", "{body}");
    }
    let (code, body) = flusso.wait(&applied, "searchable").await;
    assert_eq!(
        code, 200,
        "the freeze is vouched for while detached: {body}"
    );
    let read = live::documents(&os_url, &alias).await;
    assert_eq!(
        read.len(),
        50,
        "nothing of the lane is written while detached"
    );
    assert_eq!(read[&1].as_deref(), Some("applied"));

    // Hold the rebuild's writes to its new generation, so it is caught
    // filling.
    let building = format!("{alias}_2");
    let held = building.clone();
    proxy.rule(move |seen| {
        if seen.bulk_to(&held) {
            Act::Hold
        } else {
            Act::Pass
        }
    });
    let (ok, out, err) = flusso.admin(&["reindex", "t"]);
    assert!(ok, "reindex: {out} {err}");
    let rebuilding = flusso
        .until_status("rebuilding", Duration::from_secs(60), |s| {
            s["sinks"]["primary"]["lanes"]["t"]["rebuilding"].is_u64()
        })
        .await;
    assert_eq!(
        rebuilding["sinks"]["primary"]["indexes"]["t"], "backfilling",
        "{rebuilding:#}"
    );
    live::eventually(
        "the rebuild writes its generation",
        Duration::from_secs(60),
        || {
            let proxy = proxy.clone();
            let building = building.clone();
            async move { proxy.count(|seen| seen.bulk_to(&building)) > 0 }
        },
    )
    .await;
    let (code, body) = flusso.wait(&after, "applied").await;
    assert_eq!(code, 503, "{body}");
    assert_eq!(body["reason"], "rebuilding", "{body}");
    let (code, body) = flusso.wait(&applied, "searchable").await;
    assert_eq!(
        code, 200,
        "the freeze is vouched for while rebuilding: {body}"
    );
    assert_eq!(
        live::documents(&os_url, &alias).await.len(),
        50,
        "reads stay on the old generation while the new one fills"
    );

    proxy.heal();
    proxy.release();
    flusso.until_seeded().await;
    flusso.until_lane("open").await;
    let (code, body) = flusso.wait(&after, "searchable").await;
    assert_eq!(code, 200, "{body}");
    live::assert_index_equals_table(&os_url, &alias, &pool).await;
    assert!(
        live::record_gone(&os_url, &alias).await,
        "the record is gone"
    );
    assert_eq!(live::generations(&os_url, &alias).await, vec![building]);
    flusso.stop();
}

/// The documents a `_bulk` writes to `index` (its index actions).
fn indexed_into(seen: &live::Seen, index: &str) -> usize {
    seen.body
        .lines()
        .filter(|line| {
            line.starts_with("{\"index\"") && line.contains(&format!("\"_index\":\"{index}\""))
        })
        .count()
}

/// Rebuild resumed at a restart. A rebuild killed mid-walk, with writes
/// after the kill, is found `rebuilding` by the next start, which resumes the
/// walk from its checkpoint (it writes fewer documents than the table holds),
/// seeds, and the index equals the table.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_rebuild_killed_mid_walk_resumes_from_its_checkpoint_at_the_next_start() {
    let (_pg, pg_url) = live::postgres(&[]).await;
    let (_os, os_url) = common::start_opensearch().await;
    let proxy = OsProxy::start(&os_url).await;
    let pool = live::pool(&pg_url).await;
    live::insert(&pool, 1..=20_000, "before").await;
    let project = Project::new("resume", &pg_url, &proxy.url, "");

    let mut flusso = Flusso::start(&project, &[]).await;
    flusso.until_seeded().await;
    let alias = live::hash_alias(&os_url).await;
    live::detach_while_running(&mut flusso, &proxy, &os_url, &pool, 50_000).await;
    live::commit(
        &pool,
        "UPDATE t SET name = 'while detached' WHERE id <= 100",
    )
    .await;

    // Each page of the rebuild lands a second late: the walk outlasts a
    // checkpoint (one every ten seconds).
    let building = format!("{alias}_2");
    let slowed = building.clone();
    proxy.rule(move |seen| {
        if seen.bulk_to(&slowed) {
            Act::Delay(1_000)
        } else {
            Act::Pass
        }
    });
    let (ok, out, err) = flusso.admin(&["reindex", "t"]);
    assert!(ok, "reindex: {out} {err}");
    flusso
        .until_status("a checkpoint", Duration::from_secs(90), |s| {
            s["sinks"]["primary"]["lanes"]["t"]["backfill_checkpoints"]
                .as_u64()
                .is_some_and(|n| n >= 1)
        })
        .await;
    flusso.kill();
    let written_before: usize = proxy
        .seen()
        .iter()
        .map(|s| indexed_into(s, &building))
        .sum();
    assert!(
        written_before < 20_000,
        "killed mid-walk: {written_before} written"
    );
    let record = live::meta_doc(&os_url, &format!("detach:{alias}"))
        .await
        .unwrap();
    assert_eq!(record["state"], "rebuilding", "{record}");

    live::commit(
        &pool,
        "UPDATE t SET name = 'after the kill' WHERE id BETWEEN 19900 AND 20000",
    )
    .await;
    live::commit(&pool, "DELETE FROM t WHERE id BETWEEN 1 AND 20").await;
    live::commit(
        &pool,
        "INSERT INTO t SELECT g, 'new' FROM generate_series(20001, 20050) g",
    )
    .await;

    proxy.heal();
    let seen_before_restart = proxy.seen().len();
    let mut flusso = Flusso::start(&project, &[]).await;
    flusso
        .until_status("rebuilding again", Duration::from_secs(60), |s| {
            s["sinks"]["primary"]["lanes"]["t"]["rebuilding"].is_u64()
        })
        .await;
    let log = flusso.stderr();
    assert!(
        log.contains("a rebuild under way: the lane is attached and its walk resumes")
            && log.contains("requesting a resumed backfill"),
        "{log}"
    );
    flusso.until_seeded().await;
    flusso.until_lane("open").await;
    let written_after: usize = proxy
        .seen()
        .iter()
        .skip(seen_before_restart)
        .map(|s| indexed_into(s, &building))
        .sum();
    let rows = live::table(&pool).await.len();
    assert!(
        written_after < rows,
        "the walk resumed from its checkpoint: {written_after} written of {rows} rows"
    );
    live::assert_index_equals_table(&os_url, &alias, &pool).await;
    assert!(
        live::record_gone(&os_url, &alias).await,
        "the record is gone"
    );
    assert_eq!(live::generations(&os_url, &alias).await, vec![building]);
    flusso.stop();
}
