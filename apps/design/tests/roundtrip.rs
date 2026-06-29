//! Property-based ("fuzz") test of the codegen round-trip: generate random but
//! *valid* `IndexSchema`s spanning the whole grammar — scalars, objects, all
//! four join verbs, all six aggregate ops, geo/map/custom, `order_by`/`limit` —
//! and assert each one survives `schema_to_yaml → parse → convert` unchanged
//! (and so resolves to the same mapping). This is the deep correctness net the
//! dev-schema fixtures can't give: it explores shapes nobody wrote by hand.
//!
//! The generator mirrors the parser's invariants so a failure means *codegen*
//! is wrong, not the fixture:
//!  - `identifier` is excluded (conversion injects an `analyzer` option),
//!  - `map` fields carry the `dynamic` option the conversion injects,
//!  - to-many joins are `nullable` (the parser forbids `required` there),
//!  - `belongs_to` takes no `order_by`/`limit`, `has_one` no `limit`.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;

use proptest::prelude::*;
use schema_core::common::ColumnName;
use schema_core::{
    Aggregate, AggregateKey, AggregateOp, Column, DatabaseSchema, Field, FieldName, FieldSource,
    FlussoType, GenericValue, Geo, IndexSchema, Join, JoinKind, OrderBy, ParseFrom, Relation,
    TableName, Through, Transform,
};
use schema_index_yaml::SchemaYaml;

const DEPTH: u32 = 2;

fn col() -> impl Strategy<Value = ColumnName> {
    prop::sample::select(vec!["a", "b", "c", "col1", "amount", "val"])
        .prop_map(|s| ColumnName::try_new(s).unwrap())
}
fn table() -> impl Strategy<Value = TableName> {
    prop::sample::select(vec!["t_a", "t_b", "t_c", "orders"])
        .prop_map(|s| TableName::try_new(s).unwrap())
}
fn scalar_type() -> impl Strategy<Value = FlussoType> {
    use FlussoType::*;
    prop::sample::select(vec![
        Text, Keyword, Enum, Uuid, Boolean, Short, Integer, Long, Float, Double, Decimal, Date,
        Timestamp, Binary, Json,
    ])
}
fn leaf_type() -> impl Strategy<Value = FlussoType> {
    use FlussoType::*;
    prop::sample::select(vec![Text, Keyword, Integer, Long, Double, Date])
}
fn elem_type() -> impl Strategy<Value = FlussoType> {
    prop::sample::select(vec![FlussoType::Long, FlussoType::Keyword])
}
fn transforms() -> impl Strategy<Value = Vec<Transform>> {
    prop_oneof![
        Just(vec![]),
        Just(vec![Transform::Lowercase]),
        Just(vec![Transform::Trim]),
        Just(vec![Transform::Lowercase, Transform::Trim]),
    ]
}

fn through() -> impl Strategy<Value = Through> {
    (table(), col(), col()).prop_map(|(table, left_key, right_key)| Through {
        table,
        left_key,
        right_key,
    })
}
fn agg_key() -> impl Strategy<Value = AggregateKey> {
    prop_oneof![
        col().prop_map(AggregateKey::Direct),
        through().prop_map(AggregateKey::Through),
    ]
}
fn order_by_opt() -> impl Strategy<Value = Option<Vec<OrderBy>>> {
    use schema_core::Direction;
    let dir = prop_oneof![
        Just(None),
        Just(Some(Direction::Asc)),
        Just(Some(Direction::Desc))
    ];
    let one = (col(), dir).prop_map(|(column, direction)| OrderBy { column, direction });
    prop_oneof![Just(None), prop::collection::vec(one, 1..=2).prop_map(Some)]
}

