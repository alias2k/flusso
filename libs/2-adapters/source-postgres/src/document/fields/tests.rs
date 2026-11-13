use kernel::{Column, FieldName, FieldSource, FlussoType, Join, JoinKind};

use super::*;

fn table(name: &str) -> TableName {
    TableName::try_new(name).unwrap()
}

fn column(name: &str) -> ColumnName {
    ColumnName::try_new(name).unwrap()
}

fn id_field() -> Field {
    Field {
        field: FieldName::try_new("id").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: column("id"),
            ty: FlussoType::Keyword,
            nullable: false,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array: false,
        }),
    }
}

fn join(name: &str, on: &str, kind: JoinKind, fields: Vec<Field>) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: table(on),
            kind,
            primary_key: column("id"),
            nullable: false,
            filters: None,
            order_by: None,
            limit: None,
            fields,
            indexed: true,
            on: vec![],
        })),
    }
}

fn children(fields: Vec<Field>) -> Field {
    join(
        "children",
        "items",
        JoinKind::HasMany {
            foreign_key: column("parent_id"),
        },
        fields,
    )
}

fn path_names(fields: &[Field], target: &str) -> Vec<Vec<String>> {
    let mut prefix = Vec::new();
    let mut paths = Vec::new();
    find_paths(fields, &table(target), &mut prefix, &mut paths);
    paths
        .iter()
        .map(|path| path.iter().map(|hop| hop.table().to_string()).collect())
        .collect()
}

#[test]
fn a_self_join_is_a_path_to_the_root_table() {
    let fields = vec![id_field(), children(vec![id_field()])];

    assert_eq!(path_names(&fields, "items"), [["items"]]);
}

#[test]
fn a_nested_self_join_stops_at_its_declared_depth() {
    let fields = vec![id_field(), children(vec![children(vec![id_field()])])];

    assert_eq!(
        path_names(&fields, "items"),
        [vec!["items"], vec!["items", "items"]]
    );
}

#[test]
fn a_schema_without_a_self_join_has_no_path_to_its_root_table() {
    let orders = join(
        "orders",
        "orders",
        JoinKind::HasMany {
            foreign_key: column("user_id"),
        },
        vec![id_field()],
    );

    assert!(path_names(&[id_field(), orders], "users").is_empty());
}

#[test]
fn an_aggregate_via_hops_adds_one_step_per_hop() {
    let hop = |table_name: &str, foreign_key: &str| kernel::ViaHop {
        table: table(table_name),
        foreign_key: column(foreign_key),
        filters: None,
        on: vec![],
    };
    let task_count = Field {
        field: FieldName::try_new("taskCount").unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(kernel::Aggregate {
            table: table("tasks"),
            op: kernel::AggregateOp::Count,
            key: kernel::AggregateKey::Direct(column("project_id")),
            value_type: None,
            filters: None,
            distinct: false,
            via: vec![hop("teams", "owner_id"), hop("projects", "team_id")],
            on: vec![],
        })),
    };
    let fields = [id_field(), task_count];

    assert_eq!(path_names(&fields, "teams"), [["teams"]]);
    assert_eq!(path_names(&fields, "projects"), [["teams", "projects"]]);
    assert_eq!(
        path_names(&fields, "tasks"),
        [["teams", "projects", "tasks"]]
    );
}
