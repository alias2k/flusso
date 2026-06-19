//! Property tests over the query builder. The join/aggregate/filter/geo
//! generation is intricate and the example tests cover only fixed shapes;
//! these feed it randomly-generated schemas — arbitrary nestings of joins,
//! aggregates, groups, geo points, and filters — and assert structural
//! invariants that must hold for *any* well-formed schema:
//!
//! - the builder never panics (a panic fails the test);
//! - parentheses balance (the nested subqueries open and close cleanly);
//! - the `$n` placeholders are exactly `1..=params.len()`, contiguous and in
//!   range — so every bound parameter is referenced and none dangles;
//! - double-quotes balance (every quoted identifier is closed).
//!
//! The identifier universe and the `pks`/`col_types` maps are fixed and
//! complete, so the builder's lookups always resolve — failures come from the
//! SQL it assembles, not from missing metadata. Generated string values are
//! restricted to a quote/paren-free alphabet so they can't forge the structural
//! markers the invariants check.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use proptest::prelude::*;
use schema_core::{
    Aggregate, AggregateKey, AggregateOp, Column, Direction, Field, FieldName, FieldSource, Filter,
    FilterOp, FilterValue, FlussoType, Geo, Join, JoinKind, NullCheckFilter, NullOp, OrderBy,
    Relation, SoftDelete, SoftDeleteColumn, Through, Transform, ValueOpFilter,
};

use super::*;

// A small, fixed identifier universe so every generated name is valid and
// reused across tables, and the metadata maps below can cover all of it.
const TABLES: &[&str] = &["users", "orders", "orgs", "items"];
const COLUMNS: &[&str] = &[
    "id",
    "name",
    "email",
    "total",
    "status",
    "user_id",
    "org_id",
    "created_at",
];
const FIELDS: &[&str] = &["a", "b", "c", "d", "e", "f", "g"];

fn pk_map() -> HashMap<String, ColumnName> {
    TABLES
        .iter()
        .map(|t| ((*t).to_owned(), ColumnName::try_new("id").unwrap()))
        .collect()
}

fn col_type_map() -> HashMap<(String, String), String> {
    let mut m = HashMap::new();
    for table in TABLES {
        for column in COLUMNS {
            m.insert(
                ((*table).to_owned(), (*column).to_owned()),
                "text".to_owned(),
            );
        }
    }
    m
}

fn db_schema() -> DatabaseSchema {
    DatabaseSchema::try_new("public").unwrap()
}

fn table() -> impl Strategy<Value = TableName> {
    prop::sample::select(TABLES.to_vec()).prop_map(|s| TableName::try_new(s).unwrap())
}
fn column() -> impl Strategy<Value = ColumnName> {
    prop::sample::select(COLUMNS.to_vec()).prop_map(|s| ColumnName::try_new(s).unwrap())
}
fn field_name() -> impl Strategy<Value = FieldName> {
    prop::sample::select(FIELDS.to_vec()).prop_map(|s| FieldName::try_new(s).unwrap())
}

/// Scalar string values restricted to a quote/paren/`$`-free alphabet, so a
/// generated literal can't forge the structural markers the invariants check.
fn safe_string() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            prop::char::range('a', 'z'),
            prop::char::range('0', '9'),
            Just(' '),
        ],
        0..6,
    )
    .prop_map(|cs| cs.into_iter().collect())
}

fn scalar_value() -> impl Strategy<Value = GenericValue> {
    prop_oneof![
        any::<i64>().prop_map(GenericValue::Int),
        any::<bool>().prop_map(GenericValue::Bool),
        safe_string().prop_map(GenericValue::String),
        Just(GenericValue::Null),
    ]
}

