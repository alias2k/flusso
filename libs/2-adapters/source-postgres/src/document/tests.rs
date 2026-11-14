//! The root row's own document follows the changed-columns rule, a key with a
//! null part addresses no document, and a key carrying a hop's link names the
//! parent itself. These schemas resolve without touching the database, so the
//! pool is never opened: every key column is cached as `integer`, a type whose
//! equal values are identical, so no link is looked up on the root table.

use std::collections::BTreeMap;

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, Field, FieldName, FieldSource, FlussoType,
    GenericValue, Join, JoinKind, Relation, Through,
};

use super::*;

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn users() -> TableName {
    TableName::try_new("users").unwrap()
}

fn builder() -> PgDocumentBuilder {
    builder_with(Vec::new())
}

/// `users(id, email)` plus `extra` fields.
fn builder_with(extra: Vec<Field>) -> PgDocumentBuilder {
    let mut schema = IndexSchema {
        version: 1,
        table: users(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(column("id")),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("email").unwrap(),
            options: Default::default(),
            emit: true,
            source: FieldSource::Column(Column {
                column: column("email"),
                ty: FlussoType::Keyword,
                nullable: false,
                transforms: Vec::new(),
                default: None,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
                array: false,
            }),
        }],
    };
    schema.fields.extend(extra);
    let spec = SourceSpec::new(BTreeMap::from([(
        IndexName::try_new("users").unwrap(),
        schema,
    )]));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@localhost/unused")
        .unwrap();
    let builder = PgDocumentBuilder::new(pool, Arc::new(spec));
    seed_integer_columns(
        &builder,
        &[
            ("users", "id"),
            ("users", "parent_id"),
            ("orders", "user_id"),
            ("events", "user_id"),
            ("user_tags", "user_id"),
        ],
    );
    builder
}

/// Cache `integer` as the type of each `(table, column)`, as the catalog would
/// report it, so resolving reads no catalog either.
fn seed_integer_columns(builder: &PgDocumentBuilder, columns: &[(&str, &str)]) {
    let mut cache = builder.col_type_cache.lock();
    for (table, column) in columns {
        cache.insert(
            (
                "public".to_owned(),
                (*table).to_owned(),
                (*column).to_owned(),
            ),
            ColumnMeta {
                sql_type: "integer".to_owned(),
                base_type: None,
                base_name: "int4".to_owned(),
                element_name: None,
                deterministic: true,
                nullable: true,
            },
        );
    }
}

fn key() -> RowKey {
    RowKey(vec![(column("id"), GenericValue::Int(1))])
}

fn image(email: &str, last_seen: i64) -> RowImage {
    RowImage(vec![
        (column("id"), GenericValue::Int(1)),
        (column("email"), GenericValue::String(email.into())),
        (column("last_seen"), GenericValue::BigInt(last_seen)),
    ])
}

#[tokio::test]
async fn a_root_update_of_unread_columns_resolves_to_nothing() {
    let ids = builder()
        .resolve(
            &users(),
            &key(),
            Some(&image("a@x", 1)),
            Some(&image("a@x", 2)),
        )
        .await
        .unwrap();
    assert!(ids.is_empty(), "{ids:?}");
}

#[tokio::test]
async fn a_root_update_of_a_read_column_resolves_to_its_own_document() {
    let ids = builder()
        .resolve(
            &users(),
            &key(),
            Some(&image("a@x", 1)),
            Some(&image("b@x", 1)),
        )
        .await
        .unwrap();
    assert_eq!(ids.len(), 1);
}

#[tokio::test]
async fn a_root_change_without_both_images_resolves_to_its_own_document() {
    let builder = builder();
    for (before, after) in [
        (None, Some(image("a@x", 2))),
        (Some(image("a@x", 1)), None),
        (None, None),
    ] {
        let ids = builder
            .resolve(&users(), &key(), before.as_ref(), after.as_ref())
            .await
            .unwrap();
        assert_eq!(ids.len(), 1);
    }
}

/// `has_many: orders` on `orders.user_id`.
fn orders() -> Field {
    Field {
        field: FieldName::try_new("orders").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("orders").unwrap(),
            kind: JoinKind::HasMany {
                foreign_key: column("user_id"),
            },
            primary_key: column("id"),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: Vec::new(),
            indexed: true,
            on: vec![],
        })),
    }
}

