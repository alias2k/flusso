//! Property-based ("fuzz") test of the codegen round-trip: generate random but
//! *valid* `IndexSchema`s spanning the whole grammar — scalars, objects, all
//! four join verbs, all six aggregate ops, geo/map/custom, `order_by`/`limit` —
//! and assert each one survives `schema_to_yaml → parse → convert` unchanged
//! (and so resolves to the same mapping). This is the deep correctness net the
//! dev-schema fixtures can't give: it explores shapes nobody wrote by hand.
//!
//! A second property writes the schema as a hand-written, commented file,
//! saves random edits into it (`design::keep`), and asserts every comment
//! stays with the field it names, goes with a deleted one, and that nothing
//! else is lost or moved.
//!
//! The generator mirrors the parser's invariants so a failure means *codegen*
//! is wrong, not the fixture:
//!  - `identifier` is excluded (conversion injects an `analyzer` option),
//!  - `map` fields carry the `dynamic` option the conversion injects,
//!  - to-many joins are `nullable` (the parser forbids `required` there),
//!  - `belongs_to` takes no `order_by`/`limit`, `has_one` no `limit`,
//!  - `distinct` only on an aggregate over `through`,
//!  - `subfields: false` only on `text`/`keyword`/`uuid`, `indexed: false` only
//!    on `json` and joins (none of which get generated `options`),
//!  - values (`constant`, `default`, `options`) are ones the parser can produce:
//!    `Null`, `Bool`, `BigInt`, a `Decimal` read from an `f64` or an integer past
//!    `i64`, `String` (including ones that look like another kind), and, outside
//!    a `default`, `Array`/`Map` of those.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

use std::collections::BTreeMap;

use config::yaml::SchemaYaml;
use design::keep::{FieldId, SchemaIdentity};
use kernel::common::ColumnName;
use kernel::{
    Aggregate, AggregateKey, AggregateOp, BinaryOp, Column, ColumnPair, DatabaseSchema, Expression,
    Field, FieldName, FieldSource, Filter, FlussoType, GenericValue, Geo, IndexSchema, Join,
    JoinKind, NullCheckFilter, NullOp, OrderBy, ParseFrom, Relation, TableName, Through, Transform,
    ViaHop,
};
use proptest::prelude::*;
use rust_decimal::Decimal;

const DEPTH: u32 = 2;

/// Strings that read as another kind, or need quoting or a block, unquoted.
const TRICKY: &[&str] = &[
    "null",
    "~",
    "Null",
    "NULL",
    "true",
    "False",
    "yes",
    "on",
    "123",
    "-5",
    "+5",
    "0x1F",
    "0o17",
    "1e3",
    "1.5",
    "1.50",
    ".nan",
    ".inf",
    "-.inf",
    "1_000",
    "",
    " ",
    " a",
    "a ",
    "a: b",
    "a:b",
    "#x",
    "a #b",
    "x\ny",
    "x\n",
    "x\n\n",
    " x\ny",
    "\n",
    "- a",
    "-",
    "?",
    "'",
    "\"",
    "@x",
    "!x",
    "!Decimal 1.5",
    "&x",
    "*x",
    "|",
    ">",
    "{a}",
    "[a]",
    ",",
    "%x",
    "2024-01-01",
    "1:20",
    "\t",
    "a\tb",
    "\u{feff}x",
    "\r",
    "é",
    "<<",
];

fn string() -> impl Strategy<Value = String> {
    prop_oneof![
        prop::sample::select(TRICKY).prop_map(String::from),
        any::<String>(),
    ]
}

/// A decimal the parser produces: a non-`i64` YAML number is read through
/// `f64` as the `Decimal` of its shortest text, an integer past `i64` exactly.
/// Left out: a text past `Decimal`'s 28 places, which the parser rounds (scale
/// 28); it comes back equal in value and content hash, its scale may not.
fn decimal() -> impl Strategy<Value = Decimal> {
    let typed = prop::sample::select(vec![
        "1.5",
        "1.50",
        "0.1",
        "1e3",
        "1E3",
        "1e16",
        "1e-7",
        "-0.0",
        "5.0",
        "-2.5e10",
        "0.30000000000000004",
        "123456.789",
    ])
    .prop_map(|text| text.parse::<f64>().unwrap());
    let float = prop_oneof![typed, any::<f64>()].prop_filter_map("no decimal reads it", |f| {
        serde_yaml::Number::from(f)
            .to_string()
            .parse::<Decimal>()
            .ok()
            .filter(|d| d.scale() < 28)
    });
    let wide = ((i64::MAX.unsigned_abs() + 1)..=u64::MAX).prop_map(Decimal::from);
    prop_oneof![
        3 => float,
        1 => Just(Decimal::from(12_345_678_901_234_567_890_u64)),
        1 => wide,
    ]
}

