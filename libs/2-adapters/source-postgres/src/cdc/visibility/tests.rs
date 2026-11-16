use std::time::Duration;

use super::*;

fn sees(snapshot: &str, xid: u32) -> bool {
    Snapshot::parse(snapshot).unwrap().sees(xid)
}

#[test]
fn a_snapshot_parses_with_and_without_running_xids() {
    assert_eq!(
        Snapshot::parse("100:110:105,107").unwrap(),
        Snapshot {
            xmin: 100,
            xmax: 110,
            running: vec![105, 107],
        }
    );
    assert_eq!(
        Snapshot::parse("736:736:").unwrap().running,
        Vec::<u64>::new()
    );
    for invalid in ["", "1:2", "a:2:", "1:2:x", "1:2:3,"] {
        assert!(Snapshot::parse(invalid).is_err(), "{invalid:?}");
    }
}

/// The reviewer's case (issue #343, R1): at epoch 1 the held transaction's
/// xid is `xmax`'s low 32 bits. Treating `xmax` as the next xid moved it an
/// epoch back, below `xmin`, where it read as visible.
#[test]
fn the_newest_transaction_at_or_past_xmax_is_not_visible_at_any_epoch() {
    assert!(!sees("736:736:", 736), "epoch 0, at xmax");
    assert!(!sees("736:736:", 737), "epoch 0, past xmax");
    assert!(sees("736:736:", 735), "epoch 0, just below");
    assert!(!sees("4294968030:4294968030:", 734), "epoch 1, at xmax");
    assert!(!sees("4294968030:4294968030:", 735), "epoch 1, past xmax");
    assert!(sees("4294968030:4294968030:", 733), "epoch 1, just below");
}

#[test]
fn widening_crosses_an_epoch_boundary_both_ways() {
    // xmax just past the boundary; a transaction from just before it.
    assert_eq!(widen(u32::MAX - 1, 4_294_967_300), 4_294_967_294);
    assert!(sees("4294967299:4294967300:", u32::MAX - 1));
    // xmax just before the boundary; a held transaction just past it.
    assert_eq!(widen(3, 4_294_967_290), 4_294_967_299);
    assert!(!sees("4294967280:4294967290:", 3));
    assert_eq!(widen(0, 0), 0);
}

#[test]
fn a_running_transaction_between_xmin_and_xmax_is_not_visible() {
    assert!(!sees("100:110:105", 105));
    assert!(sees("100:110:105", 106));
    assert!(sees("100:110:105", 99));
    assert!(!sees("4294967396:4294967406:4294967401", 105));
    assert!(sees("4294967396:4294967406:4294967401", 106));
}

#[test]
fn polls_every_half_millisecond_then_back_off_to_fifty() {
    let fast = Duration::from_micros(500);
    assert_eq!(poll_delay(Duration::ZERO, Duration::ZERO), fast);
    assert_eq!(poll_delay(Duration::from_millis(19), fast), fast);
    assert_eq!(
        poll_delay(Duration::from_millis(20), fast),
        Duration::from_millis(1)
    );
    let mut delay = fast;
    let mut waited = Duration::from_millis(20);
    for _ in 0..20 {
        delay = poll_delay(waited, delay);
        waited += delay;
    }
    assert_eq!(delay, Duration::from_millis(50));
}

#[test]
fn every_form_of_synchronous_standby_names_is_read() {
    let lists = |names: &str| names_standby(names, "flusso");
    assert!(!lists(""));
    assert!(lists("flusso"));
    assert!(lists("FLUSSO"));
    assert!(lists("other, flusso"));
    assert!(lists("*"));
    assert!(lists("2 (other, flusso)"));
    assert!(lists("FIRST 1 (other, \"Flusso\")"));
    assert!(lists("ANY 1 (*)"));
    assert!(lists("any 2(a,flusso,b)"));
    assert!(!lists("nobody"));
    assert!(!lists("FIRST 1 (flusso_replica)"));
    assert!(!lists("\"flusso,x\""));
    assert!(names_standby("\"a\"\"b\"", "a\"b"));
    assert!(!lists("first 1 (other)"), "the keyword is no name");
}

/// Every widened xid around `xmin`, `xmax` and the 2^32 boundary, at epochs
/// 0 to 2, against Postgres's own `pg_visible_in_snapshot` on the literal
/// snapshot: the rule and the widening agree with the server.
#[tokio::test]
#[ignore = "requires docker"]
async fn postgres_agrees_on_every_widened_xid() {
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    let container = test_postgres::image().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect(&format!(
            "postgres://postgres:postgres@127.0.0.1:{port}/postgres"
        ))
        .await
        .unwrap();
    let epoch = 1u64 << 32;
    let mut snapshots = Vec::new();
    for base in [0, epoch, 2 * epoch] {
        for xmax_low in [740u64, 4, epoch - 3] {
            let xmax = base + xmax_low;
            snapshots.push(format!("{xmax}:{xmax}:"));
            if xmax > 20 {
                let xmin = xmax - 10;
                snapshots.push(format!("{xmin}:{xmax}:{},{}", xmin + 2, xmin + 5));
            }
        }
    }
    let mut compared = 0;
    for text in &snapshots {
        let snapshot = Snapshot::parse(text).unwrap();
        for offset in -14i64..=6 {
            let Some(full) = snapshot.xmax.checked_add_signed(offset) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation)]
            let xid = full as u32;
            assert_eq!(widen(xid, snapshot.xmax), full, "{text} {xid}");
            let postgres: bool = sqlx::query_scalar(
                "SELECT pg_visible_in_snapshot($1::text::xid8, $2::pg_snapshot)",
            )
            .bind(full.to_string())
            .bind(text)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(snapshot.sees(xid), postgres, "{text}, xid {xid} ({full})");
            compared += 1;
        }
    }
    assert!(compared > 300, "{compared}");
}
