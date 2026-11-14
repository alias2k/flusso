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
        emit: true,
        source: FieldSource::Column(kernel::Column {
            column: c(column),
            ty: kernel::FlussoType::Keyword,
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
    // Both the single-key and batched (`IN`) forms cast to the column's type.
    let schema = index(Some("id"), None, vec![col_field("id", "id")]);
    let col_types = types(&[("users", "id", "uuid")]);
    let key = GenericValue::Uuid("3f2a1b9c-0000-0000-0000-000000000000".parse().unwrap());

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
        emit: true,
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
            indexed: true,
            on: vec![],
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
        emit: true,
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
            indexed: true,
            on: vec![],
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
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("orders"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
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
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("orders"),
            op: AggregateOp::Ids {
                element_type: kernel::FlussoType::Long,
            },
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
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
        emit: true,
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
            via: Vec::new(),
            on: vec![],
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
            AggregateOp::Sum(c("weight").into()),
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
            AggregateOp::Sum(c("weight").into()),
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
        emit: true,
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
            indexed: true,
            on: vec![],
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
        r#"SELECT format('%s', "root"."id") AS "doc_key", "root"."id" AS "doc_key:typed", json_build_object('id', "root"."id", 'email', "root"."email") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" IN ($1::bigint, $2::bigint)"#
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

/// A `soft_delete` `field` projected only inside a relation still reads that
/// column on the root row: the predicate names `root."deleted_at"`.
#[test]
fn soft_delete_field_inside_a_relation_reads_the_root_column() {
    let orders = Field {
        field: f("orders"),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: t("orders"),
            kind: JoinKind::HasMany {
                foreign_key: c("user_id"),
            },
            primary_key: c("id"),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![col_field("id", "id"), col_field("deleted_at", "deleted_at")],
            indexed: true,
            on: vec![],
        })),
    };
    let schema = index(
        Some("id"),
        Some(SoftDelete::Field(kernel::SoftDeleteField {
            field: f("deleted_at"),
            when: None,
        })),
        vec![col_field("id", "id"), orders],
    );
    let mut pks = HashMap::new();
    pks.insert("orders".to_owned(), c("id"));
    let (sql, _) = documents_query(
        &schema,
        &c("id"),
        &[GenericValue::Int(1)],
        &pks,
        &id_types(),
    )
    .unwrap();
    assert!(
        sql.as_str().ends_with(
            r#"WHERE "root"."id" IN ($1::bigint) AND NOT ((CASE WHEN "root"."deleted_at" IS NULL THEN false WHEN pg_typeof("root"."deleted_at") = 'boolean'::regtype THEN "root"."deleted_at"::text::boolean ELSE true END))"#
        ),
        "{}",
        sql.as_str()
    );
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

/// A key its type's parser can't represent stays text and is cast from text,
/// a statement of its own (issue #295): bound where a typed key was
/// prepared, sqlx would send it in the wrong format.
#[test]
fn a_key_carried_as_text_is_cast_from_text() {
    let schema = index(Some("id"), None, vec![col_field("id", "id")]);
    let col_types = types(&[("users", "id", "date")]);
    let key = GenericValue::String("infinity".to_owned());

    let (single, _) = document_query(
        &schema,
        &[(c("id"), key.clone())],
        &HashMap::new(),
        &col_types,
    )
    .unwrap();
    assert!(
        single
            .as_str()
            .ends_with(r#"WHERE "root"."id" = $1::text::date"#),
        "{}",
        single.as_str()
    );
    let (batched, _) = documents_query(
        &schema,
        &c("id"),
        &[key.clone()],
        &HashMap::new(),
        &col_types,
    )
    .unwrap();
    assert!(
        batched
            .as_str()
            .ends_with(r#"WHERE "root"."id" IN ($1::text::date)"#),
        "{}",
        batched.as_str()
    );
    let (reverse, _) =
        reverse_query(&db(), &t("users"), &c("id"), &[(c("id"), key)], &col_types).unwrap();
    assert!(
        reverse.as_str().ends_with(r#"WHERE "id" = $1::text::date"#),
        "{}",
        reverse.as_str()
    );
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
        r#"SELECT format('%s', "user_id") AS "user_id", "user_id" AS "user_id:typed" FROM "public"."orders" WHERE "id" = $1::bigint"#
    );
    assert_eq!(params, vec![GenericValue::Int(9)]);
}

fn eq(column: &str, value: &str) -> Filter {
    Filter::ValueOp(ValueOpFilter {
        column: c(column),
        op: FilterOp::Eq,
        value: FilterValue::Single(value.to_owned()),
    })
}

fn hop(table: &str, foreign_key: &str, filters: Option<Vec<Filter>>) -> kernel::ViaHop {
    kernel::ViaHop {
        table: t(table),
        foreign_key: c(foreign_key),
        filters,
        on: vec![],
    }
}

/// A `users` document with one aggregate field `agg`, rendered with every
/// table keyed by `id`.
fn aggregate_sql(aggregate: Aggregate) -> String {
    let field = Field {
        field: f("agg"),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(aggregate)),
    };
    let schema = index(Some("id"), None, vec![field]);
    let pks = ["teams", "projects", "tasks", "tags"]
        .into_iter()
        .map(|table| (table.to_owned(), c("id")))
        .collect();
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &types(&[
            ("users", "id", "bigint"),
            ("projects", "archived", "boolean"),
            ("tasks", "status", "text"),
        ]),
    )
    .unwrap();
    sql.as_str().to_owned()
}

#[test]
fn exists_on_one_hop_is_an_exists_subquery() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Exists,
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
        }),
        wrap(
            r#"(EXISTS (SELECT 1 FROM "public"."tasks" AS "rel1" WHERE "rel1"."user_id" = "root"."id"))"#
        ),
    );
}