/// A number as a user types it into the inspector: an optional sign (`+`
/// too), leading zeros, integer digits up to past `u64`, a fraction up to past
/// what a decimal holds, an exponent up to past `f64`, and whitespace around.
fn typed_number() -> impl Strategy<Value = String> {
    let digits = |max: usize| {
        prop_oneof![
            3 => prop::collection::vec(0u8..=9, 0..=max.min(6)),
            1 => prop::collection::vec(0u8..=9, 0..=max),
        ]
        .prop_map(|d| {
            d.into_iter()
                .map(|n| char::from(b'0' + n))
                .collect::<String>()
        })
    };
    let pinned = prop::sample::select(vec![
        "1e3",
        "1E3",
        "1e-7",
        "1e21",
        "1.5e300",
        "1e400",
        "9007199254740993",
        "9223372036854775807",
        "9223372036854775808",
        "18446744073709551615",
        "18446744073709551616",
        "-9223372036854775809",
        "-0",
        "-0.0",
        "0e0",
        "+5",
        "007",
        "+.5",
        "0.1000000000000000000001",
        "3.14159265358979323846",
        " 42 ",
    ])
    .prop_map(String::from);
    let built = (
        prop::sample::select(vec!["", "-", "+"]),
        prop::sample::select(vec!["", "0", "00"]),
        digits(26),
        prop::option::of(digits(30)),
        prop::option::weighted(
            0.3,
            (prop::sample::select(vec!["e", "E", "e+", "e-"]), 0u32..=330),
        ),
        prop::sample::select(vec!["", " ", "\n "]),
    )
        .prop_filter_map("no digit", |(sign, zeros, int, fraction, exponent, pad)| {
            let mantissa = match &fraction {
                Some(f) => format!("{zeros}{int}.{f}"),
                None => format!("{zeros}{int}"),
            };
            if !mantissa.chars().any(|c| c.is_ascii_digit()) {
                return None;
            }
            let exponent = exponent.map(|(e, n)| format!("{e}{n}")).unwrap_or_default();
            Some(format!("{pad}{sign}{mantissa}{exponent}{pad}"))
        });
    prop_oneof![1 => pinned, 3 => built]
}

/// A value the inspector accepts from a typed number (refused ones dropped).
fn typed_value() -> impl Strategy<Value = GenericValue> {
    typed_number().prop_filter_map("refused", |text| {
        design::value::read_typed(&text, design::value::ValueSlot::Constant).ok()
    })
}

fn scalar_value() -> BoxedStrategy<GenericValue> {
    prop_oneof![
        Just(GenericValue::Null),
        any::<bool>().prop_map(GenericValue::Bool),
        prop_oneof![any::<i64>(), -3i64..=3, Just(i64::MIN), Just(i64::MAX)]
            .prop_map(GenericValue::BigInt),
        decimal().prop_map(GenericValue::Decimal),
        typed_value(),
        string().prop_map(GenericValue::String),
    ]
    .boxed()
}

/// A column `default`: `default: null` reads as no default, so never `Null`.
fn default() -> impl Strategy<Value = Option<GenericValue>> {
    prop::option::of(
        scalar_value().prop_filter("reads as no default", |v| !matches!(v, GenericValue::Null)),
    )
}

fn value() -> BoxedStrategy<GenericValue> {
    scalar_value()
        .prop_recursive(2, 8, 3, |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..=3).prop_map(GenericValue::Array),
                prop::collection::btree_map(string(), inner, 0..=3).prop_map(GenericValue::Map),
            ]
        })
        .boxed()
}

/// Mapping options; never `dynamic`, which the parser sets on a `map`.
fn options() -> BoxedStrategy<BTreeMap<String, GenericValue>> {
    prop_oneof![
        2 => Just(BTreeMap::new()),
        1 => prop::collection::btree_map(
            string().prop_filter("set by the parser", |k| k != "dynamic"),
            value(),
            1..=3,
        ),
    ]
    .boxed()
}

