//! Config-coverage e2e: every shape a document schema can take, exercised
//! against a real Postgres so the *server-side* document SQL actually runs.
//!
//! Where the existing `integration.rs` covers one one-to-many join plus a
//! boolean soft delete, this file fans out across the rest of the config
//! surface — every join arity (one-to-one, one-to-many, many-to-many through a
//! junction, and joins nested inside joins), every aggregate op (count, sum,
//! avg, min, max, and a count through a junction), every filter operator,
//! transforms, defaults, both soft-delete forms (column and field, with and
//! without a `when`), column-type decoding, and reverse resolution across
//! direct / through / multi-hop relations.
//!
//! Requires Docker. Ignored by default; run with:
//!
//! ```text
//! cargo test -p sources-postgres --test config_coverage -- --ignored
//! ```

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;
use std::sync::Arc;

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, ColumnPair, DatabaseSchema,
    Direction, Field, FieldName, FieldSource, Filter, FilterOp, FilterValue, FlussoType,
    GenericValue, IndexName, IndexSchema, Join, JoinKind, NullCheckFilter, NullOp, OrderBy,
    RawFilter, RawFilterValue, Relation, SoftDelete, SoftDeleteColumn, SoftDeleteField, TableName,
    Through, Transform, ValueOpFilter, ViaHop,
};
use source::document::{Document, DocumentBuilder, DocumentId};
use source::{Anchor, DiagnosticKind, RowKey, SourceSpec, validate_indexes};
use source_postgres::PgDocumentBuilder;
use sqlx::postgres::PgPoolOptions;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::ContainerAsync;
use testcontainers_modules::testcontainers::runners::AsyncRunner;

// ---------------------------------------------------------------------------
// The shared world: one schema and dataset every test draws on.
// ---------------------------------------------------------------------------

/// DDL + seed data. A small e-commerce shape: users with profiles (1:1),
/// orders (1:N) with items (1:N nested), and tags via a junction (M:N).
const SEED: &[&str] = &[
    "CREATE TABLE users (
        id int PRIMARY KEY,
        name text,
        email text,
        status text NOT NULL,
        bio text,
        archived boolean NOT NULL DEFAULT false,
        deleted_at timestamptz,
        joined date,
        uid uuid,
        balance numeric,
        last_seen timestamptz
    )",
    "CREATE TABLE profiles (id int PRIMARY KEY, user_id int NOT NULL, headline text)",
    "CREATE TABLE orders (
        id int PRIMARY KEY,
        user_id int NOT NULL,
        total numeric NOT NULL,
        status text NOT NULL,
        placed_at timestamptz NOT NULL
    )",
    "CREATE TABLE order_items (
        id int PRIMARY KEY,
        order_id int NOT NULL,
        sku text NOT NULL,
        qty int NOT NULL,
        price numeric NOT NULL
    )",
    "CREATE TABLE tags (id int PRIMARY KEY, label text NOT NULL)",
    "CREATE TABLE user_tags (user_id int NOT NULL, tag_id int NOT NULL, PRIMARY KEY (user_id, tag_id))",
    // users: 1 active (full data), 2 banned+archived, 3 active+archived, 4 deleted.
    "INSERT INTO users (id, name, email, status, bio, archived, deleted_at, joined, uid, balance, last_seen) VALUES
        (1, 'Ada Lovelace', '  ADA@X.IO  ', 'active', 'Math pioneer', false, NULL, '2020-01-01', '11111111-1111-1111-1111-111111111111', 42.50, '2021-06-01T12:00:00Z'),
        (2, 'Alan Turing', 'alan@x.io', 'banned', NULL, true, NULL, '2020-02-02', NULL, NULL, NULL),
        (3, 'Grace Hopper', 'grace@x.io', 'active', 'Compiler', true, NULL, NULL, NULL, NULL, NULL),
        (4, 'Katherine', 'kat@x.io', 'active', NULL, false, '2020-03-03T00:00:00Z', NULL, NULL, NULL, NULL)",
    "INSERT INTO profiles (id, user_id, headline) VALUES (100, 1, 'Countess of Lovelace')",
    // user 1 has 3 orders (two fulfilled, one pending); user 2 has 1.
    "INSERT INTO orders (id, user_id, total, status, placed_at) VALUES
        (10, 1, 19.99, 'fulfilled', '2021-01-01T00:00:00Z'),
        (11, 1, 5.00, 'pending', '2021-02-01T00:00:00Z'),
        (12, 1, 100.00, 'fulfilled', '2021-03-01T00:00:00Z'),
        (20, 2, 50.00, 'fulfilled', '2021-01-15T00:00:00Z')",
    "INSERT INTO order_items (id, order_id, sku, qty, price) VALUES
        (1000, 12, 'sku-a', 2, 9.99),
        (1001, 12, 'sku-b', 1, 0.01),
        (1002, 11, 'sku-c', 5, 1.00)",
    "INSERT INTO tags (id, label) VALUES (1, 'red'), (2, 'green'), (3, 'blue')",
    "INSERT INTO user_tags (user_id, tag_id) VALUES (1, 1), (1, 2), (1, 3), (2, 1)",
];

/// Start a Postgres container, seed it, and return the live container (kept
/// alive by the caller) plus its connection URL.
async fn start_seeded() -> (ContainerAsync<Postgres>, String) {
    let container = Postgres::default().start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in SEED {
        sqlx::query(*statement).execute(&pool).await.unwrap();
    }
    (container, url)
}

async fn builder(url: &str, schema: IndexSchema) -> PgDocumentBuilder {
    PgDocumentBuilder::connect(url, Arc::new(spec(schema)))
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// Joins: one-to-one, one-to-many (ordered + limited), many-to-many through a
// junction, and a join nested inside a join.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn joins_assemble_every_arity_including_nested_and_through() {
    let (_pg, url) = start_seeded().await;

    let profile = join_field(
        "profile",
        Join {
            table: table("profiles"),
            primary_key: column("id"),
            nullable: false,
            kind: JoinKind::HasOne {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col("headline", "headline")],
            indexed: true,
            on: vec![],
        },
    );
    // One-to-many, newest first, capped at 2; each order folds in its items.
    let orders = join_field(
        "orders",
        Join {
            table: table("orders"),
            primary_key: column("id"),
            nullable: false,
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: Some(vec![OrderBy {
                column: column("placed_at"),
                direction: Some(Direction::Desc),
            }]),
            limit: Some(2),
            fields: vec![
                col("id", "id"),
                col("status", "status"),
                join_field(
                    "items",
                    Join {
                        table: table("order_items"),
                        primary_key: column("id"),
                        nullable: false,
                        kind: JoinKind::HasMany {
                            foreign_key: column("order_id"),
                        },
                        filters: None,
                        order_by: Some(vec![OrderBy {
                            column: column("sku"),
                            direction: Some(Direction::Asc),
                        }]),
                        limit: None,
                        fields: vec![col("sku", "sku"), col("qty", "qty")],
                        indexed: true,
                        on: vec![],
                    },
                ),
            ],
            indexed: true,
            on: vec![],
        },
    );
    // Many-to-many through the user_tags junction, labels A→Z.
    let tags = join_field(
        "tags",
        Join {
            table: table("tags"),
            primary_key: column("id"),
            nullable: false,
            kind: JoinKind::ManyToMany {
                through: Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                    filters: None,
                },
            },
            filters: None,
            order_by: Some(vec![OrderBy {
                column: column("label"),
                direction: Some(Direction::Asc),
            }]),
            limit: None,
            fields: vec![col("label", "label")],
            indexed: true,
            on: vec![],
        },
    );

    let builder = builder(
        &url,
        users_schema(vec![col("id", "id"), profile, orders, tags], None),
    )
    .await;
    let body = upsert(&builder, 1).await;

    // One-to-one: a single nested object, not an array.
    let GenericValue::Map(profile) = body.get("profile").unwrap() else {
        panic!("profile should be a nested object");
    };
    assert_eq!(str_of(profile, "headline"), "Countess of Lovelace");

    // One-to-many: limited to 2 and ordered newest-first → orders 12 then 11.
    let orders = arr_of(&body, "orders");
    assert_eq!(orders.len(), 2, "limit caps the one-to-many at two rows");
    let GenericValue::Map(first) = orders.first().unwrap() else {
        panic!("order should be an object");
    };
    assert_eq!(
        int_of(first.get("id").unwrap()),
        12,
        "DESC by placed_at → 12 first"
    );

    // Nested join: order 12's items, ordered by sku.
    let items = arr_of(first, "items");
    assert_eq!(items.len(), 2);
    let GenericValue::Map(item) = items.first().unwrap() else {
        panic!("item should be an object");
    };
    assert_eq!(str_of(item, "sku"), "sku-a");

    // Many-to-many through the junction: all three tags, sorted A→Z.
    let labels: Vec<&str> = arr_of(&body, "tags")
        .iter()
        .map(|t| {
            let GenericValue::Map(m) = t else {
                panic!("tag object")
            };
            str_of(m, "label")
        })
        .collect();
    assert_eq!(labels, vec!["blue", "green", "red"]);
}

// ---------------------------------------------------------------------------
// Aggregates: count, sum, avg, min, max (direct), a filtered count, and a
// count through a junction.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn aggregates_cover_every_op_and_through() {
    let (_pg, url) = start_seeded().await;

    let fields = vec![
        col("id", "id"),
        agg_field("order_count", orders_agg(AggregateOp::Count, None)),
        agg_field(
            "total_spent",
            orders_agg(AggregateOp::Sum(column("total").into()), None),
        ),
        agg_field(
            "avg_order",
            orders_agg(AggregateOp::Avg(column("total").into()), None),
        ),
        agg_field(
            "min_order",
            orders_agg(AggregateOp::Min(column("total").into()), None),
        ),
        agg_field(
            "max_order",
            orders_agg(AggregateOp::Max(column("total").into()), None),
        ),
        // count restricted by a filter on the related rows.
        agg_field(
            "fulfilled_orders",
            orders_agg(AggregateOp::Count, Some(vec![eq("status", "fulfilled")])),
        ),
        // count across the many-to-many junction.
        agg_field(
            "tag_count",
            Aggregate {
                table: table("tags"),
                op: AggregateOp::Count,
                key: AggregateKey::Through(Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                    filters: None,
                }),
                value_type: None,
                filters: None,
                distinct: false,
                via: Vec::new(),
                on: vec![],
            },
        ),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;
    let body = upsert(&builder, 1).await;

    // user 1: orders 19.99 + 5.00 + 100.00.
    assert_eq!(int_of(body.get("order_count").unwrap()), 3);
    assert!((num_of(body.get("total_spent").unwrap()) - 124.99).abs() < 1e-6);
    assert!((num_of(body.get("avg_order").unwrap()) - (124.99 / 3.0)).abs() < 1e-4);
    assert!((num_of(body.get("min_order").unwrap()) - 5.00).abs() < 1e-6);
    assert!((num_of(body.get("max_order").unwrap()) - 100.00).abs() < 1e-6);
    assert_eq!(int_of(body.get("fulfilled_orders").unwrap()), 2);
    assert_eq!(int_of(body.get("tag_count").unwrap()), 3);
}