fn scalar_source() -> BoxedStrategy<FieldSource> {
    (scalar_type(), col(), any::<bool>(), transforms())
        .prop_map(|(ty, column, nullable, transforms)| {
            FieldSource::Column(Column {
                column,
                ty,
                nullable,
                transforms,
                default: None,
            })
        })
        .boxed()
}
fn geo_source() -> BoxedStrategy<FieldSource> {
    (col(), col(), any::<bool>())
        .prop_map(|(lat, lon, nullable)| FieldSource::Geo(Geo { lat, lon, nullable }))
        .boxed()
}
fn map_source() -> BoxedStrategy<FieldSource> {
    (leaf_type(), col(), any::<bool>())
        .prop_map(|(values, column, nullable)| {
            FieldSource::Column(Column {
                column,
                ty: FlussoType::Map {
                    values: Box::new(values),
                },
                nullable,
                transforms: Vec::new(),
                default: None,
            })
        })
        .boxed()
}
fn custom_source() -> BoxedStrategy<FieldSource> {
    let pg = prop::collection::vec(
        prop::sample::select(vec!["tsvector", "hstore", "cidr"]),
        1..=2,
    );
    let os = prop::sample::select(vec!["keyword", "text", "integer"]);
    (pg, os, col(), any::<bool>())
        .prop_map(|(postgres, opensearch, column, nullable)| {
            FieldSource::Column(Column {
                column,
                ty: FlussoType::Custom {
                    postgres: postgres.into_iter().map(String::from).collect(),
                    opensearch: opensearch.to_string(),
                },
                nullable,
                transforms: Vec::new(),
                default: None,
            })
        })
        .boxed()
}
fn aggregate_source() -> BoxedStrategy<FieldSource> {
    let mk =
        |table: TableName, op: AggregateOp, key: AggregateKey, value_type: Option<FlussoType>| {
            FieldSource::Relation(Relation::Aggregate(Aggregate {
                table,
                op,
                key,
                value_type,
                filters: None,
            }))
        };
    prop_oneof![
        (table(), agg_key()).prop_map(move |(t, k)| mk(t, AggregateOp::Count, k, None)),
        (table(), col(), scalar_type(), agg_key()).prop_map(move |(t, c, vt, k)| mk(
            t,
            AggregateOp::Sum(c),
            k,
            Some(vt)
        )),
        (table(), col(), scalar_type(), agg_key()).prop_map(move |(t, c, vt, k)| mk(
            t,
            AggregateOp::Min(c),
            k,
            Some(vt)
        )),
        (table(), col(), scalar_type(), agg_key()).prop_map(move |(t, c, vt, k)| mk(
            t,
            AggregateOp::Max(c),
            k,
            Some(vt)
        )),
        (table(), col(), agg_key()).prop_map(move |(t, c, k)| mk(t, AggregateOp::Avg(c), k, None)),
        (table(), elem_type(), agg_key()).prop_map(move |(t, et, k)| mk(
            t,
            AggregateOp::Ids { element_type: et },
            k,
            None
        )),
    ]
    .boxed()
}