fn col() -> impl Strategy<Value = ColumnName> {
    prop::sample::select(vec!["a", "b", "c", "col1", "amount", "val"])
        .prop_map(|s| ColumnName::try_new(s).unwrap())
}
/// An aggregate input: a plain column, or arithmetic over columns and
/// literals, built as a tree so every operator and nesting shape is reached.
fn input() -> impl Strategy<Value = Expression> {
    let leaf = prop_oneof![
        col().prop_map(Expression::from),
        prop::sample::select(vec!["2", "0.5", "100"]).prop_map(|n| n.parse().unwrap()),
    ];
    leaf.prop_recursive(3, 12, 2, |inner| {
        prop_oneof![
            inner.clone().prop_map(|e| Expression::Negate(Box::new(e))),
            (
                prop::sample::select(vec![
                    BinaryOp::Add,
                    BinaryOp::Subtract,
                    BinaryOp::Multiply,
                    BinaryOp::Divide,
                ]),
                inner.clone(),
                inner,
            )
                .prop_map(|(op, left, right)| Expression::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }),
        ]
    })
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
/// A `sum`/`min`/`max` input with a `value_type` the loader accepts for it:
/// any scalar for a plain column, a numeric one for an expression.
fn typed_input() -> impl Strategy<Value = (Expression, FlussoType)> {
    use FlussoType::*;
    let numeric = prop::sample::select(vec![Short, Integer, Long, Float, Double, Decimal]);
    (input(), scalar_type(), numeric).prop_map(|(input, scalar, numeric)| {
        let value_type = if input.as_column().is_some() {
            scalar
        } else {
            numeric
        };
        (input, value_type)
    })
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

fn junction_filters() -> impl Strategy<Value = Option<Vec<Filter>>> {
    prop::option::of(
        col()
            .prop_map(|column| {
                Filter::NullCheck(NullCheckFilter {
                    column,
                    op: NullOp::IsNull,
                })
            })
            .prop_map(|filter| vec![filter]),
    )
}
fn through() -> impl Strategy<Value = Through> {
    (table(), col(), col(), junction_filters()).prop_map(|(table, left_key, right_key, filters)| {
        Through {
            table,
            left_key,
            right_key,
            filters,
        }
    })
}
/// Up to two distinct `on` pairs (a duplicate is rejected at load).
fn on_pairs() -> impl Strategy<Value = Vec<ColumnPair>> {
    prop::collection::vec((col(), col()), 0..=2).prop_map(|pairs| {
        let mut on: Vec<ColumnPair> = Vec::new();
        for (local, related) in pairs {
            let pair = ColumnPair { local, related };
            if !on.contains(&pair) {
                on.push(pair);
            }
        }
        on
    })
}
fn via() -> impl Strategy<Value = Vec<ViaHop>> {
    let hop = (table(), col(), junction_filters(), on_pairs()).prop_map(
        |(table, foreign_key, filters, on)| ViaHop {
            table,
            foreign_key,
            filters,
            on,
        },
    );
    prop::collection::vec(hop, 0..=2)
}
fn agg_key() -> impl Strategy<Value = AggregateKey> {
    prop_oneof![
        col().prop_map(AggregateKey::Direct),
        through().prop_map(AggregateKey::Through),
    ]
}
fn order_by_opt() -> impl Strategy<Value = Option<Vec<OrderBy>>> {
    use kernel::Direction;
    let dir = prop_oneof![
        Just(None),
        Just(Some(Direction::Asc)),
        Just(Some(Direction::Desc))
    ];
    let one = (col(), dir).prop_map(|(column, direction)| OrderBy { column, direction });
    prop_oneof![Just(None), prop::collection::vec(one, 1..=2).prop_map(Some)]
}

/// A declared enum order: sometimes empty (a bare enum), sometimes an ordered
/// prefix of a fixed variant list (unique, valid tokens).
fn enum_variants() -> impl Strategy<Value = Vec<String>> {
    prop::sample::select(vec![
        vec![],
        vec!["low"],
        vec!["low", "medium"],
        vec!["low", "medium", "high"],
    ])
    .prop_map(|vs| vs.into_iter().map(String::from).collect())
}

fn scalar_source() -> BoxedStrategy<FieldSource> {
    (
        scalar_type(),
        col(),
        any::<bool>(),
        transforms(),
        enum_variants(),
        any::<bool>(),
        any::<bool>(),
        default(),
    )
        .prop_map(
            |(ty, column, nullable, transforms, variants, bare, stored, default)| {
                // A declared order only rides an `enum`; every other type has none.
                let enum_order = if matches!(ty, FlussoType::Enum) {
                    variants
                } else {
                    Vec::new()
                };
                FieldSource::Column(Column {
                    column,
                    nullable,
                    transforms,
                    default,
                    enum_order,
                    subfields: !(bare
                        && matches!(
                            ty,
                            FlussoType::Text | FlussoType::Keyword | FlussoType::Uuid
                        )),
                    indexed: !(stored && matches!(ty, FlussoType::Json)),
                    ty,
                })
            },
        )
        .boxed()
}
/// A script field: a script-returnable leaf, optionally an array, with a source
/// that exercises YAML quoting (backticks, quotes, a trailing newline, several lines).
fn script_source() -> BoxedStrategy<FieldSource> {
    use FlussoType::*;
    (
        prop::sample::select(vec![
            Text, Keyword, Enum, Uuid, Boolean, Long, Double, Decimal, Date, Timestamp,
        ]),
        prop::sample::select(vec![
            "this.a",
            "`${this.a} ${root.b}`",
            "\"quoted: yes\"\n",
            "let x = 1;\nx + 1\n",
        ]),
        any::<bool>(),
        any::<bool>(),
        enum_variants(),
        any::<bool>(),
    )
        .prop_map(|(ty, source, nullable, array, variants, bare)| {
            let enum_order = if matches!(ty, Enum) {
                variants
            } else {
                Vec::new()
            };
            FieldSource::Script(kernel::Script {
                source: source.to_owned(),
                nullable,
                array,
                enum_order,
                subfields: !(bare && matches!(ty, Text | Keyword | Uuid)),
                ty,
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
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
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
    (pg, os, col(), any::<bool>(), default())
        .prop_map(|(postgres, opensearch, column, nullable, default)| {
            FieldSource::Column(Column {
                column,
                ty: FlussoType::Custom {
                    postgres: postgres.into_iter().map(String::from).collect(),
                    opensearch: opensearch.to_string(),
                },
                nullable,
                transforms: Vec::new(),
                default,
                enum_order: Vec::new(),
                subfields: true,
                indexed: true,
            })
        })
        .boxed()
}
fn constant_source() -> BoxedStrategy<FieldSource> {
    value().prop_map(FieldSource::Constant).boxed()
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
                distinct: false,
                via: Vec::new(),
                on: vec![],
            }))
        };
    let ops = prop_oneof![
        (table(), agg_key()).prop_map(move |(t, k)| mk(t, AggregateOp::Count, k, None)),
        (table(), agg_key()).prop_map(move |(t, k)| mk(t, AggregateOp::Exists, k, None)),
        (table(), typed_input(), agg_key()).prop_map(move |(t, (c, vt), k)| mk(
            t,
            AggregateOp::Sum(c),
            k,
            Some(vt)
        )),
        (table(), typed_input(), agg_key()).prop_map(move |(t, (c, vt), k)| mk(
            t,
            AggregateOp::Min(c),
            k,
            Some(vt)
        )),
        (table(), typed_input(), agg_key()).prop_map(move |(t, (c, vt), k)| mk(
            t,
            AggregateOp::Max(c),
            k,
            Some(vt)
        )),
        (table(), input(), agg_key()).prop_map(move |(t, c, k)| mk(
            t,
            AggregateOp::Avg(c),
            k,
            None
        )),
        (table(), elem_type(), agg_key()).prop_map(move |(t, et, k)| mk(
            t,
            AggregateOp::Ids { element_type: et },
            k,
            None
        )),
    ];
    (ops, any::<bool>(), via(), on_pairs())
        .prop_map(|(mut source, distinct, via, on)| {
            if let FieldSource::Relation(Relation::Aggregate(aggregate)) = &mut source {
                let direct = matches!(aggregate.key, AggregateKey::Direct(_));
                aggregate.distinct = distinct && !direct;
                aggregate.via = via;
                if direct {
                    aggregate.on = on;
                }
            }
            source
        })
        .boxed()
}

fn join_source(depth: u32) -> BoxedStrategy<FieldSource> {
    (join_shape(depth), any::<bool>(), on_pairs())
        .prop_map(|(mut source, indexed, on)| {
            if let FieldSource::Relation(Relation::Join(join)) = &mut source {
                join.indexed = indexed;
                if !matches!(join.kind, JoinKind::ManyToMany { .. }) {
                    join.on = on;
                }
            }
            source
        })
        .boxed()
}

fn join_shape(depth: u32) -> BoxedStrategy<FieldSource> {
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
            indexed: true,
            on: vec![],
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
        script_source(),
        constant_source(),
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
    prop::collection::vec(
        (source(depth), prop::bool::weighted(0.85), options()),
        1..=4,
    )
    .prop_map(|sources| {
        sources
            .into_iter()
            .enumerate()
            .map(|(i, (source, emit, options))| Field {
                field: FieldName::try_new(format!("f{i}")).unwrap(),
                options: options_for(&source, options),
                emit,
                source,
            })
            .collect()
    })
    .boxed()
}

/// The options a field of this source carries after parsing: the generated
/// ones where the field takes options (an unindexed field, a constant and a
/// group take none here), plus the `dynamic` the parser sets on a `map`.
fn options_for(
    source: &FieldSource,
    mut options: BTreeMap<String, GenericValue>,
) -> BTreeMap<String, GenericValue> {
    match source {
        FieldSource::Column(c) if matches!(c.ty, FlussoType::Map { .. }) => {
            options.insert("dynamic".to_owned(), GenericValue::Bool(true));
            options
        }
        FieldSource::Column(c) if c.indexed => options,
        FieldSource::Geo(_) | FieldSource::Script(_) => options,
        FieldSource::Relation(Relation::Aggregate(_)) => options,
        _ => BTreeMap::new(),
    }
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

/// 256 cases, or `PROPTEST_CASES` when set.
fn config() -> ProptestConfig {
    ProptestConfig {
        cases: std::env::var("PROPTEST_CASES")
            .ok()
            .and_then(|n| n.parse().ok())
            .unwrap_or(256),
        ..ProptestConfig::default()
    }
}

/// A typed number's exact value, read independently of the designer: `None`
/// when no decimal holds it exactly.
fn exact(text: &str) -> Option<Decimal> {
    let text = text.trim().trim_start_matches('+');
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.trim_start_matches('+').parse::<i64>().ok()?),
        None => (text, 0),
    };
    let mut value = Decimal::from_str_exact(mantissa).ok()?;
    for _ in 0..exponent.unsigned_abs() {
        value = if exponent > 0 {
            value.checked_mul(Decimal::TEN)?
        } else {
            let next = value.checked_div(Decimal::TEN)?;
            (next.checked_mul(Decimal::TEN)? == value).then_some(next)?
        };
    }
    Some(value)
}

