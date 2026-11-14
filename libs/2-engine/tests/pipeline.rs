//! Full-pipeline e2e: real Postgres (logical replication) → the real ingest
//! engine → a lane → the real sink engine over a **real OpenSearch sink**,
//! asserting the *actual indexed document* after each change. Nothing is mocked.
//!
//! This is the suite that proves the one thing flusso exists to do — keep an
//! OpenSearch index in step with Postgres. The sibling `wal.rs` drives a
//! *recording* sink, so it only sees *which op* the engine emitted; it cannot
//! catch a change the engine turns into the *wrong* op (an update silently
//! rebuilt as a tombstone, a soft-deleted row that should have been removed).
//! Here we read the index back over HTTP and assert the document's contents.
//!
//! Coverage:
//! - [`live_crud_round_trips_across_key_types`] — insert / update / delete on the
//!   live path, across `uuid` / `int` / `bigint` / `text` primary keys. The
//!   key-type matrix guards a class of bug where the WAL key decoder
//!   (`cdc/pgoutput.rs::typed_value`, by OID) and the read-back decoder
//!   (`document/value.rs::decode_column`, by SQL type name) disagree on the
//!   [`GenericValue`] variant for a column: the engine's key match then misses
//!   and the change is written as a tombstone. `bigint`/`text` are the controls
//!   the two decoders already agreed on; `uuid`/`int` are the ones they must.
//! - [`soft_delete_tombstones_when_set_and_restores_when_unset`] — with a
//!   soft-delete marker (boolean *and* timestamp), a row whose marker is set is
//!   removed from the index even though it still exists in Postgres, and clearing
//!   the marker brings it back.
//! - [`backfill_indexes_active_rows_and_skips_soft_deleted`] — the initial seed:
//!   rows that pre-date the slot are indexed by backfill, except soft-deleted
//!   ones, which must never appear.
//! - [`restart_reseeds_when_the_generation_index_was_deleted`] and
//!   [`restart_rebuilds_when_the_replication_slot_was_dropped`] — the two ways a
//!   seed marker can lie (issue #120): the generation index it names is gone, or
//!   the replication slot that fed it is gone. Either way a restart must refill
//!   the index rather than trust the marker; the slot case must also drop rows
//!   that no longer exist at the source, which only a fresh generation can do.
//! - [`deleted_and_reparented_children_leave_their_old_parent`] — a hard-deleted
//!   `has_many` child, a child moved to another parent, and a row deleted three
//!   relations deep (`belongs_to → has_many → has_many`) all rebuild the
//!   document they used to sit in, traced through the WAL pre-image (issue #140,
//!   ADR 0007). The child tables start on `DEFAULT` and the capture sets them
//!   to `FULL` itself (`manage_replica_identity`); the root is set to `FULL` by
//!   hand, so the same test proves a `FULL` root keeps its plain `_id`.
//! - [`a_self_referencing_has_many_rebuilds_the_parent`] — a `has_many` back at
//!   the root table (issue #167): inserting, renaming, deleting, or re-parenting
//!   a child or grandchild rebuilds every document that embeds it, to the
//!   declared depth, while the row's own document still gets its own op.
//! - [`an_update_of_an_unread_column_writes_nothing`] — in the same tree, an
//!   update that changes only a column no document reads (issue #169) writes
//!   neither the row's own document nor its parent's (their `_version`s hold),
//!   while renaming the row still rewrites both.
//! - [`a_root_filtered_tree_indexes_only_top_level_rows`] — the tree the
//!   related-tables guide recommends: the same two levels plus a root filter
//!   `parent_id is_null`. Only top-level rows get a document, through backfill,
//!   child/grandchild insert/update/delete, re-parenting, promotion to the top
//!   level, and demotion under another row.
//!
//! - [`binary_columns_and_constants_are_indexed_as_written`] — a `bytea` at the
//!   root, as `bytea[]`, and inside `belongs_to` / `has_one` / `has_many` /
//!   `many_to_many` and two levels deep reaches OpenSearch as base64 of its
//!   bytes, on backfill and after a live update (issue #291: it was the column's
//!   Postgres hex text); a `constant` number, list, list mixing kinds, and
//!   object land as written.
//!
//! - [`date_and_time_columns_are_indexed_and_queryable`] — `date`, `timestamp`
//!   (without time zone) and `timestamptz` columns, plain and as arrays, at 0,
//!   3 and 6 fractional digits, land on backfill and after a live insert and
//!   update, under a non-UTC database `TimeZone`; a `range` query and a sort
//!   over each see the instants Postgres holds (issue #304: a naive timestamp
//!   was written with a space, which a `date` field refuses).
//!
//! - [`an_id_past_512_bytes_is_decided_by_on_error`] — a text key OpenSearch
//!   refuses as an `_id` (over 512 bytes) is one rejected document, decided by
//!   `on_error`: `skip` quarantines it and acks the batch, `stop` stops the sink
//!   engine with the batch unacked; the rest of its batch lands either way
//!   (issue #301: it failed the whole bulk request).
//!
//! Each change is verified by polling a realtime `GET {index}/_doc/{id}` (reads
//! the translog, so no refresh wait) until the index reflects the expectation or
//! a deadline trips — so the assertions are deterministic despite async flush.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p engine --test pipeline -- --ignored
//! ```

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use engine::{EngineError, FailurePolicies, FailurePolicy, IngestEngine, SinkControl, SinkEngine};
use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldName, FieldSource, Filter, FlussoType,
    GenericValue, IndexName, IndexSchema, Join, JoinKind, NullCheckFilter, NullOp, Relation,
    Secret, SinkName, SoftDelete, SoftDeleteColumn, TableName,
};
use sink::Sink;
use sink_opensearch::OpensearchSink;
use source::SourceSpec;
use source::cdc::ChangeCapture;
use source::document::DocumentBuilder;
use source_postgres::{PgDocumentBuilder, ReplicationConfig, WalChangeCapture};
use sqlx::AssertSqlSafe;
use sqlx::postgres::{PgPool, PgPoolOptions};
use stream::Stream;
use stream_channel::ChannelStream;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::core::wait::HttpWaitStrategy;
use testcontainers_modules::testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, GenericImage, ImageExt};

// ───────────────────────────── test 1: live CRUD ─────────────────────────────

/// One index in the key-type matrix: a Postgres `id` column type, the SQL
/// literal for a concrete id, and that id's string form as the OpenSearch `_id`.
struct Case {
    /// Logical index / table name.
    index: &'static str,
    /// SQL type of the `id` primary key.
    id_type: &'static str,
    /// SQL literal for the row's id (`'…'` for text/uuid, bare for integers).
    id_literal: &'static str,
    /// The document `_id` the sink derives — the key value in string form.
    doc_id: &'static str,
}