/// A junction that is a real entity: `purchases` has its own key, can hold
/// the same product twice, carries a `status`, and one row points at a product
/// that doesn't exist.
const PURCHASES: &[&str] = &[
    "CREATE TABLE products (id int PRIMARY KEY, weight int NOT NULL, archived boolean NOT NULL)",
    "CREATE TABLE purchases (id int PRIMARY KEY, user_id int NOT NULL, product_id int NOT NULL, status text NOT NULL)",
    "INSERT INTO products (id, weight, archived) VALUES (1, 2, false), (2, 3, false), (3, 7, true)",
    "INSERT INTO purchases (id, user_id, product_id, status) VALUES
        (1, 1, 1, 'ok'), (2, 1, 1, 'ok'), (3, 1, 2, 'cancelled'), (4, 1, 3, 'ok'), (5, 1, 99, 'ok')",
];

fn purchases_agg(
    op: AggregateOp,
    distinct: bool,
    junction_filters: Option<Vec<Filter>>,
    filters: Option<Vec<Filter>>,
) -> Aggregate {
    Aggregate {
        table: table("products"),
        op,
        key: AggregateKey::Through(Through {
            table: table("purchases"),
            left_key: column("user_id"),
            right_key: column("product_id"),
            filters: junction_filters,
        }),
        value_type: Some(FlussoType::Long),
        filters,
        distinct,
        via: Vec::new(),
        on: vec![],
    }
}

fn ids_of(map: &BTreeMap<String, GenericValue>, key: &str) -> Vec<i64> {
    let mut ids: Vec<i64> = arr_of(map, key).iter().map(int_of).collect();
    ids.sort_unstable();
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn aggregates_over_a_junction_entity_honour_distinct_and_both_filter_sets() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in PURCHASES {
        sqlx::query(*statement).execute(&pool).await.unwrap();
    }

    let weight = || AggregateOp::Sum(column("weight").into());
    let ids = || AggregateOp::Ids {
        element_type: FlussoType::Long,
    };
    let not_cancelled = || Some(vec![value_op("status", FilterOp::Neq, single("cancelled"))]);
    let active = || Some(vec![eq("archived", "false")]);
    let fields = vec![
        col("id", "id"),
        agg_field("line_weight", purchases_agg(weight(), false, None, None)),
        agg_field("distinct_weight", purchases_agg(weight(), true, None, None)),
        agg_field(
            "live_line_weight",
            purchases_agg(weight(), false, not_cancelled(), None),
        ),
        agg_field(
            "live_active_weight",
            purchases_agg(weight(), true, not_cancelled(), active()),
        ),
        agg_field(
            "distinct_count",
            purchases_agg(AggregateOp::Count, true, None, None),
        ),
        agg_field("product_ids", purchases_agg(ids(), false, None, None)),
        agg_field(
            "distinct_product_ids",
            purchases_agg(ids(), true, None, None),
        ),
        agg_field(
            "active_product_ids",
            purchases_agg(ids(), true, None, active()),
        ),
    ];
    let builder = builder(&url, users_schema(fields, None)).await;
    let body = upsert(&builder, 1).await;

    // Purchases 1–4 reach products 1, 1, 2, 3; purchase 5 targets a missing
    // product and never counts.
    assert_eq!(int_of(body.get("line_weight").unwrap()), 2 + 2 + 3 + 7);
    assert_eq!(int_of(body.get("distinct_weight").unwrap()), 2 + 3 + 7);
    assert_eq!(int_of(body.get("live_line_weight").unwrap()), 2 + 2 + 7);
    assert_eq!(int_of(body.get("live_active_weight").unwrap()), 2);
    assert_eq!(int_of(body.get("distinct_count").unwrap()), 3);
    assert_eq!(ids_of(&body, "product_ids"), vec![1, 1, 2, 3]);
    assert_eq!(ids_of(&body, "distinct_product_ids"), vec![1, 2, 3]);
    assert_eq!(ids_of(&body, "active_product_ids"), vec![1, 2]);

    // A junction row's own column changing resolves to its parent, and the
    // rebuilt document sees the junction filter flip.
    let parents = builder
        .resolve(&table("purchases"), &row_key("id", 3), None, None)
        .await
        .unwrap();
    assert_eq!(parents.iter().map(id_of).collect::<Vec<_>>(), vec![1]);
    sqlx::query("UPDATE purchases SET status = 'ok' WHERE id = 3")
        .execute(&pool)
        .await
        .unwrap();
    let body = upsert(&builder, 1).await;
    assert_eq!(int_of(body.get("live_line_weight").unwrap()), 2 + 2 + 3 + 7);
}

// ---------------------------------------------------------------------------
// Filters: every operator, exercised as a count over the related orders.
//
// All comparisons run against the `status` *text* column. Filter values are
// bound as text, so they only line up with text columns — see the companion
// limitation test below.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn filters_cover_every_operator() {
    let (_pg, url) = start_seeded().await;

    // user 1's orders have statuses: fulfilled, pending, fulfilled.
    let fields = vec![
        col("id", "id"),
        count_where(
            "eq",
            vec![value_op("status", FilterOp::Eq, single("fulfilled"))],
        ),
        count_where(
            "neq",
            vec![value_op("status", FilterOp::Neq, single("fulfilled"))],
        ),
        count_where("lt", vec![value_op("status", FilterOp::Lt, single("m"))]),
        count_where(
            "lte",
            vec![value_op("status", FilterOp::Lte, single("fulfilled"))],
        ),
        count_where("gt", vec![value_op("status", FilterOp::Gt, single("m"))]),
        count_where(
            "gte",
            vec![value_op("status", FilterOp::Gte, single("pending"))],
        ),
        count_where(
            "like",
            vec![value_op("status", FilterOp::Like, single("ful%"))],
        ),
        count_where(
            "ilike",
            vec![value_op("status", FilterOp::Ilike, single("FUL%"))],
        ),
        count_where(
            "in_list",
            vec![value_op(
                "status",
                FilterOp::In,
                FilterValue::List(vec!["pending".into(), "fulfilled".into()]),
            )],
        ),
        count_where(
            "not_in",
            vec![value_op(
                "status",
                FilterOp::NotIn,
                FilterValue::List(vec!["pending".into()]),
            )],
        ),
        count_where(
            "between",
            vec![value_op(
                "status",
                FilterOp::Between,
                FilterValue::Range("a".into(), "g".into()),
            )],
        ),
        count_where("is_not_null", vec![null_check("status", NullOp::IsNotNull)]),
        count_where("is_null", vec![null_check("status", NullOp::IsNull)]),
        count_where("raw", vec![raw("status = 'pending'")]),
        // two filters compose with AND: fulfilled AND placed in 2021-01.
        count_where(
            "combined",
            vec![eq("status", "fulfilled"), raw("placed_at < '2021-02-01'")],
        ),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;
    let body = upsert(&builder, 1).await;

    assert_eq!(int_of(body.get("eq").unwrap()), 2, "two fulfilled");
    assert_eq!(int_of(body.get("neq").unwrap()), 1, "one not-fulfilled");
    assert_eq!(int_of(body.get("lt").unwrap()), 2, "'fulfilled' < 'm'");
    assert_eq!(int_of(body.get("lte").unwrap()), 2);
    assert_eq!(int_of(body.get("gt").unwrap()), 1, "'pending' > 'm'");
    assert_eq!(int_of(body.get("gte").unwrap()), 1);
    assert_eq!(int_of(body.get("like").unwrap()), 2);
    assert_eq!(
        int_of(body.get("ilike").unwrap()),
        2,
        "case-insensitive matches both"
    );
    assert_eq!(int_of(body.get("in_list").unwrap()), 3);
    assert_eq!(int_of(body.get("not_in").unwrap()), 2);
    assert_eq!(
        int_of(body.get("between").unwrap()),
        2,
        "'fulfilled' within a..g"
    );
    assert_eq!(int_of(body.get("is_not_null").unwrap()), 3);
    assert_eq!(int_of(body.get("is_null").unwrap()), 0);
    assert_eq!(int_of(body.get("raw").unwrap()), 1);
    assert_eq!(
        int_of(body.get("combined").unwrap()),
        1,
        "only order 10 is both"
    );
}

/// Filter operands are cast to the filtered column's real SQL type, so
/// comparisons are *typed*: a `numeric` column compares numerically, a
/// `timestamptz` as an instant, an `int` as an integer — not lexicographically
/// as text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn filters_compare_by_the_columns_real_type() {
    let (_pg, url) = start_seeded().await;

    // user 1's orders: total 19.99 / 5.00 / 100.00, placed Jan / Feb / Mar 2021.
    let fields = vec![
        col("id", "id"),
        // numeric '>' compares numerically. Text coercion would read '19.99'
        // and '100.00' as starting with '1' < '9' and return 0; numeric → 2.
        count_where("over_9", vec![value_op("total", FilterOp::Gt, single("9"))]),
        // numeric BETWEEN, also numeric: only 19.99 is in [10, 50].
        count_where(
            "mid_range",
            vec![value_op(
                "total",
                FilterOp::Between,
                FilterValue::Range("10".into(), "50".into()),
            )],
        ),
        // numeric IN with a fractional literal matches exactly.
        count_where(
            "exact_totals",
            vec![value_op(
                "total",
                FilterOp::In,
                FilterValue::List(vec!["5.00".into(), "100.00".into()]),
            )],
        ),
        // timestamptz '>' compares as an instant.
        count_where(
            "after_mid_jan",
            vec![value_op(
                "placed_at",
                FilterOp::Gt,
                single("2021-01-15T00:00:00Z"),
            )],
        ),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;
    let body = upsert(&builder, 1).await;

    assert_eq!(
        int_of(body.get("over_9").unwrap()),
        2,
        "19.99 and 100.00 exceed 9"
    );
    assert_eq!(
        int_of(body.get("mid_range").unwrap()),
        1,
        "only 19.99 is in [10, 50]"
    );
    assert_eq!(
        int_of(body.get("exact_totals").unwrap()),
        2,
        "5.00 and 100.00 match"
    );
    assert_eq!(
        int_of(body.get("after_mid_jan").unwrap()),
        2,
        "Feb and Mar orders"
    );
}