fn value_op_filter() -> impl Strategy<Value = ValueOpFilter> {
    let single = (
        column(),
        prop_oneof![
            Just(FilterOp::Eq),
            Just(FilterOp::Neq),
            Just(FilterOp::Lt),
            Just(FilterOp::Lte),
            Just(FilterOp::Gt),
            Just(FilterOp::Gte),
            Just(FilterOp::Like),
            Just(FilterOp::Ilike),
        ],
        safe_string(),
    )
        .prop_map(|(column, op, v)| ValueOpFilter {
            column,
            op,
            value: FilterValue::Single(v),
        });
    let list = (
        column(),
        prop_oneof![Just(FilterOp::In), Just(FilterOp::NotIn)],
        prop::collection::vec(safe_string(), 1..4),
    )
        .prop_map(|(column, op, vs)| ValueOpFilter {
            column,
            op,
            value: FilterValue::List(vs),
        });
    let between =
        (column(), safe_string(), safe_string()).prop_map(|(column, lo, hi)| ValueOpFilter {
            column,
            op: FilterOp::Between,
            value: FilterValue::Range(lo, hi),
        });
    // No `Filter::Raw` — its SQL is passed through verbatim and would
    // legitimately defeat the paren/quote invariants; the builder doesn't
    // shape it, so it isn't what these tests are checking.
    prop_oneof![single, list, between]
}

fn filter() -> impl Strategy<Value = Filter> {
    prop_oneof![
        (
            column(),
            prop_oneof![Just(NullOp::IsNull), Just(NullOp::IsNotNull)]
        )
            .prop_map(|(column, op)| Filter::NullCheck(NullCheckFilter { column, op })),
        value_op_filter().prop_map(Filter::ValueOp),
    ]
}

fn filters_opt() -> impl Strategy<Value = Option<Vec<Filter>>> {
    prop::option::of(prop::collection::vec(filter(), 0..3))
}

fn order_by_opt() -> impl Strategy<Value = Option<Vec<OrderBy>>> {
    prop::option::of(prop::collection::vec(
        (
            column(),
            prop::option::of(prop_oneof![Just(Direction::Asc), Just(Direction::Desc)]),
        )
            .prop_map(|(column, direction)| OrderBy { column, direction }),
        0..3,
    ))
}

fn through() -> impl Strategy<Value = Through> {
    (table(), column(), column()).prop_map(|(table, left_key, right_key)| Through {
        table,
        left_key,
        right_key,
    })
}

fn join_kind() -> impl Strategy<Value = JoinKind> {
    prop_oneof![
        column().prop_map(|column| JoinKind::BelongsTo { column }),
        column().prop_map(|foreign_key| JoinKind::HasOne { foreign_key }),
        column().prop_map(|foreign_key| JoinKind::HasMany { foreign_key }),
        through().prop_map(|through| JoinKind::ManyToMany { through }),
    ]
}

fn aggregate() -> impl Strategy<Value = Aggregate> {
    (
        table(),
        prop_oneof![
            Just(AggregateOp::Count),
            column().prop_map(AggregateOp::Sum),
            column().prop_map(AggregateOp::Avg),
            column().prop_map(AggregateOp::Min),
            column().prop_map(AggregateOp::Max),
        ],
        prop_oneof![
            column().prop_map(AggregateKey::Direct),
            through().prop_map(AggregateKey::Through),
        ],
        filters_opt(),
    )
        .prop_map(|(table, op, key, filters)| Aggregate {
            table,
            op,
            key,
            value_type: None,
            filters,
        })
}

/// A recursive `FieldSource`: leaves (column, geo, constant, aggregate) plus
/// nesting via `Group` and `Join`, both of which carry child fields.
fn field_source() -> impl Strategy<Value = FieldSource> {
    let leaf = prop_oneof![
        (
            column(),
            prop::collection::vec(
                prop_oneof![Just(Transform::Lowercase), Just(Transform::Trim)],
                0..3
            ),
            prop::option::of(scalar_value()),
        )
            .prop_map(|(column, transforms, default)| FieldSource::Column(Column {
                column,
                ty: FlussoType::Keyword,
                nullable: true,
                transforms,
                default,
            })),
        (column(), column()).prop_map(|(lat, lon)| FieldSource::Geo(Geo {
            lat,
            lon,
            nullable: true
        })),
        scalar_value().prop_map(FieldSource::Constant),
        aggregate().prop_map(|a| FieldSource::Relation(Relation::Aggregate(a))),
    ];
    leaf.prop_recursive(3, 32, 4, |inner| {
        // `inner` (a BoxedStrategy) is Clone, so build the child-fields
        // strategy on demand for each arm rather than cloning the VecStrategy.
        let children = || {
            let child = (field_name(), inner.clone()).prop_map(|(field, source)| Field {
                field,
                options: Default::default(),
                source,
            });
            prop::collection::vec(child, 1..4)
        };
        prop_oneof![
            children().prop_map(FieldSource::Group),
            (
                join_kind(),
                table(),
                column(),
                filters_opt(),
                order_by_opt(),
                prop::option::of(any::<u64>()),
                children(),
            )
                .prop_map(
                    |(kind, table, primary_key, filters, order_by, limit, fields)| {
                        FieldSource::Relation(Relation::Join(Join {
                            table,
                            kind,
                            primary_key,
                            filters,
                            order_by,
                            limit,
                            fields,
                        }))
                    }
                ),
        ]
    })
}

