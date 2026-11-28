//! A detached index reattached with its gap accepted, live (ADR 0012 stage
//! 10, slice 3's checklist): the real `flusso run` over Postgres and
//! OpenSearch on a `backfill = false` sink, `flusso reattach --accept-gap`.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo nextest run -p flusso-cli --run-ignored all -E 'binary(detach_reattach)'
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

/// Reattach with a gap. On a sink that can't rebuild, `reattach
/// --accept-gap` keeps the gap on the seed marker; `/wait` inside it is
/// `409 gap`, before and after a restart, and vouched after it; once
/// `backfill` is turned on, the rebuild's seed ends it and the index equals
/// the table.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_reattach_keeps_its_gap_until_a_seed_refusing_waits_inside_it_across_a_restart() {
    let (_pg, pg_url) = live::postgres(&[]).await;
    let (_os, os_url) = common::start_opensearch().await;
    let proxy = OsProxy::start(&os_url).await;
    let pool = live::pool(&pg_url).await;
    let project = Project::new("gap", &pg_url, &proxy.url, "backfill = false");

    let mut flusso = Flusso::start(&project, &[]).await;
    flusso.until_lane("open").await;
    let attached = live::commit(
        &pool,
        "INSERT INTO t SELECT g, 'live' FROM generate_series(1, 10) g",
    )
    .await;
    let (code, body) = flusso.wait(&attached, "searchable").await;
    assert_eq!(code, 200, "{body}");
    let alias = live::hash_alias(&os_url).await;

    let detaching = live::detach_while_running(&mut flusso, &proxy, &os_url, &pool, 100).await;
    let record = live::meta_doc(&os_url, &format!("detach:{alias}"))
        .await
        .unwrap();
    assert_eq!(
        (&record["state"], &record["live_only"]),
        (&"detached".into(), &true.into()),
        "{record}"
    );
    let inside = live::commit(&pool, "UPDATE t SET name = 'in the gap' WHERE id <= 5").await;

    let (ok, out, err) = flusso.admin(&["reattach", "t", "--sink", "primary", "--accept-gap"]);
    assert!(ok, "reattach: {out} {err}");
    flusso.until_lane("open").await;
    let marker = live::meta_doc(&os_url, &alias).await.unwrap();
    let gaps = marker["gaps"].as_array().cloned().unwrap_or_default();
    assert_eq!(
        gaps.len(),
        1,
        "the gap is kept on the seed marker: {marker}"
    );
    assert!(
        live::record_gone(&os_url, &alias).await,
        "the record is gone"
    );

    let after = live::commit(&pool, "INSERT INTO t VALUES (200, 'after the reattach')").await;
    for flusso_run in 0..2 {
        for position in [&detaching, &inside] {
            let (code, body) = flusso.wait(position, "applied").await;
            assert_eq!(code, 409, "run {flusso_run}: inside the gap: {body}");
            assert_eq!(body["reason"], "gap", "{body}");
        }
        let (code, body) = flusso.wait(&after, "searchable").await;
        assert_eq!(code, 200, "run {flusso_run}: after the gap: {body}");
        if flusso_run == 0 {
            flusso.stop();
            flusso = Flusso::start(&project, &[]).await;
            flusso.until_lane("open").await;
        }
    }
    flusso.stop();

    // `backfill` on: the gap keeps refusing until the rebuild into a new
    // generation is seeded, which ends it.
    project.write("gap", &pg_url, &proxy.url, "");
    let mut flusso = Flusso::start(&project, &[]).await;
    live::eventually(
        "the rebuild over the gap is seeded",
        Duration::from_secs(90),
        || {
            let os_url = os_url.clone();
            let alias = alias.clone();
            async move {
                let marker = live::meta_doc(&os_url, &alias).await.unwrap();
                marker["active_generation"] == 2 && marker["seeded"] == true
            }
        },
    )
    .await;
    flusso
        .until_status("the gap ended", Duration::from_secs(30), |s| {
            let lane = &s["sinks"]["primary"]["lanes"]["t"];
            lane["health"] == "open" && lane["gaps"].is_null()
        })
        .await;
    let (code, body) = flusso.wait(&inside, "searchable").await;
    assert_eq!(code, 200, "the seed ended the gap: {body}");
    let marker = live::meta_doc(&os_url, &alias).await.unwrap();
    assert!(
        marker["gaps"].as_array().is_none_or(Vec::is_empty),
        "no gap after the seed: {marker}"
    );
    assert_eq!(marker["active_generation"], 2, "{marker}");
    live::assert_index_equals_table(&os_url, &alias, &pool).await;
    flusso.stop();
}

/// Where a reattach is killed: just before one of its writes, or just after
/// its commit point (the record's delete), before the lane is attached.
#[derive(Debug, Clone, Copy)]
enum Step {
    BeforeTheGap,
    BeforeTheDelete,
    AfterTheDelete,
}

