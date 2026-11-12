//! The root row's own document follows the changed-columns rule, and a key
//! with a null part addresses no document. These schemas resolve without
//! touching the database, so the pool is never opened.

use std::collections::BTreeMap;

use kernel::{
    Column, Field, FieldName, FieldSource, FlussoType, GenericValue, Join, JoinKind, Relation,
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
    PgDocumentBuilder::new(pool, Arc::new(spec))
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
