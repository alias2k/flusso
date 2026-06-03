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

use schema_core::{
    Aggregate, AggregateOp, Column, ColumnName, Config, ConnectionUrl, DatabaseSchema, Direction,
    Field, FieldName, FieldSource, Filter, FilterOp, FilterValue, GenericValue, Index, IndexName,
    IndexSchema, Join, JoinKey, JoinType, NullCheckFilter, NullOp, OrderBy, RawFilter,
    RawFilterValue, Relation, SoftDelete, SoftDeleteColumn, SoftDeleteField, Source, SourceType,
    TableName, Through, Transform, ValueOpFilter,
};
use sources_core::RowKey;
use sources_core::document::{Document, DocumentBuilder, DocumentId};
use sources_postgres::PgDocumentBuilder;
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
    PgDocumentBuilder::connect(url, Arc::new(config(url, schema)))
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
            join_type: JoinType::OneToOne,
            key: JoinKey::Direct(column("user_id")),
            filters: None,
            order_by: None,
            limit: None,
        },
        vec![col("headline", "headline")],
    );
    // One-to-many, newest first, capped at 2; each order folds in its items.
    let orders = join_field(
        "orders",
        Join {
            table: table("orders"),
            join_type: JoinType::OneToMany,
            key: JoinKey::Direct(column("user_id")),
            filters: None,
            order_by: Some(vec![OrderBy {
                column: column("placed_at"),
                direction: Some(Direction::Desc),
            }]),
            limit: Some(2),
        },
        vec![
            col("id", "id"),
            col("status", "status"),
            join_field(
                "items",
                Join {
                    table: table("order_items"),
                    join_type: JoinType::OneToMany,
                    key: JoinKey::Direct(column("order_id")),
                    filters: None,
                    order_by: Some(vec![OrderBy {
                        column: column("sku"),
                        direction: Some(Direction::Asc),
                    }]),
                    limit: None,
                },
                vec![col("sku", "sku"), col("qty", "qty")],
            ),
        ],
    );
    // Many-to-many through the user_tags junction, labels A→Z.
    let tags = join_field(
        "tags",
        Join {
            table: table("tags"),
            join_type: JoinType::ManyToMany,
            key: JoinKey::Through(Through {
                table: table("user_tags"),
                left_key: column("user_id"),
                right_key: column("tag_id"),
            }),
            filters: None,
            order_by: Some(vec![OrderBy {
                column: column("label"),
                direction: Some(Direction::Asc),
            }]),
            limit: None,
        },
        vec![col("label", "label")],
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
            orders_agg(AggregateOp::Sum(column("total")), None),
        ),
        agg_field(
            "avg_order",
            orders_agg(AggregateOp::Avg(column("total")), None),
        ),
        agg_field(
            "min_order",
            orders_agg(AggregateOp::Min(column("total")), None),
        ),
        agg_field(
            "max_order",
            orders_agg(AggregateOp::Max(column("total")), None),
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
                key: JoinKey::Through(Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                }),
                filters: None,
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
            transforms: vec![Transform::Trim, Transform::Lowercase],
            default: None,
        }),
        ..base("email")
    };
    let bio = Field {
        source: FieldSource::Column(Column {
            column: column("bio"),
            transforms: Vec::new(),
            default: Some(GenericValue::String("(no bio)".into())),
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
                join_type: JoinType::OneToOne,
                key: JoinKey::Direct(column("user_id")),
                filters: None,
                order_by: None,
                limit: None,
            },
            vec![col("headline", "headline")],
        ),
        join_field(
            "orders",
            Join {
                table: table("orders"),
                join_type: JoinType::OneToMany,
                key: JoinKey::Direct(column("user_id")),
                filters: None,
                order_by: None,
                limit: None,
            },
            vec![
                col("id", "id"),
                join_field(
                    "items",
                    Join {
                        table: table("order_items"),
                        join_type: JoinType::OneToMany,
                        key: JoinKey::Direct(column("order_id")),
                        filters: None,
                        order_by: None,
                        limit: None,
                    },
                    vec![col("sku", "sku")],
                ),
            ],
        ),
        join_field(
            "tags",
            Join {
                table: table("tags"),
                join_type: JoinType::ManyToMany,
                key: JoinKey::Through(Through {
                    table: table("user_tags"),
                    left_key: column("user_id"),
                    right_key: column("tag_id"),
                }),
                filters: None,
                order_by: None,
                limit: None,
            },
            vec![col("label", "label")],
        ),
    ];

    let builder = builder(&url, users_schema(fields, None)).await;

    // Root table: the key is the document id.
    assert_eq!(
        builder.resolve(&table("users"), &key(1)).await.unwrap(),
        vec![doc(1)],
    );

    // Direct FK: an order change resolves to its owning user.
    assert_eq!(
        builder.resolve(&table("orders"), &key(10)).await.unwrap(),
        vec![doc(1)],
    );

    // One-to-one direct FK: a profile change resolves to its user.
    assert_eq!(
        builder
            .resolve(&table("profiles"), &row_key("id", 100))
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // Through, far side: tag 1 is held by users 1 and 2.
    let mut roots = builder.resolve(&table("tags"), &key(1)).await.unwrap();
    roots.sort_by_key(id_of);
    assert_eq!(roots, vec![doc(1), doc(2)]);

    // Through, junction side: a user_tags row carrying user_id resolves directly.
    assert_eq!(
        builder
            .resolve(&table("user_tags"), &row_key("user_id", 1))
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // Multi-hop: an item change walks order_items → orders → users.
    assert_eq!(
        builder
            .resolve(&table("order_items"), &key(1002))
            .await
            .unwrap(),
        vec![doc(1)],
    );

    // A table the schema never references resolves to nothing.
    assert!(
        builder
            .resolve(&table("tags"), &key(999))
            .await
            .unwrap()
            .is_empty()
    );
}

// ---------------------------------------------------------------------------
// Builders for config, schema, fields, filters, and the value assertions.
// ---------------------------------------------------------------------------

fn config(connection_url: &str, schema: IndexSchema) -> Config {
    Config {
        source: Source {
            source_type: SourceType::Postgres,
            connection_url: ConnectionUrl::try_new(connection_url).unwrap(),
        },
        sinks: BTreeMap::new(),
        indexes: BTreeMap::from([(
            index_name("users"),
            Index {
                enabled: true,
                schema,
            },
        )]),
    }
}

fn users_schema(fields: Vec<Field>, soft_delete: Option<SoftDelete>) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: table("users"),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete,
        fields,
    }
}

