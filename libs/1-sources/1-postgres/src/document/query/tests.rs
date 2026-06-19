//! Example-based tests over the entry queries: fixed schema shapes asserted
//! against their exact generated SQL.
#![allow(clippy::unwrap_used)]

use schema_core::{
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
fn f(n: &str) -> schema_core::FieldName {
    schema_core::FieldName::try_new(n).unwrap()
}
fn col_field(name: &str, column: &str) -> Field {
    Field {
        field: f(name),
        options: Default::default(),
        source: FieldSource::Column(schema_core::Column {
            column: c(column),
            ty: schema_core::FlussoType::Keyword,
            nullable: true,
            transforms: Vec::new(),
            default: None,
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
                element_type: schema_core::FlussoType::Long,
            },
            key: AggregateKey::Direct(c("user_id")),
            value_type: None,
            filters: None,
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

#[test]
fn aggregate_ids_through_collects_off_the_junction() {
    let ids = Field {
        field: f("tag_ids"),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: t("tags"),
            op: AggregateOp::Ids {
                element_type: schema_core::FlussoType::Long,
            },
            key: AggregateKey::Through(schema_core::Through {
                table: t("post_tags"),
                left_key: c("post_id"),
                right_key: c("tag_id"),
            }),
            value_type: None,
            filters: None,
        })),
    };
    let schema = index(Some("id"), None, vec![ids]);
    let mut pks = HashMap::new();
    pks.insert("tags".to_owned(), c("id"));
    let (sql, _) = document_query(
        &schema,
        &[(c("id"), GenericValue::Int(1))],
        &pks,
        &id_types(),
    )
    .unwrap();
    // The junction's right_key already holds the far PK, so no join to `tags`.
    assert_eq!(
        sql.as_str(),
        r#"SELECT json_build_object('tag_ids', (SELECT coalesce(json_agg("rel1"."tag_id"), '[]'::json) FROM "public"."post_tags" AS "rel1" WHERE "rel1"."post_id" = "root"."id")) AS "document" FROM "public"."users" AS "root" WHERE "root"."id" = $1::bigint"#
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
