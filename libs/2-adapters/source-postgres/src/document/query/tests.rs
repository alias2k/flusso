//! Example-based tests over the entry queries: fixed schema shapes asserted
//! against their exact generated SQL.
#![allow(clippy::unwrap_used)]

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Direction, Field, FieldSource, Filter, FilterOp,
    FilterValue, IndexSchema, Join, JoinKind, OrderBy, Relation, SoftDelete, SoftDeleteColumn,
    ValueOpFilter,
};

use super::*;

fn db() -> DatabaseSchema {
    DatabaseSchema::try_new("public").unwrap()
}
fn t(n: &str) -> TableName {
    TableName::try_new(n).unwrap()
}
fn c(n: &str) -> ColumnName {
    ColumnName::try_new(n).unwrap()
}
fn f(n: &str) -> kernel::FieldName {
    kernel::FieldName::try_new(n).unwrap()
}
fn col_field(name: &str, column: &str) -> Field {
    Field {
        field: f(name),
        options: Default::default(),
        source: FieldSource::Column(kernel::Column {
            column: c(column),
            ty: kernel::FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
        }),
    }
}
/// A `(table, column) -> sql_type` map from triples, for the keyed-predicate
/// casts. The keyed lookup casts each `$n` to its column's catalog type, so
/// every test that keys a query must declare the key column's type here.
fn types(triples: &[(&str, &str, &str)]) -> HashMap<(String, String), String> {
    triples
        .iter()
        .map(|(table, column, ty)| {
            (
                ((*table).to_owned(), (*column).to_owned()),
                (*ty).to_owned(),
            )
        })
        .collect()
}

/// The common case: the root `users.id` key is a `bigint`.
fn id_types() -> HashMap<(String, String), String> {
    types(&[("users", "id", "bigint")])
}

fn index(
    primary_key: Option<&str>,
    soft_delete: Option<SoftDelete>,
    fields: Vec<Field>,
) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: t("users"),
        db_schema: db(),
        primary_key: primary_key.map(c),
        doc_id: None,
        soft_delete,
        filters: None,
        fields,
    }
}

#[test]
fn columns_only() {
    let schema = index(
        Some("id"),
        None,
        vec![col_field("id", "id"), col_field("email", "email")],
    );
    let (sql, params) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
    assert_eq!(params, vec![GenericValue::Int(7)]);
}