impl Step {
    fn kills(self, seen: &live::Seen, alias: &str, deleted: bool) -> bool {
        let gap = seen
            .path
            .starts_with(&format!("/flusso_meta/_update/{alias}"))
            && seen.body.contains("\"gaps\"");
        let delete = seen
            .path
            .starts_with(&format!("/flusso_meta/_doc/detach:{alias}"))
            && seen.body.contains("\"forgotten\"");
        match self {
            Step::BeforeTheGap => gap,
            Step::BeforeTheDelete => delete,
            Step::AfterTheDelete => deleted && seen.meta(),
        }
    }
}

/// A reattach killed before each step comes back detached, or with its gap:
/// never attached without one. Each step's request is left unanswered (never
/// forwarded) and the process killed; the next start either keeps the lane
/// detached or attaches it with the gap, and a wait inside the dropped
/// changes is never vouched for.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_reattach_killed_before_each_step_comes_back_detached_or_with_its_gap() {
    for step in [
        Step::BeforeTheGap,
        Step::BeforeTheDelete,
        Step::AfterTheDelete,
    ] {
        let (_pg, pg_url) = live::postgres(&[]).await;
        let (_os, os_url) = common::start_opensearch().await;
        let proxy = OsProxy::start(&os_url).await;
        let pool = live::pool(&pg_url).await;
        let project = Project::new("killed", &pg_url, &proxy.url, "backfill = false");
        let mut flusso = Flusso::start(&project, &[]).await;
        flusso.until_lane("open").await;
        live::commit(
            &pool,
            "INSERT INTO t SELECT g, 'live' FROM generate_series(1, 10) g",
        )
        .await;
        let alias = live::hash_alias(&os_url).await;
        live::detach_while_running(&mut flusso, &proxy, &os_url, &pool, 100).await;
        let inside = live::commit(&pool, "UPDATE t SET name = 'in the gap' WHERE id <= 5").await;

        let deleted = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (target, seen_delete) = (alias.clone(), std::sync::Arc::clone(&deleted));
        proxy.rule(move |seen| {
            if step.kills(
                seen,
                &target,
                seen_delete.load(std::sync::atomic::Ordering::SeqCst),
            ) {
                return Act::Hang;
            }
            if seen
                .path
                .starts_with(&format!("/flusso_meta/_doc/detach:{target}"))
                && seen.body.contains("\"forgotten\"")
            {
                seen_delete.store(true, std::sync::atomic::Ordering::SeqCst);
            }
            Act::Pass
        });
        let reattach = {
            let private = flusso.private.to_string();
            std::thread::spawn(move || {
                std::process::Command::new(env!("CARGO_BIN_EXE_flusso"))
                    .args([
                        "reattach",
                        "t",
                        "--sink",
                        "primary",
                        "--accept-gap",
                        "--server",
                        &private,
                    ])
                    .output()
                    .unwrap()
            })
        };
        live::eventually(
            &format!("{step:?} reached"),
            Duration::from_secs(60),
            || {
                let proxy = proxy.clone();
                let alias = alias.clone();
                let deleted = std::sync::Arc::clone(&deleted);
                async move {
                    proxy.seen().iter().any(|seen| {
                        step.kills(
                            seen,
                            &alias,
                            deleted.load(std::sync::atomic::Ordering::SeqCst),
                        )
                    })
                }
            },
        )
        .await;
        flusso.kill();
        let _ = reattach.join();
        proxy.heal();

        let mut flusso = Flusso::start(&project, &[]).await;
        let status = flusso
            .until_status("staged", Duration::from_secs(60), |s| {
                let health = &s["sinks"]["primary"]["lanes"]["t"]["health"];
                health == "detached" || health == "open"
            })
            .await;
        let lane = &status["sinks"]["primary"]["lanes"]["t"];
        let record = live::meta_doc(&os_url, &format!("detach:{alias}")).await;
        let marker = live::meta_doc(&os_url, &alias).await.unwrap();
        let gaps = marker["gaps"].as_array().cloned().unwrap_or_default();
        if lane["health"] == "open" {
            assert!(
                live::record_gone(&os_url, &alias).await && !gaps.is_empty(),
                "{step:?}: attached only with its gap: lane {lane}, record {record:?}, marker {marker}"
            );
        } else {
            assert!(
                !live::record_gone(&os_url, &alias).await,
                "{step:?}: detached by its record: {record:?}"
            );
        }
        let (code, body) = flusso.wait(&inside, "applied").await;
        assert!(
            code == 409 || (code == 503 && body["reason"] == "detached"),
            "{step:?}: a change dropped is never vouched for: {code} {body}"
        );
        let expected = match step {
            Step::BeforeTheGap => ("detached", 0),
            Step::BeforeTheDelete => ("detached", 1),
            Step::AfterTheDelete => ("open", 1),
        };
        assert_eq!(
            (lane["health"].as_str().unwrap(), gaps.len()),
            expected,
            "{step:?}: lane {lane}, marker {marker}"
        );
        flusso.stop();
    }
}
