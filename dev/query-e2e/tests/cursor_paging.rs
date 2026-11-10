//! Real-service e2e for page cursors (issue #156, ADR 0008).
//!
//! Proves what the unit tests can't, against a **live** OpenSearch indexed by
//! the **real sink**:
//!
//! - **Live walk** (the default): a delete behind the position skips nothing,
//!   an insert after it shows up, and moving the hash alias to a copied
//!   generation mid-walk repeats nothing.
//! - **Pinned walk** (`point_in_time`): a write mid-walk never shows up, an
//!   alias swap doesn't disturb it, and once a reindex drops the generation it
//!   pinned the next page is `CursorExpired`; `close` ends it early.
//! - **Prefixed union, live**: colliding ids stay apart (the `_index`
//!   tiebreak), and a reindex of one index mid-walk repeats no tie — the resume
//!   follows the hash alias to its new generation. An `ids` walk pages too.
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
async fn a_live_walk_stays_fresh_and_follows_a_reindex() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let users = seed_users(&sink, &base).await;
    let client = Client::connect(&base).unwrap();

    let walk = || User::query().size(2);
    let first = walk().cursor(None).send(&client).await.unwrap();
    let mut seen: Vec<String> = first.hits.iter().map(|hit| hit.id.clone()).collect();
    assert_eq!(seen, ["1", "2"]);
    let next = first.next.expect("a full first page has a next");
    assert!(!next.is_pinned(), "a cursor walk is live by default");

    // A delete behind the position, an insert after it, then the hash alias
    // moves to a copy of the generation — what a reindex looks like to a reader.
    delete_doc(&base, &User::physical_index(), "1").await;
    write(
        &sink,
        &users.index,
        "6",
        document("email", "u6@example.com"),
    )
    .await;
    refresh(&base, &User::physical_index()).await;
    let gen1 = format!("{}_1", User::physical_index());
    let copy = format!("{}_99", User::physical_index());
    copy_index(&base, &gen1, &copy).await;
    swap_alias(&base, &User::physical_index(), &gen1, &copy).await;

    let mut cursor = Some(next);
    let mut pages = 1;
    while let Some(resumed) = cursor {
        let page = walk().cursor(resumed).send(&client).await.unwrap();
        seen.extend(page.hits.iter().map(|hit| hit.id.clone()));
        cursor = page.next;
        pages += 1;
        assert!(pages < 10, "the walk must end");
    }
    assert_eq!(
        seen,
        ["1", "2", "3", "4", "5", "6"],
        "the delete behind shifts nothing, the insert after shows up, the swap repeats nothing"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_pinned_walk_reads_one_view_until_its_generation_drops() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let users = seed_users(&sink, &base).await;
    let client = Client::connect(&base).unwrap();

    // Two pinned walks open their point-in-time on generation 1.
    let walk = || User::query().size(2);
    let first = walk()
        .cursor(None)
        .point_in_time()
        .send(&client)
        .await
        .unwrap();
    let doomed = walk()
        .cursor(None)
        .point_in_time()
        .send(&client)
        .await
        .unwrap();
    let abandoned = walk()
        .cursor(None)
        .point_in_time()
        .send(&client)
        .await
        .unwrap();
    assert_eq!(first.total, 5);
    let mut seen: Vec<String> = first.hits.iter().map(|hit| hit.id.clone()).collect();

    // A write that sorts after everything seen so far (a live walk would return
    // it), then the hash alias moves to an empty generation while generation 1
    // still exists.
    write(
        &sink,
        &users.index,
        "6",
        document("email", "u6@example.com"),
    )
    .await;
    refresh(&base, &User::physical_index()).await;
    let gen1 = format!("{}_1", User::physical_index());
    let swapped = format!("{}_99", User::physical_index());
    swap_alias(&base, &User::physical_index(), &gen1, &swapped).await;
    let offset = User::query().send(&client).await.unwrap();
    assert_eq!(
        offset.total, 0,
        "offset paging now reads the new generation"
    );

    // The resume follows the token's mode: no `point_in_time()` needed here.
    let mut next = first.next;
    let mut pages = 1;
    while let Some(cursor) = next {
        assert!(cursor.is_pinned());
        let resumed: PageCursor = cursor.to_string().parse().unwrap();
        let page = walk().cursor(resumed).send(&client).await.unwrap();
        assert_eq!(page.total, 5, "every page reports the pinned view's total");
        seen.extend(page.hits.iter().map(|hit| hit.id.clone()));
        next = page.next;
        pages += 1;
        assert!(pages < 10, "the walk must end");
    }
    assert_eq!(
        seen,
        ["1", "2", "3", "4", "5"],
        "no duplicate, no skip, no mid-walk write, no generation mix"
    );

    // `close` ends a pinned walk early.
    let abandoned = abandoned.next.expect("a full first page has a next");
    abandoned.close(&client).await.unwrap();
    abandoned.close(&client).await.unwrap();
    let closed = walk().cursor(abandoned).send(&client).await;
    assert!(
        matches!(closed, Err(Error::CursorExpired { .. })),
        "a closed pinned walk is expired, got {closed:?}"
    );

    // Back to the sink's own lifecycle: a reindex activates generation 2 and
    // drops generation 1, so a walk still pinned to it ends.
    swap_alias(&base, &User::physical_index(), &swapped, &gen1).await;
    delete_index(&base, &swapped).await;
    sink.reindex(&users).await.unwrap();
    sink.ensure_index(&users).await.unwrap();
    sink.mark_seeded(&users.index).await.unwrap();
    let expired = walk()
        .cursor(doomed.next.expect("a full first page has a next"))
        .send(&client)
        .await;
    assert!(
        matches!(expired, Err(Error::CursorExpired { .. })),
        "a dropped generation expires the pinned walk, got {expired:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_prefixed_live_union_walk_follows_a_reindex_without_repeating_ties() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base).with_index_prefix("e2e_");
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
    let path = StoreItem::query()
        .physical_path()
        .split(',')
        .map(|index| format!("e2e_{index}"))
        .collect::<Vec<_>>()
        .join(",");
    refresh(&base, &path).await;
    let client = Client::connect(&base).unwrap().index_prefix("e2e_");

    // `match_all`: every score ties, so the order is `_index` then `_id` —
    // orders 1, orders 2, users 1, users 2.
    let walk = || StoreItem::query().size(1);
    let mut seen = Vec::new();
    let mut cursor: Option<PageCursor> = None;
    for page_number in 1..10 {
        if page_number == 4 {
            // Mid-walk, right after `users 1`: users is reindexed into
            // generation 2. Resuming after `…users…_1` would repeat both users.
            let alias = format!("e2e_{}", User::physical_index());
            let gen1 = format!("{alias}_1");
            let gen2 = format!("{alias}_2");
            copy_index(&base, &gen1, &gen2).await;
            swap_alias(&base, &alias, &gen1, &gen2).await;
        }
        let page = walk().cursor(cursor).send(&client).await.unwrap();
        for hit in &page.hits {
            seen.push(match &hit.source {
                StoreItem::User(user) => format!("user:{}:{}", hit.id, user.email),
                StoreItem::Order(order) => format!("order:{}:{}", hit.id, order.status),
            });
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(
        seen,
        [
            "order:1:open",
            "order:2:paid",
            "user:1:a@example.com",
            "user:2:b@example.com"
        ],
        "colliding ids stay apart and the reindex repeats nothing"
    );

    // A single-index `ids` walk under the same prefix.
    let mut ids = Vec::new();
    let mut cursor: Option<PageCursor> = None;
    loop {
        let page = User::query()
            .size(1)
            .cursor(cursor)
            .ids(&client)
            .await
            .unwrap();
        ids.extend(page.ids);
        match page.next {
            Some(next) => cursor = Some(next),
            None => break,
        }
    }
    assert_eq!(ids, ["1", "2"]);
}

/// Seed `users` 1..=5 (generation 1 + the hash alias) and make them searchable.
async fn seed_users(sink: &sink_opensearch::OpensearchSink, base: &str) -> kernel::IndexMapping {
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
    seed_many(sink, &users, &docs).await;
    refresh(base, &User::physical_index()).await;
    users
}

/// Copy every document of `from` into a new index `to`, searchable at once.
async fn copy_index(base: &str, from: &str, to: &str) {
    reqwest::Client::new()
        .post(format!("{base}/_reindex?refresh=true"))
        .json(&serde_json::json!({ "source": { "index": from }, "dest": { "index": to } }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

async fn delete_doc(base: &str, index: &str, id: &str) {
    reqwest::Client::new()
        .delete(format!("{base}/{index}/_doc/{id}?refresh=true"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

/// Point `alias` at `to` instead of `from`, atomically (creating `to` empty
/// when missing).
async fn swap_alias(base: &str, alias: &str, from: &str, to: &str) {
    let http = reqwest::Client::new();
    let _ = http.put(format!("{base}/{to}")).send().await.unwrap();
    http.post(format!("{base}/_aliases"))
        .json(&serde_json::json!({ "actions": [
            { "remove": { "index": from, "alias": alias } },
            { "add": { "index": to, "alias": alias } },
        ]}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}

async fn delete_index(base: &str, index: &str) {
    reqwest::Client::new()
        .delete(format!("{base}/{index}"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
}
