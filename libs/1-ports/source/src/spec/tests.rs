use std::collections::BTreeMap;

use kernel::{
    Column, ColumnName, DatabaseSchema, Field, FieldSource, FlussoType, IndexName, IndexSchema,
    Join, JoinKind, Relation, TableName, Through,
};

use super::{QualifiedTable, SourceSpec};

fn index_name(name: &str) -> IndexName {
    IndexName::try_new(name).unwrap()
}

fn column_field(name: &str) -> Field {
    Field {
        field: kernel::FieldName::try_new(name).unwrap(),
        options: Default::default(),
        source: FieldSource::Column(Column {
            column: kernel::ColumnName::try_new(name).unwrap(),
            ty: FlussoType::Keyword,
            nullable: false,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
        }),
    }
}

/// A one-column schema over `public.<table>`, enough to resolve a mapping.
fn schema(table: &str) -> IndexSchema {
    IndexSchema {
        version: 1,
        table: kernel::TableName::try_new(table).unwrap(),
        db_schema: DatabaseSchema::try_new("public").unwrap(),
        primary_key: Some(kernel::ColumnName::try_new("id").unwrap()),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields: vec![Field {
            field: kernel::FieldName::try_new("id").unwrap(),
            options: Default::default(),
            source: FieldSource::Column(Column {
                column: kernel::ColumnName::try_new("id").unwrap(),
                ty: FlussoType::Keyword,
                nullable: false,
                transforms: Vec::new(),
                default: None,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
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

/// `books` over public.books with a has_many join to `reviews` and a
/// many_to_many to `tags` through the `book_tags` junction.
fn books() -> IndexSchema {
    let mut books = schema("books");
    books.fields.push(Field {
        field: kernel::FieldName::try_new("reviews").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("reviews").unwrap(),
            kind: JoinKind::HasMany {
                foreign_key: kernel::ColumnName::try_new("book_id").unwrap(),
            },
            primary_key: kernel::ColumnName::try_new("id").unwrap(),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("body")],
            indexed: true,
            on: vec![],
        })),
    });
    books.fields.push(Field {
        field: kernel::FieldName::try_new("tags").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("tags").unwrap(),
            kind: JoinKind::ManyToMany {
                through: Through {
                    table: TableName::try_new("book_tags").unwrap(),
                    left_key: kernel::ColumnName::try_new("book_id").unwrap(),
                    right_key: kernel::ColumnName::try_new("tag_id").unwrap(),
                    filters: None,
                },
            },
            primary_key: kernel::ColumnName::try_new("id").unwrap(),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("name")],
            indexed: true,
            on: vec![],
        })),
    });
    books
}

#[test]
fn all_tables_collects_roots_relations_and_junctions() {
    let mut indexes = BTreeMap::new();
    indexes.insert(index_name("books"), books());
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

#[test]
fn pre_image_columns_are_child_foreign_keys_and_junction_left_keys() {
    let spec = SourceSpec::new(BTreeMap::from([(index_name("books"), books())]));
    let public = DatabaseSchema::try_new("public").unwrap();
    let entry = |table: &str, column: &str| {
        (
            QualifiedTable::new(public.clone(), TableName::try_new(table).unwrap()),
            vec![ColumnName::try_new(column).unwrap()],
        )
    };

    assert_eq!(
        spec.pre_image_columns(),
        BTreeMap::from([entry("book_tags", "book_id"), entry("reviews", "book_id")])
    );
}

#[test]
fn pre_images_list_every_table_read_with_its_links() {
    let spec = SourceSpec::new(BTreeMap::from([(index_name("books"), books())]));
    let public = DatabaseSchema::try_new("public").unwrap();
    let entry = |table: &str, columns: &[&str]| {
        (
            QualifiedTable::new(public.clone(), TableName::try_new(table).unwrap()),
            columns
                .iter()
                .map(|column| ColumnName::try_new(*column).unwrap())
                .collect::<Vec<_>>(),
        )
    };

    assert_eq!(
        spec.pre_images(),
        BTreeMap::from([
            entry("book_tags", &["book_id"]),
            entry("books", &[]),
            entry("reviews", &["book_id"]),
            entry("tags", &[]),
        ])
    );
    assert_eq!(
        spec.pre_images().into_keys().collect::<Vec<_>>(),
        spec.all_tables().into_iter().collect::<Vec<_>>()
    );
}

#[test]
fn pre_image_columns_carry_a_self_join_link_on_the_root_table() {
    let mut items = schema("items");
    items.fields.push(Field {
        field: kernel::FieldName::try_new("children").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("items").unwrap(),
            kind: JoinKind::HasMany {
                foreign_key: kernel::ColumnName::try_new("parent_id").unwrap(),
            },
            primary_key: kernel::ColumnName::try_new("id").unwrap(),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields: vec![column_field("name")],
            indexed: true,
            on: vec![],
        })),
    });
    let spec = SourceSpec::new(BTreeMap::from([(index_name("items"), items)]));
    let items_table = QualifiedTable::new(
        DatabaseSchema::try_new("public").unwrap(),
        TableName::try_new("items").unwrap(),
    );

    assert_eq!(
        spec.pre_image_columns(),
        BTreeMap::from([(items_table, vec![ColumnName::try_new("parent_id").unwrap()])])
    );
}

/// `users` with a `via` aggregate: users → teams → projects → tasks.
fn users_with_task_count() -> IndexSchema {
    let hop = |table: &str, foreign_key: &str| kernel::ViaHop {
        table: TableName::try_new(table).unwrap(),
        foreign_key: ColumnName::try_new(foreign_key).unwrap(),
        filters: None,
        on: vec![],
    };
    let mut users = schema("users");
    users.fields.push(Field {
        field: kernel::FieldName::try_new("taskCount").unwrap(),
        options: Default::default(),
        source: FieldSource::Relation(Relation::Aggregate(kernel::Aggregate {
            table: TableName::try_new("tasks").unwrap(),
            op: kernel::AggregateOp::Count,
            key: kernel::AggregateKey::Direct(ColumnName::try_new("project_id").unwrap()),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![hop("teams", "owner_id"), hop("projects", "team_id")],
            on: vec![],
        })),
    });
    users
}

#[test]
fn all_tables_include_every_via_hop() {
    let spec = SourceSpec::new(BTreeMap::from([(
        index_name("users"),
        users_with_task_count(),
    )]));
    let public = DatabaseSchema::try_new("public").unwrap();
    let qt = |t: &str| QualifiedTable::new(public.clone(), TableName::try_new(t).unwrap());
    assert_eq!(
        spec.all_tables(),
        [qt("projects"), qt("tasks"), qt("teams"), qt("users")]
            .into_iter()
            .collect()
    );
}

#[test]
fn pre_image_columns_carry_each_via_hop_s_foreign_key() {
    let spec = SourceSpec::new(BTreeMap::from([(
        index_name("users"),
        users_with_task_count(),
    )]));
    let public = DatabaseSchema::try_new("public").unwrap();
    let entry = |table: &str, column: &str| {
        (
            QualifiedTable::new(public.clone(), TableName::try_new(table).unwrap()),
            vec![ColumnName::try_new(column).unwrap()],
        )
    };
    assert_eq!(
        spec.pre_image_columns(),
        BTreeMap::from([
            entry("projects", "team_id"),
            entry("tasks", "project_id"),
            entry("teams", "owner_id"),
        ])
    );
}