fn null_key() -> RowKey {
    RowKey(vec![(column("id"), GenericValue::Null)])
}

#[tokio::test]
async fn a_root_change_with_a_null_key_addresses_no_document() {
    let builder = builder();
    let before = RowImage(vec![
        (column("id"), GenericValue::Null),
        (column("email"), GenericValue::String("a@x".into())),
    ]);
    for before in [None, Some(&before)] {
        let ids = builder
            .resolve(&users(), &null_key(), before, None)
            .await
            .unwrap();
        assert!(ids.is_empty(), "{ids:?}");
    }
}

/// A null key used to render as the `_id` `"null"`, the very id of a row whose
/// text key is the string `"null"`: only that row's own change may reach it.
#[tokio::test]
async fn a_null_key_never_reaches_the_document_keyed_by_the_string_null() {
    let builder = builder();
    let ids = builder
        .resolve(&users(), &null_key(), None, None)
        .await
        .unwrap();
    assert!(ids.iter().all(|id| id.rendered() != "null"), "{ids:?}");
    assert!(ids.is_empty(), "{ids:?}");

    let text_null = RowKey(vec![(column("id"), GenericValue::String("null".into()))]);
    let ids = builder
        .resolve(&users(), &text_null, None, None)
        .await
        .unwrap();
    assert_eq!(
        ids.iter().map(DocumentId::rendered).collect::<Vec<_>>(),
        ["null"]
    );
}

#[tokio::test]
async fn a_child_change_with_a_null_key_still_rebuilds_the_parent_its_old_row_names() {
    let builder = builder_with(vec![orders()]);
    let orders = TableName::try_new("orders").unwrap();
    let before = RowImage(vec![
        (column("id"), GenericValue::Null),
        (column("user_id"), GenericValue::Int(77)),
    ]);
    let ids = builder
        .resolve(&orders, &null_key(), Some(&before), None)
        .await
        .unwrap();
    assert_eq!(
        ids,
        [DocumentId {
            index: IndexName::try_new("users").unwrap(),
            key: RowKey(vec![(column("id"), GenericValue::Int(77))]),
        }]
    );

    let ids = builder
        .resolve(&orders, &null_key(), None, None)
        .await
        .unwrap();
    assert!(ids.is_empty(), "nothing names a parent: {ids:?}");
}

fn users_id(id: i32) -> DocumentId {
    DocumentId {
        index: IndexName::try_new("users").unwrap(),
        key: RowKey(vec![(column("id"), GenericValue::Int(id))]),
    }
}

fn join_field(name: &str, table: &str, kind: JoinKind) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new(table).unwrap(),
            kind,
            primary_key: column("id"),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields: Vec::new(),
            indexed: true,
            on: vec![],
        })),
    }
}

/// `count: eventCount` over `events.user_id`, a table without a primary key.
fn event_count() -> Field {
    Field {
        field: FieldName::try_new("eventCount").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: TableName::try_new("events").unwrap(),
            op: AggregateOp::Count,
            key: AggregateKey::Direct(column("user_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: vec![],
        })),
    }
}

/// An `events` row keyed the way a table without a primary key is under
/// `REPLICA IDENTITY FULL`: by every column.
fn event(user_id: i32, note: Option<&str>) -> RowKey {
    RowKey(vec![
        (column("user_id"), GenericValue::Int(user_id)),
        (column("kind"), GenericValue::String("click".into())),
        (
            column("note"),
            note.map_or(GenericValue::Null, |note| GenericValue::String(note.into())),
        ),
    ])
}

#[tokio::test]
async fn a_keyless_child_row_with_a_null_column_resolves_to_its_parent() {
    let builder = builder_with(vec![event_count()]);
    let events = TableName::try_new("events").unwrap();
    for key in [event(77, None), event(77, Some("x"))] {
        let ids = builder.resolve(&events, &key, None, None).await.unwrap();
        assert_eq!(ids, [users_id(77)], "insert {key:?}");
    }
}