proptest! {
    #![proptest_config(config())]

    /// A number typed into the inspector is either refused or stored as exactly
    /// its typed value, in the form its saved file reads back as.
    #[test]
    fn a_typed_number_is_refused_or_saved_as_its_exact_value(text in typed_number()) {
        let decimal = |v: &GenericValue| match v {
            GenericValue::BigInt(n) => Some(Decimal::from(*n)),
            GenericValue::Decimal(d) => Some(*d),
            _ => None,
        };
        match design::value::read_typed(&text, design::value::ValueSlot::Constant) {
            Ok(value) => {
                let read = decimal(&value);
                prop_assert!(read.is_some(), "{text:?} became {value:?}");
                prop_assert_eq!(read, exact(&text), "{:?} changed value", text);
                let integer_form = !text.contains(['.', 'e', 'E']);
                prop_assert_eq!(
                    matches!(value, GenericValue::BigInt(_)),
                    integer_form && text.trim().parse::<i64>().is_ok(),
                    "{:?} read as {:?}", text, value
                );
            }
            Err(problem) => {
                use design::value::ValueProblem::{Inexact, OutOfRange};
                prop_assert!(matches!(problem, Inexact { .. } | OutOfRange { .. }), "{text:?}: {problem:?}");
                let short = text.trim().trim_start_matches(['+', '-']).trim_start_matches('0');
                let significant = short.chars().filter(char::is_ascii_digit).count();
                prop_assert!(
                    !(text.trim().trim_start_matches('+').parse::<i64>().is_ok()
                        || (significant <= 15 && !text.contains(['e', 'E']) && short.len() <= 18)),
                    "{:?} refused as {:?} though the file holds it", text, problem
                );
            }
        }
    }

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

/// One edit the designer can make to a fields list somewhere in the tree.
#[derive(Debug, Clone)]
enum Edit {
    Remove(usize),
    Move(usize, usize),
    Rename(usize),
    Replace(usize, FieldSource),
    Insert(usize, FieldSource),
    Required(usize),
}

fn edit() -> impl Strategy<Value = (usize, Edit)> {
    let at = 0..8usize;
    let kind = prop_oneof![
        2 => at.clone().prop_map(Edit::Remove),
        2 => (at.clone(), at.clone()).prop_map(|(a, b)| Edit::Move(a, b)),
        2 => at.clone().prop_map(Edit::Rename),
        1 => (at.clone(), source(1)).prop_map(|(i, s)| Edit::Replace(i, s)),
        1 => (at.clone(), source(1)).prop_map(|(i, s)| Edit::Insert(i, s)),
        1 => at.prop_map(Edit::Required),
    ];
    (0..16usize, kind)
}

fn children(field: &Field) -> &[Field] {
    match &field.source {
        FieldSource::Group(children) => children,
        FieldSource::Relation(Relation::Join(join)) => &join.fields,
        _ => &[],
    }
}

fn is_container(field: &Field) -> bool {
    matches!(
        field.source,
        FieldSource::Group(_) | FieldSource::Relation(Relation::Join(_))
    )
}

/// A schema and the designer's id of every field, kept in step through edits.
#[derive(Debug, Clone)]
struct Tracked {
    schema: IndexSchema,
    ids: Vec<FieldId>,
    next: usize,
}

impl Tracked {
    fn new(schema: IndexSchema) -> Self {
        let mut tracked = Self {
            ids: Vec::new(),
            schema,
            next: 0,
        };
        tracked.ids = tracked.fresh(&tracked.schema.fields.clone());
        tracked
    }

    fn fresh(&mut self, fields: &[Field]) -> Vec<FieldId> {
        fields
            .iter()
            .map(|field| {
                self.next += 1;
                let uid = format!("u{}", self.next);
                FieldId {
                    name: field.field.as_ref().to_owned(),
                    fields: self.fresh(children(field)),
                    uid,
                }
            })
            .collect()
    }

    fn apply(&mut self, (list, edit): &(usize, Edit)) {
        let lists = count_lists(&self.schema.fields);
        let mut pick = list % lists;
        let path = nth_path(&self.schema.fields, &mut pick, &mut Vec::new()).unwrap();
        self.next += 1;
        let serial = self.next;
        let fresh_ids = match edit {
            Edit::Replace(_, source) | Edit::Insert(_, source) => {
                let probe = Field {
                    field: FieldName::try_new("probe").unwrap(),
                    options: BTreeMap::new(),
                    emit: true,
                    source: source.clone(),
                };
                self.fresh(children(&probe))
            }
            _ => Vec::new(),
        };
        let (fields, ids) = lists_at(&mut self.schema.fields, &mut self.ids, &path);
        let n = fields.len();
        match edit {
            Edit::Remove(i) if n > 1 => {
                fields.remove(i % n);
                ids.remove(i % n);
            }
            Edit::Remove(_) => {}
            Edit::Move(a, b) => {
                let field = fields.remove(a % n);
                let id = ids.remove(a % n);
                fields.insert(b % n, field);
                ids.insert(b % n, id);
            }
            Edit::Rename(i) => {
                let name = format!("rn{serial}");
                fields[i % n].field = FieldName::try_new(&name).unwrap();
                ids[i % n].name = name;
            }
            Edit::Replace(i, source) => {
                fields[i % n].options = options_for(source, BTreeMap::new());
                fields[i % n].source = source.clone();
                ids[i % n].fields = fresh_ids;
            }
            Edit::Insert(i, source) => {
                let name = format!("add{serial}");
                fields.insert(
                    i % (n + 1),
                    Field {
                        field: FieldName::try_new(&name).unwrap(),
                        options: options_for(source, BTreeMap::new()),
                        emit: true,
                        source: source.clone(),
                    },
                );
                ids.insert(
                    i % (n + 1),
                    FieldId {
                        name,
                        uid: format!("u{serial}"),
                        fields: fresh_ids,
                    },
                );
            }
            Edit::Required(i) => match &mut fields[i % n].source {
                FieldSource::Column(column) => column.nullable = !column.nullable,
                FieldSource::Script(script) => script.nullable = !script.nullable,
                _ => {}
            },
        }
    }
}

/// How many fields lists the tree has.
fn count_lists(fields: &[Field]) -> usize {
    1 + fields
        .iter()
        .filter(|f| is_container(f))
        .map(|f| count_lists(children(f)))
        .sum::<usize>()
}

/// The positions leading to the `n`th fields list, depth first.
fn nth_path(fields: &[Field], n: &mut usize, path: &mut Vec<usize>) -> Option<Vec<usize>> {
    if *n == 0 {
        return Some(path.clone());
    }
    *n -= 1;
    for (i, field) in fields.iter().enumerate() {
        if !is_container(field) {
            continue;
        }
        path.push(i);
        if let Some(found) = nth_path(children(field), n, path) {
            return Some(found);
        }
        path.pop();
    }
    None
}

fn lists_at<'a>(
    fields: &'a mut Vec<Field>,
    ids: &'a mut Vec<FieldId>,
    path: &[usize],
) -> (&'a mut Vec<Field>, &'a mut Vec<FieldId>) {
    let Some((&i, rest)) = path.split_first() else {
        return (fields, ids);
    };
    let children = match &mut fields[i].source {
        FieldSource::Group(children) => children,
        FieldSource::Relation(Relation::Join(join)) => &mut join.fields,
        _ => unreachable!("a path leads through containers"),
    };
    lists_at(children, &mut ids[i].fields, rest)
}

