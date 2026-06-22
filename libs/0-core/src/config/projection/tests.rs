#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use crate::common::{ColumnName, FieldName, IndexName, TableName};
use crate::config::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Field, FieldSource, FlussoType,
    IndexSchema, Join, JoinKind, MappingType, Relation,
};

fn ids_schema(element_type: FlussoType) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("users").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("orderIds").unwrap(),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Aggregate(Aggregate {
                table: TableName::try_new("orders").unwrap(),
                op: AggregateOp::Ids { element_type },
                key: AggregateKey::Direct(crate::common::ColumnName::try_new("user_id").unwrap()),
                value_type: None,
                filters: None,
            })),
        }],
    }
}

fn belongs_to_schema(nullable: bool) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: TableName::try_new("tickets").unwrap(),
        db_schema: DatabaseSchema::default(),
        primary_key: None,
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: FieldName::try_new("author").unwrap(),
            options: Default::default(),
            source: FieldSource::Relation(Relation::Join(Join {
                table: TableName::try_new("users").unwrap(),
                kind: JoinKind::BelongsTo {
                    column: ColumnName::try_new("author_id").unwrap(),
                },
                primary_key: ColumnName::try_new("id").unwrap(),
                nullable,
                filters: None,
                order_by: None,
                limit: None,
                fields: vec![],
            })),
        }],
    }
}

#[test]
fn to_one_join_nullability_flows_into_the_mapping() {
    let mapping = belongs_to_schema(true).resolve(IndexName::try_new("tickets").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Object);
    assert!(field.nullable, "nullable to-one join → nullable object");

    let mapping = belongs_to_schema(false).resolve(IndexName::try_new("tickets").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Object);
    assert!(!field.nullable, "required to-one join → non-null object");
}

#[test]
fn ids_projects_to_a_non_null_element_typed_array() {
    let mapping = ids_schema(FlussoType::Long).resolve(IndexName::try_new("users").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Long);
    assert!(field.array);
    assert!(!field.nullable);

    let mapping = ids_schema(FlussoType::Keyword).resolve(IndexName::try_new("users").unwrap());
    let field = &mapping.fields[0];
    assert_eq!(field.mapping.mapping_type, MappingType::Keyword);
    assert!(field.array);
    assert!(!field.nullable);
}