/// A re-parenting update of a keyless row changes its key, so the stream emits
/// a delete of the old key and an upsert of the new one; a delete logs no
/// pre-image beyond that key. Each names its own parent.
#[tokio::test]
async fn a_keyless_child_resolves_a_reparent_and_a_delete_from_its_keys() {
    let builder = builder_with(vec![event_count()]);
    let events = TableName::try_new("events").unwrap();
    let old = builder
        .resolve(&events, &event(1, None), None, None)
        .await
        .unwrap();
    let new = builder
        .resolve(&events, &event(2, None), None, None)
        .await
        .unwrap();
    assert_eq!((old, new), (vec![users_id(1)], vec![users_id(2)]));

    let before = RowImage(event(3, None).0);
    let deleted = builder
        .resolve(&events, &event(3, None), Some(&before), None)
        .await
        .unwrap();
    assert_eq!(deleted, [users_id(3)]);
}

#[tokio::test]
async fn a_keyless_child_row_with_a_null_link_names_no_parent() {
    let builder = builder_with(vec![event_count()]);
    let events = TableName::try_new("events").unwrap();
    let key = RowKey(vec![
        (column("user_id"), GenericValue::Null),
        (column("kind"), GenericValue::String("click".into())),
    ]);
    let ids = builder.resolve(&events, &key, None, None).await.unwrap();
    assert!(ids.is_empty(), "{ids:?}");
}

#[tokio::test]
async fn a_junction_key_names_its_parent_by_left_key_alone() {
    let builder = builder_with(vec![join_field(
        "tags",
        "tags",
        JoinKind::ManyToMany {
            through: Through {
                table: TableName::try_new("user_tags").unwrap(),
                left_key: column("user_id"),
                right_key: column("tag_id"),
                filters: None,
            },
        },
    )]);
    let user_tags = TableName::try_new("user_tags").unwrap();
    let resolve = async |user_id: GenericValue, tag_id: GenericValue| {
        let key = RowKey(vec![
            (column("user_id"), user_id),
            (column("tag_id"), tag_id),
        ]);
        builder.resolve(&user_tags, &key, None, None).await.unwrap()
    };
    assert_eq!(
        resolve(GenericValue::Int(7), GenericValue::Null).await,
        [users_id(7)],
        "a null in the other part still names the parent"
    );
    assert!(
        resolve(GenericValue::Null, GenericValue::Int(3))
            .await
            .is_empty(),
        "a null `left_key` links no parent"
    );
}

#[tokio::test]
async fn a_belongs_to_target_with_a_null_key_resolves_nothing_without_a_query() {
    let builder = builder_with(vec![join_field(
        "team",
        "teams",
        JoinKind::BelongsTo {
            column: column("team_id"),
        },
    )]);
    let teams = TableName::try_new("teams").unwrap();
    let before = RowImage(vec![
        (column("id"), GenericValue::Null),
        (column("name"), GenericValue::String("core".into())),
    ]);
    for before in [None, Some(&before)] {
        let ids = builder
            .resolve(&teams, &null_key(), before, None)
            .await
            .unwrap();
        assert!(ids.is_empty(), "{ids:?}");
    }
}

#[tokio::test]
async fn a_self_referencing_row_with_a_null_key_rebuilds_the_parent_its_old_row_names() {
    let builder = builder_with(vec![join_field(
        "children",
        "users",
        JoinKind::HasMany {
            foreign_key: column("parent_id"),
        },
    )]);
    let before = RowImage(vec![
        (column("id"), GenericValue::Null),
        (column("parent_id"), GenericValue::Int(5)),
    ]);
    let ids = builder
        .resolve(&users(), &null_key(), Some(&before), None)
        .await
        .unwrap();
    assert_eq!(ids, [users_id(5)], "the old parent, never the row itself");
}

/// Only a key whose equal values can render differently has its links read
/// back from the root table (issue #295).
#[test]
fn equal_keys_render_alike_except_for_numbers_and_loose_text() {
    let meta = |base_name: &str, deterministic: bool| ColumnMeta {
        sql_type: base_name.to_owned(),
        base_type: None,
        base_name: base_name.to_owned(),
        element_name: None,
        deterministic,
        nullable: false,
    };
    for exact in [
        "int4",
        "int8",
        "uuid",
        "bytea",
        "date",
        "timestamptz",
        "text",
        "bpchar",
    ] {
        assert!(meta(exact, true).equal_is_identical(), "{exact}");
    }
    for loose in ["numeric", "float4", "float8", "citext"] {
        assert!(!meta(loose, true).equal_is_identical(), "{loose}");
    }
    assert!(!meta("text", false).equal_is_identical());
}