/// How a generated file is laid out: what a hand-written one varies.
#[derive(Debug, Clone)]
struct Layout {
    /// Spaces per level.
    width: usize,
    /// A pseudo-random stream for the per-list and per-item choices.
    seed: u64,
}

impl Layout {
    fn roll(&mut self, one_in: u64) -> bool {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed % one_in == 0
    }
}

fn layout() -> impl Strategy<Value = Layout> {
    (2usize..=4, any::<u64>()).prop_map(|(width, seed)| Layout {
        width,
        seed: seed | 1,
    })
}

/// What a generated file wrote, to check a save against.
#[derive(Debug, Default)]
struct Written {
    /// Every `# heading N` and `# end of list` comment, with the list it is in
    /// (`""` for the root list).
    list_comments: Vec<(String, String)>,
    /// Fields whose `# inner` comment was written.
    inner: Vec<String>,
}

/// `key: value` as serde_yaml prints it, indented to `col`.
fn entry_lines(key: &serde_yaml::Value, value: &serde_yaml::Value, col: usize) -> Vec<String> {
    let mut map = serde_yaml::Mapping::new();
    map.insert(key.clone(), value.clone());
    let text = serde_yaml::to_string(&map).unwrap();
    text.lines()
        .map(|l| format!("{}{l}", " ".repeat(col)))
        .collect()
}

