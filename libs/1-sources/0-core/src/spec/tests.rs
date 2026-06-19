use std::collections::BTreeMap;

use schema_core::{
    Column, DatabaseSchema, Field, FieldSource, FlussoType, IndexName, IndexSchema, Join, JoinKind,
    Relation, TableName, Through,
};

use super::{QualifiedTable, SourceSpec};

fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn column_field(name: &str) -> Field {
    Field {
        field: schema_core::FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: schema_core::ColumnName::try_new(name).unwrap(),
            ty: FlussoType::Keyword,
            nullable: false,
            transforms: Vec::new(),
            default: None,
        }),
    }
}

/// A one-column schema over `public.<table>`, enough to resolve a mapping.
fn schema(table: &str) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: schema_core::TableName::try_new(table).unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(schema_core::ColumnName::try_new("id").unwrap()),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: schema_core::FieldName::try_new("id").unwrap(),
            options: Default::default(),
            source: FieldSource::Column(Column {
                column: schema_core::ColumnName::try_new("id").unwrap(),
                ty: FlussoType::Keyword,
                nullable: false,
                transforms: Vec::new(),
                default: None,
            }),
        }],
    }
}

#[test]
fn accessors_expose_indexes_in_name_order() {
    let mut indexes = BTreeMap::new();
    indexes.insert(index_name("b"), schema("bees"));
    indexes.insert(index_name("a"), schema("ants"));
    let spec = SourceSpec::new(indexes);

    let names: Vec<&str> = spec.indexes().map(|(name, _)| name.as_ref()).collect();
    assert_eq!(names, ["a", "b"]);
    assert!(spec.schema(&index_name("a")).is_some());
    assert!(spec.schema(&index_name("missing")).is_none());

    let mappings = spec.index_mappings();
    assert_eq!(mappings.len(), 2);
    assert_eq!(mappings.first().unwrap().index.as_ref(), "a");
}

#[test]
fn all_tables_collects_roots_relations_and_junctions() {
    // `books` over public.books with a has_many join to `reviews` and a
    // many_to_many to `tags` through the `book_tags` junction.
    let mut books = schema("books");
    books.fields.push(Field {
        field: schema_core::FieldName::try_new("reviews").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("reviews").unwrap(),
            kind: JoinKind::HasMany {
                foreign_key: schema_core::ColumnName::try_new("book_id").unwrap(),
            },
            primary_key: schema_core::ColumnName::try_new("id").unwrap(),
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("body")],
        })),
    });
    books.fields.push(Field {
        field: schema_core::FieldName::try_new("tags").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("tags").unwrap(),
            kind: JoinKind::ManyToMany {
                through: Through {
                    table: TableName::try_new("book_tags").unwrap(),
                    left_key: schema_core::ColumnName::try_new("book_id").unwrap(),
                    right_key: schema_core::ColumnName::try_new("tag_id").unwrap(),
                },
            },
            primary_key: schema_core::ColumnName::try_new("id").unwrap(),
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("name")],
        })),
    });

    let mut indexes = BTreeMap::new();
    indexes.insert(index_name("books"), books);
    // A second index sharing no tables, to prove the set spans all indexes.
    indexes.insert(index_name("ants"), schema("ants"));
    let spec = SourceSpec::new(indexes);

    let public = DatabaseSchema::try_new("public").unwrap();
    let qt = |t: &str| QualifiedTable::new(public.clone(), TableName::try_new(t).unwrap());
    let tables = spec.all_tables();

    assert_eq!(
        tables,
        [
            qt("ants"),
            qt("book_tags"),
            qt("books"),
            qt("reviews"),
            qt("tags"),
        ]
        .into_iter()
        .collect()
    );
}