#[test]
fn an_aggregate_via_one_hop_matches_the_hop_s_rows_of_the_parent() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Exists,
            key: AggregateKey::Direct(c("project_id")),
            value_type: None,
            filters: Some(vec![eq("status", "completed")]),
            distinct: false,
            via: vec![hop("projects", "user_id", Some(vec![eq("archived", "false")]))],
            on: vec![],
        }),
        wrap(
            r#"(EXISTS (SELECT 1 FROM "public"."tasks" AS "rel2" WHERE "rel2"."project_id" IN (SELECT "rel1"."id" FROM "public"."projects" AS "rel1" WHERE "rel1"."user_id" = "root"."id" AND ("rel1"."archived" = $1::boolean)) AND ("rel2"."status" = $2::text)))"#
        )
        .replace("\"root\".\"id\" = $1::bigint", "\"root\".\"id\" = $3::bigint"),
    );
}

#[test]
fn an_aggregate_via_two_hops_nests_one_level_per_hop() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(c("project_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![
                hop("teams", "owner_id", None),
                hop("projects", "team_id", None)
            ],
            on: vec![],
        }),
        wrap(
            r#"(SELECT count(*) FROM "public"."tasks" AS "rel3" WHERE "rel3"."project_id" IN (SELECT "rel2"."id" FROM "public"."projects" AS "rel2" WHERE "rel2"."team_id" IN (SELECT "rel1"."id" FROM "public"."teams" AS "rel1" WHERE "rel1"."owner_id" = "root"."id")))"#
        ),
    );
}