const CASES: &[Case] = &[
    // The two the WAL/read-back decoders must be made to agree on.
    Case {
        index: "by_uuid",
        id_type: "uuid",
        id_literal: "'095b7d61-cbbc-49d6-842c-231d06b81e7a'",
        doc_id: "095b7d61-cbbc-49d6-842c-231d06b81e7a",
    },
    Case {
        index: "by_int",
        id_type: "int",
        id_literal: "7",
        doc_id: "7",
    },
    // Controls: types the decoders already agreed on. They must keep working.
    Case {
        index: "by_bigint",
        id_type: "bigint",
        id_literal: "8",
        doc_id: "8",
    },
    Case {
        index: "by_text",
        id_type: "text",
        id_literal: "'sku-9'",
        doc_id: "sku-9",
    },
];

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn live_crud_round_trips_across_key_types() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    let tables = CASES.iter().map(|c| c.index).collect::<Vec<_>>();
    for case in CASES {
        create_table(
            &pg.pool,
            &format!(
                "CREATE TABLE \"{}\" (id {} PRIMARY KEY, name text)",
                case.index, case.id_type
            ),
        )
        .await;
    }
    let spec = SourceSpec::new(
        CASES
            .iter()
            .map(|c| (index_name(c.index), simple_schema(c.index, None)))
            .collect::<BTreeMap<_, _>>(),
    );
    let live = start_pipeline(&pg, &os, spec, &tables).await;
    live.await_seeded(CASES.len()).await;

    for case in CASES {
        let what = case.index;

        // INSERT → the document appears with the inserted value.
        exec(
            &pg.pool,
            &format!(
                "INSERT INTO \"{}\" (id, name) VALUES ({}, 'first')",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, Some("first"))
            .await
            .unwrap_or_else(|got| panic!("[{what}] INSERT did not land: want first, saw {got}"));

        // UPDATE a non-key column → the document must be *updated*, not deleted.
        // This is the exact failure that motivated the suite: a live update on a
        // uuid/int key was rebuilt with a mismatched key and written as a
        // tombstone, so the row vanished from the index.
        exec(
            &pg.pool,
            &format!(
                "UPDATE \"{}\" SET name = 'second' WHERE id = {}",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, Some("second"))
            .await
            .unwrap_or_else(|got| {
                panic!(
                    "[{what}] UPDATE of a non-key column did not propagate: want second, saw \
                     {got} (\"absent\" means the update was wrongly applied as a tombstone)"
                )
            });

        // DELETE → the document is gone.
        exec(
            &pg.pool,
            &format!(
                "DELETE FROM \"{}\" WHERE id = {}",
                case.index, case.id_literal
            ),
        )
        .await;
        live.assert_eventually(case.index, case.doc_id, None)
            .await
            .unwrap_or_else(|got| {
                panic!("[{what}] DELETE did not tombstone: want absent, saw {got}")
            });
    }

    live.shutdown();
}

// ──────────────────────────── test 2: soft delete ────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn soft_delete_tombstones_when_set_and_restores_when_unset() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    // Two marker shapes: a boolean flag and a nullable timestamp. flusso treats a
    // marker as "deleted" when it is truthy — a true boolean, or *any* non-null
    // value — so both forms must tombstone when set and restore when cleared.
    create_table(
        &pg.pool,
        "CREATE TABLE soft_bool (id uuid PRIMARY KEY, name text, archived boolean NOT NULL DEFAULT false)",
    )
    .await;
    create_table(
        &pg.pool,
        "CREATE TABLE soft_ts (id int PRIMARY KEY, name text, deleted_at timestamptz)",
    )
    .await;

    let spec = SourceSpec::new(BTreeMap::from([
        (
            index_name("soft_bool"),
            simple_schema("soft_bool", Some(soft_delete_on("archived"))),
        ),
        (
            index_name("soft_ts"),
            simple_schema("soft_ts", Some(soft_delete_on("deleted_at"))),
        ),
    ]));
    let live = start_pipeline(&pg, &os, spec, &["soft_bool", "soft_ts"]).await;
    live.await_seeded(2).await;

    let uuid = "11111111-2222-3333-4444-555555555555";

    // ── boolean marker ──
    exec(
        &pg.pool,
        &format!("INSERT INTO soft_bool (id, name, archived) VALUES ('{uuid}', 'alive', false)"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_bool] active row should be indexed, saw {got}"));

    // Set the marker → the row still exists in Postgres but must leave the index.
    exec(
        &pg.pool,
        &format!("UPDATE soft_bool SET archived = true WHERE id = '{uuid}'"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, None)
        .await
        .unwrap_or_else(|got| {
            panic!("[soft_bool] soft-deleted row should be tombstoned, saw {got}")
        });

    // Clear the marker → the row returns to the index.
    exec(
        &pg.pool,
        &format!("UPDATE soft_bool SET archived = false WHERE id = '{uuid}'"),
    )
    .await;
    live.assert_eventually("soft_bool", uuid, Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_bool] restored row should be re-indexed, saw {got}"));

    // ── timestamp marker ──
    exec(
        &pg.pool,
        "INSERT INTO soft_ts (id, name) VALUES (1, 'alive')",
    )
    .await;
    live.assert_eventually("soft_ts", "1", Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] active row should be indexed, saw {got}"));

    exec(
        &pg.pool,
        "UPDATE soft_ts SET deleted_at = now() WHERE id = 1",
    )
    .await;
    live.assert_eventually("soft_ts", "1", None)
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] soft-deleted row should be tombstoned, saw {got}"));

    exec(
        &pg.pool,
        "UPDATE soft_ts SET deleted_at = NULL WHERE id = 1",
    )
    .await;
    live.assert_eventually("soft_ts", "1", Some("alive"))
        .await
        .unwrap_or_else(|got| panic!("[soft_ts] restored row should be re-indexed, saw {got}"));

    live.shutdown();
}

// ───────────────────────────── test 3: backfill ──────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn backfill_indexes_active_rows_and_skips_soft_deleted() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    create_table(
        &pg.pool,
        "CREATE TABLE seeded (id int PRIMARY KEY, name text, archived boolean NOT NULL DEFAULT false)",
    )
    .await;
    // Rows that exist *before* the slot — only backfill can surface them. One is
    // soft-deleted and must be skipped; the others must be indexed.
    exec(
        &pg.pool,
        "INSERT INTO seeded (id, name, archived) VALUES \
         (1, 'one', false), (2, 'two', false), (3, 'gone', true)",
    )
    .await;

    let spec = SourceSpec::new(BTreeMap::from([(
        index_name("seeded"),
        simple_schema("seeded", Some(soft_delete_on("archived"))),
    )]));
    let live = start_pipeline(&pg, &os, spec, &["seeded"]).await;
    live.await_seeded(1).await;

    // Active rows are backfilled; the soft-deleted one never appears.
    live.assert_eventually("seeded", "1", Some("one"))
        .await
        .unwrap_or_else(|got| panic!("[seeded] active row 1 should be backfilled, saw {got}"));
    live.assert_eventually("seeded", "2", Some("two"))
        .await
        .unwrap_or_else(|got| panic!("[seeded] active row 2 should be backfilled, saw {got}"));
    live.assert_eventually("seeded", "3", None)
        .await
        .unwrap_or_else(|got| panic!("[seeded] soft-deleted row 3 must not be indexed, saw {got}"));

    live.shutdown();
}

// ─────────────────── tests 4 + 5: the seed marker can lie ────────────────────