/// The schema as a hand-written file: the layout's indent width, each list
/// indented under its key or not, blank-line groups and section headings,
/// every field with a comment above it, one at the end of its first line and
/// sometimes one inside it, and some lists with a comment after their last
/// item. Keys in `omit` (field uid, key) are left out.
fn write(
    schema: &IndexSchema,
    ids: &[FieldId],
    layout: &mut Layout,
    omit: &[(String, String)],
) -> (String, Written) {
    let tree = design::codegen::schema_value(schema).unwrap();
    let serde_yaml::Value::Mapping(root) = tree else {
        unreachable!()
    };
    let mut out = vec!["# A header.".to_owned(), String::new()];
    let mut written = Written::default();
    let mut headings = 0;
    for (key, value) in &root {
        if key.as_str() == Some("fields") {
            out.push("fields:".to_owned());
            let last = true;
            write_fields(
                value,
                ids,
                0,
                "",
                last,
                layout,
                omit,
                &mut out,
                &mut written,
                &mut headings,
            );
        } else {
            out.extend(entry_lines(key, value, 0));
        }
    }
    (out.join("\n") + "\n", written)
}

#[allow(clippy::too_many_arguments)]
fn write_fields(
    items: &serde_yaml::Value,
    ids: &[FieldId],
    key_col: usize,
    owner: &str,
    last_key: bool,
    layout: &mut Layout,
    omit: &[(String, String)],
    out: &mut Vec<String>,
    written: &mut Written,
    headings: &mut usize,
) -> bool {
    let items = items.as_sequence().unwrap();
    if items.is_empty() {
        let last = out.last_mut().unwrap();
        last.push_str(" []");
        return false;
    }
    let dash = if layout.roll(3) {
        key_col
    } else {
        key_col + layout.width
    };
    let pad = " ".repeat(dash);
    let keeps_blanks = |line: &String| {
        line.trim_end()
            .rsplit([' ', '-'])
            .next()
            .is_some_and(|last| last.starts_with(['|', '>']) && last.contains('+'))
    };
    for (item, id) in items.iter().zip(ids) {
        // A blank line after a keep-chomping block scalar would be its content.
        let after_kept = out
            .iter()
            .rev()
            .take_while(|l| !l.trim_start().starts_with("# about"))
            .any(keeps_blanks);
        if after_kept {
        } else if layout.roll(4) {
            *headings += 1;
            out.push(String::new());
            out.push(format!("{pad}# heading {headings}"));
            out.push(String::new());
            written
                .list_comments
                .push((owner.to_owned(), format!("# heading {headings}")));
        } else if layout.roll(4) {
            out.push(String::new());
        }
        out.push(format!("{pad}# about {}", id.uid));
        let map = item.as_mapping().unwrap();
        let col = dash + 2;
        // A comment at the item's column after a list written at that column
        // is the list's, not the item's.
        let mut ends_in_compact_list = false;
        for (k, (key, value)) in map.iter().enumerate() {
            if k > 0 && omit.contains(&(id.uid.clone(), key.as_str().unwrap().to_owned())) {
                continue;
            }
            if key.as_str() == Some("fields") {
                out.push(format!("{}fields:", " ".repeat(col)));
                let last = k + 1 == map.len();
                ends_in_compact_list = write_fields(
                    value, &id.fields, col, &id.uid, last, layout, omit, out, written, headings,
                );
                continue;
            }
            ends_in_compact_list = value.as_sequence().is_some_and(|items| !items.is_empty());
            let mut lines = entry_lines(key, value, col);
            if k == 0 {
                lines[0].replace_range(dash..col, "- ");
                lines[0].push_str(&format!(" # tail {}", id.uid));
            }
            out.extend(lines);
        }
        if !ends_in_compact_list && layout.roll(3) {
            out.push(format!("{}# inner {}", " ".repeat(col), id.uid));
            written.inner.push(id.uid.clone());
        }
    }
    // At the key's own column, a comment above the next key is that key's.
    if (dash > key_col || last_key) && layout.roll(3) {
        out.push(format!("{pad}# end of list {owner}"));
        written
            .list_comments
            .push((owner.to_owned(), format!("# end of list {owner}")));
    }
    dash == key_col
}

