//! The root row's own document follows the changed-columns rule. A root-only
//! schema resolves without touching the database, so the pool is never opened.

use std::collections::BTreeMap;

use kernel::{Column, Field, FieldName, FieldSource, FlussoType, GenericValue};

use super::*;

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn users() -> TableName {
    TableName::try_new("users").unwrap()
}

fn builder() -> PgDocumentBuilder {
    let schema = IndexSchema {
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