fn base(name: &str) -> Field {
    // A field with no database source — a literal-null constant unless the
    // caller overrides `source`.
    Field {
        field: field(name),
        mapping: None,
        source: FieldSource::Constant(GenericValue::Null),
    }
}

fn col(name: &str, source_column: &str) -> Field {
    Field {
        source: FieldSource::Column(Column {
            column: column(source_column),
            transforms: Vec::new(),
            default: None,
        }),
        ..base(name)
    }
}

fn join_field(name: &str, join: Join, sub: Vec<Field>) -> Field {
    Field {
        source: FieldSource::Relation(Relation::Join { join, fields: sub }),
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
        key: JoinKey::Direct(column("user_id")),
        filters,
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
        Some((_, GenericValue::Int(i))) => *i,
        _ => panic!("expected an int document key"),
    }
}

fn key(id: i64) -> RowKey {
    row_key("id", id)
}

fn row_key(col: &str, id: i64) -> RowKey {
    RowKey(vec![(column(col), GenericValue::Int(id))])
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

fn int_of(value: &GenericValue) -> i64 {
    match value {
        GenericValue::Int(i) => *i,
        other => panic!("expected an int, got {other:?}"),
    }
}

/// Coerce a numeric document value (int or decimal) to f64 for tolerant
/// comparison — sums and averages come back as decimals.
fn num_of(value: &GenericValue) -> f64 {
    match value {
        GenericValue::Int(i) => *i as f64,
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