fn all_ids<'a>(ids: &'a [FieldId], into: &mut BTreeMap<&'a str, &'a FieldId>) {
    for id in ids {
        into.insert(&id.uid, id);
        all_ids(&id.fields, into);
    }
}

/// The lists (by owner, `""` for the root) that exist both before and after.
fn living_lists(
    before: &[FieldId],
    after: &[FieldId],
    schema_before: &IndexSchema,
    schema_after: &IndexSchema,
) -> Vec<String> {
    fn containers(fields: &[Field], ids: &[FieldId], into: &mut Vec<String>) {
        for (field, id) in fields.iter().zip(ids) {
            if is_container(field) {
                into.push(id.uid.clone());
                containers(children(field), &id.fields, into);
            }
        }
    }
    let (mut was, mut is) = (Vec::new(), Vec::new());
    containers(&schema_before.fields, before, &mut was);
    containers(&schema_after.fields, after, &mut is);
    let mut living: Vec<String> = was.into_iter().filter(|uid| is.contains(uid)).collect();
    living.push(String::new());
    living
}

fn read(text: &str) -> Result<serde_json::Value, String> {
    let entity = SchemaYaml::try_parse(text).map_err(|e| e.to_string())?;
    IndexSchema::try_from(entity)
        .map(|s| serde_json::to_value(&s).unwrap())
        .map_err(|e| e.to_string())
}

/// A hand-written-looking file for `tracked`, with up to three keys a field
/// item may leave out (ones whose absence reads back the same) left out.
fn hand_written(tracked: &Tracked, layout: &Layout) -> (String, Written) {
    let expected = Ok(serde_json::to_value(&tracked.schema).unwrap());
    let mut omit: Vec<(String, String)> = Vec::new();
    let tree = design::codegen::schema_value(&tracked.schema).unwrap();
    let mut candidates = Vec::new();
    fn keys(value: &serde_yaml::Value, ids: &[FieldId], into: &mut Vec<(String, String)>) {
        let Some(items) = value.get("fields").and_then(|v| v.as_sequence()) else {
            return;
        };
        for (item, id) in items.iter().zip(ids) {
            for key in item.as_mapping().unwrap().keys().skip(1) {
                if key.as_str() != Some("fields") {
                    into.push((id.uid.clone(), key.as_str().unwrap().to_owned()));
                }
            }
            keys(item, &id.fields, into);
        }
    }
    keys(&tree, &tracked.ids, &mut candidates);
    let mut picker = layout.clone();
    for candidate in candidates {
        if omit.len() == 3 || !picker.roll(4) {
            continue;
        }
        omit.push(candidate);
        let (text, _) = write(&tracked.schema, &tracked.ids, &mut layout.clone(), &omit);
        if read(&text) != expected {
            omit.pop();
        }
    }
    write(&tracked.schema, &tracked.ids, &mut layout.clone(), &omit)
}

