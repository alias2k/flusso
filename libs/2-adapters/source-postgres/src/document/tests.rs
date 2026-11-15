//! The root row's own document follows the changed-columns rule, a key with a
//! null part addresses no document, and a key carrying a hop's link names the
//! parent itself. These schemas resolve without touching the database, so the
//! pool is never opened: every key column is cached as `integer`, a type whose
//! equal values are identical, so no link is looked up on the root table.

use std::cell::Cell;
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
    for (table, column) in columns {
        builder.col_type_cache.insert(
            "public",
            table,
            column,
            Arc::new(ColumnMeta {
                sql_type: "integer".to_owned(),
                type_oid: 23,
                type_mod: -1,
                key_type: "pg_catalog.int4".to_owned(),
                base_type: None,
                base_name: "int4".to_owned(),
                element_name: None,
                deterministic: true,
                nullable: true,
            }),
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
        type_oid: 0,
        type_mod: -1,
        key_type: base_name.to_owned(),
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

thread_local! {
    /// Catalog lookups (`column_meta` calls, cache hits included) on this thread.
    pub(super) static COLUMN_LOOKUPS: Cell<usize> = const { Cell::new(0) };
    /// Searches of an index's field tree for the paths to a table, on this thread.
    pub(super) static PATH_SEARCHES: Cell<usize> = const { Cell::new(0) };
}

fn tags_through_user_tags() -> Field {
    join_field(
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
    )
}

fn user_tag(user_id: i32) -> RowKey {
    RowKey(vec![
        (column("user_id"), GenericValue::Int(user_id)),
        (column("tag_id"), GenericValue::Int(3)),
    ])
}

/// Whether a path's root key needs reading back depends on the index alone,
/// so once it is known, a change walked from its key reads no catalog.
#[tokio::test]
async fn a_path_walked_from_the_key_looks_up_no_column_once_known() {
    let builder = builder_with(vec![tags_through_user_tags()]);
    let user_tags = TableName::try_new("user_tags").unwrap();
    builder
        .resolve(&user_tags, &user_tag(1), None, None)
        .await
        .unwrap();

    COLUMN_LOOKUPS.with(|count| count.set(0));
    for user_id in 2..5 {
        let ids = builder
            .resolve(&user_tags, &user_tag(user_id), None, None)
            .await
            .unwrap();
        assert_eq!(ids, [users_id(user_id)]);
    }
    assert_eq!(COLUMN_LOOKUPS.with(Cell::get), 0);
}

/// The paths from each table back to the root are found once, when the
/// builder is built, never per change.
#[tokio::test]
async fn resolving_a_change_searches_no_field_tree() {
    let builder = builder_with(vec![tags_through_user_tags(), event_count()]);
    PATH_SEARCHES.with(|count| count.set(0));
    for (table, key) in [
        (users(), key()),
        (TableName::try_new("user_tags").unwrap(), user_tag(1)),
        (TableName::try_new("events").unwrap(), event(1, None)),
        (TableName::try_new("products").unwrap(), key()),
    ] {
        builder.resolve(&table, &key, None, None).await.unwrap();
    }
    assert_eq!(PATH_SEARCHES.with(Cell::get), 0);
}

/// The key type the catalog lookup reads for a column of `sql_type`: its
/// base `typname`, qualified, with no size.
fn catalog_key_type(sql_type: &str) -> &str {
    match sql_type {
        "character varying(5)" => r#"pg_catalog."varchar""#,
        "numeric(10,2)" => "pg_catalog.numeric",
        "character(5)" => "pg_catalog.bpchar",
        other => other,
    }
}

fn cache_type(builder: &PgDocumentBuilder, table: &str, column: &str, sql_type: &str, oid: u32) {
    builder.col_type_cache.insert(
        "public",
        table,
        column,
        Arc::new(ColumnMeta {
            sql_type: sql_type.to_owned(),
            type_oid: oid,
            type_mod: -1,
            key_type: catalog_key_type(sql_type).to_owned(),
            base_type: None,
            base_name: sql_type.to_owned(),
            element_name: None,
            deterministic: true,
            nullable: true,
        }),
    );
}

/// The kept answer follows the column types: a `Relation` message naming a
/// new type for the link column (`forget_changed`), and a failed document
/// query (`forget_table`), each make the next change ask again.
#[tokio::test]
async fn a_changed_column_type_reaches_the_kept_root_link_answer() {
    let builder = builder_with(vec![tags_through_user_tags()]);
    let user_tags = TableName::try_new("user_tags").unwrap();
    let schema = builder
        .spec
        .schema(&IndexName::try_new("users").unwrap())
        .unwrap();
    let path = builder
        .reads
        .paths(&IndexName::try_new("users").unwrap(), &user_tags)
        .first()
        .unwrap();
    let steps = path.address.steps(&schema.fields).unwrap();
    let ask = async || {
        builder
            .reads_root_from_a_link(schema, &column("id"), path, &steps)
            .await
            .unwrap()
    };
    assert_eq!(ask().await, None, "an integer link to an integer key");

    builder
        .col_type_cache
        .forget_changed("public", "user_tags", [("user_id", 25, -1)].into_iter());
    cache_type(&builder, "user_tags", "user_id", "text", 25);
    assert_eq!(
        ask().await,
        Some((user_tags.clone(), column("user_id"))),
        "a text link to an integer key is read back"
    );

    builder.col_type_cache.forget_table("public", "user_tags");
    cache_type(&builder, "user_tags", "user_id", "integer", 23);
    assert_eq!(ask().await, None, "back to integer after a failed query");
}

/// A key is cast to its column's type without the modifier: an explicit
/// cast to `varchar(5)`, `numeric(10,2)` or `char(5)` truncates or rounds
/// without an error, so a key read after the column was resized would match
/// no row (issue #335).
#[tokio::test]
async fn no_key_cast_carries_a_type_modifier() {
    let builder = builder();
    let schema = builder
        .spec
        .schema(&IndexName::try_new("users").unwrap())
        .unwrap()
        .clone();
    for sized in ["character varying(5)", "numeric(10,2)", "character(5)"] {
        cache_type(&builder, "users", "id", sized, 1043);
        let mut keyed = HashMap::new();
        builder
            .add_key_column_types(&schema, &[&column("id")], &mut keyed)
            .await
            .unwrap();
        let reverse = builder
            .cast_types(
                &schema.db_schema,
                &users(),
                &[(column("id"), GenericValue::Int(1))],
                |column| (users(), column.clone()),
            )
            .await
            .unwrap();
        for cast in keyed.values().chain(reverse.values()) {
            assert!(!cast.contains('('), "{sized} is cast as {cast}");
        }
    }
}

/// A reverse lookup that fails drops the cached types of every table its
/// index reads, as a failed document query does: one written from a type
/// that changed would fail the same way on every retry otherwise.
#[tokio::test]
async fn a_failed_resolve_forgets_the_column_types_of_its_index() {
    let builder = builder_with(vec![event_count()]);
    cache_type(&builder, "events", "id", "integer", 23);
    let events = TableName::try_new("events").unwrap();
    let change = RowKey(vec![(column("id"), GenericValue::Int(1))]);
    assert!(builder.col_type_cache.caches("public", "events"));

    let resolved = builder.resolve(&events, &change, None, None).await;

    assert!(resolved.is_err(), "the pool reaches no database");
    assert!(!builder.col_type_cache.caches("public", "events"));
    assert!(!builder.col_type_cache.caches("public", "users"));
}

/// Counts the `WARN` events logged on this thread while it is the default.
#[derive(Default)]
struct Warnings(std::sync::atomic::AtomicUsize);

impl tracing::Subscriber for Warnings {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::WARN {
            self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[tokio::test]
async fn a_change_on_an_unread_table_warns_once_per_table() {
    let warnings = Arc::new(Warnings::default());
    let _default = tracing::subscriber::set_default(Arc::clone(&warnings));
    let builder = builder();
    for table in ["products", "products", "products", "reviews", "reviews"] {
        let ids = builder
            .resolve(&TableName::try_new(table).unwrap(), &key(), None, None)
            .await
            .unwrap();
        assert!(ids.is_empty());
    }
    assert_eq!(warnings.0.load(std::sync::atomic::Ordering::Relaxed), 2);
}

/// Every value filter, wherever it sits: the root, a soft-delete `when`, a
/// `has_many`, a `belongs_to`, a junction, a `via` hop and an aggregate.
fn filtered_everywhere() -> IndexSchema {
    let yaml = "
version: 1
table: users
primary_key: id
filters:
  - { column: status, op: neq, value: archived }
soft_delete:
  column: deleted
  when:
    - { column: amount, op: gte, value: 1.005 }
fields:
  - { integer: id, required: true }
  - has_many: pets
    table: pets
    foreign_key: user_id
    primary_key: id
    filters:
      - { column: status, op: eq, value: archived }
    fields:
      - { integer: id, required: true }
  - belongs_to: team
    table: teams
    column: team_id
    primary_key: id
    filters:
      - { column: amount, op: in, value: [1.005, 2] }
    fields:
      - { integer: id, required: true }
  - many_to_many: tags
    table: tags
    primary_key: id
    through:
      table: user_tags
      left_key: user_id
      right_key: tag_id
      filters:
        - { column: status, op: not_in, value: [archived] }
    fields:
      - { integer: id, required: true }
  - count: toyCount
    table: toys
    foreign_key: pet_id
    filters:
      - { column: amount, op: between, value: [1.005, 9] }
    via:
      - table: pets
        foreign_key: user_id
        filters:
          - { column: status, op: like, value: archived% }
";
    use kernel::ParseFrom;
    IndexSchema::try_from(config::yaml::SchemaYaml::try_parse(yaml).unwrap()).unwrap()
}

/// A filter operand is cast to its column's type without the modifier, as a
/// key is: an explicit cast to `varchar(5)` or `numeric(10,2)` cuts or
/// rounds the operand without an error, so `eq archived` would compare
/// against `archi` and `neq` would drop the row it should keep (issue #338).
#[tokio::test]
async fn no_filter_operand_cast_carries_a_type_modifier() {
    let schema = filtered_everywhere();
    let spec = SourceSpec::new(BTreeMap::from([(
        IndexName::try_new("users").unwrap(),
        schema.clone(),
    )]));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .connect_lazy("postgres://unused@localhost/unused")
        .unwrap();
    let builder = PgDocumentBuilder::new(pool, Arc::new(spec));
    for table in ["users", "pets", "teams", "user_tags", "toys"] {
        cache_type(&builder, table, "status", "character varying(5)", 1043);
        cache_type(&builder, table, "amount", "numeric(10,2)", 1700);
    }
    cache_type(&builder, "users", "id", "integer", 23);
    for table in ["pets", "teams", "tags", "toys"] {
        builder
            .pk_cache
            .lock()
            .unwrap()
            .insert(("public".to_owned(), table.to_owned()), column("id"));
    }

    let mut types = builder.filter_column_types(&schema).await.unwrap();
    assert_eq!(types.len(), 6, "{types:?}");
    builder
        .add_key_column_types(&schema, &[&column("id")], &mut types)
        .await
        .unwrap();
    let pks = builder.relation_pks(&schema).await.unwrap();
    let (sql, params) = query::document_query(
        &schema,
        &[(column("id"), GenericValue::Int(1))],
        &pks,
        &types,
        &query::ByteColumns::new(),
    )
    .unwrap();
    let sql = sql.as_str();
    assert_eq!(params.len(), 10, "{sql}");
    let casts: Vec<&str> = sql
        .split('$')
        .skip(1)
        .filter_map(|after| {
            after
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .strip_prefix("::")
        })
        .collect();
    assert_eq!(casts.len(), params.len(), "{sql}");
    let unsized_operands = casts
        .iter()
        .filter(|cast| {
            cast.starts_with(r#"pg_catalog."varchar""#) || cast.starts_with("pg_catalog.numeric")
        })
        .count();
    assert_eq!(
        unsized_operands,
        params.len() - 1,
        "every operand but the key is cast to its unsized type: {sql}"
    );
}