/// Seed a `things` index from `rows` (SQL `VALUES` tuples) and stop the engine
/// cleanly, returning the spec to restart with and the generation index that
/// served the seed.
// ─────────────────── test 6: deleted and re-parented children ───────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn deleted_and_reparented_children_leave_their_old_parent() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    for ddl in [
        "CREATE TABLE parent (id int PRIMARY KEY, name text)",
        "CREATE TABLE child (id int PRIMARY KEY, parent_id int NOT NULL REFERENCES parent(id), label text)",
        "CREATE TABLE customers (id int PRIMARY KEY, name text)",
        "CREATE TABLE orders (id int PRIMARY KEY, name text, customer_id int NOT NULL REFERENCES customers(id))",
        "CREATE TABLE addresses (id int PRIMARY KEY, customer_id int NOT NULL REFERENCES customers(id), name text)",
        "CREATE TABLE lines (id int PRIMARY KEY, address_id int NOT NULL REFERENCES addresses(id), label text)",
        "ALTER TABLE parent REPLICA IDENTITY FULL",
    ] {
        create_table(&pg.pool, ddl).await;
    }

    let mut parents = simple_schema("parent", None);
    parents.fields.push(join_field(
        "children",
        "child",
        JoinKind::HasMany {
            foreign_key: column("parent_id"),
        },
        vec![column_field("label", "label")],
    ));
    let mut orders = simple_schema("orders", None);
    orders.fields.push(join_field(
        "customer",
        "customers",
        JoinKind::BelongsTo {
            column: column("customer_id"),
        },
        vec![
            column_field("name", "name"),
            join_field(
                "addresses",
                "addresses",
                JoinKind::HasMany {
                    foreign_key: column("customer_id"),
                },
                vec![
                    column_field("name", "name"),
                    join_field(
                        "lines",
                        "lines",
                        JoinKind::HasMany {
                            foreign_key: column("address_id"),
                        },
                        vec![column_field("label", "label")],
                    ),
                ],
            ),
        ],
    ));
    let spec = SourceSpec::new(BTreeMap::from([
        (index_name("parent"), parents),
        (index_name("orders"), orders),
    ]));
    let live = start_pipeline(
        &pg,
        &os,
        spec,
        &[
            "parent",
            "child",
            "customers",
            "orders",
            "addresses",
            "lines",
        ],
    )
    .await;
    live.await_seeded(2).await;

    let children = |doc: &serde_json::Value| labels(doc.pointer("/children"));
    let lines = |doc: &serde_json::Value| {
        let mut out = Vec::new();
        for address in doc
            .pointer("/customer/addresses")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            out.extend(labels(address.get("lines")));
        }
        out.sort();
        out
    };

    // A FULL root keeps a plain `_id`, and the child shows up under its parent.
    for sql in [
        "INSERT INTO parent VALUES (1, 'p1'), (3, 'p3')",
        "INSERT INTO child VALUES (10, 1, 'kept'), (11, 1, 'deleted'), (12, 1, 'moved')",
    ] {
        exec(&pg.pool, sql).await;
    }
    live.assert_doc_eventually("parent", "1", &children, &["deleted", "kept", "moved"])
        .await;

    // DELETE a child → its old parent drops it.
    exec(&pg.pool, "DELETE FROM child WHERE id = 11").await;
    live.assert_doc_eventually("parent", "1", &children, &["kept", "moved"])
        .await;

    // Re-parent a child → it leaves the old parent and joins the new one.
    exec(&pg.pool, "UPDATE child SET parent_id = 3 WHERE id = 12").await;
    live.assert_doc_eventually("parent", "1", &children, &["kept"])
        .await;
    live.assert_doc_eventually("parent", "3", &children, &["moved"])
        .await;

    // Three relations deep: only the first hop reads the pre-image.
    for sql in [
        "INSERT INTO customers VALUES (5, 'c5')",
        "INSERT INTO orders VALUES (50, 'o50', 5)",
        "INSERT INTO addresses VALUES (500, 5, 'home')",
        "INSERT INTO lines VALUES (5000, 500, 'kept'), (5001, 500, 'deleted')",
    ] {
        exec(&pg.pool, sql).await;
    }
    live.assert_doc_eventually("orders", "50", &lines, &["deleted", "kept"])
        .await;
    exec(&pg.pool, "DELETE FROM lines WHERE id = 5001").await;
    live.assert_doc_eventually("orders", "50", &lines, &["kept"])
        .await;

    live.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_self_referencing_has_many_rebuilds_the_parent() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    create_table(
        &pg.pool,
        "CREATE TABLE items (id int PRIMARY KEY, parent_id int REFERENCES items(id), name text)",
    )
    .await;

    let children = |nested: Vec<Field>| {
        let mut fields = vec![column_field("label", "name")];
        fields.extend(nested);
        join_field(
            "children",
            "items",
            JoinKind::HasMany {
                foreign_key: column("parent_id"),
            },
            fields,
        )
    };
    let mut items = simple_schema("items", None);
    items.fields.push(children(vec![children(vec![])]));
    let spec = SourceSpec::new(BTreeMap::from([(index_name("items"), items)]));
    let live = start_pipeline(&pg, &os, spec, &["items"]).await;
    live.await_seeded(1).await;

    let child_labels = |doc: &serde_json::Value| labels(doc.pointer("/children"));
    let grandchild_labels = |doc: &serde_json::Value| {
        let mut out = Vec::new();
        for child in doc
            .pointer("/children")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            out.extend(labels(child.get("children")));
        }
        out.sort();
        out
    };

    exec(
        &pg.pool,
        "INSERT INTO items VALUES (1, NULL, 'p1'), (3, NULL, 'p3')",
    )
    .await;
    exec(
        &pg.pool,
        "INSERT INTO items VALUES (10, 1, 'kept'), (11, 1, 'deleted'), (12, 1, 'moved')",
    )
    .await;
    live.assert_doc_eventually("items", "1", &child_labels, &["deleted", "kept", "moved"])
        .await;
    live.assert_eventually("items", "11", Some("deleted"))
        .await
        .unwrap();

    exec(&pg.pool, "UPDATE items SET name = 'renamed' WHERE id = 10").await;
    live.assert_doc_eventually(
        "items",
        "1",
        &child_labels,
        &["deleted", "moved", "renamed"],
    )
    .await;
    live.assert_eventually("items", "10", Some("renamed"))
        .await
        .unwrap();

    exec(&pg.pool, "DELETE FROM items WHERE id = 11").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["moved", "renamed"])
        .await;
    live.assert_eventually("items", "11", None).await.unwrap();

    exec(&pg.pool, "UPDATE items SET parent_id = 3 WHERE id = 12").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["renamed"])
        .await;
    live.assert_doc_eventually("items", "3", &child_labels, &["moved"])
        .await;

    // Two declared levels: a grandchild rebuilds its grandparent too.
    exec(&pg.pool, "INSERT INTO items VALUES (100, 10, 'grand')").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["grand"])
        .await;
    live.assert_doc_eventually("items", "10", &child_labels, &["grand"])
        .await;
    exec(&pg.pool, "UPDATE items SET name = 'regrand' WHERE id = 100").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["regrand"])
        .await;
    exec(&pg.pool, "DELETE FROM items WHERE id = 100").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &[])
        .await;
    live.assert_doc_eventually("items", "10", &child_labels, &[])
        .await;
    live.assert_eventually("items", "100", None).await.unwrap();

    live.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn an_update_of_an_unread_column_writes_nothing() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    create_table(
        &pg.pool,
        "CREATE TABLE items (id int PRIMARY KEY, parent_id int REFERENCES items(id), \
         name text, views int NOT NULL DEFAULT 0)",
    )
    .await;
    let mut items = simple_schema("items", None);
    items.fields.push(join_field(
        "children",
        "items",
        JoinKind::HasMany {
            foreign_key: column("parent_id"),
        },
        vec![column_field("label", "name")],
    ));
    let spec = SourceSpec::new(BTreeMap::from([(index_name("items"), items)]));
    let live = start_pipeline(&pg, &os, spec, &["items"]).await;
    live.await_seeded(1).await;
    let child_labels = |doc: &serde_json::Value| labels(doc.pointer("/children"));

    exec(&pg.pool, "INSERT INTO items VALUES (1, NULL, 'p1')").await;
    exec(&pg.pool, "INSERT INTO items VALUES (10, 1, 'kid')").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["kid"])
        .await;
    live.assert_eventually("items", "10", Some("kid"))
        .await
        .unwrap();
    let parent = live.fetch_version("items", "1").await;
    let child = live.fetch_version("items", "10").await;
    assert!(parent.is_some() && child.is_some());

    exec(&pg.pool, "UPDATE items SET views = views + 1 WHERE id = 10").await;
    // A later change on the same stream: once it lands, the update is past.
    exec(&pg.pool, "INSERT INTO items VALUES (50, NULL, 'marker')").await;
    live.assert_eventually("items", "50", Some("marker"))
        .await
        .unwrap();
    assert_eq!(
        live.fetch_version("items", "1").await,
        parent,
        "parent rewritten"
    );
    assert_eq!(
        live.fetch_version("items", "10").await,
        child,
        "row rewritten"
    );

    exec(&pg.pool, "UPDATE items SET name = 'renamed' WHERE id = 10").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["renamed"])
        .await;
    live.assert_eventually("items", "10", Some("renamed"))
        .await
        .unwrap();
    assert!(live.fetch_version("items", "1").await > parent);
    assert!(live.fetch_version("items", "10").await > child);

    live.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn a_root_filtered_tree_indexes_only_top_level_rows() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    create_table(
        &pg.pool,
        "CREATE TABLE items (id int PRIMARY KEY, parent_id int REFERENCES items(id), name text)",
    )
    .await;
    exec(
        &pg.pool,
        "INSERT INTO items VALUES (1, NULL, 'p1'), (2, NULL, 'p2'), \
         (10, 1, 'c10'), (100, 10, 'g100')",
    )
    .await;

    let children = |nested: Vec<Field>| {
        let mut fields = vec![column_field("label", "name")];
        fields.extend(nested);
        join_field(
            "children",
            "items",
            JoinKind::HasMany {
                foreign_key: column("parent_id"),
            },
            fields,
        )
    };
    let mut items = simple_schema("items", None);
    items.filters = Some(vec![Filter::NullCheck(NullCheckFilter {
        column: column("parent_id"),
        op: NullOp::IsNull,
    })]);
    items.fields.push(children(vec![children(vec![])]));
    let spec = SourceSpec::new(BTreeMap::from([(index_name("items"), items)]));
    let live = start_pipeline(&pg, &os, spec, &["items"]).await;
    live.await_seeded(1).await;

    let child_labels = |doc: &serde_json::Value| labels(doc.pointer("/children"));
    let grandchild_labels = |doc: &serde_json::Value| {
        let mut out = Vec::new();
        for child in doc
            .pointer("/children")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            out.extend(labels(child.get("children")));
        }
        out.sort();
        out
    };
    let absent = |id: &'static str| {
        let live = &live;
        async move {
            live.assert_eventually("items", id, None)
                .await
                .unwrap_or_else(|got| panic!("[items/{id}] must have no document, saw {got}"));
        }
    };

    live.assert_doc_eventually("items", "1", &child_labels, &["c10"])
        .await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["g100"])
        .await;
    live.assert_doc_eventually("items", "2", &child_labels, &[])
        .await;
    absent("10").await;
    absent("100").await;

    exec(&pg.pool, "INSERT INTO items VALUES (11, 1, 'c11')").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["c10", "c11"])
        .await;
    absent("11").await;
    exec(&pg.pool, "INSERT INTO items VALUES (101, 10, 'g101')").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["g100", "g101"])
        .await;
    absent("101").await;

    exec(&pg.pool, "UPDATE items SET name = 'c11b' WHERE id = 11").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["c10", "c11b"])
        .await;
    absent("11").await;
    exec(&pg.pool, "UPDATE items SET name = 'g101b' WHERE id = 101").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["g100", "g101b"])
        .await;
    absent("101").await;

    exec(&pg.pool, "DELETE FROM items WHERE id = 101").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["g100"])
        .await;
    exec(&pg.pool, "DELETE FROM items WHERE id = 11").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["c10"])
        .await;
    absent("11").await;
    absent("101").await;

    exec(&pg.pool, "UPDATE items SET parent_id = 2 WHERE id = 10").await;
    live.assert_doc_eventually("items", "1", &child_labels, &[])
        .await;
    live.assert_doc_eventually("items", "2", &child_labels, &["c10"])
        .await;
    live.assert_doc_eventually("items", "2", &grandchild_labels, &["g100"])
        .await;
    absent("10").await;

    exec(&pg.pool, "UPDATE items SET parent_id = NULL WHERE id = 10").await;
    live.assert_doc_eventually("items", "2", &child_labels, &[])
        .await;
    live.assert_doc_eventually("items", "10", &child_labels, &["g100"])
        .await;
    live.assert_eventually("items", "10", Some("c10"))
        .await
        .unwrap();

    exec(&pg.pool, "UPDATE items SET parent_id = 1 WHERE id = 2").await;
    live.assert_doc_eventually("items", "1", &child_labels, &["p2"])
        .await;
    absent("2").await;
    exec(&pg.pool, "UPDATE items SET parent_id = 2 WHERE id = 10").await;
    live.assert_doc_eventually("items", "1", &grandchild_labels, &["c10"])
        .await;
    absent("10").await;

    live.shutdown();
}