fn join_source(depth: u32) -> BoxedStrategy<FieldSource> {
    let mk = |table: TableName,
              kind: JoinKind,
              primary_key: ColumnName,
              nullable: bool,
              order_by: Option<Vec<OrderBy>>,
              limit: Option<u64>,
              fields: Vec<Field>| {
        FieldSource::Relation(Relation::Join(Join {
            table,
            kind,
            primary_key,
            nullable,
            filters: None,
            order_by,
            limit,
            fields,
        }))
    };
    let limit_opt = prop_oneof![Just(None), (1u64..1000).prop_map(Some)];
    prop_oneof![
        // belongs_to: to-one, no order_by/limit.
        (table(), col(), col(), any::<bool>(), fields(depth - 1)).prop_map({
            move |(t, pk, column, nullable, fs)| {
                mk(
                    t,
                    JoinKind::BelongsTo { column },
                    pk,
                    nullable,
                    None,
                    None,
                    fs,
                )
            }
        }),
        // has_one: to-one, order_by allowed, no limit.
        (
            table(),
            col(),
            col(),
            any::<bool>(),
            order_by_opt(),
            fields(depth - 1)
        )
            .prop_map({
                move |(t, pk, fk, nullable, ob, fs)| {
                    mk(
                        t,
                        JoinKind::HasOne { foreign_key: fk },
                        pk,
                        nullable,
                        ob,
                        None,
                        fs,
                    )
                }
            }),
        // has_many: to-many — the parser hardcodes nullable=false; order_by + limit.
        (
            table(),
            col(),
            col(),
            order_by_opt(),
            limit_opt.clone(),
            fields(depth - 1)
        )
            .prop_map({
                move |(t, pk, fk, ob, limit, fs)| {
                    mk(
                        t,
                        JoinKind::HasMany { foreign_key: fk },
                        pk,
                        false,
                        ob,
                        limit,
                        fs,
                    )
                }
            }),
        // many_to_many: to-many, through a junction (nullable=false, as above).
        (
            table(),
            col(),
            through(),
            order_by_opt(),
            limit_opt,
            fields(depth - 1)
        )
            .prop_map({
                move |(t, pk, thr, ob, limit, fs)| {
                    mk(
                        t,
                        JoinKind::ManyToMany { through: thr },
                        pk,
                        false,
                        ob,
                        limit,
                        fs,
                    )
                }
            }),
    ]
    .boxed()
}

fn source(depth: u32) -> BoxedStrategy<FieldSource> {
    let leaves = prop_oneof![
        scalar_source(),
        geo_source(),
        map_source(),
        custom_source(),
        aggregate_source(),
    ];
    if depth == 0 {
        return leaves.boxed();
    }
    prop_oneof![
        4 => leaves,
        1 => fields(depth - 1).prop_map(FieldSource::Group).boxed(),
        2 => join_source(depth),
    ]
    .boxed()
}

fn fields(depth: u32) -> BoxedStrategy<Vec<Field>> {
    prop::collection::vec(source(depth), 1..=4)
        .prop_map(|sources| {
            sources
                .into_iter()
                .enumerate()
                .map(|(i, source)| Field {
                    field: FieldName::try_new(format!("f{i}")).unwrap(),
                    options: options_for(&source),
                    source,
                })
                .collect()
        })
        .boxed()
}

/// The options the *parser* would attach to a field of this source, so the
/// generated schema matches what re-parsing produces.
fn options_for(source: &FieldSource) -> BTreeMap<String, GenericValue> {
    if let FieldSource::Column(c) = source
        && matches!(c.ty, FlussoType::Map { .. })
    {
        return BTreeMap::from([("dynamic".to_owned(), GenericValue::Bool(true))]);
    }
    BTreeMap::new()
}

fn schema() -> impl Strategy<Value = IndexSchema> {
    let db = prop::sample::select(vec!["public", "app", "store"])
        .prop_map(|s| DatabaseSchema::try_new(s).unwrap());
    (table(), db, col(), fields(DEPTH)).prop_map(|(table, db_schema, pk, fields)| IndexSchema {
        version: 1,
        table,
        db_schema,
        primary_key: Some(pk),
        doc_id: None,
        soft_delete: None,
        filters: None,
        fields,
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn any_valid_schema_roundtrips_through_codegen(schema in schema()) {
        let yaml = design::codegen::schema_to_yaml(&schema).unwrap();
        let entity = SchemaYaml::try_parse(&yaml)
            .map_err(|e| TestCaseError::fail(format!("generated YAML did not parse: {e}\n{yaml}")))?;
        let reparsed = IndexSchema::try_from(entity)
            .map_err(|e| TestCaseError::fail(format!("generated YAML did not convert: {e}\n{yaml}")))?;
        prop_assert_eq!(
            serde_json::to_value(&schema).unwrap(),
            serde_json::to_value(&reparsed).unwrap(),
            "round-trip changed the schema; emitted:\n{}",
            yaml
        );
    }
}