fn field() -> impl Strategy<Value = Field> {
    (field_name(), field_source()).prop_map(|(field, source)| Field {
        field,
        options: Default::default(),
        source,
    })
}

fn index_schema() -> impl Strategy<Value = IndexSchema> {
    (
        table(),
        prop::collection::vec(field(), 1..5),
        filters_opt(),
        prop::option::of(
            (column(), filters_opt())
                .prop_map(|(column, when)| SoftDelete::Column(SoftDeleteColumn { column, when })),
        ),
    )
        .prop_map(|(table, fields, filters, soft_delete)| IndexSchema {
            version: 1,
            table,
            db_schema: db_schema(),
            primary_key: Some(ColumnName::try_new("id").unwrap()),
            doc_id: None,
            soft_delete,
            filters,
            fields,
        })
}

/// Parentheses balance, scanning left to right, never dipping below zero.
fn parens_balanced(sql: &str) -> bool {
    let mut depth: i64 = 0;
    for ch in sql.chars() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// The set of `$n` placeholders in `sql` is exactly `{1..=params_len}` —
/// contiguous, none missing, none out of range.
fn placeholders_match(sql: &str, params_len: usize) -> bool {
    let mut found = std::collections::BTreeSet::new();
    let bytes = sql.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > start {
                if let Ok(n) = sql[start..j].parse::<usize>() {
                    found.insert(n);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    let expected: std::collections::BTreeSet<usize> = (1..=params_len).collect();
    found == expected
}

fn assert_valid(sql: &str, params_len: usize) -> std::result::Result<(), TestCaseError> {
    prop_assert!(parens_balanced(sql), "unbalanced parens: {sql}");
    prop_assert!(
        placeholders_match(sql, params_len),
        "placeholders not 1..={params_len}: {sql}"
    );
    prop_assert!(
        sql.matches('"').count().is_multiple_of(2),
        "unbalanced double-quotes: {sql}"
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn document_query_is_structurally_valid(schema in index_schema()) {
        let key = [(ColumnName::try_new("id").unwrap(), GenericValue::Int(1))];
        let result = document_query(&schema, &key, &pk_map(), &col_type_map());
        prop_assert!(result.is_ok(), "builder errored: {:?}", result.err());
        let (sql, params) = result.unwrap();
        assert_valid(sql.as_str(), params.len())?;
    }

    #[test]
    fn documents_query_is_structurally_valid(schema in index_schema()) {
        let pk = ColumnName::try_new("id").unwrap();
        let keys = [GenericValue::Int(1), GenericValue::Int(2)];
        let result = documents_query(&schema, &pk, &keys, &pk_map(), &col_type_map());
        prop_assert!(result.is_ok(), "builder errored: {:?}", result.err());
        let (sql, params) = result.unwrap();
        assert_valid(sql.as_str(), params.len())?;
    }

    #[test]
    fn reverse_query_is_structurally_valid(
        table in table(),
        select_column in column(),
        key_column in column(),
    ) {
        let key = [(key_column, GenericValue::Int(5))];
        let (sql, params) =
            reverse_query(&db_schema(), &table, &select_column, &key, &col_type_map()).unwrap();
        assert_valid(sql.as_str(), params.len())?;
    }
}
