#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use crate::common::{FieldName, IndexName, TableName};
use crate::config::{
    Aggregate, AggregateKey, AggregateOp, DatabaseSchema, Field, FieldSource, FlussoType,
    IndexSchema, MappingType, Relation,
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