proptest! {
    #![proptest_config(config())]

    /// Edits saved into a hand-written, commented file: every field that
    /// survives keeps exactly its own comments, a deleted field's comments
    /// are gone, and no other comment is lost or duplicated.
    #[test]
    fn each_comment_stays_with_its_field_through_a_save(
        schema in schema(),
        layout in layout(),
        edits in prop::collection::vec(edit(), 1..=5),
    ) {
        let tracked = Tracked::new(schema);
        let (base, written) = hand_written(&tracked, &layout);
        prop_assert_eq!(read(&base), Ok(serde_json::to_value(&tracked.schema).unwrap()), "{}", base);

        let untouched = design::keep::schema_yaml(Some(&base), &tracked.schema, None).unwrap();
        prop_assert_eq!(&untouched.text, &base);

        let mut edited = tracked.clone();
        for edit in &edits {
            edited.apply(edit);
        }
        let identity = SchemaIdentity { before: tracked.ids.clone(), after: edited.ids.clone() };
        let saved = design::keep::schema_yaml(Some(&base), &edited.schema, Some(&identity)).unwrap();
        prop_assert!(saved.notice.is_none(), "fell back:\n{}\n--- base ---\n{}", saved.text, base);
        prop_assert_eq!(saved.dropped, 0, "{}\n--- base ---\n{}", saved.text, base);
        prop_assert_eq!(read(&saved.text), Ok(serde_json::to_value(&edited.schema).unwrap()), "{}", saved.text);

        let text = &saved.text;
        let lines: Vec<&str> = text.lines().collect();
        let count = |needle: &str| lines.iter().filter(|l| l.trim_start() == needle || l.ends_with(&format!(" {needle}"))).count();
        let ctx = format!("\n--- saved ---\n{text}\n--- base ---\n{base}\n--- edits ---\n{edits:?}");
        let (mut was, mut is) = (BTreeMap::new(), BTreeMap::new());
        all_ids(&tracked.ids, &mut was);
        all_ids(&edited.ids, &mut is);
        prop_assert_eq!(count("# A header."), 1, "{}", ctx);
        for uid in was.keys() {
            let about = format!("# about {uid}");
            let tail = format!("# tail {uid}");
            let inner = format!("# inner {uid}");
            let wrote_inner = written.inner.iter().any(|u| u == uid);
            match is.get(uid) {
                None => {
                    prop_assert_eq!(count(&about) + count(&tail) + count(&inner), 0, "deleted {}{}", uid, ctx);
                }
                Some(id) => {
                    prop_assert_eq!(count(&about), 1, "{}{}", about, ctx);
                    prop_assert_eq!(count(&tail), 1, "{}{}", tail, ctx);
                    prop_assert_eq!(count(&inner), usize::from(wrote_inner), "{}{}", inner, ctx);
                    let at = lines.iter().position(|l| l.trim_start() == about).unwrap();
                    let dash = lines[at + 1];
                    prop_assert!(dash.ends_with(&format!(" {tail}")), "{} not above its field{}", about, ctx);
                    let col = dash.len() - dash.trim_start().len();
                    prop_assert!(dash[col..].starts_with("- "), "{}", ctx);
                    let first: serde_yaml::Mapping = serde_yaml::from_str(&dash[col + 2..]).unwrap();
                    prop_assert_eq!(first.values().next().and_then(|v| v.as_str()), Some(id.name.as_str()), "{}{}", about, ctx);
                    if wrote_inner {
                        let end = lines.iter().position(|l| l.trim_start() == inner).unwrap();
                        prop_assert!(end > at + 1, "{}{}", inner, ctx);
                        for line in &lines[at + 2..end] {
                            let d = line.len() - line.trim_start().len();
                            let content = !line.trim().is_empty() && !line.trim_start().starts_with('#');
                            prop_assert!(!(content && d <= col), "{} left its field at {:?}{}", inner, line, ctx);
                        }
                    }
                }
            }
        }
        for uid in is.keys().filter(|uid| !was.contains_key(*uid)) {
            prop_assert_eq!(count(&format!("# about {uid}")), 0, "{}", ctx);
        }
        let living = living_lists(&tracked.ids, &edited.ids, &tracked.schema, &edited.schema);
        for (owner, comment) in &written.list_comments {
            let expected = usize::from(living.contains(owner));
            prop_assert_eq!(count(comment), expected, "{} of list {:?}{}", comment, owner, ctx);
        }
    }
}