// ---------------------------------------------------------------------------
// Transforms and defaults.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn transforms_and_defaults_apply() {
    let (_pg, url) = start_seeded().await;

    let email = Field {
        source: FieldSource::Column(Column {
            column: column("email"),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: vec![Transform::Trim, Transform::Lowercase],
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
        ..base("email")
    };
    let bio = Field {
        source: FieldSource::Column(Column {
            column: column("bio"),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: Some(GenericValue::String("(no bio)".into())),
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
        ..base("bio")
    };
    // A constant field with no database source renders the literal directly.
    let source = Field {
        source: FieldSource::Constant(GenericValue::String("seed".into())),
        ..base("source")
    };

    let builder = builder(
        &url,
        users_schema(vec![col("id", "id"), email, bio, source], None),
    )
    .await;

    // user 1: '  ADA@X.IO  ' trimmed + lowercased; bio present; literal default.
    let ada = upsert(&builder, 1).await;
    assert_eq!(str_of(&ada, "email"), "ada@x.io");
    assert_eq!(str_of(&ada, "bio"), "Math pioneer");
    assert_eq!(str_of(&ada, "source"), "seed");

    // user 2: bio is NULL → falls back to the default.
    let alan = upsert(&builder, 2).await;
    assert_eq!(str_of(&alan, "bio"), "(no bio)");
}

// ---------------------------------------------------------------------------
// Array columns (issue #276).
// ---------------------------------------------------------------------------

fn array_col(
    name: &str,
    ty: FlussoType,
    transforms: Vec<Transform>,
    default: Option<GenericValue>,
) -> Field {
    Field {
        source: FieldSource::Column(Column {
            column: column(name),
            ty,
            nullable: true,
            transforms,
            default,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: true,
        }),
        ..base(name)
    }
}

fn strings(items: &[&str]) -> GenericValue {
    GenericValue::Array(
        items
            .iter()
            .map(|item| GenericValue::String((*item).to_owned()))
            .collect(),
    )
}

/// A `text[]` and a `jsonb` array column: transforms per element, null
/// elements dropped, a lone value wrapped, the default list on a null column,
/// a structured element failing the document on both build paths; and the
/// checks against the catalog: the element type takes the transforms, a flag
/// over a plain column is an error, a `[]` column without one a warning.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn array_columns_clean_up_their_value_and_are_checked_against_the_catalog() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE DOMAIN tag_list AS text[]",
        "CREATE DOMAIN tag_doc AS jsonb",
        "ALTER TABLE users ADD COLUMN labels text[], ADD COLUMN prefs jsonb, \
         ADD COLUMN notes jsonb, ADD COLUMN codes int[], ADD COLUMN dom_tags tag_list, \
         ADD COLUMN dom_prefs tag_doc, ADD COLUMN dom_plain tag_list",
        "UPDATE users SET labels = ARRAY[' Red ', NULL, 'Blue'], prefs = '[\"X\", null]', \
         notes = '[{\"a\": 1}, null]', codes = '{1,2}', dom_tags = ARRAY[' Up ', NULL], \
         dom_prefs = '[\"Q\"]', dom_plain = ARRAY['p'] WHERE id = 1",
        "UPDATE users SET prefs = '\"Solo\"' WHERE id = 2",
        "UPDATE users SET labels = '{}', prefs = '[{\"bad\": 1}]' WHERE id = 3",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let integer = |name: &str| {
        let mut field = col(name, name);
        if let FieldSource::Column(column) = &mut field.source {
            column.ty = FlussoType::Integer;
        }
        field
    };
    let schema = users_schema(
        vec![
            integer("id"),
            array_col(
                "labels",
                FlussoType::Keyword,
                vec![Transform::Trim, Transform::Lowercase],
                Some(strings(&["none"])),
            ),
            array_col(
                "prefs",
                FlussoType::Keyword,
                vec![Transform::Lowercase],
                None,
            ),
            array_col("notes", FlussoType::Json, Vec::new(), None),
            integer("codes"),
            Field {
                source: FieldSource::Column(Column {
                    column: column("name"),
                    array: true,
                    ..match col("nick", "name").source {
                        FieldSource::Column(column) => column,
                        _ => unreachable!(),
                    }
                }),
                ..base("nick")
            },
            array_col(
                "dom_tags",
                FlussoType::Keyword,
                vec![Transform::Trim, Transform::Lowercase],
                None,
            ),
            array_col(
                "dom_prefs",
                FlussoType::Keyword,
                vec![Transform::Lowercase],
                None,
            ),
            col("dom_plain", "dom_plain"),
        ],
        None,
    );
    let spec = spec(schema.clone());
    let documents = builder(&url, schema).await;

    let found: Vec<_> = validate_indexes(&spec, &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| (d.dotted_path(), d.kind))
        .collect();
    assert_eq!(
        found,
        vec![
            ("codes".to_owned(), DiagnosticKind::UndeclaredArrayColumn),
            ("nick".to_owned(), DiagnosticKind::NotAnArrayColumn),
            // A domain is named by its own name, which no declared type
            // accepts; its base type decides that it holds an array, and its
            // element type (`text`) takes the transforms.
            ("dom_tags".to_owned(), DiagnosticKind::TypeMismatch),
            (
                "dom_plain".to_owned(),
                DiagnosticKind::UndeclaredArrayColumn
            ),
            ("dom_plain".to_owned(), DiagnosticKind::TypeMismatch),
        ]
    );

    let ada = upsert(&documents, 1).await;
    assert_eq!(ada["labels"], strings(&["red", "blue"]));
    assert_eq!(ada["prefs"], strings(&["x"]));
    assert_eq!(
        ada["notes"],
        GenericValue::Array(vec![GenericValue::Map(BTreeMap::from([(
            "a".to_owned(),
            GenericValue::BigInt(1)
        )]))])
    );
    assert_eq!(arr_of(&ada, "codes").len(), 2);
    assert_eq!(ada["nick"], strings(&["Ada Lovelace"]));
    assert_eq!(ada["dom_tags"], strings(&["up"]));
    assert_eq!(ada["dom_prefs"], strings(&["q"]));

    let alan = upsert(&documents, 2).await;
    assert_eq!(alan["labels"], strings(&["none"]));
    assert_eq!(alan["prefs"], strings(&["solo"]));
    assert_eq!(alan["notes"], GenericValue::Null);

    let failed = |document: &Document| match document {
        Document::Failed { failure, .. } => failure.field.clone(),
        other => panic!("expected a failed document, got {other:?}"),
    };
    assert_eq!(failed(&documents.build(&doc(3)).await.unwrap()), "prefs");
    let many = documents.build_many(&[doc(1), doc(3)]).await.unwrap();
    let by_id = |id: i64| many.iter().find(|d| id_of(d.id()) == id).unwrap();
    assert!(matches!(by_id(1), Document::Upsert { .. }));
    assert_eq!(failed(by_id(3)), "prefs");
}

// ---------------------------------------------------------------------------
// Soft delete: column form (boolean with a `when`, and a non-boolean marker)
// and field form.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn soft_delete_column_and_field_forms() {
    let (_pg, url) = start_seeded().await;

    // Column form, boolean marker, narrowed by `when`: deleted only when
    // archived AND status = 'banned'.
    let when_builder = builder(
        &url,
        users_schema(
            vec![col("id", "id")],
            Some(SoftDelete::Column(SoftDeleteColumn {
                column: column("archived"),
                when: Some(vec![eq("status", "banned")]),
            })),
        ),
    )
    .await;
    assert!(
        is_tombstone(&when_builder, 2).await,
        "archived + banned → deleted"
    );
    assert!(
        !is_tombstone(&when_builder, 3).await,
        "archived but active → kept"
    );
    assert!(!is_tombstone(&when_builder, 1).await, "not archived → kept");

    // Column form, non-boolean marker: any present timestamp means deleted
    // (exercises the pg_typeof branch).
    let ts_builder = builder(
        &url,
        users_schema(
            vec![col("id", "id")],
            Some(SoftDelete::Column(SoftDeleteColumn {
                column: column("deleted_at"),
                when: None,
            })),
        ),
    )
    .await;
    assert!(
        is_tombstone(&ts_builder, 4).await,
        "deleted_at set → deleted"
    );
    assert!(
        !is_tombstone(&ts_builder, 1).await,
        "deleted_at null → kept"
    );

    // Field form: the marker is named indirectly through a mapped field.
    let field_builder = builder(
        &url,
        users_schema(
            vec![col("id", "id"), col("is_archived", "archived")],
            Some(SoftDelete::Field(SoftDeleteField {
                field: field("is_archived"),
                when: None,
            })),
        ),
    )
    .await;
    assert!(
        is_tombstone(&field_builder, 2).await,
        "archived field truthy → deleted"
    );
    assert!(
        !is_tombstone(&field_builder, 1).await,
        "archived field false → kept"
    );
}

// ---------------------------------------------------------------------------
// Column-type decoding: the server assembles JSON, so each Postgres type must
// land as the right GenericValue in the document.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn column_types_decode_into_the_document() {
    let (_pg, url) = start_seeded().await;

    let fields = vec![
        col("id", "id"),
        col("name", "name"),
        col("uid", "uid"),
        col("joined", "joined"),
        col("archived", "archived"),
        col("balance", "balance"),
        col("last_seen", "last_seen"),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;
    let body = upsert(&builder, 1).await;

    assert_eq!(int_of(body.get("id").unwrap()), 1, "int → Int");
    assert_eq!(str_of(&body, "name"), "Ada Lovelace", "text → String");
    assert_eq!(
        str_of(&body, "uid"),
        "11111111-1111-1111-1111-111111111111",
        "uuid → String",
    );
    assert_eq!(str_of(&body, "joined"), "2020-01-01", "date → String");
    assert_eq!(
        body.get("archived").unwrap(),
        &GenericValue::Bool(false),
        "bool → Bool"
    );
    assert!(
        (num_of(body.get("balance").unwrap()) - 42.50).abs() < 1e-6,
        "numeric → Decimal"
    );
    assert!(
        str_of(&body, "last_seen").starts_with("2021-06-01"),
        "timestamptz → ISO String",
    );
}

// ---------------------------------------------------------------------------
// Reverse resolution: a change to a related row resolves back to the root
// document keys, across direct FK, through-junction (far and junction sides),
// and a multi-hop nested chain.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn reverse_resolution_walks_direct_through_and_nested() {
    let (_pg, url) = start_seeded().await;

    // A schema that relates to every table we want to resolve from.
    let fields = vec![
        col("id", "id"),
        join_field(
            "profile",
            Join {
                table: table("profiles"),
                primary_key: column("id"),
                nullable: false,
                kind: JoinKind::HasOne {
                    foreign_key: column("user_id"),
                },
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![col("headline", "headline")],
                indexed: true,
                on: vec![],
            },
        ),
        join_field(
            "orders",
            Join {
                table: table("orders"),
                primary_key: column("id"),
                nullable: false,
                kind: JoinKind::HasMany {
                    foreign_key: column("user_id"),
                },
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![
                    col("id", "id"),
                    join_field(
                        "items",
                        Join {
                            table: table("order_items"),
                            primary_key: column("id"),
                            nullable: false,
                            kind: JoinKind::HasMany {
                                foreign_key: column("order_id"),
                            },
                            filters: None,
                            order_by: None,
                            limit: None,
                            fields: vec![col("sku", "sku")],
                            indexed: true,
                            on: vec![],
                        },
                    ),
                ],
                indexed: true,
                on: vec![],
            },
        ),
        join_field(
            "tags",
            Join {
                table: table("tags"),
                primary_key: column("id"),
                nullable: false,
                kind: JoinKind::ManyToMany {
                    through: Through {
                        table: table("user_tags"),
                        left_key: column("user_id"),
                        right_key: column("tag_id"),
                        filters: None,
                    },
                },
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![col("label", "label")],
                indexed: true,
                on: vec![],
            },
        ),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;

    // Root table: the key is the document id.
    assert_eq!(
        builder
            .resolve(&table("users"), &key(1), None, None)
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // Direct FK: an order change resolves to its owning user.
    assert_eq!(
        builder
            .resolve(&table("orders"), &key(10), None, None)
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // One-to-one direct FK: a profile change resolves to its user.
    assert_eq!(
        builder
            .resolve(&table("profiles"), &row_key("id", 100), None, None)
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // Through, far side: tag 1 is held by users 1 and 2.
    let mut roots = builder
        .resolve(&table("tags"), &key(1), None, None)
        .await
        .unwrap();
    roots.sort_by_key(id_of);
    assert_eq!(roots, vec![doc(1), doc(2)]);

    // Through, junction side: a user_tags row carrying user_id resolves directly.
    assert_eq!(
        builder
            .resolve(&table("user_tags"), &row_key("user_id", 1), None, None)
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // Multi-hop: an item change walks order_items → orders → users.
    assert_eq!(
        builder
            .resolve(&table("order_items"), &key(1002), None, None)
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // A table the schema never references resolves to nothing.
    assert!(
        builder
            .resolve(&table("tags"), &key(999), None, None)
            .await
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// belongs_to: the parent row holds the key. Assembly correlates the target's
// primary key with the parent's own column; reverse resolution finds the
// referrers on the parent table itself — so a changed (or deleted) target
// re-emits every document pointing at it.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn belongs_to_assembles_and_reverse_resolves() {
    let (_pg, url) = start_seeded().await;

    // An order pointing at a user that does not exist — a dangling reference.
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query(
        "INSERT INTO orders (id, user_id, total, status, placed_at)
         VALUES (30, 999, 1.00, 'pending', '2021-04-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // Documents rooted at `orders`, each embedding the user its own `user_id`
    // points at. (The index keeps the helpers' name; only the root differs.)
    let schema = IndexSchema {
        version: 1,
        table: table("orders"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![
            col("id", "id"),
            join_field(
                "buyer",
                Join {
                    table: table("users"),
                    primary_key: column("id"),
                    nullable: false,
                    kind: JoinKind::BelongsTo {
                        column: column("user_id"),
                    },
                    filters: None,
                    order_by: None,
                    limit: None,
                    fields: vec![col("name", "name"), col("email", "email")],
                    indexed: true,
                    on: vec![],
                },
            ),
        ],
    };
    let builder = builder(&url, schema).await;

    // Assembly: the order's own `user_id` selects the embedded user.
    let body = upsert(&builder, 10).await;
    let GenericValue::Map(buyer) = body.get("buyer").unwrap() else {
        panic!("buyer should be a nested object");
    };
    assert_eq!(str_of(buyer, "name"), "Ada Lovelace");

    // A dangling pointer renders a null object, not an error.
    let body = upsert(&builder, 30).await;
    assert!(
        matches!(body.get("buyer"), Some(GenericValue::Null)),
        "missing target → null object, got {:?}",
        body.get("buyer"),
    );

    // Reverse: a user change re-emits every order pointing at it — found on
    // the parent table itself (`WHERE user_id = 1`).
    let mut roots = builder
        .resolve(&table("users"), &key(1), None, None)
        .await
        .unwrap();
    roots.sort_by_key(id_of);
    assert_eq!(roots, vec![doc(10), doc(11), doc(12)]);

    // Reverse, deleted/absent target: the referrers are alive and still
    // resolve — they rebuild with a null object instead of going stale.
    assert_eq!(
        builder
            .resolve(&table("users"), &key(999), None, None)
            .await
            .unwrap(),
        vec![doc(30)],
    );
}

// ---------------------------------------------------------------------------
// Root filters: only matching root rows become documents; a row leaving the
// set tombstones, exactly like soft-delete. Covers both the single-document
// and batched build paths.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn root_filters_scope_which_rows_become_documents() {
    let (_pg, url) = start_seeded().await;

    // Documents exist only for active, non-archived users.
    let mut schema = users_schema(vec![col("id", "id"), col("name", "name")], None);
    schema.filters = Some(vec![
        eq("status", "active"),
        value_op("archived", FilterOp::Eq, single("false")),
    ]);
    let builder = builder(&url, schema).await;

    // User 1 (active, not archived) is a document.
    let body = upsert(&builder, 1).await;
    assert_eq!(str_of(&body, "name"), "Ada Lovelace");

    // User 2 (banned) and user 3 (archived) are outside the set → tombstones.
    assert!(is_tombstone(&builder, 2).await);
    assert!(is_tombstone(&builder, 3).await);

    // The batched path agrees: requested keys outside the set come back as
    // tombstones, matching keys as upserts.
    let documents = builder.build_many(&[doc(1), doc(2), doc(3)]).await.unwrap();
    let upserts = documents
        .iter()
        .filter(|d| matches!(d, Document::Upsert { .. }))
        .count();
    assert_eq!((documents.len(), upserts), (3, 1));

    // A row that leaves the set on UPDATE tombstones on its next rebuild.
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    sqlx::query("UPDATE users SET status = 'banned' WHERE id = 1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(is_tombstone(&builder, 1).await);
}

// ---------------------------------------------------------------------------
// `flusso check`: every table and column a relation names is looked up.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn validation_reports_a_typo_in_every_relation_position() {
    let (_pg, url) = start_seeded().await;
    let pair = |local: &str, related: &str| ColumnPair {
        local: column(local),
        related: column(related),
    };
    let join = |table_name: &str, kind: JoinKind, fields: Vec<Field>| Join {
        table: table(table_name),
        primary_key: column("id"),
        nullable: true,
        kind,
        filters: None,
        order_by: None,
        limit: None,
        fields,
        indexed: true,
        on: vec![],
    };

    let profile = join_field(
        "profile",
        Join {
            primary_key: column("pk"),
            filters: Some(vec![eq("headlin", "x"), raw("whatever IS NULL")]),
            order_by: Some(vec![OrderBy {
                column: column("rank"),
                direction: None,
            }]),
            on: vec![pair("nam", "headlne")],
            ..join(
                "profiles",
                JoinKind::HasOne {
                    foreign_key: column("userid"),
                },
                vec![col("headline", "headline")],
            )
        },
    );
    let owner = join_field(
        "owner",
        join(
            "users",
            JoinKind::BelongsTo {
                column: column("owner_id"),
            },
            vec![],
        ),
    );
    let orders = join_field(
        "orders",
        join(
            "orders",
            JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            vec![join_field(
                "items",
                join(
                    "order_item",
                    JoinKind::HasMany {
                        foreign_key: column("order_id"),
                    },
                    vec![col("sku", "sku")],
                ),
            )],
        ),
    );
    let tags = join_field(
        "tags",
        join(
            "tags",
            JoinKind::ManyToMany {
                through: Through {
                    table: table("user_tags"),
                    left_key: column("userid"),
                    right_key: column("tagid"),
                    filters: Some(vec![null_check("tag", NullOp::IsNull)]),
                },
            },
            vec![col("label", "label")],
        ),
    );
    let missing_junction = join_field(
        "labels",
        join(
            "tags",
            JoinKind::ManyToMany {
                through: Through {
                    table: table("user_tag"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                    filters: None,
                },
            },
            vec![col("label", "label")],
        ),
    );
    let item_count = agg_field(
        "itemCount",
        Aggregate {
            table: table("order_items"),
            key: AggregateKey::Direct(column("orderid")),
            via: vec![ViaHop {
                table: table("orders"),
                foreign_key: column("userid"),
                filters: Some(vec![eq("state", "x")]),
                on: vec![pair("nme", "stat")],
            }],
            on: vec![pair("totl", "prce")],
            filters: Some(vec![eq("quantity", "1")]),
            ..orders_agg(AggregateOp::Count, None)
        },
    );
    let lost_hop = agg_field(
        "lostHop",
        Aggregate {
            table: table("order_items"),
            key: AggregateKey::Direct(column("order_id")),
            via: vec![ViaHop {
                table: table("order"),
                foreign_key: column("user_id"),
                filters: None,
                on: vec![],
            }],
            ..orders_agg(AggregateOp::Count, None)
        },
    );
    let lost_table = agg_field(
        "lostTable",
        Aggregate {
            table: table("ordrs"),
            ..orders_agg(AggregateOp::Count, None)
        },
    );

    let schema = users_schema(
        vec![
            col("name", "name"),
            profile,
            owner,
            orders,
            tags,
            missing_junction,
            item_count,
            lost_hop,
            lost_table,
        ],
        None,
    );
    let spec = spec(schema);
    let documents = builder(&url, spec.schema(&index_name("users")).unwrap().clone()).await;
    let found: Vec<(String, DiagnosticKind, String)> = validate_indexes(&spec, &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| {
            assert_eq!(d.severity, d.kind.severity(), "{d:?}");
            (d.dotted_path(), d.kind, d.message)
        })
        .collect();
    let expected: Vec<(String, DiagnosticKind, String)> = [
        (
            "profile",
            DiagnosticKind::UnknownColumn,
            "`foreign_key` references unknown column `public.profiles.userid`",
        ),
        (
            "profile",
            DiagnosticKind::UnknownColumn,
            "`on.local` references unknown column `public.users.nam`",
        ),
        (
            "profile",
            DiagnosticKind::UnknownColumn,
            "`on.related` references unknown column `public.profiles.headlne`",
        ),
        (
            "profile",
            DiagnosticKind::IgnoredPrimaryKey,
            "`primary_key` references unknown column `public.profiles.pk`; ignored: the join is keyed by the table's own primary key",
        ),
        (
            "profile",
            DiagnosticKind::UnknownColumn,
            "`filters` references unknown column `public.profiles.headlin`",
        ),
        (
            "profile",
            DiagnosticKind::UnknownColumn,
            "`order_by` references unknown column `public.profiles.rank`",
        ),
        (
            "owner",
            DiagnosticKind::UnknownColumn,
            "`column` references unknown column `public.users.owner_id`",
        ),
        (
            "orders.items",
            DiagnosticKind::UnknownTable,
            "`table` references unknown table `public.order_item`",
        ),
        (
            "tags",
            DiagnosticKind::UnknownColumn,
            "`through.left_key` references unknown column `public.user_tags.userid`",
        ),
        (
            "tags",
            DiagnosticKind::UnknownColumn,
            "`through.right_key` references unknown column `public.user_tags.tagid`",
        ),
        (
            "tags",
            DiagnosticKind::UnknownColumn,
            "`through.filters` references unknown column `public.user_tags.tag`",
        ),
        (
            "labels",
            DiagnosticKind::UnknownTable,
            "`through.table` references unknown table `public.user_tag`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`via.foreign_key` references unknown column `public.orders.userid`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`via.on.local` references unknown column `public.users.nme`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`via.on.related` references unknown column `public.orders.stat`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`via.filters` references unknown column `public.orders.state`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`foreign_key` references unknown column `public.order_items.orderid`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`on.local` references unknown column `public.orders.totl`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`on.related` references unknown column `public.order_items.prce`",
        ),
        (
            "itemCount",
            DiagnosticKind::UnknownColumn,
            "`filters` references unknown column `public.order_items.quantity`",
        ),
        (
            "lostHop",
            DiagnosticKind::UnknownTable,
            "`via.table` references unknown table `public.order`",
        ),
        (
            "lostTable",
            DiagnosticKind::UnknownTable,
            "`table` references unknown table `public.ordrs`",
        ),
    ]
    .iter()
    .map(|(path, kind, message)| ((*path).to_owned(), *kind, (*message).to_owned()))
    .collect();
    assert_eq!(found, expected);
}

/// Issue #211: the root's own keys, on the root table — `filters`, and the
/// `soft_delete` marker's `column`, `field` and `when`. Also pins why the
/// dangling `field` must be an error: the document query skips such a marker
/// without a word, so a soft-deleted row stays a document.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn validation_reports_a_typo_in_every_root_position() {
    let (_pg, url) = start_seeded().await;
    let problems = |schema: IndexSchema| {
        let url = url.clone();
        async move {
            let spec = spec(schema);
            let documents = builder(&url, spec.schema(&index_name("users")).unwrap().clone()).await;
            validate_indexes(&spec, &documents)
                .await
                .unwrap()
                .into_iter()
                .map(|d| {
                    assert_eq!(d.severity, d.kind.severity(), "{d:?}");
                    (d.dotted_path(), d.kind, d.message)
                })
                .collect::<Vec<_>>()
        }
    };
    let expected = |pairs: &[(&str, DiagnosticKind, &str)]| {
        pairs
            .iter()
            .map(|(path, kind, message)| ((*path).to_owned(), *kind, (*message).to_owned()))
            .collect::<Vec<_>>()
    };
    let fields = || vec![col("name", "name"), col("email", "email")];

    let mut filtered = users_schema(
        fields(),
        Some(SoftDelete::Column(SoftDeleteColumn {
            column: column("deleted"),
            when: Some(vec![
                eq("stats", "banned"),
                null_check("archivd", NullOp::IsNull),
                raw("whatever"),
            ]),
        })),
    );
    filtered.filters = Some(vec![eq("emial", "x"), null_check("bi", NullOp::IsNull)]);
    assert_eq!(
        problems(filtered).await,
        expected(&[
            (
                "filters",
                DiagnosticKind::UnknownColumn,
                "`filters` references unknown column `public.users.emial`"
            ),
            (
                "filters",
                DiagnosticKind::UnknownColumn,
                "`filters` references unknown column `public.users.bi`"
            ),
            (
                "soft_delete",
                DiagnosticKind::UnknownColumn,
                "`column` references unknown column `public.users.deleted`"
            ),
            (
                "soft_delete",
                DiagnosticKind::UnknownColumn,
                "`when` references unknown column `public.users.stats`"
            ),
            (
                "soft_delete",
                DiagnosticKind::UnknownColumn,
                "`when` references unknown column `public.users.archivd`"
            ),
        ]),
    );

    let dangling = users_schema(
        fields(),
        Some(SoftDelete::Field(SoftDeleteField {
            field: field("deleted"),
            when: None,
        })),
    );
    let silent = builder(&url, dangling.clone()).await;
    assert!(
        !is_tombstone(&silent, 4).await,
        "a marker that reads no column is skipped: the deleted row stays"
    );
    assert_eq!(
        problems(dangling).await,
        expected(&[(
            "soft_delete",
            DiagnosticKind::UnreadSoftDeleteMarker,
            "`field` names `deleted`, but no field has that name"
        )]),
    );

    let mut placed_at = col("placed_at", "placed_at");
    if let FieldSource::Column(column) = &mut placed_at.source {
        column.ty = FlussoType::Timestamp;
    }
    let orders = join_field(
        "orders",
        Join {
            table: table("orders"),
            primary_key: column("id"),
            nullable: true,
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![placed_at],
            indexed: true,
            on: vec![],
        },
    );
    let as_join = users_schema(
        vec![col("name", "name"), orders.clone()],
        Some(SoftDelete::Field(SoftDeleteField {
            field: field("orders"),
            when: None,
        })),
    );
    assert_eq!(
        problems(as_join).await,
        expected(&[(
            "soft_delete",
            DiagnosticKind::UnreadSoftDeleteMarker,
            "`field` names `orders`, but the first field of that name is not a column"
        )]),
    );
    let in_join = users_schema(
        vec![col("name", "name"), orders],
        Some(SoftDelete::Field(SoftDeleteField {
            field: field("placed_at"),
            when: None,
        })),
    );
    assert_eq!(
        problems(in_join).await,
        expected(&[(
            "soft_delete",
            DiagnosticKind::UnknownColumn,
            "`field` references unknown column `public.users.placed_at`"
        )]),
    );
}

/// A Postgres enum column and a domain column are reported by their own type
/// names, which no declared type accepts: a `TypeMismatch` for `flusso check`.
/// The document still builds with their values, which is why that kind does
/// not stop `flusso run`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn an_enum_or_domain_column_is_a_type_mismatch_that_still_builds() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TYPE mood AS ENUM ('calm', 'busy')",
        "CREATE DOMAIN email_address AS text CHECK (VALUE LIKE '%@%')",
        "ALTER TABLE users ADD COLUMN mood mood DEFAULT 'calm', \
         ADD COLUMN contact email_address DEFAULT 'ada@example.com'",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let mut mood = col("mood", "mood");
    if let FieldSource::Column(column) = &mut mood.source {
        column.ty = FlussoType::Enum;
    }
    let schema = users_schema(vec![mood, col("contact", "contact")], None);
    let spec = spec(schema.clone());
    let documents = builder(&url, schema).await;

    let found: Vec<_> = validate_indexes(&spec, &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| (d.dotted_path(), d.kind, d.message))
        .collect();
    let mismatch = |path: &str, sql_type: &str| {
        (
            path.to_owned(),
            DiagnosticKind::TypeMismatch,
            format!("declared type does not accept the column's database type `{sql_type}`"),
        )
    };
    assert_eq!(
        found,
        vec![
            mismatch("mood", "mood"),
            mismatch("contact", "email_address")
        ]
    );

    let body = upsert(&documents, 1).await;
    assert_eq!(str_of(&body, "mood"), "calm");
    assert_eq!(str_of(&body, "contact"), "ada@example.com");
}

/// A `time` or `timetz` column under a `timestamp` or `date` field (issue
/// #309): OpenSearch refuses every non-null value under a `date` mapping with
/// no `format`, so `validate_indexes` reports it where every document carries
/// the field (the root, a plain object), through a domain, and instead of the
/// `date` type mismatch. Not reported: a field that sets any `options` (a
/// `format`, `ignore_malformed`, a `type` of its own), an input-only one, one
/// in a join (a parent with no related row indexes), and a `min`/`max` (a
/// parent with no related rows indexes).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_time_of_day_under_a_date_mapping_is_reported() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE DOMAIN opening AS time(3)",
        "ALTER TABLE users ADD COLUMN opens time, ADD COLUMN closes timetz, \
         ADD COLUMN slots time[], ADD COLUMN door opening",
        "ALTER TABLE orders ADD COLUMN slot time with time zone",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let column = |name: &str, source: &str, ty: &str, extra: serde_json::Value| {
        let mut field = serde_json::json!({ "field": name, "source": { "column": {
            "column": source, "ty": ty, "nullable": true,
        } } });
        if let (Some(field), Some(extra)) = (field.as_object_mut(), extra.as_object()) {
            for (key, value) in extra {
                if key == "array" {
                    field["source"]["column"]["array"] = value.clone();
                } else {
                    field.insert(key.clone(), value.clone());
                }
            }
        }
        field
    };
    let none = serde_json::json!({});
    let latest = |name: &str, ty: &str| {
        serde_json::json!({ "field": name, "source": { "relation": {
        "aggregate": {
            "table": "orders", "op": { "max": "slot" }, "key": { "direct": "user_id" },
            "value_type": ty,
        },
    } } })
    };
    let orders = |indexed: bool| {
        serde_json::json!({ "field": if indexed { "orders" } else { "raw" },
        "source": { "relation": { "join": {
            "table": "orders", "kind": { "has_many": { "foreign_key": "user_id" } },
            "primary_key": "id", "indexed": indexed,
            "fields": [column("slot", "slot", "timestamp", serde_json::json!({}))],
        } } } })
    };
    let fields: Vec<Field> = serde_json::from_value(serde_json::json!([
        column("opens", "opens", "timestamp", none.clone()),
        column("closes", "closes", "date", none.clone()),
        column(
            "slots",
            "slots",
            "timestamp",
            serde_json::json!({ "array": true })
        ),
        column("door", "door", "timestamp", none.clone()),
        latest("latest", "timestamp"),
        orders(true),
        column(
            "formatted",
            "opens",
            "timestamp",
            serde_json::json!({ "options": { "format": { "String": "strict_date_optional_time||HH:mm:ss" } } })
        ),
        column(
            "lenient",
            "closes",
            "timestamp",
            serde_json::json!({ "options": { "ignore_malformed": { "Bool": true } } })
        ),
        column(
            "hidden",
            "opens",
            "timestamp",
            serde_json::json!({ "emit": false })
        ),
        orders(false),
        column("placed", "joined", "date", none.clone()),
        column(
            "typed",
            "opens",
            "timestamp",
            serde_json::json!({ "options": { "type": { "String": "keyword" } } })
        ),
        serde_json::json!({ "field": "hours", "source": { "group": [
            column("opens", "opens", "timestamp", none),
        ] } }),
    ]))
    .unwrap();
    let schema = users_schema(fields, None);
    let spec = spec(schema.clone());
    let documents = builder(&url, schema).await;

    let found: Vec<_> = validate_indexes(&spec, &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| (d.dotted_path(), d.kind))
        .collect();
    let time = |path: &str| (path.to_owned(), DiagnosticKind::TimeOfDayAsDate);
    assert_eq!(
        found,
        vec![
            time("opens"),
            time("closes"),
            time("slots"),
            time("door"),
            time("hours.opens"),
        ]
    );
    let message = validate_indexes(&spec, &documents)
        .await
        .unwrap()
        .into_iter()
        .find(|d| d.dotted_path() == "closes")
        .unwrap()
        .message;
    assert_eq!(
        message,
        "a `date` field over the column's database type `time with time zone`: a time of day \
         is no date to OpenSearch, so it refuses every document whose value is not null. Index \
         it as text with a `custom` field (`postgres: [time with time zone]`, `opensearch: \
         keyword`), or as a `timestamp` field (a `date` field takes only a `date` column) \
         with `options: { format: \
         \"strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]XXX||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]X\" }`"
    );
}

/// A root keyed by a type the document builder can't read back is refused at
/// `primary_key` (issue #295): every key reads back as null, so even a row
/// that exists builds as a tombstone. Every type it reads, through a domain
/// too, passes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_root_key_the_builder_cannot_read_back_is_refused() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE EXTENSION citext",
        "CREATE TYPE mood AS ENUM ('calm', 'busy')",
        "CREATE DOMAIN score AS int",
        "CREATE DOMAIN tag AS citext",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let keyed = [
        ("inet", "'10.0.0.1'", false),
        ("mood", "'calm'", false),
        ("money", "'1.00'", false),
        ("jsonb", "'{}'", false),
        ("\"char\"", "'a'", false),
        ("bytea", "'\\x01'", true),
        ("citext", "'Ada'", true),
        ("score", "1", true),
        ("tag", "'Ada'", true),
        ("char(4)", "'AB'", true),
        ("numeric", "1.5", true),
        ("date", "'2024-01-01'", true),
        ("time", "'10:00'", true),
        ("timestamp", "'2024-01-01 10:00'", true),
        ("timestamptz", "'2024-01-01 10:00+00'", true),
    ];
    for (n, (sql_type, value, _)) in keyed.iter().enumerate() {
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE keyed_{n} (id {sql_type} PRIMARY KEY, name text); \
             INSERT INTO keyed_{n} VALUES ({value}, 'a')"
        )))
        .execute(&pool)
        .await
        .unwrap();
    }

    for (n, (sql_type, value, readable)) in keyed.into_iter().enumerate() {
        let mut schema = users_schema(vec![col("name", "name")], None);
        schema.table = table(&format!("keyed_{n}"));
        let documents = builder(&url, schema.clone()).await;
        let found: Vec<_> = validate_indexes(&spec(schema), &documents)
            .await
            .unwrap()
            .into_iter()
            .map(|d| (d.dotted_path(), d.kind.as_str(), d.message))
            .collect();
        if readable {
            assert_eq!(found, vec![], "{sql_type}");
            continue;
        }
        assert_eq!(
            found,
            vec![(
                "primary_key".to_owned(),
                "unsupported_key_type",
                format!(
                    "`primary_key` `id` is `{sql_type}`, a type that can't be read back as a \
                     document key: no document would ever be written"
                ),
            )],
            "{sql_type}"
        );
        let key = value.trim_matches('\'');
        let build = documents.build_many(&[doc_keyed(key)]).await.unwrap();
        assert!(
            matches!(build.as_slice(), [Document::Delete { .. }]),
            "an existing `{sql_type}` row builds as a tombstone: {build:?}"
        );
    }
}

/// A link Postgres can't compare with the key it names (issues #313, #299's
/// facts): every relation position, each against the key it is joined to.
/// The document query joins the two columns with `=`, so a pair with no `=`
/// operator fails to plan and every build of the index fails, whatever the
/// rows hold. Pairs Postgres compares across types (`int` with `bigint`,
/// `varchar` with `text`) are not reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn validation_reports_a_link_postgres_cannot_compare_with_its_key() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE owners (id uuid PRIMARY KEY, code text, name text)",
        "CREATE TABLE codes (id uuid PRIMARY KEY, name text)",
        "CREATE TABLE pets (id int PRIMARY KEY, owner text, code uuid, name text)",
        "CREATE TABLE cards (id int PRIMARY KEY, owner_ref varchar(36), name text)",
        "CREATE TABLE toys (id bigint PRIMARY KEY, name text)",
        "CREATE TABLE owner_toys (owner text, toy int, PRIMARY KEY (owner, toy))",
        "CREATE TABLE visits (id int PRIMARY KEY, pet_id bytea)",
        "CREATE TABLE kept (id int PRIMARY KEY, owner_id uuid, name text)",
        "INSERT INTO owners VALUES ('00000000-0000-0000-0000-00000000000a', 'x', 'o')",
    ] {
        sqlx::raw_sql(sqlx::AssertSqlSafe(statement))
            .execute(&pool)
            .await
            .unwrap();
    }
    let pair = |local: &str, related: &str| ColumnPair {
        local: column(local),
        related: column(related),
    };
    let join = |table_name: &str, kind: JoinKind| Join {
        table: table(table_name),
        primary_key: column("id"),
        nullable: true,
        kind,
        filters: None,
        order_by: None,
        limit: None,
        fields: vec![col("name", "name")],
        indexed: true,
        on: vec![],
    };
    let aggregate = |table_name: &str, key: AggregateKey, via: Vec<ViaHop>| Aggregate {
        table: table(table_name),
        op: AggregateOp::Count,
        key,
        value_type: None,
        filters: None,
        distinct: false,
        via,
        on: vec![],
    };
    let owner_toys = || Through {
        table: table("owner_toys"),
        left_key: column("owner"),
        right_key: column("toy"),
        filters: None,
    };
    let fields = vec![
        join_field(
            "pets",
            Join {
                on: vec![pair("code", "code")],
                ..join(
                    "pets",
                    JoinKind::HasMany {
                        foreign_key: column("owner"),
                    },
                )
            },
        ),
        join_field(
            "card",
            join(
                "cards",
                JoinKind::HasOne {
                    foreign_key: column("owner_ref"),
                },
            ),
        ),
        join_field(
            "code",
            join(
                "codes",
                JoinKind::BelongsTo {
                    column: column("code"),
                },
            ),
        ),
        join_field(
            "toys",
            join(
                "toys",
                JoinKind::ManyToMany {
                    through: owner_toys(),
                },
            ),
        ),
        agg_field(
            "petCount",
            aggregate("pets", AggregateKey::Direct(column("owner")), vec![]),
        ),
        agg_field(
            "toyCount",
            aggregate("toys", AggregateKey::Through(owner_toys()), vec![]),
        ),
        agg_field(
            "visitCount",
            aggregate(
                "visits",
                AggregateKey::Direct(column("pet_id")),
                vec![ViaHop {
                    table: table("pets"),
                    foreign_key: column("owner"),
                    filters: None,
                    on: vec![],
                }],
            ),
        ),
        join_field(
            "kept",
            join(
                "kept",
                JoinKind::HasMany {
                    foreign_key: column("owner_id"),
                },
            ),
        ),
    ];
    let mut schema = users_schema(fields, None);
    schema.table = table("owners");
    let documents = builder(&url, schema.clone()).await;
    let found: Vec<_> = validate_indexes(&spec(schema), &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| (d.dotted_path(), d.kind.as_str(), d.message))
        .collect();
    let cannot = |role: &str, link: &str, link_type: &str, key: &str, key_type: &str| {
        format!(
            "`{role}` `{link}` (`{link_type}`) can't be compared with `{key}` (`{key_type}`): \
             every document build fails"
        )
    };
    let expected = [
        (
            "pets",
            cannot("foreign_key", "pets.owner", "text", "owners.id", "uuid"),
        ),
        (
            "pets",
            cannot("on.related", "pets.code", "uuid", "owners.code", "text"),
        ),
        (
            "card",
            cannot(
                "foreign_key",
                "cards.owner_ref",
                "character varying(36)",
                "owners.id",
                "uuid",
            ),
        ),
        (
            "code",
            cannot("column", "owners.code", "text", "codes.id", "uuid"),
        ),
        (
            "toys",
            cannot(
                "through.left_key",
                "owner_toys.owner",
                "text",
                "owners.id",
                "uuid",
            ),
        ),
        (
            "petCount",
            cannot("foreign_key", "pets.owner", "text", "owners.id", "uuid"),
        ),
        (
            "toyCount",
            cannot(
                "through.left_key",
                "owner_toys.owner",
                "text",
                "owners.id",
                "uuid",
            ),
        ),
        (
            "visitCount",
            cannot("via.foreign_key", "pets.owner", "text", "owners.id", "uuid"),
        ),
        (
            "visitCount",
            cannot(
                "foreign_key",
                "visits.pet_id",
                "bytea",
                "pets.id",
                "integer",
            ),
        ),
    ];
    assert_eq!(
        found,
        expected
            .into_iter()
            .map(|(path, message)| (path.to_owned(), "link_type_mismatch", message))
            .collect::<Vec<_>>()
    );

    let build = documents
        .build_many(&[DocumentId {
            index: index_name("users"),
            key: RowKey(vec![(
                column("id"),
                GenericValue::Uuid("00000000-0000-0000-0000-00000000000a".parse().unwrap()),
            )]),
        }])
        .await;
    assert!(
        build
            .as_ref()
            .is_err_and(|e| e.to_string().contains("operator does not exist")),
        "every build fails, whatever the rows hold: {build:?}"
    );
}

/// A check whose probe the role may not run (issue #324): casting to a type
/// needs USAGE on the type's schema, reading a column of it does not. The
/// transform and the link checks are each reported as skipped, with the
/// database's reason, and the documents still build.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn a_check_the_role_may_not_run_is_reported_as_skipped() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE SCHEMA hidden",
        "CREATE DOMAIN hidden.label AS text",
        "CREATE DOMAIN hidden.ref AS int",
        "ALTER TABLE users ADD COLUMN label hidden.label DEFAULT 'ADA'",
        "CREATE TABLE badges (id int PRIMARY KEY, user_ref hidden.ref, name text)",
        "INSERT INTO badges VALUES (1, 1, 'first')",
        "CREATE ROLE reader LOGIN PASSWORD 'reader'",
        "GRANT SELECT ON ALL TABLES IN SCHEMA public TO reader",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let (_, address) = url.rsplit_once('@').unwrap();
    let reader_url = format!("postgres://reader:reader@{address}");

    let mut label = col("label", "label");
    if let FieldSource::Column(column) = &mut label.source {
        column.transforms = vec![Transform::Lowercase];
    }
    let badges = join_field(
        "badges",
        Join {
            table: table("badges"),
            primary_key: column("id"),
            nullable: true,
            kind: JoinKind::HasMany {
                foreign_key: column("user_ref"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col("name", "name")],
            indexed: true,
            on: vec![],
        },
    );
    let schema = users_schema(vec![label, badges], None);
    let documents = builder(&reader_url, schema.clone()).await;
    let found: Vec<_> = validate_indexes(&spec(schema), &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| {
            (
                d.dotted_path(),
                d.kind.as_str(),
                format!("{:?}", d.severity),
                d.message,
            )
        })
        .collect();
    let skipped = |path: &str, message: &str| {
        (
            path.to_owned(),
            "check_skipped",
            "Warning".to_owned(),
            message.to_owned(),
        )
    };
    assert_eq!(
        found,
        vec![
            (
                "label".to_owned(),
                "type_mismatch",
                "Error".to_owned(),
                "declared type does not accept the column's database type `hidden.label`"
                    .to_owned(),
            ),
            skipped(
                "label",
                "could not check `transforms` `lowercase` against the column's database type \
                 `hidden.label`: permission denied for schema hidden",
            ),
            skipped(
                "badges",
                "could not check `foreign_key` `badges.user_ref` (`hidden.ref`) against \
                 `users.id` (`integer`): permission denied for schema hidden",
            ),
        ],
    );

    let body = upsert(&documents, 1).await;
    assert_eq!(body["label"], GenericValue::String("ada".to_owned()));
    assert!(
        matches!(&body["badges"], GenericValue::Array(badges) if badges.len() == 1),
        "{body:?}"
    );
}

/// An `ids` aggregate's `element_type` against the related table's key type
/// (issue #299). OpenSearch maps the array as the element type: one that can
/// hold no value of the key's type rejects every document collecting a key,
/// so it is an error; one that holds only some, or changes them, is a
/// warning; one that holds them all is not reported.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn validation_checks_an_ids_element_type_against_the_related_key() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for key in ["bigint", "int", "uuid", "numeric", "text", "bytea"] {
        let name = key.replace(' ', "_");
        sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
            "CREATE TABLE keyed_{name} (id {key} PRIMARY KEY, user_id int)"
        )))
        .execute(&pool)
        .await
        .unwrap();
    }
    let ids = |name: &str, element: &str, key: &str| {
        agg_field(
            name,
            Aggregate {
                table: table(&format!("keyed_{key}")),
                op: AggregateOp::Ids {
                    element_type: serde_json::from_value(serde_json::json!(element)).unwrap(),
                },
                key: AggregateKey::Direct(column("user_id")),
                value_type: None,
                filters: None,
                distinct: false,
                via: Vec::new(),
                on: vec![],
            },
        )
    };
    let schema = users_schema(
        vec![
            ids("binaryOverBigint", "binary", "bigint"),
            ids("jsonOverInt", "json", "int"),
            ids("longOverUuid", "long", "uuid"),
            ids("booleanOverInt", "boolean", "int"),
            ids("integerOverBigint", "integer", "bigint"),
            ids("dateOverText", "date", "text"),
            ids("dateOverBigint", "date", "bigint"),
            ids("longOverInt", "long", "int"),
            ids("longOverBigint", "long", "bigint"),
            ids("keywordOverUuid", "keyword", "uuid"),
            ids("decimalOverNumeric", "decimal", "numeric"),
            ids("binaryOverBytea", "binary", "bytea"),
            ids("textOverBigint", "text", "bigint"),
        ],
        None,
    );
    let documents = builder(&url, schema.clone()).await;
    let found: Vec<_> = validate_indexes(&spec(schema), &documents)
        .await
        .unwrap()
        .into_iter()
        .map(|d| (d.dotted_path(), d.kind.as_str(), d.message))
        .collect();
    let rejects = |element: &str, key: &str, table: &str| {
        format!(
            "`element_type` `{element}` can't hold the `{key}` keys of `{table}`: OpenSearch \
             rejects every document that collects one"
        )
    };
    let some = |element: &str, key: &str, table: &str| {
        format!(
            "`element_type` `{element}` holds only some `{key}` keys of `{table}`: OpenSearch \
             rejects a document that collects another, or stores the key altered (a fraction \
             truncated), so two keys can become equal"
        )
    };
    let changes = |element: &str, key: &str, table: &str, how: &str| {
        format!("`element_type` `{element}` changes the `{key}` keys of `{table}`: {how}")
    };
    assert_eq!(
        found,
        vec![
            (
                "binaryOverBigint".to_owned(),
                "ids_element_lossy",
                changes(
                    "binary",
                    "bigint",
                    "keyed_bigint",
                    "they are indexed as written, not as bytes"
                ),
            ),
            (
                "jsonOverInt".to_owned(),
                "ids_element_mismatch",
                rejects("json", "integer", "keyed_int"),
            ),
            (
                "longOverUuid".to_owned(),
                "ids_element_mismatch",
                rejects("long", "uuid", "keyed_uuid"),
            ),
            (
                "booleanOverInt".to_owned(),
                "ids_element_mismatch",
                rejects("boolean", "integer", "keyed_int"),
            ),
            (
                "integerOverBigint".to_owned(),
                "ids_element_lossy",
                some("integer", "bigint", "keyed_bigint"),
            ),
            (
                "dateOverText".to_owned(),
                "ids_element_lossy",
                some("date", "text", "keyed_text"),
            ),
            (
                "dateOverBigint".to_owned(),
                "ids_element_lossy",
                changes(
                    "date",
                    "bigint",
                    "keyed_bigint",
                    "they are read as milliseconds since 1970"
                ),
            ),
        ]
    );
}