#[test]
fn a_junction_after_via_hops_ties_its_left_key_to_the_last_hop() {
    let through = AggregateKey::Through(kernel::Through {
        table: t("task_tags"),
        left_key: c("task_id"),
        right_key: c("tag_id"),
        filters: None,
    });
    let via = vec![
        hop("projects", "user_id", None),
        hop("tasks", "project_id", None),
    ];
    let aggregate = |distinct| Aggregate {
        table: t("tags"),
        op: AggregateOp::Count,
        key: through.clone(),
        value_type: None,
        filters: None,
        distinct,
        via: via.clone(),
        on: vec![],
    };
    let chain = r#"IN (SELECT "rel2"."id" FROM "public"."tasks" AS "rel2" WHERE "rel2"."project_id" IN (SELECT "rel1"."id" FROM "public"."projects" AS "rel1" WHERE "rel1"."user_id" = "root"."id"))"#;
    assert_eq!(
        aggregate_sql(aggregate(false)),
        wrap(&format!(
            r#"(SELECT count(*) FROM "public"."tags" AS "rel3" JOIN "public"."task_tags" AS "rel4" ON "rel4"."tag_id" = "rel3"."id" WHERE "rel4"."task_id" {chain})"#
        )),
    );
    assert_eq!(
        aggregate_sql(aggregate(true)),
        wrap(&format!(
            r#"(SELECT count(*) FROM "public"."tags" AS "rel3" WHERE "rel3"."id" IN (SELECT "rel4"."tag_id" FROM "public"."task_tags" AS "rel4" WHERE "rel4"."task_id" {chain}))"#
        )),
    );
}

#[test]
fn exists_over_a_junction_wraps_the_junction_subquery() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tags"),
            op: AggregateOp::Exists,
            key: AggregateKey::Through(kernel::Through {
                table: t("task_tags"),
                left_key: c("task_id"),
                right_key: c("tag_id"),
                filters: None,
            }),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![hop("tasks", "user_id", None)],
            on: vec![],
        }),
        wrap(
            r#"(EXISTS (SELECT 1 FROM "public"."tags" AS "rel2" JOIN "public"."task_tags" AS "rel3" ON "rel3"."tag_id" = "rel2"."id" WHERE "rel3"."task_id" IN (SELECT "rel1"."id" FROM "public"."tasks" AS "rel1" WHERE "rel1"."user_id" = "root"."id")))"#
        ),
    );
}

fn pair(local: &str, related: &str) -> kernel::ColumnPair {
    kernel::ColumnPair {
        local: c(local),
        related: c(related),
    }
}

fn join_field(
    name: &str,
    table: &str,
    kind: JoinKind,
    on: Vec<kernel::ColumnPair>,
    fields: Vec<Field>,
) -> Field {
    Field {
        field: f(name),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: t(table),
            kind,
            primary_key: c("id"),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields,
            indexed: true,
            on,
        })),
    }
}

#[test]
fn on_pairs_join_every_join_kind_to_its_parent_row_at_any_depth() {
    let shop = join_field(
        "shop",
        "shops",
        JoinKind::BelongsTo {
            column: c("shop_id"),
        },
        vec![pair("region", "region_code")],
        vec![col_field("name", "name")],
    );
    let invoice = join_field(
        "invoice",
        "invoices",
        JoinKind::HasOne {
            foreign_key: c("order_id"),
        },
        vec![pair("tenant_id", "tenant_id")],
        vec![col_field("number", "number")],
    );
    let orders = join_field(
        "orders",
        "orders",
        JoinKind::HasMany {
            foreign_key: c("user_id"),
        },
        vec![pair("tenant_id", "tenant_id"), pair("region", "region")],
        vec![shop, invoice],
    );
    let schema = index(Some("id"), None, vec![orders]);
    let pks = ["orders", "shops", "invoices"]
        .into_iter()
        .map(|table| (table.to_owned(), c("id")))
        .collect();
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('orders', (SELECT coalesce(json_agg(json_build_object('shop', (SELECT json_build_object('name', "rel2"."name") FROM "public"."shops" AS "rel2" WHERE "rel2"."id" = "rel1"."shop_id" AND "rel2"."region_code" = "rel1"."region" LIMIT 1), 'invoice', (SELECT json_build_object('number', "rel3"."number") FROM "public"."invoices" AS "rel3" WHERE "rel3"."order_id" = "rel1"."id" AND "rel3"."tenant_id" = "rel1"."tenant_id" LIMIT 1))), '[]'::json) FROM (SELECT "rel4".* FROM "public"."orders" AS "rel4" WHERE "rel4"."user_id" = "root"."id" AND "rel4"."tenant_id" = "root"."tenant_id" AND "rel4"."region" = "root"."region") AS "rel1")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

#[test]
fn on_pairs_join_a_direct_aggregate_to_its_parent_row() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![pair("tenant_id", "tenant_id")],
        }),
        wrap(
            r#"(SELECT count(*) FROM "public"."tasks" AS "rel1" WHERE "rel1"."user_id" = "root"."id" AND "rel1"."tenant_id" = "root"."tenant_id")"#
        ),
    );
}