// ─────────────────────────────── binary columns ──────────────────────────────

/// A 100-byte value covering every high/low nibble class, longer than the 57
/// bytes after which Postgres's own `encode(…, 'base64')` wraps its output.
const BLOB_HEX: &str = "00ff7f80";

/// The bytes of `repeat(BLOB_HEX, n)`, decoded.
fn blob(n: usize) -> Vec<u8> {
    BLOB_HEX
        .repeat(n)
        .as_bytes()
        .chunks(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

/// `decode(repeat(BLOB_HEX, n), 'hex')`.
fn blob_sql(n: usize) -> String {
    format!("decode(repeat('{BLOB_HEX}', {n}), 'hex')")
}

/// Standard base64 back to bytes; `None` for anything that is not base64.
fn base64_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let digits = text.trim_end_matches('=');
    if text.len() % 4 != 0 || text.len() - digits.len() > 2 {
        return None;
    }
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in digits.bytes() {
        acc = (acc << 6) | ALPHABET.iter().position(|a| *a == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Every `binary` value in the document at the given pointers, base64-decoded,
/// rendered as one line per pointer so a failure shows what OpenSearch stored.
fn blobs_at<'a>(pointers: &'a [&'a str]) -> impl Fn(&serde_json::Value) -> Vec<String> + 'a {
    move |doc| {
        pointers
            .iter()
            .map(|pointer| match doc.pointer(pointer) {
                Some(serde_json::Value::String(text)) => match base64_decode(text) {
                    Some(bytes) => format!("{pointer}={}", bytes.len()),
                    None => format!("{pointer}=not base64: {text:.12}"),
                },
                Some(serde_json::Value::Array(items)) => format!(
                    "{pointer}={:?}",
                    items
                        .iter()
                        .map(|item| item.as_str().and_then(base64_decode).map(|b| b.len()))
                        .collect::<Vec<_>>()
                ),
                other => format!("{pointer}={other:?}"),
            })
            .collect()
    }
}

/// Every pointer's value (an array's first element) base64-decodes to `want`.
fn assert_blobs(doc: &serde_json::Value, pointers: &[&str], want: &[u8], when: &str) {
    for pointer in pointers {
        let value = match doc.pointer(pointer) {
            Some(serde_json::Value::Array(items)) => items.first(),
            other => other,
        };
        let text = value.and_then(serde_json::Value::as_str);
        assert_eq!(
            text.and_then(base64_decode).as_deref(),
            Some(want),
            "{pointer} {when}: {text:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn binary_columns_and_constants_are_indexed_as_written() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;

    for ddl in [
        "CREATE TABLE tags (id int PRIMARY KEY, blob bytea)",
        "CREATE TABLE things (id int PRIMARY KEY, name text, tag_id int REFERENCES tags(id), \
         blob bytea, blobs bytea[])",
        "CREATE TABLE notes (id int PRIMARY KEY, thing_id int REFERENCES things(id), \
         tag_id int REFERENCES tags(id), blob bytea)",
        "CREATE TABLE covers (id int PRIMARY KEY, thing_id int REFERENCES things(id), blob bytea)",
        "CREATE TABLE badges (id int PRIMARY KEY, blob bytea)",
        "CREATE TABLE thing_badges (thing_id int REFERENCES things(id), \
         badge_id int REFERENCES badges(id), PRIMARY KEY (thing_id, badge_id))",
    ] {
        create_table(&pg.pool, ddl).await;
    }
    for sql in [
        format!("INSERT INTO tags VALUES (1, {})", blob_sql(25)),
        format!(
            "INSERT INTO things VALUES (1, 'seeded', 1, {b}, ARRAY[{b}, {c}])",
            b = blob_sql(25),
            c = blob_sql(1)
        ),
        format!("INSERT INTO notes VALUES (1, 1, 1, {})", blob_sql(25)),
        format!("INSERT INTO covers VALUES (1, 1, {})", blob_sql(25)),
        format!("INSERT INTO badges VALUES (1, {})", blob_sql(25)),
        "INSERT INTO thing_badges VALUES (1, 1)".to_owned(),
    ] {
        exec(&pg.pool, &sql).await;
    }

    let constant = |name: &str, value: GenericValue| Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Constant(value),
    };
    let mut things = simple_schema("things", None);
    things.fields.extend([
        constant("weight", GenericValue::BigInt(5)),
        constant(
            "labels",
            GenericValue::Array(vec![
                GenericValue::String("a".to_owned()),
                GenericValue::String("b".to_owned()),
            ]),
        ),
        constant(
            "codes",
            GenericValue::Array(vec![
                GenericValue::BigInt(1),
                GenericValue::String("n/a".to_owned()),
            ]),
        ),
        constant(
            "meta",
            GenericValue::Map(BTreeMap::from([
                ("origin".to_owned(), GenericValue::String("crm".to_owned())),
                (
                    "rank".to_owned(),
                    GenericValue::Array(vec![GenericValue::BigInt(1), GenericValue::BigInt(2)]),
                ),
            ])),
        ),
        binary_field("blob", false),
        binary_field("blobs", true),
        join_field(
            "tag",
            "tags",
            JoinKind::BelongsTo {
                column: column("tag_id"),
            },
            vec![binary_field("blob", false)],
        ),
        join_field(
            "cover",
            "covers",
            JoinKind::HasOne {
                foreign_key: column("thing_id"),
            },
            vec![binary_field("blob", false)],
        ),
        join_field(
            "notes",
            "notes",
            JoinKind::HasMany {
                foreign_key: column("thing_id"),
            },
            vec![
                binary_field("blob", false),
                join_field(
                    "tag",
                    "tags",
                    JoinKind::BelongsTo {
                        column: column("tag_id"),
                    },
                    vec![binary_field("blob", false)],
                ),
            ],
        ),
        join_field(
            "badges",
            "badges",
            JoinKind::ManyToMany {
                through: kernel::Through {
                    table: table("thing_badges"),
                    left_key: column("thing_id"),
                    right_key: column("badge_id"),
                    filters: None,
                },
            },
            vec![binary_field("blob", false)],
        ),
    ]);
    let spec = SourceSpec::new(BTreeMap::from([(index_name("things"), things)]));
    let live = start_pipeline(
        &pg,
        &os,
        spec,
        &[
            "tags",
            "things",
            "notes",
            "covers",
            "badges",
            "thing_badges",
        ],
    )
    .await;
    live.await_seeded(1).await;

    let pointers = [
        "/blob",
        "/blobs",
        "/tag/blob",
        "/cover/blob",
        "/notes/0/blob",
        "/notes/0/tag/blob",
        "/badges/0/blob",
    ];
    let observe = blobs_at(&pointers);
    let full = blob(25).len();
    let expect = |len: usize| {
        let lines = [
            format!("/blob={len}"),
            format!("/blobs=[Some({len}), Some({})]", blob(1).len()),
            format!("/tag/blob={len}"),
            format!("/cover/blob={len}"),
            format!("/notes/0/blob={len}"),
            format!("/notes/0/tag/blob={len}"),
            format!("/badges/0/blob={len}"),
        ];
        lines
    };

    // Backfill (`build_many`): every position holds the bytes the row holds.
    let want = expect(full);
    let want: Vec<&str> = want.iter().map(String::as_str).collect();
    live.assert_doc_eventually("things", "1", &observe, &want)
        .await;
    let doc = live.fetch_source("things", "1").await.unwrap();
    assert_blobs(&doc, &pointers, &blob(25), "on backfill");
    assert_eq!(doc.get("weight"), Some(&serde_json::json!(5)), "{doc}");
    assert_eq!(
        doc.get("labels"),
        Some(&serde_json::json!(["a", "b"])),
        "{doc}"
    );
    assert_eq!(
        doc.get("codes"),
        Some(&serde_json::json!([1, "n/a"])),
        "{doc}"
    );
    assert_eq!(
        doc.get("meta"),
        Some(&serde_json::json!({ "origin": "crm", "rank": [1, 2] })),
        "{doc}"
    );

    // Live (`build`): new bytes at every position, written through the WAL.
    for sql in [
        format!("UPDATE tags SET blob = {} WHERE id = 1", blob_sql(3)),
        format!(
            "UPDATE things SET blob = {b}, blobs = ARRAY[{b}, {c}] WHERE id = 1",
            b = blob_sql(3),
            c = blob_sql(1)
        ),
        format!("UPDATE notes SET blob = {} WHERE id = 1", blob_sql(3)),
        format!("UPDATE covers SET blob = {} WHERE id = 1", blob_sql(3)),
        format!("UPDATE badges SET blob = {} WHERE id = 1", blob_sql(3)),
    ] {
        exec(&pg.pool, &sql).await;
    }
    let want = expect(blob(3).len());
    let want: Vec<&str> = want.iter().map(String::as_str).collect();
    live.assert_doc_eventually("things", "1", &observe, &want)
        .await;
    let doc = live.fetch_source("things", "1").await.unwrap();
    assert_blobs(&doc, &pointers, &blob(3), "after a live update");

    live.shutdown();
}

/// The `date` / `timestamp` / `timestamptz` value each `moments` row holds, as
/// written to its document, for the fields [`date_and_time_columns_are_indexed_and_queryable`] reads.
fn moments(doc: &serde_json::Value) -> Vec<String> {
    ["/d", "/ts", "/tz", "/ds", "/tss", "/tzs"]
        .iter()
        .map(|pointer| {
            format!(
                "{pointer}={}",
                doc.pointer(pointer).unwrap_or(&serde_json::Value::Null)
            )
        })
        .collect()
}

/// The ids of the `moments` documents `body` finds, in hit order, after a refresh.
async fn search_ids(live: &Pipeline, body: serde_json::Value) -> Vec<String> {
    live.http
        .post(format!("{}/moments/_refresh", live.os_url))
        .send()
        .await
        .unwrap();
    let found: serde_json::Value = live
        .http
        .post(format!("{}/moments/_search", live.os_url))
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    found
        .pointer("/hits/hits")
        .and_then(serde_json::Value::as_array)
        .unwrap_or_else(|| panic!("search failed: {found}"))
        .iter()
        .filter_map(|hit| hit.get("_id").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect()
}

/// A `date`, a `timestamp without time zone`, and a `timestamptz` column, each
/// also as an array, land in OpenSearch on backfill and after a live insert and
/// update, as ISO 8601 a `date` field takes with its default format, and a
/// `range` query and a sort over them see the instants Postgres holds (issue
/// #304: a naive timestamp was written with a space and every document holding
/// one was rejected). The database's `TimeZone` is not UTC, so a `timestamptz`
/// is still written as its UTC instant and a naive one as written.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn date_and_time_columns_are_indexed_and_queryable() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    exec(
        &pg.pool,
        "ALTER DATABASE postgres SET timezone = 'America/New_York'",
    )
    .await;
    create_table(
        &pg.pool,
        "CREATE TABLE moments (id int PRIMARY KEY, name text, d date, ts timestamp, \
         tz timestamptz, ds date[], tss timestamp[], tzs timestamptz[])",
    )
    .await;
    exec(
        &pg.pool,
        "INSERT INTO moments VALUES \
         (1, 'one', '2024-01-01', '2024-01-01 10:00:00', '2024-01-01 10:00:00+00', \
          '{2024-01-01}', '{\"2024-01-01 10:00:00\"}', '{\"2024-01-01 10:00:00+00\"}'), \
         (2, 'two', '2024-01-02', '2024-01-02 03:04:05.123', '2024-01-02 03:04:05.123+00', \
          '{2024-01-02}', '{\"2024-01-02 03:04:05.123\"}', '{\"2024-01-02 03:04:05.123+00\"}'), \
         (3, 'three', '2024-01-03', '2024-01-03 03:04:05.123456', \
          '2024-01-03 03:04:05.123456+00', '{2024-01-03}', '{\"2024-01-03 03:04:05.123456\"}', \
          '{\"2024-01-03 03:04:05.123456+00\"}')",
    )
    .await;

    let mut schema = simple_schema("moments", None);
    schema.fields.extend([
        typed_field("d", FlussoType::Date, false),
        typed_field("ts", FlussoType::Timestamp, false),
        typed_field("tz", FlussoType::Timestamp, false),
        typed_field("ds", FlussoType::Date, true),
        typed_field("tss", FlussoType::Timestamp, true),
        typed_field("tzs", FlussoType::Timestamp, true),
    ]);
    let spec = SourceSpec::new(BTreeMap::from([(index_name("moments"), schema)]));
    let live = start_pipeline(&pg, &os, spec, &["moments"]).await;
    live.await_seeded(1).await;

    let row = |day: &str, ts: &str, tz: &str| -> Vec<String> {
        vec![
            format!("/d=\"{day}\""),
            format!("/ts=\"{ts}\""),
            format!("/tz=\"{tz}\""),
            format!("/ds=[\"{day}\"]"),
            format!("/tss=[\"{ts}\"]"),
            format!("/tzs=[\"{tz}\"]"),
        ]
    };
    let assert_row = async |id: &str, want: Vec<String>| {
        let want: Vec<&str> = want.iter().map(String::as_str).collect();
        live.assert_doc_eventually("moments", id, &moments, &want)
            .await;
    };

    assert_row(
        "1",
        row(
            "2024-01-01",
            "2024-01-01T10:00:00",
            "2024-01-01T10:00:00+00:00",
        ),
    )
    .await;
    assert_row(
        "2",
        row(
            "2024-01-02",
            "2024-01-02T03:04:05.123",
            "2024-01-02T03:04:05.123+00:00",
        ),
    )
    .await;
    assert_row(
        "3",
        row(
            "2024-01-03",
            "2024-01-03T03:04:05.123456",
            "2024-01-03T03:04:05.123456+00:00",
        ),
    )
    .await;

    exec(
        &pg.pool,
        "INSERT INTO moments VALUES (4, 'four', '2024-01-04', '2024-01-04 00:00:00.5', \
         '2024-01-04 00:00:00.5+00', '{2024-01-04}', '{\"2024-01-04 00:00:00.5\"}', \
         '{\"2024-01-04 00:00:00.5+00\"}')",
    )
    .await;
    exec(
        &pg.pool,
        "UPDATE moments SET d = '2024-01-05', ts = '2024-01-05 12:00:00', \
         tz = '2024-01-05 12:00:00+00', ds = '{2024-01-05}', tss = '{\"2024-01-05 12:00:00\"}', \
         tzs = '{\"2024-01-05 12:00:00+00\"}' WHERE id = 1",
    )
    .await;
    assert_row(
        "4",
        row(
            "2024-01-04",
            "2024-01-04T00:00:00.500",
            "2024-01-04T00:00:00.500+00:00",
        ),
    )
    .await;
    assert_row(
        "1",
        row(
            "2024-01-05",
            "2024-01-05T12:00:00",
            "2024-01-05T12:00:00+00:00",
        ),
    )
    .await;

    for field in ["d", "ts", "tz", "ds", "tss", "tzs"] {
        let range = search_ids(
            &live,
            serde_json::json!({
                "query": { "range": { field: { "gte": "2024-01-02", "lt": "2024-01-04" } } },
                "sort": [{ field: "asc" }],
            }),
        )
        .await;
        assert_eq!(range, ["2", "3"], "range over {field}");
        let sorted = search_ids(&live, serde_json::json!({ "sort": [{ field: "desc" }] })).await;
        assert_eq!(sorted, ["1", "4", "3", "2"], "sort over {field}");
    }
    let to_the_millisecond = search_ids(
        &live,
        serde_json::json!({
            "query": { "range": { "ts": { "gte": "2024-01-02T03:04:05.123", "lte": "2024-01-02T03:04:05.123" } } },
        }),
    )
    .await;
    assert_eq!(to_the_millisecond, ["2"]);

    live.shutdown();
}

/// The sorted `label`s of a nested array, empty when absent.
fn labels(array: Option<&serde_json::Value>) -> Vec<String> {
    let mut out: Vec<String> = array
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("label").and_then(serde_json::Value::as_str))
        .map(str::to_owned)
        .collect();
    out.sort();
    out
}

async fn seed_things_then_stop(pg: &Pg, os: &Os, rows: &str) -> (SourceSpec, String) {
    create_table(
        &pg.pool,
        "CREATE TABLE things (id int PRIMARY KEY, name text)",
    )
    .await;
    exec(
        &pg.pool,
        &format!("INSERT INTO things (id, name) VALUES {rows}"),
    )
    .await;

    let spec = SourceSpec::new(BTreeMap::from([(
        index_name("things"),
        simple_schema("things", None),
    )]));
    let first = start_pipeline(pg, os, spec.clone(), &["things"]).await;
    first.await_seeded(1).await;
    let generation = first
        .alias_targets("things")
        .await
        .pop()
        .expect("the convenience alias points at the seeded generation");
    first.shutdown_and_release(pg).await;
    (spec, generation)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn restart_reseeds_when_the_generation_index_was_deleted() {
    // Issue #120, case A: the operator deletes the generation index between
    // runs. The seed marker still says "seeded"; the next run must not believe
    // it. Deleting the index is the "rebuild this" signal.
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    let (spec, generation) = seed_things_then_stop(&pg, &os, "(1, 'one'), (2, 'two')").await;

    // ── The operator deletes the generation index while flusso is stopped ────
    let status = reqwest::Client::new()
        .delete(format!("{}/{generation}", os.url))
        .send()
        .await
        .unwrap()
        .status();
    assert!(status.is_success(), "DELETE {generation} returned {status}");

    // ── Restart: the marker is contradicted by the index → reseed ────────────
    let second = spawn_pipeline(&pg, &os, spec).await;
    second
        .assert_eventually("things", "1", Some("one"))
        .await
        .unwrap_or_else(|got| panic!("[things] row 1 should be reseeded, saw {got}"));
    second
        .assert_eventually("things", "2", Some("two"))
        .await
        .unwrap_or_else(|got| panic!("[things] row 2 should be reseeded, saw {got}"));
    assert_eq!(
        second.alias_targets("things").await,
        vec![generation],
        "the recreated generation keeps its number and serves the alias"
    );
    second.shutdown();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn restart_rebuilds_when_the_replication_slot_was_dropped() {
    // Issue #120, case B: the source is replaced. Modelled as "the slot is gone
    // and the data changed while flusso was down" — a dropped slot is what a
    // dropdb/createdb or a fresh cluster leaves behind, since Postgres refuses
    // to drop a database a logical slot is bound to. The index still holds the
    // old rows; the restart must rebuild it from the *current* rows, which
    // means adding what's new and dropping what's gone.
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    let (spec, old_generation) = seed_things_then_stop(&pg, &os, "(1, 'one'), (3, 'three')").await;

    // ── While flusso is down: the slot vanishes and the data moves on ────────
    exec(&pg.pool, "SELECT pg_drop_replication_slot('flusso')").await;
    exec(&pg.pool, "DELETE FROM things WHERE id = 3").await;
    exec(&pg.pool, "INSERT INTO things (id, name) VALUES (4, 'four')").await;

    // ── Restart: no slot → the seed can't be trusted → rebuild ───────────────
    let second = spawn_pipeline(&pg, &os, spec).await;
    let new_generation = second
        .await_alias_moved_off("things", &old_generation)
        .await;
    assert_ne!(new_generation, old_generation);
    second
        .assert_eventually("things", "4", Some("four"))
        .await
        .unwrap_or_else(|got| panic!("[things] row 4 should be in the rebuilt index, saw {got}"));
    second
        .assert_eventually("things", "1", Some("one"))
        .await
        .unwrap_or_else(|got| panic!("[things] row 1 should survive the rebuild, saw {got}"));
    second
        .assert_eventually("things", "3", None)
        .await
        .unwrap_or_else(|got| {
            panic!("[things] row 3 no longer exists at the source and must be gone, saw {got}")
        });
    let old_index_exists = reqwest::Client::new()
        .head(format!("{}/{old_generation}", os.url))
        .send()
        .await
        .unwrap()
        .status()
        .is_success();
    assert!(
        !old_index_exists,
        "the superseded generation is dropped once the rebuilt one is swapped in"
    );

    // ── And live capture works on the new slot ───────────────────────────────
    exec(&pg.pool, "INSERT INTO things (id, name) VALUES (5, 'five')").await;
    second
        .assert_eventually("things", "5", Some("five"))
        .await
        .unwrap_or_else(|got| panic!("[things] live insert after the rebuild, saw {got}"));
    second.shutdown();
}

// ───────────────────── an id OpenSearch refuses (issue #301) ─────────────────────

/// The slot's `confirmed_flush_lsn` has reached `lsn`.
async fn slot_confirmed_past(pool: &PgPool, lsn: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT confirmed_flush_lsn >= $1::pg_lsn FROM pg_replication_slots \
         WHERE slot_name = 'flusso'",
    )
    .bind(lsn)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn current_lsn(pool: &PgPool) -> String {
    sqlx::query_scalar("SELECT pg_current_wal_lsn()::text")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A text key past OpenSearch's 512-byte `_id` limit is one rejected document,
/// decided by `on_error`: under `skip` it is quarantined, the rest of its
/// transaction lands, the batch is acked (the slot confirms past it) and the
/// engine keeps going; under `stop` the rest still lands, then the sink engine
/// stops with the batch unacked.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires docker"]
async fn an_id_past_512_bytes_is_decided_by_on_error() {
    let pg = start_postgres().await;
    let os = start_opensearch().await;
    for name in ["long_skip", "long_stop"] {
        create_table(
            &pg.pool,
            &format!("CREATE TABLE {name} (id text PRIMARY KEY, name text)"),
        )
        .await;
    }
    exec(
        &pg.pool,
        "CREATE PUBLICATION flusso FOR TABLE long_skip, long_stop",
    )
    .await;
    exec(
        &pg.pool,
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')",
    )
    .await;
    let spec = SourceSpec::new(BTreeMap::from([
        (index_name("long_skip"), simple_schema("long_skip", None)),
        (index_name("long_stop"), simple_schema("long_stop", None)),
    ]));
    let policies =
        FailurePolicies::new(FailurePolicy::Skip).with_override("long_stop", FailurePolicy::Stop);
    let live = spawn_pipeline_with(&pg, &os, spec, policies).await;
    live.await_seeded(2).await;
    let long = "k".repeat(600);

    exec(
        &pg.pool,
        &format!("INSERT INTO long_skip (id, name) VALUES ('{long}', 'long'), ('normal', 'kept')"),
    )
    .await;
    let skipped_at = current_lsn(&pg.pool).await;
    live.assert_eventually("long_skip", "normal", Some("kept"))
        .await
        .unwrap_or_else(|got| panic!("skip: the rest of the batch did not land, saw {got}"));
    let deadline = Instant::now() + Duration::from_secs(30);
    while !slot_confirmed_past(&pg.pool, &skipped_at).await {
        live.fail_if_stopped();
        assert!(Instant::now() < deadline, "skip: the batch was never acked");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    exec(
        &pg.pool,
        "INSERT INTO long_skip (id, name) VALUES ('after', 'next')",
    )
    .await;
    live.assert_eventually("long_skip", "after", Some("next"))
        .await
        .unwrap_or_else(|got| panic!("skip: the engine did not keep going, saw {got}"));

    exec(
        &pg.pool,
        &format!("INSERT INTO long_stop (id, name) VALUES ('{long}', 'long'), ('normal', 'kept')"),
    )
    .await;
    let stopped_at = current_lsn(&pg.pool).await;
    let deadline = Instant::now() + Duration::from_secs(60);
    let outcome = loop {
        if let Some(outcome) = live.outcome.lock().unwrap().take() {
            break outcome;
        }
        assert!(Instant::now() < deadline, "stop: the engine never stopped");
        tokio::time::sleep(Duration::from_millis(150)).await;
    };
    assert!(
        matches!(&outcome, Err(EngineError::DocumentsRejected(1, reason)) if reason.contains("512")),
        "stop: the sink engine stops on the one rejection, got {outcome:?}"
    );
    assert_eq!(
        live.fetch_name("long_stop", "normal").await.as_deref(),
        Some("kept"),
        "stop: the rest of the batch still landed",
    );
    assert!(
        !slot_confirmed_past(&pg.pool, &stopped_at).await,
        "stop: the batch stays unacked",
    );
    live.shutdown();
}

// ───────────────────────────────── harness ───────────────────────────────────

/// A running Postgres with logical replication and a connection pool.
struct Pg {
    _container: ContainerAsync<Postgres>,
    pool: PgPool,
    url: String,
    port: u16,
}

/// A running OpenSearch and its base URL.
struct Os {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

/// A live pipeline: the task running both engines plus a client for reading
/// the index back.
struct Pipeline {
    engine: tokio::task::JoinHandle<()>,
    /// How the run ended, once it has. Every poll below checks it so a run that
    /// stopped on an error fails the test immediately, with the error, instead
    /// of timing out on an assertion that can never become true.
    outcome: Arc<std::sync::Mutex<Option<engine::Result<()>>>>,
    http: reqwest::Client,
    os_url: String,
}

impl Pipeline {
    fn fail_if_stopped(&self) {
        if let Some(outcome) = self.outcome.lock().unwrap().as_ref() {
            panic!("the engine stopped: {outcome:?}");
        }
    }

    /// Wait until the engine has seeded `want` indexes — finished `ensure_index`
    /// + backfill and marked each seeded. Issuing a change before that races
    /// startup (the change predates the live stream), which is a harness
    /// artifact, not what these tests probe. Only markers with `seeded: true`
    /// count: `ensure_index` writes an *unseeded* marker up front, so a bare doc
    /// count would return while the backfill is still running (and a shutdown
    /// then could beat `mark_seeded`, leaving the index unseeded for the next run).
    async fn await_seeded(&self, want: usize) {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.fail_if_stopped();
            let _ = self
                .http
                .post(format!("{}/flusso_meta/_refresh", self.os_url))
                .send()
                .await;
            let count = self
                .http
                .post(format!("{}/flusso_meta/_count", self.os_url))
                .json(&serde_json::json!({ "query": { "term": { "seeded": true } } }))
                .send()
                .await
                .ok()
                .filter(|r| r.status().is_success());
            let count = match count {
                Some(r) => r
                    .json::<serde_json::Value>()
                    .await
                    .ok()
                    .and_then(|b| b.get("count").and_then(serde_json::Value::as_u64))
                    .unwrap_or(0),
                None => 0,
            };
            if count as usize >= want {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "engine did not seed {want} indexes in time (seeded markers = {count})"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Poll `{index}/_doc/{id}` until the document's `name` equals `expected`
    /// (or the document is absent, for `expected = None`). `Err(observed)` on a
    /// deadline, so the caller can report what it actually saw.
    async fn assert_eventually(
        &self,
        index: &str,
        id: &str,
        expected: Option<&str>,
    ) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.fail_if_stopped();
            let observed = match self.fetch_name(index, id).await {
                Some(name) => name,
                None => "absent".to_owned(),
            };
            let matches = match expected {
                Some(want) => observed == want,
                None => observed == "absent",
            };
            if matches {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(observed);
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Poll `{index}/_doc/{id}` until `project` of its `_source` equals
    /// `expected`; panics with the last observation at the deadline.
    async fn assert_doc_eventually(
        &self,
        index: &str,
        id: &str,
        project: &dyn Fn(&serde_json::Value) -> Vec<String>,
        expected: &[&str],
    ) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            self.fail_if_stopped();
            let observed = self.fetch_source(index, id).await.map(|doc| project(&doc));
            if observed.as_deref().is_some_and(|got| got == expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "[{index}/{id}] want {expected:?}, saw {observed:?}"
            );
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }

    /// Realtime `GET {index}/_doc/{id}`'s `_version`, which every write bumps.
    async fn fetch_version(&self, index: &str, id: &str) -> Option<u64> {
        let resp = self
            .http
            .get(format!("{}/{index}/_doc/{id}", self.os_url))
            .send()
            .await
            .ok()?;
        let body: serde_json::Value = resp.json().await.ok()?;
        body.get("_version").and_then(serde_json::Value::as_u64)
    }

    /// Realtime `GET {index}/_doc/{id}`'s `_source`, `None` if absent.
    async fn fetch_source(&self, index: &str, id: &str) -> Option<serde_json::Value> {
        let resp = self
            .http
            .get(format!("{}/{index}/_doc/{id}", self.os_url))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        if body.get("found").and_then(serde_json::Value::as_bool) != Some(true) {
            return None;
        }
        body.get("_source").cloned()
    }

    /// Realtime `GET {index}/_doc/{id}` (reads the translog — no refresh wait).
    /// `Some(name)` if the document exists, `None` if it does not.
    async fn fetch_name(&self, index: &str, id: &str) -> Option<String> {
        let resp = self
            .http
            .get(format!("{}/{index}/_doc/{id}", self.os_url))
            .send()
            .await
            .ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let body: serde_json::Value = resp.json().await.ok()?;
        if body.get("found").and_then(serde_json::Value::as_bool) != Some(true) {
            return None;
        }
        Some(
            body.get("_source")
                .and_then(|s| s.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("<missing>")
                .to_owned(),
        )
    }

    /// The concrete indexes the alias `alias` currently points at.
    async fn alias_targets(&self, alias: &str) -> Vec<String> {
        let resp = self
            .http
            .get(format!("{}/_alias/{alias}", self.os_url))
            .send()
            .await
            .unwrap();
        if !resp.status().is_success() {
            return Vec::new();
        }
        let body: serde_json::Value = resp.json().await.unwrap();
        body.as_object()
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Poll until the convenience alias `alias` points at something other than
    /// `not` — the moment a rebuilt generation is swapped in.
    async fn await_alias_moved_off(&self, alias: &str, not: &str) -> String {
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            self.fail_if_stopped();
            let targets = self.alias_targets(alias).await;
            if let Some(target) = targets.iter().find(|t| t.as_str() != not) {
                return target.clone();
            }
            if Instant::now() >= deadline {
                // What OpenSearch holds at the deadline — the index inventory,
                // aliases, and seed markers — is the fastest way to tell *which*
                // step of a rebuild never happened.
                let mut state = String::new();
                for path in [
                    "_cat/indices?v",
                    "_cat/aliases?v",
                    "flusso_meta/_search?pretty",
                ] {
                    let body = self
                        .http
                        .get(format!("{}/{path}", self.os_url))
                        .send()
                        .await
                        .unwrap()
                        .text()
                        .await
                        .unwrap();
                    state.push_str(&format!("\n--- {path}\n{body}"));
                }
                panic!("the alias {alias} never moved off {not} (still {targets:?}){state}");
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Stop the engine and wait for its replication connection to be gone, so
    /// the slot can be dropped or the next run can attach to it.
    async fn shutdown_and_release(self, pg: &Pg) {
        self.engine.abort();
        let _ = self.engine.await;
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let active: Option<i32> = sqlx::query_scalar(
                "SELECT active_pid FROM pg_replication_slots WHERE slot_name = 'flusso'",
            )
            .fetch_optional(&pg.pool)
            .await
            .unwrap()
            .flatten();
            match active {
                None => return,
                Some(pid) => {
                    let _ = sqlx::query("SELECT pg_terminate_backend($1)")
                        .bind(pid)
                        .execute(&pg.pool)
                        .await;
                }
            }
            assert!(
                Instant::now() < deadline,
                "the replication connection never released the slot"
            );
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    fn shutdown(self) {
        self.engine.abort();
    }
}

/// Bring up the publication + slot over `tables`, then the real engine driving
/// the real OpenSearch sink for `spec` (see [`spawn_pipeline`]).
async fn start_pipeline(pg: &Pg, os: &Os, spec: SourceSpec, tables: &[&str]) -> Pipeline {
    let quoted = tables
        .iter()
        .map(|t| format!("\"{t}\""))
        .collect::<Vec<_>>()
        .join(", ");
    exec(
        &pg.pool,
        &format!("CREATE PUBLICATION flusso FOR TABLE {quoted}"),
    )
    .await;
    exec(
        &pg.pool,
        "SELECT pg_create_logical_replication_slot('flusso', 'pgoutput')",
    )
    .await;
    spawn_pipeline(pg, os, spec).await
}

/// A (re)start against an already-provisioned Postgres: builds a fresh source,
/// document builder, and sink — as a new `flusso run` does — and runs both
/// engines. The publication and slot are whatever the database has.
async fn spawn_pipeline(pg: &Pg, os: &Os, spec: SourceSpec) -> Pipeline {
    spawn_pipeline_with(pg, os, spec, FailurePolicies::default()).await
}

/// [`spawn_pipeline`] with the sink engine's `on_error` policies set.
async fn spawn_pipeline_with(
    pg: &Pg,
    os: &Os,
    spec: SourceSpec,
    policies: FailurePolicies,
) -> Pipeline {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_test_writer()
        .try_init();
    let pre_image_columns = spec.pre_images();
    let documents = Arc::new(
        PgDocumentBuilder::connect(&pg.url, Arc::new(spec))
            .await
            .unwrap(),
    );
    let sink: Arc<dyn Sink> = Arc::new(opensearch_sink(&os.url));
    let replication = ReplicationConfig::new(
        "127.0.0.1",
        "postgres",
        "postgres",
        "postgres",
        "flusso",
        "flusso",
    )
    .with_port(pg.port);
    let capture: Arc<dyn ChangeCapture> = Arc::new(
        WalChangeCapture::new(replication, pg.url.clone())
            .with_pre_image_management(pre_image_columns, true),
    );
    let outcome = Arc::new(std::sync::Mutex::new(None));
    let record = Arc::clone(&outcome);
    Pipeline {
        engine: tokio::spawn(async move {
            let result = run_deployment(capture, documents, sink, policies).await;
            *record.lock().unwrap() = Some(result);
        }),
        outcome,
        http: reqwest::Client::new(),
        os_url: os.url.clone(),
    }
}

/// One deployment the way the daemon runs it: stage the sink engine, then run
/// it and the ingest engine together until either stops. `Ok` when the live
/// stream ends; the first engine error otherwise.
async fn run_deployment(
    capture: Arc<dyn ChangeCapture>,
    documents: Arc<dyn DocumentBuilder>,
    sink: Arc<dyn Sink>,
    policies: FailurePolicies,
) -> engine::Result<()> {
    let name = SinkName::try_new("e2e").unwrap();
    let stream: Arc<dyn Stream> = Arc::new(ChannelStream::new(1024, [name.clone()]));
    let continuity = capture.continuity().await?;
    let mappings = documents.index_mappings().await?;
    let sink_engine = SinkEngine::new(name.clone(), sink, Arc::clone(&stream), mappings)
        .with_failure_policies(policies);
    sink_engine.stage(continuity).await?;
    let ingest = IngestEngine::new(capture, documents, stream, vec![name]);
    let (_control, mut control_rx) = tokio::sync::mpsc::channel::<SinkControl>(1);
    tokio::select! {
        result = ingest.run() => result,
        result = sink_engine.run(&mut control_rx) => result,
    }
}

async fn start_postgres() -> Pg {
    let container = Postgres::default()
        .with_tag("16-alpine")
        .with_cmd([
            "postgres",
            "-c",
            "wal_level=logical",
            "-c",
            "max_wal_senders=10",
            "-c",
            "max_replication_slots=10",
        ])
        .start()
        .await
        .unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    Pg {
        _container: container,
        pool,
        url,
        port,
    }
}

async fn start_opensearch() -> Os {
    let container = GenericImage::new("opensearchproject/opensearch", "2")
        .with_exposed_port(9200.tcp())
        .with_wait_for(WaitFor::http(
            HttpWaitStrategy::new("/_cluster/health")
                .with_port(9200.tcp())
                .with_expected_status_code(200u16)
                .with_poll_interval(Duration::from_secs(1)),
        ))
        .with_env_var("discovery.type", "single-node")
        .with_env_var("DISABLE_SECURITY_PLUGIN", "true")
        .with_env_var("DISABLE_INSTALL_DEMO_CONFIG", "true")
        .with_env_var("OPENSEARCH_JAVA_OPTS", "-Xms512m -Xmx512m")
        .with_startup_timeout(Duration::from_secs(180))
        .start()
        .await
        .expect("opensearch container should start");
    let port = container.get_host_port_ipv4(9200).await.unwrap();
    Os {
        _container: container,
        url: format!("http://127.0.0.1:{port}"),
    }
}

async fn create_table(pool: &PgPool, ddl: &str) {
    exec(pool, ddl).await;
}

async fn exec(pool: &PgPool, sql: &str) {
    sqlx::query(AssertSqlSafe(sql.to_owned()))
        .execute(pool)
        .await
        .unwrap();
}

fn opensearch_sink(os_url: &str) -> OpensearchSink {
    let config = sink_opensearch::OpensearchConfig {
        url: Secret::Value(os_url.to_owned()),
        username: None,
        password: None,
        tls_verify: false,
        ca_file: None,
        batch_size: 1000,
        max_bytes: 10 * 1024 * 1024,
        timeout_secs: 30,
        max_retries: 3,
        pipeline: None,
        number_of_shards: 1,
        number_of_replicas: 1,
        refresh_interval: "1s".to_owned(),
        text_analysis: sink_opensearch::TextAnalysis::Builtin,
        auto_subfields: true,
        index_settings: Default::default(),
    };
    OpensearchSink::from_config(&SinkName::try_new("e2e").unwrap(), &config).unwrap()
}

/// A root table with an `id` key, a `name` column, and an optional soft-delete
/// marker. (The marker column is read raw, so it is not a mapped field.)
fn simple_schema(table_name: &str, soft_delete: Option<SoftDelete>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table(table_name),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete,
        filters: None,
        fields: vec![column_field("id", "id"), column_field("name", "name")],
    }
}

/// A join field `name` over `table` (keyed by `id`), projecting `fields`.
fn join_field(name: &str, table_name: &str, kind: JoinKind, fields: Vec<Field>) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: table(table_name),
            kind,
            primary_key: column("id"),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields,
            indexed: true,
            on: vec![],
        })),
    }
}

fn soft_delete_on(col: &str) -> SoftDelete {
    SoftDelete::Column(SoftDeleteColumn {
        column: column(col),
        when: None,
    })
}

fn column_field(name: &str, col: &str) -> Field {
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column(col),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

/// A `binary` field `name` over the column of the same name.
fn binary_field(name: &str, array: bool) -> Field {
    typed_field(name, FlussoType::Binary, array)
}

/// A field `name` of type `ty` over the column of the same name.
fn typed_field(name: &str, ty: FlussoType, array: bool) -> Field {
    let mut field = column_field(name, name);
    if let FieldSource::Column(column) = &mut field.source {
        column.ty = ty;
        column.array = array;
    }
    field
}

fn field(name: &str) -> FieldName {
    FieldName::try_new(name).unwrap()
}
fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}
fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}
fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}
