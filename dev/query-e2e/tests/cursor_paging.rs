//! Real-service e2e for page cursors (issue #156, ADR 0008).
//!
//! Proves what the unit tests can't: against a **live** OpenSearch, indexed by
//! the **real sink**, a cursor walk reads one snapshot. A document written
//! mid-walk never shows up, a reindex that moves the hash alias to a fresh
//! generation doesn't disturb it, and once the reindex drops the generation the
//! walk pinned, the next page is `CursorExpired`. A combined walk keeps two
//! indexes' colliding ids apart (the `_index` tiebreak), and an early
//! `PageCursor::close` ends the walk.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

mod common;

use std::collections::BTreeSet;

use common::{document, mapping, refresh, seed_many, sink, start_opensearch, write};
use flusso_query::{
    Client, Error, FlussoMultiDocument, FlussoRoot, FlussoScope, PageCursor, Result, Segment,
};
use kernel::MappingType;
use sink::Sink;

#[derive(Debug, serde::Deserialize)]
struct User {
    email: String,
}
impl FlussoScope for User {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for User {
    const INDEX: &'static str = "users";
    const SCHEMA_HASH: &'static str = "00000001";
}

#[derive(Debug, serde::Deserialize)]
struct Order {
    status: String,
}
impl FlussoScope for Order {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for Order {
    const INDEX: &'static str = "orders";
    const SCHEMA_HASH: &'static str = "00000002";
}

#[derive(Debug)]
enum StoreItem {
    User(User),
    Order(Order),
}

impl FlussoMultiDocument for StoreItem {
    const TARGETS: &'static [(&'static str, &'static str)] = &[
        (User::INDEX, User::SCHEMA_HASH),
        (Order::INDEX, Order::SCHEMA_HASH),
    ];

    fn decode(physical_index: &str, source: serde_json::Value) -> Result<Self> {
        if physical_index == User::physical_index() {
            return Ok(Self::User(serde_json::from_value(source)?));
        }
        if physical_index == Order::physical_index() {
            return Ok(Self::Order(serde_json::from_value(source)?));
        }
        Err(Error::UnexpectedIndex {
            index: physical_index.to_owned(),
        })
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_cursor_walk_reads_one_snapshot_across_writes_and_a_reindex() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let users = mapping("users", 1, "email", MappingType::Keyword);
    let seeded: Vec<(String, kernel::GenericValue)> = (1..=5)
        .map(|i| {
            (
                i.to_string(),
                document("email", &format!("u{i}@example.com")),
            )
        })
        .collect();
    let docs: Vec<(&str, kernel::GenericValue)> = seeded
        .iter()
        .map(|(id, doc)| (id.as_str(), doc.clone()))
        .collect();
    seed_many(&sink, &users, &docs).await;
    refresh(&base, &User::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    // Two walks open their snapshot on generation 1.
    let walk = || User::query().size(2);
    let first = walk().cursor(None).send(&client).await.unwrap();
    let doomed = walk().cursor(None).send(&client).await.unwrap();
    assert_eq!(first.total, 5);
    let mut seen: Vec<String> = first.hits.iter().map(|hit| hit.id.clone()).collect();
    assert!(
        first.hits.iter().all(|hit| hit.sort.len() == 1),
        "the `_id` tiebreak is the sort"
    );

    // A write that sorts ahead of everything the walk has seen, then a reindex
    // that stages generation 2.
    write(
        &sink,
        &users.index,
        "0",
        document("email", "u0@example.com"),
    )
    .await;
    refresh(&base, &User::physical_index()).await;
    sink.reindex(&users).await.unwrap();
    sink.ensure_index(&users).await.unwrap();

    // The walk finishes on its snapshot: every seeded id once, nothing new.
    let mut next = first.next;
    let mut pages = 1;
    while let Some(cursor) = next {
        let token = cursor.to_string();
        let resumed: PageCursor = token.parse().unwrap();
        let page = walk().cursor(resumed).send(&client).await.unwrap();
        assert_eq!(page.total, 5, "every page reports the snapshot's total");
        seen.extend(page.hits.iter().map(|hit| hit.id.clone()));
        next = page.next;
        pages += 1;
        assert!(pages < 10, "the walk must end");
    }
    assert_eq!(
        seen,
        ["1", "2", "3", "4", "5"],
        "no duplicate, no skip, no mid-walk write"
    );

    // Activating generation 2 drops generation 1: a walk still pinned to it ends.
    sink.mark_seeded(&users.index).await.unwrap();
    let expired = walk()
        .cursor(doomed.next.expect("a full first page has a next"))
        .send(&client)
        .await;
    assert!(
        matches!(expired, Err(Error::CursorExpired { .. })),
        "a dropped generation expires the cursor, got {expired:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_combined_walk_keeps_colliding_ids_apart_and_close_ends_it() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let users = mapping("users", 1, "email", MappingType::Keyword);
    let orders = mapping("orders", 2, "status", MappingType::Keyword);
    seed_many(
        &sink,
        &users,
        &[
            ("1", document("email", "a@example.com")),
            ("2", document("email", "b@example.com")),
        ],
    )
    .await;
    seed_many(
        &sink,
        &orders,
        &[
            ("1", document("status", "open")),
            ("2", document("status", "paid")),
        ],
    )
    .await;
    let path = StoreItem::query().physical_path().to_owned();
    refresh(&base, &path).await;
    let client = Client::connect(&base).unwrap();

    let mut seen = BTreeSet::new();
    let mut cursor: Option<PageCursor> = None;
    for _ in 0..10 {
        let page = StoreItem::query()
            .size(1)
            .cursor(cursor)
            .send(&client)
            .await
            .unwrap();
        for hit in &page.hits {
            let kind = match &hit.source {
                StoreItem::User(user) => format!("user:{}", user.email),
                StoreItem::Order(order) => format!("order:{}", order.status),
            };
            assert!(seen.insert((kind, hit.id.clone())), "no hit twice");
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        seen.len(),
        4,
        "both indexes' ids 1 and 2 all come back: {seen:?}"
    );

    let open = StoreItem::query()
        .size(1)
        .cursor(None)
        .send(&client)
        .await
        .unwrap();
    let next = open.next.expect("a full first page has a next");
    next.close(&client).await.unwrap();
    next.close(&client).await.unwrap();
    let closed = StoreItem::query().size(1).cursor(next).send(&client).await;
    assert!(
        matches!(closed, Err(Error::CursorExpired { .. })),
        "a closed cursor is expired, got {closed:?}"
    );
}