/// A document keyed as a live change carries a value of a type the WAL
/// decoder leaves as text.
fn doc_keyed(key: &str) -> DocumentId {
    DocumentId {
        index: index_name("users"),
        key: RowKey(vec![(column("id"), GenericValue::String(key.to_owned()))]),
    }
}

/// What `validate_indexes` reports that a document build can't get past
/// (issue #225), each against what the build really does:
///
/// - a declared root `primary_key` naming no column on a table keyed by one
///   column, or naming a column other than that key;
/// - a relation table without a single-column primary key;
/// - `transforms` the column's type doesn't take. Postgres decides, so every
///   type that takes `lower`/`trim` through an implicit cast or its own
///   function passes, and a chain fails on its first refused step;
/// - a root table without a single-column primary key: reported, the build
///   runs. On one, a declared `primary_key` naming no column breaks only a
///   relation: a `has_many` fails the root's build, a `belongs_to` fails the
///   change on its target, and with no relation the build runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires docker"]
async fn validation_reports_what_no_document_build_gets_past() {
    let (_pg, url) = start_seeded().await;
    let pool = PgPoolOptions::new().connect(&url).await.unwrap();
    for statement in [
        "CREATE TABLE notes (user_id int, body text)",
        "INSERT INTO notes (user_id, body) VALUES (1, 'first')",
        "CREATE EXTENSION citext",
        "CREATE TYPE mood AS ENUM ('calm', 'busy')",
        "CREATE DOMAIN email_address AS text",
        "CREATE DOMAIN score AS int",
        "ALTER TABLE users ADD COLUMN mood mood DEFAULT 'calm', \
         ADD COLUMN contact email_address DEFAULT 'ADA@example.com', \
         ADD COLUMN handle varchar(20) DEFAULT 'Ada', \
         ADD COLUMN code char(4) DEFAULT 'AB', \
         ADD COLUMN nick name DEFAULT 'Ada', \
         ADD COLUMN login citext DEFAULT 'Ada', \
         ADD COLUMN span int4range DEFAULT '[1,5)', \
         ADD COLUMN aliases text[] DEFAULT '{a}', \
         ADD COLUMN extra jsonb DEFAULT '{}', \
         ADD COLUMN rank score DEFAULT 1",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }
    let found = |schema: IndexSchema| {
        let url = url.clone();
        async move {
            let documents = builder(&url, schema.clone()).await;
            let found = validate_indexes(&spec(schema), &documents)
                .await
                .unwrap()
                .into_iter()
                .map(|d| {
                    let at = match d.anchor {
                        Anchor::Field => d.dotted_path(),
                        Anchor::IndexKey => format!("@{}", d.dotted_path()),
                        _ => unreachable!("a new anchor"),
                    };
                    (at, d.kind, d.message)
                })
                .collect::<Vec<_>>();
            (found, documents.build_many(&[doc(1)]).await)
        }
    };
    let expected = |pairs: &[(&str, DiagnosticKind, &str)]| {
        pairs
            .iter()
            .map(|(path, kind, message)| ((*path).to_owned(), *kind, (*message).to_owned()))
            .collect::<Vec<_>>()
    };
    let upserted = |build: &source::Result<Vec<Document>>| {
        matches!(build.as_deref(), Ok([Document::Upsert { .. }]))
    };

    let mut unknown_key = users_schema(vec![col("name", "name")], None);
    unknown_key.primary_key = Some(column("user_id"));
    let (diagnostics, build) = found(unknown_key).await;
    assert_eq!(
        diagnostics,
        expected(&[(
            "@primary_key",
            DiagnosticKind::UnknownColumn,
            "`primary_key` references unknown column `public.users.user_id`"
        )]),
    );
    assert!(build.is_err(), "the batched build reads the declared key");

    let mut other_key = users_schema(vec![col("name", "name")], None);
    other_key.primary_key = Some(column("email"));
    let (diagnostics, build) = found(other_key).await;
    assert_eq!(
        diagnostics,
        expected(&[(
            "@primary_key",
            DiagnosticKind::RootPrimaryKeyMismatch,
            "`primary_key` is `email`, but `public.users` is keyed by `id`: the backfill and \
             every root change would match the wrong row or none"
        )]),
    );
    assert!(
        matches!(build.as_deref(), Ok([Document::Delete { .. }])),
        "a live row keyed by `id` is matched on `email`: {build:?}"
    );

    let has_many = |name: &str, table_name: &str| {
        join_field(
            name,
            Join {
                table: table(table_name),
                primary_key: column("user_id"),
                nullable: true,
                kind: JoinKind::HasMany {
                    foreign_key: column("user_id"),
                },
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![],
                indexed: true,
                on: vec![],
            },
        )
    };
    for (name, problem) in [
        ("notes", "has no primary key"),
        (
            "user_tags",
            "has a composite primary key (`user_id`, `tag_id`)",
        ),
    ] {
        let (diagnostics, build) = found(users_schema(vec![has_many(name, name)], None)).await;
        assert_eq!(
            diagnostics,
            vec![(
                name.to_owned(),
                DiagnosticKind::RelationPrimaryKey,
                format!(
                    "`table` `public.{name}` {problem}; a relation table needs a single-column one"
                ),
            )],
        );
        assert!(build.is_err(), "{name}: the build needs the key");
    }

    let transformed = |name: &str, transforms: &[Transform]| Field {
        source: FieldSource::Column(Column {
            column: column(name),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: transforms.to_vec(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
        ..base(name)
    };
    let lower = &[Transform::Lowercase][..];
    let lower_trim = &[Transform::Lowercase, Transform::Trim][..];
    for (name, transforms, accepted) in [
        ("handle", lower_trim, true),
        ("code", lower_trim, true),
        ("nick", lower_trim, true),
        ("login", lower_trim, true),
        ("contact", lower_trim, true),
        ("span", lower, true),
        ("id", lower, false),
        ("aliases", lower, false),
        ("extra", lower, false),
        ("mood", lower, false),
        ("rank", lower, false),
        ("span", lower_trim, false),
    ] {
        let (diagnostics, build) =
            found(users_schema(vec![transformed(name, transforms)], None)).await;
        let refused = diagnostics
            .iter()
            .any(|(_, kind, _)| *kind == DiagnosticKind::TransformNotApplicable);
        assert_eq!(
            !refused, accepted,
            "{name} {transforms:?}: reported {diagnostics:?}"
        );
        assert_eq!(
            upserted(&build),
            accepted,
            "{name} {transforms:?}: built {build:?}"
        );
    }
    let (diagnostics, _) = found(users_schema(vec![transformed("mood", lower)], None)).await;
    assert!(
        diagnostics.contains(&(
            "mood".to_owned(),
            DiagnosticKind::TransformNotApplicable,
            "`transforms` `lowercase` do not apply to the column's database type `mood`".to_owned(),
        )),
        "{diagnostics:?}"
    );

    let keyless_root = |primary_key: Option<&str>, related: Option<Field>| {
        let mut fields = vec![col("body", "body")];
        fields.extend(related);
        let mut schema = users_schema(fields, None);
        schema.table = table("notes");
        schema.primary_key = primary_key.map(column);
        schema
    };
    let no_key = (
        "@table".to_owned(),
        DiagnosticKind::RootPrimaryKey,
        "`public.notes` has no primary key: the backfill skips it; live changes are still \
         indexed"
            .to_owned(),
    );
    let (diagnostics, _) = found(keyless_root(None, None)).await;
    assert_eq!(diagnostics, vec![no_key.clone()]);
    let (diagnostics, _) = found(keyless_root(Some("id"), None)).await;
    assert_eq!(
        diagnostics,
        vec![
            no_key.clone(),
            (
                "@primary_key".to_owned(),
                DiagnosticKind::RootPrimaryKey,
                "`primary_key` references unknown column `public.notes.id`; a root change built by \
                 its whole key does not read it"
                    .to_owned(),
            ),
        ],
    );
    let whole_row = DocumentId {
        index: index_name("users"),
        key: RowKey(vec![
            (column("user_id"), GenericValue::Int(1)),
            (column("body"), GenericValue::String("first".into())),
        ]),
    };
    let documents = builder(&url, keyless_root(Some("id"), None)).await;
    assert!(
        matches!(
            documents.build_many(&[whole_row.clone()]).await.as_deref(),
            Ok([Document::Upsert { .. }])
        ),
        "a root change keyed by its whole row never reads the declared key"
    );

    let unknown = (
        "@primary_key".to_owned(),
        DiagnosticKind::UnknownColumn,
        "`primary_key` references unknown column `public.notes.id`".to_owned(),
    );
    let orders = join_field(
        "orders",
        Join {
            table: table("orders"),
            primary_key: column("id"),
            nullable: true,
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![],
            indexed: true,
            on: vec![],
        },
    );
    let (diagnostics, _) = found(keyless_root(Some("id"), Some(orders.clone()))).await;
    assert_eq!(diagnostics, vec![no_key.clone(), unknown.clone()]);
    let documents = builder(&url, keyless_root(Some("id"), Some(orders))).await;
    assert!(
        documents.build_many(&[whole_row]).await.is_err(),
        "a `has_many` keys on the declared column"
    );

    let author = join_field(
        "author",
        Join {
            table: table("users"),
            primary_key: column("id"),
            nullable: true,
            kind: JoinKind::BelongsTo {
                column: column("user_id"),
            },
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col("name", "name")],
            indexed: true,
            on: vec![],
        },
    );
    let (diagnostics, _) = found(keyless_root(Some("id"), Some(author.clone()))).await;
    assert_eq!(diagnostics, vec![no_key, unknown]);
    let documents = builder(&url, keyless_root(Some("id"), Some(author))).await;
    assert!(
        documents
            .resolve(&table("users"), &key(1), None, None)
            .await
            .is_err(),
        "a change on a `belongs_to` target is traced to the root by the declared column"
    );
}

// ---------------------------------------------------------------------------
// Builders for config, schema, fields, filters, and the value assertions.
// ---------------------------------------------------------------------------

fn spec(schema: IndexSchema) -> SourceSpec {
    SourceSpec::new(BTreeMap::from([(index_name("users"), schema)]))
}

fn users_schema(fields: Vec<Field>, soft_delete: Option<SoftDelete>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete,
        filters: None,
        fields,
    }
}

fn base(name: &str) -> Field {
    // A field with no database source — a literal-null constant unless the
    // caller overrides `source`.
    Field {
        field: field(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Constant(GenericValue::Null),
    }
}

fn col(name: &str, source_column: &str) -> Field {
    Field {
        source: FieldSource::Column(Column {
            column: column(source_column),
            ty: FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
        ..base(name)
    }
}

fn join_field(name: &str, join: Join) -> Field {
    Field {
        source: FieldSource::Relation(Relation::Join(join)),
        ..base(name)
    }
}

fn agg_field(name: &str, aggregate: Aggregate) -> Field {
    Field {
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
        ..base(name)
    }
}

/// An aggregate over user 1's orders via the direct `user_id` foreign key.
fn orders_agg(op: AggregateOp, filters: Option<Vec<Filter>>) -> Aggregate {
    Aggregate {
        table: table("orders"),
        op,
        key: AggregateKey::Direct(column("user_id")),
        value_type: None,
        filters,
        distinct: false,
        via: Vec::new(),
        on: vec![],
    }
}

/// A `count` of the related orders under the given filters, named `name`.
fn count_where(name: &str, filters: Vec<Filter>) -> Field {
    agg_field(name, orders_agg(AggregateOp::Count, Some(filters)))
}

fn value_op(col: &str, op: FilterOp, value: FilterValue) -> Filter {
    Filter::ValueOp(ValueOpFilter {
        column: column(col),
        op,
        value,
    })
}

fn eq(col: &str, value: &str) -> Filter {
    value_op(col, FilterOp::Eq, single(value))
}

fn null_check(col: &str, op: NullOp) -> Filter {
    Filter::NullCheck(NullCheckFilter {
        column: column(col),
        op,
    })
}

fn raw(sql: &str) -> Filter {
    Filter::Raw(RawFilter {
        raw: RawFilterValue::try_new(sql).unwrap(),
    })
}

fn single(value: &str) -> FilterValue {
    FilterValue::Single(value.to_owned())
}

// ---------------------------------------------------------------------------
// Build + assertion helpers.
// ---------------------------------------------------------------------------

async fn upsert(builder: &PgDocumentBuilder, id: i64) -> BTreeMap<String, GenericValue> {
    match builder.build(&doc(id)).await.unwrap() {
        Document::Upsert {
            body: GenericValue::Map(map),
            ..
        } => map,
        Document::Upsert { .. } => panic!("expected the document body to be an object"),
        Document::Delete { .. } => panic!("expected an upsert, got a tombstone"),
        Document::Failed { failure, .. } => panic!("expected an upsert, got {failure}"),
    }
}

async fn is_tombstone(builder: &PgDocumentBuilder, id: i64) -> bool {
    matches!(
        builder.build(&doc(id)).await.unwrap(),
        Document::Delete { .. }
    )
}

fn doc(id: i64) -> DocumentId {
    DocumentId {
        index: index_name("users"),
        key: key(id),
    }
}

fn id_of(document: &DocumentId) -> i64 {
    match document.key.0.first() {
        Some((_, value)) => int_of(value),
        _ => panic!("expected a document key"),
    }
}

fn key(id: i64) -> RowKey {
    row_key("id", id)
}

// The fixture tables use `int` (int4) keys, which decode to `Int`; build request
// keys as the same variant so they compare equal to resolved keys.
fn row_key(col: &str, id: i64) -> RowKey {
    RowKey(vec![(column(col), GenericValue::Int(id as i32))])
}

fn str_of<'a>(map: &'a BTreeMap<String, GenericValue>, key: &str) -> &'a str {
    match map.get(key) {
        Some(GenericValue::String(s)) => s,
        other => panic!("`{key}` should be a string, got {other:?}"),
    }
}

fn arr_of<'a>(map: &'a BTreeMap<String, GenericValue>, key: &str) -> &'a [GenericValue] {
    match map.get(key) {
        Some(GenericValue::Array(items)) => items,
        other => panic!("`{key}` should be an array, got {other:?}"),
    }
}

// Width-tolerant: a key decodes to its column's exact integer variant, while an
// integer in the assembled JSON body comes back `BigInt`.
fn int_of(value: &GenericValue) -> i64 {
    match value {
        GenericValue::SmallInt(i) => i64::from(*i),
        GenericValue::Int(i) => i64::from(*i),
        GenericValue::BigInt(i) => *i,
        other => panic!("expected an int, got {other:?}"),
    }
}

/// Coerce a numeric document value to f64 for tolerant comparison — counts come
/// back integer, sums/min/max as decimals, and averages as `Double`.
fn num_of(value: &GenericValue) -> f64 {
    match value {
        GenericValue::SmallInt(i) => f64::from(*i),
        GenericValue::Int(i) => f64::from(*i),
        GenericValue::BigInt(i) => *i as f64,
        GenericValue::Float(f) => f64::from(*f),
        GenericValue::Double(f) => *f,
        GenericValue::Decimal(d) => d.to_string().parse().unwrap(),
        GenericValue::String(s) => s.parse().unwrap(),
        other => panic!("expected a number, got {other:?}"),
    }
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