#[test]
fn keyed_predicate_casts_a_uuid_primary_key() {
    // Regression: a uuid PK decodes to a string and re-binds as `text`; without
    // the `::uuid` cast Postgres rejects `uuid = text`. Both the single-key and
    // batched (`IN`) forms must cast.
    let schema = index(Some("id"), None, vec![col_field("id", "id")]);
    let col_types = types(&[("users", "id", "uuid")]);
    let key = GenericValue::String("3f2a1b9c-0000-0000-0000-000000000000".to_owned());

    let (single, _) = document_query(
        &schema,
        &[(c("id"), key.clone())],
        &HashMap::new(),
        &col_types,
    )
    .unwrap();
    assert!(
        single.as_str().ends_with(r#"WHERE "root"."id" = $1::uuid"#),
        "{}",
        single.as_str()
    );

    let (batched, _) =
        documents_query(&schema, &c("id"), &[key], &HashMap::new(), &col_types).unwrap();
    assert!(
        batched
            .as_str()
            .ends_with(r#"WHERE "root"."id" IN ($1::uuid)"#),
        "{}",
        batched.as_str()
    );
}

#[test]
fn root_filters_fold_into_both_query_forms() {
    let mut schema = index(Some("id"), None, vec![col_field("id", "id")]);
    schema.filters = Some(vec![Filter::ValueOp(ValueOpFilter {
        column: c("status"),
        op: FilterOp::Eq,
        value: FilterValue::Single("active".to_owned()),
    })]);
    let col_types = types(&[("users", "id", "bigint"), ("users", "status", "text")]);

    let (sql, params) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &col_types,
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('id', "root"."id") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint AND ("root"."status" = $2::text)"#
    );
    assert_eq!(
        params,
        vec![
            GenericValue::Int(7),
            GenericValue::String("active".to_owned())
        ]
    );

    let (sql, _) = documents_query(
        &schema,
        &c("id"),
        &[GenericValue::Int(7)],
        &HashMap::new(),
        &col_types,
    )
    .unwrap();
    assert!(
        sql.as_str()
            .ends_with(r#"WHERE "root"."id" IN ($1::bigint) AND ("root"."status" = $2::text)"#),
        "{}",
        sql.as_str()
    );
}

#[test]
fn has_many_with_order_and_limit() {
    let orders = Field {
        field: f("orders"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: t("orders"),
            kind: JoinKind::HasMany {
                foreign_key: c("user_id"),
            },
            primary_key: c("primary_key"),
            nullable: false,
            filters: None,
            order_by: Some(vec![OrderBy {
                column: c("created_at"),
                direction: Some(Direction::Desc),
            }]),
            limit: Some(5),
            fields: vec![col_field("id", "id"), col_field("total", "total")],
        })),
    };
    let schema = index(Some("id"), None, vec![orders]);
    let mut pks = HashMap::new();
    pks.insert("orders".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('orders', (SELECT coalesce(json_agg(json_build_object('id', "rel1"."id", 'total', "rel1"."total") ORDER BY "rel1"."created_at" DESC), '[]'::json) FROM (SELECT "rel2".* FROM "public"."orders" AS "rel2" WHERE "rel2"."user_id" = "root"."id" ORDER BY "rel2"."created_at" DESC LIMIT 5) AS "rel1")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

#[test]
fn belongs_to_correlates_on_the_parent_column() {
    let org = Field {
        field: f("org"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: t("orgs"),
            kind: JoinKind::BelongsTo {
                column: c("org_id"),
            },
            primary_key: c("id"),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col_field("name", "name")],
        })),
    };
    let schema = index(Some("id"), None, vec![org]);
    let mut pks = HashMap::new();
    pks.insert("orgs".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    // The target is matched by ITS primary key against the parent's own
    // column — the reverse of a has_one — and needs no parent primary key.
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('org', (SELECT json_build_object('name', "rel1"."name") FROM "public"."orgs" AS "rel1" WHERE "rel1"."id" = "root"."org_id" LIMIT 1)) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

#[test]
fn aggregate_count() {
    let count = Field {
        field: f("order_count"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("orders"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
        })),
    };
    let schema = index(Some("id"), None, vec![count]);
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('order_count', (SELECT count(*) FROM "public"."orders" AS "rel1" WHERE "rel1"."user_id" = "root"."id")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

#[test]
fn aggregate_ids_direct_collects_the_related_pk() {
    let ids = Field {
        field: f("order_ids"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("orders"),
            op: AggregateOp::Ids {
                element_type: kernel::FlussoType::Long,
            },
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
        })),
    };
    let schema = index(Some("id"), None, vec![ids]);
    let mut pks = HashMap::new();
    pks.insert("orders".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('order_ids', (SELECT coalesce(json_agg("rel1"."id"), '[]'::json) FROM "public"."orders" AS "rel1" WHERE "rel1"."user_id" = "root"."id")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

fn null_check(column: &str) -> Filter {
    Filter::NullCheck(kernel::NullCheckFilter {
        column: c(column),
        op: kernel::NullOp::IsNull,
    })
}

/// An `orders` document aggregating `products` through `order_items`.
fn through_aggregate_sql(
    op: AggregateOp,
    distinct: bool,
    junction_filters: Option<Vec<Filter>>,
    filters: Option<Vec<Filter>>,
) -> String {
    let aggregate = Field {
        field: f("agg"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("products"),
            op,
            key: AggregateKey::Through(kernel::Through {
                table: t("order_items"),
                left_key: c("order_id"),
                right_key: c("product_id"),
                filters: junction_filters,
            }),
            value_type: None,
            filters,
            distinct,
        })),
    };
    let schema = index(Some("id"), None, vec![aggregate]);
    let mut pks = HashMap::new();
    pks.insert("products".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &types(&[("users", "id", "bigint"), ("order_items", "status", "text")]),
    )
    .unwrap();
    sql.as_str().to_owned()
}

fn wrap(aggregate: &str) -> String {
    format!(
        r#"SELECT json_build_object('agg', {aggregate}) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    )
}

#[test]
fn aggregate_through_counts_once_per_junction_row() {
    assert_eq!(
        through_aggregate_sql(
            AggregateOp::Sum(c("weight")),
            false,
            Some(vec![null_check("cancelled_at")]),
            Some(vec![null_check("archived_at")]),
        ),
        wrap(
            r#"(SELECT sum("rel1"."weight") FROM "public"."products" AS "rel1" JOIN "public"."order_items" AS "rel2" ON "rel2"."product_id" = "rel1"."id" WHERE "rel2"."order_id" = "root"."id" AND ("rel1"."archived_at" IS NULL) AND ("rel2"."cancelled_at" IS NULL))"#
        ),
    );
}

#[test]
fn distinct_aggregate_through_counts_each_target_row_once() {
    assert_eq!(
        through_aggregate_sql(
            AggregateOp::Sum(c("weight")),
            true,
            Some(vec![null_check("cancelled_at")]),
            Some(vec![null_check("archived_at")]),
        ),
        wrap(
            r#"(SELECT sum("rel1"."weight") FROM "public"."products" AS "rel1" WHERE "rel1"."id" IN (SELECT "rel2"."product_id" FROM "public"."order_items" AS "rel2" WHERE "rel2"."order_id" = "root"."id" AND ("rel2"."cancelled_at" IS NULL)) AND ("rel1"."archived_at" IS NULL))"#
        ),
    );
}

#[test]
fn aggregate_ids_through_joins_the_target_and_honours_filters() {
    assert_eq!(
        through_aggregate_sql(
            AggregateOp::Ids {
                element_type: kernel::FlussoType::Long,
            },
            false,
            None,
            Some(vec![null_check("archived_at")]),
        ),
        wrap(
            r#"(SELECT coalesce(json_agg("rel1"."id"), '[]'::json) FROM "public"."products" AS "rel1" JOIN "public"."order_items" AS "rel2" ON "rel2"."product_id" = "rel1"."id" WHERE "rel2"."order_id" = "root"."id" AND ("rel1"."archived_at" IS NULL))"#
        ),
    );
}

#[test]
fn junction_filter_value_casts_to_the_junction_column_type() {
    let sql = through_aggregate_sql(
        AggregateOp::Count,
        false,
        Some(vec![Filter::ValueOp(ValueOpFilter {
            column: c("status"),
            op: FilterOp::Neq,
            value: FilterValue::Single("cancelled".to_owned()),
        })]),
        None,
    );
    assert!(
        sql.contains(r#"AND ("rel2"."status" <> $1::text)"#),
        "{sql}"
    );
}

#[test]
fn many_to_many_join_applies_junction_filters() {
    let tags = Field {
        field: f("tags"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: t("tags"),
            kind: JoinKind::ManyToMany {
                through: kernel::Through {
                    table: t("post_tags"),
                    left_key: c("post_id"),
                    right_key: c("tag_id"),
                    filters: Some(vec![null_check("removed_at")]),
                },
            },
            primary_key: c("id"),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col_field("name", "name")],
        })),
    };
    let schema = index(Some("id"), None, vec![tags]);
    let mut pks = HashMap::new();
    pks.insert("tags".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    assert!(
        sql.as_str()
            .contains(r#"WHERE "rel3"."post_id" = "root"."id" AND ("rel3"."removed_at" IS NULL)"#),
        "{}",
        sql.as_str()
    );
}

#[test]
fn soft_delete_folds_into_where() {
    let schema = index(
        Some("id"),
        Some(SoftDelete::Column(SoftDeleteColumn {
            column: c("deleted_at"),
            when: None,
        })),
        vec![col_field("id", "id")],
    );
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert!(sql.as_str().contains(
        r#"WHERE "root"."id" = $1::bigint AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::text::boolean ELSE true END))"#
    ));
}

#[test]
fn documents_query_keys_with_in_and_selects_the_key() {
    let schema = index(
        Some("id"),
        None,
        vec![col_field("id", "id"), col_field("email", "email")],
    );
    let (sql, params) = documents_query(
        &schema,
        &c("id"),
        &[GenericValue::Int(7), GenericValue::Int(9)],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT "root"."id" AS "doc_key", json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" IN ($1::bigint, $2::bigint)"#
    );
    assert_eq!(params, vec![GenericValue::Int(7), GenericValue::Int(9)]);
}

#[test]
fn documents_query_folds_soft_delete_into_where() {
    let schema = index(
        Some("id"),
        Some(SoftDelete::Column(SoftDeleteColumn {
            column: c("deleted_at"),
            when: None,
        })),
        vec![col_field("id", "id")],
    );
    let (sql, _) = documents_query(
        &schema,
        &c("id"),
        &[GenericValue::Int(1)],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert!(sql.as_str().contains(
        r#"WHERE "root"."id" IN ($1::bigint) AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::text::boolean ELSE true END))"#
    ));
}

#[test]
fn wide_document_chunks_into_jsonb_concatenation() {
    // Postgres caps a function call at 100 arguments; `json_build_object` spends
    // two per field, so a document past 50 fields must split into chunks stitched
    // with jsonb `||` rather than overflow a single call.
    let fields: Vec<Field> = (0..120)
        .map(|i| col_field(&format!("f{i}"), &format!("c{i}")))
        .collect();
    let schema = index(Some("id"), None, fields);
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    let sql = sql.as_str();
    // 120 fields → three chunks of 50/50/20, each its own `jsonb_build_object`.
    assert_eq!(sql.matches("jsonb_build_object(").count(), 3);
    assert_eq!(sql.matches(" || ").count(), 2);
    assert!(!sql.contains("json_build_object("));
    // No single call exceeds 100 arguments: 50 pairs = 100 args, 49 commas in
    // the longest chunk between the 50 key/value tokens.
    assert!(sql.contains("'f0', \"root\".\"c0\""));
    assert!(sql.contains("'f119', \"root\".\"c119\""));
}

#[test]
fn document_at_the_chunk_boundary_stays_a_single_call() {
    // Exactly 50 fields fits one `json_build_object` (100 args) — no chunking.
    let fields: Vec<Field> = (0..50)
        .map(|i| col_field(&format!("f{i}"), &format!("c{i}")))
        .collect();
    let schema = index(Some("id"), None, fields);
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    let sql = sql.as_str();
    assert_eq!(sql.matches("json_build_object(").count(), 1);
    assert!(!sql.contains("jsonb_build_object("));
    assert!(!sql.contains(" || "));
}

#[test]
fn reverse_query_selects_foreign_key() {
    let (sql, params) = reverse_query(
        &db(),
        &t("orders"),
        &c("user_id"),
        &[(c("id"), GenericValue::Int(9))],
        &types(&[("orders", "id", "bigint")]),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT "user_id" FROM "public"."orders" WHERE "id" = $1::bigint"#
    );
    assert_eq!(params, vec![GenericValue::Int(9)]);
}