#[test]
fn on_pairs_across_via_select_each_level_s_local_columns() {
    let mut teams = hop("teams", "owner_id", None);
    teams.on = vec![pair("tenant_id", "tenant_id")];
    let projects = hop("projects", "team_id", None);
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(c("project_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![teams, projects],
            on: vec![pair("tenant_id", "tenant_id"), pair("team_id", "team_id")],
        }),
        wrap(
            r#"(SELECT count(*) FROM "public"."tasks" AS "rel3" WHERE ("rel3"."project_id", "rel3"."tenant_id", "rel3"."team_id") IN (SELECT "rel2"."id", "rel2"."tenant_id", "rel2"."team_id" FROM "public"."projects" AS "rel2" WHERE "rel2"."team_id" IN (SELECT "rel1"."id" FROM "public"."teams" AS "rel1" WHERE "rel1"."owner_id" = "root"."id" AND "rel1"."tenant_id" = "root"."tenant_id")))"#
        ),
    );
}

fn expr(input: &str) -> kernel::Expression {
    input.parse().unwrap()
}

#[test]
fn an_expression_renders_qualified_quoted_and_fully_grouped() {
    use super::sql::arithmetic;
    for (input, sql) in [
        ("amount", r#""q"."amount""#),
        (
            "quantity * unit_price",
            r#"("q"."quantity" * "q"."unit_price")"#,
        ),
        ("a + b * c", r#"("q"."a" + ("q"."b" * "q"."c"))"#),
        ("(a + b) * c", r#"(("q"."a" + "q"."b") * "q"."c")"#),
        ("a - b - c", r#"(("q"."a" - "q"."b") - "q"."c")"#),
        ("a - (b - c)", r#"("q"."a" - ("q"."b" - "q"."c"))"#),
        (
            "a / 2 / 3.5",
            r#"(("q"."a" / NULLIF(2, 0)) / NULLIF(3.5, 0))"#,
        ),
        (
            "total / quantity",
            r#"("q"."total" / NULLIF("q"."quantity", 0))"#,
        ),
        (
            "a / (b - c)",
            r#"("q"."a" / NULLIF(("q"."b" - "q"."c"), 0))"#,
        ),
        ("-a * b", r#"((-"q"."a") * "q"."b")"#),
        ("a - -b", r#"("q"."a" - (-"q"."b"))"#),
        ("--a", r#"(-(-"q"."a"))"#),
        ("-(a + 1)", r#"(-("q"."a" + 1))"#),
        (
            "Quantity*Unit_Price",
            r#"("q"."quantity" * "q"."unit_price")"#,
        ),
    ] {
        assert_eq!(arithmetic(&expr(input), "q"), sql, "{input}");
    }
}

#[test]
fn every_folding_op_wraps_the_expression() {
    for (op, name) in [
        (AggregateOp::Sum as fn(_) -> _, "sum"),
        (AggregateOp::Avg, "avg"),
        (AggregateOp::Min, "min"),
        (AggregateOp::Max, "max"),
    ] {
        assert_eq!(
            aggregate_sql(Aggregate {
                table: t("tasks"),
                op: op(expr("hours * rate")),
                key: AggregateKey::Direct(c("user_id")),
                value_type: None,
                filters: None,
                distinct: false,
                via: Vec::new(),
                on: vec![],
            }),
            wrap(&format!(
                r#"(SELECT {name}(("rel1"."hours" * "rel1"."rate")) FROM "public"."tasks" AS "rel1" WHERE "rel1"."user_id" = "root"."id")"#
            )),
        );
    }
}

#[test]
fn an_expression_reads_the_aggregated_table_through_via_and_through() {
    assert_eq!(
        aggregate_sql(Aggregate {
            table: t("tasks"),
            op: AggregateOp::Sum(expr("hours * rate")),
            key: AggregateKey::Direct(c("project_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![hop("projects", "user_id", None)],
            on: vec![],
        }),
        wrap(
            r#"(SELECT sum(("rel2"."hours" * "rel2"."rate")) FROM "public"."tasks" AS "rel2" WHERE "rel2"."project_id" IN (SELECT "rel1"."id" FROM "public"."projects" AS "rel1" WHERE "rel1"."user_id" = "root"."id"))"#
        ),
    );
    assert_eq!(
        through_aggregate_sql(AggregateOp::Sum(expr("weight * 2")), false, None, None),
        wrap(
            r#"(SELECT sum(("rel1"."weight" * 2)) FROM "public"."products" AS "rel1" JOIN "public"."order_items" AS "rel2" ON "rel2"."product_id" = "rel1"."id" WHERE "rel2"."order_id" = "root"."id")"#
        ),
    );
}

fn array_field(
    name: &str,
    transforms: Vec<kernel::Transform>,
    default: Option<GenericValue>,
) -> Field {
    let mut field = col_field(name, name);
    if let FieldSource::Column(column) = &mut field.source {
        column.array = true;
        column.transforms = transforms;
        column.default = default;
    }
    field
}

#[test]
fn an_array_column_without_transforms_reads_the_column_and_leaves_its_default_to_the_decoder() {
    let schema = index(
        Some("id"),
        None,
        vec![
            col_field("id", "id"),
            array_field(
                "tags",
                Vec::new(),
                Some(GenericValue::Array(vec![GenericValue::String(
                    "general".into(),
                )])),
            ),
        ],
    );
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('id', "root"."id", 'tags', "root"."tags") AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
    );
}

#[test]
fn an_array_column_applies_its_transforms_to_each_string_element() {
    let schema = index(
        Some("id"),
        None,
        vec![array_field(
            "tags",
            vec![kernel::Transform::Trim, kernel::Transform::Lowercase],
            None,
        )],
    );
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(7))],
        &HashMap::new(),
        &id_types(),
    )
    .unwrap();
    let column = r#""root"."tags""#;
    let value = format!("to_jsonb({column})");
    let expected = format!(
        "CASE WHEN coalesce(jsonb_typeof({value}), 'null') = 'null' THEN NULL ELSE coalesce((SELECT jsonb_agg(CASE WHEN \
         jsonb_typeof(e.v) = 'string' THEN to_jsonb(lower(trim(e.v #>> '{{}}'))) ELSE e.v END \
         ORDER BY e.o) FROM jsonb_array_elements(CASE jsonb_typeof({value}) WHEN 'array' THEN \
         {value} ELSE jsonb_build_array({value}) END) WITH ORDINALITY AS e(v, o)), '[]'::jsonb) END"
    );
    assert!(
        sql.as_str().contains(&format!("'tags', {expected})")),
        "{}",
        sql.as_str()
    );
}

/// The root-key lookup names each link's bound type, so links of different
/// types never share a prepared statement.
#[test]
fn root_keys_query_casts_each_link_from_its_bound_type() {
    let (sql, params) = root_keys_query(
        &db(),
        &t("users"),
        &c("id"),
        &[
            GenericValue::Decimal("1.500".parse().unwrap()),
            GenericValue::String("NaN".to_owned()),
        ],
        "numeric",
    )
    .unwrap();
    assert_eq!(
        sql.as_str(),
        r#"SELECT format('%s', "id") AS "id", "id" AS "id:typed" FROM "public"."users" WHERE "id" IN ($1::numeric::numeric, $2::text::numeric)"#
    );
    assert_eq!(params.len(), 2);
}
