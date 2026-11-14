//! The differential guard for the document reader (issue #327): every field kind
//! but a declared `decimal`, fed the JSON Postgres writes, must reach the sink
//! byte for byte as it did before the reader learned to keep a decimal's digits.
//!
//! `differential.expected` was recorded from `main`'s reader and sink render
//! (`coerce_document` over a `serde_json::Value`, then `sink::to_json`) at
//! 26a81616, by running this module there with `FLUSSO_BLESS=1`. Re-bless only
//! after a deliberate change to what the sink receives, and say so in the PR.
//!
//! A case is one schema level and one document text; its line is `name`, a tab,
//! and `OK <sink JSON>`, `ERR <field>: <message>` or `MALFORMED`.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use kernel::{
    Aggregate, AggregateKey, AggregateOp, Column, ColumnName, Field, FieldName, FieldSource,
    FlussoType, GenericValue, Geo, Join, JoinKind, Relation, Script, TableName,
};

use super::{DocumentError, coerce_document};

/// What the sink receives for `case`, as one line.
fn render(case: &Case) -> String {
    match coerce_document(case.doc.as_bytes(), &case.fields) {
        Ok(body) => format!("OK {}", serde_json::to_string(&sink::Json(&body)).unwrap()),
        Err(DocumentError::Failed(failure)) => {
            format!("ERR {}: {}", failure.field, failure.message)
        }
        Err(DocumentError::Malformed(_)) => "MALFORMED".to_owned(),
    }
}

const EXPECTED: &str = include_str!("differential.expected");

#[test]
fn every_field_but_a_declared_decimal_reaches_the_sink_as_before() {
    let mut actual = String::new();
    for case in cases() {
        let line = render(&case).replace('\n', "\\n");
        writeln!(actual, "{}\t{line}", case.name).unwrap();
    }
    if std::env::var_os("FLUSSO_BLESS").is_some() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/document/value/differential.expected"
        );
        std::fs::write(path, &actual).unwrap();
        return;
    }
    let expected: BTreeMap<&str, &str> = EXPECTED
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .collect();
    let mut drifted = Vec::new();
    let mut seen = 0;
    for line in actual.lines() {
        let (name, got) = line.split_once('\t').unwrap();
        seen += 1;
        match expected.get(name) {
            Some(want) if *want == got => {}
            Some(want) => drifted.push(format!("{name}\n  main:   {want}\n  branch: {got}")),
            None => drifted.push(format!("{name}: not in the recording")),
        }
    }
    assert_eq!(
        seen,
        expected.len(),
        "the corpus and the recording differ in size"
    );
    assert!(
        drifted.is_empty(),
        "{} of {seen} cases reach the sink differently from main:\n{}",
        drifted.len(),
        drifted.join("\n")
    );
}

/// One schema level and one document, as the query returns it.
struct Case {
    name: String,
    fields: Vec<Field>,
    doc: String,
}

/// JSON values as Postgres writes them into a document: `numeric` and float
/// output, text, `bytea` hex, dates, `json`/`jsonb` content, nulls.
const VALUES: &[&str] = &[
    "0",
    "5",
    "-7",
    "5.0",
    "100.0",
    "1.5",
    "-0",
    "-0.0",
    "0.1",
    "1e+16",
    "1e-07",
    "1.5e+300",
    "1e400",
    "9007199254740993",
    "9223372036854775807",
    "9223372036854775808",
    "18446744073709551616",
    "123456789012345678901234567890",
    "1234567890.123456789",
    "12.50",
    "0.000000000000000000000000000001234",
    "79228162514264337593543950335",
    r#""NaN""#,
    r#""Infinity""#,
    r#""-Infinity""#,
    r#""abc""#,
    r#""5""#,
    r#""12.50""#,
    r#""1e30""#,
    r#""\\x0102""#,
    r#""\\xzz""#,
    r#""2024-01-02""#,
    r#""2024-01-02T03:04:05.123456""#,
    r#""2024-01-02T03:04:05+00:00""#,
    r#""03:04:05""#,
    r#""03:04:05.123456+02""#,
    r#""6f1c3b2a-9d4e-4c1a-8b7f-2e5d6c7a8b90""#,
    r#""""#,
    r#""é\n\"q\"""#,
    r#""1.5,2.5""#,
    "true",
    "false",
    "null",
    "{}",
    "[]",
    r#"{"a":1.0,"b":[1,2.5,null],"c":{"d":"x"}}"#,
    r#"[1,"x",null,2.5]"#,
    "[[1]]",
    r#"{"lat":1.5,"lon":2.0}"#,
    "[2.0,1.5]",
    r#"{"x":null,"y":5.0,"z":1e+16}"#,
    r#"["a","b"]"#,
    "[1.0,2,-0.0]",
    r#"{"x":{"y":[1.25]}}"#,
];

fn name(n: &str) -> FieldName {
    FieldName::try_new(n).unwrap()
}

fn col(n: &str) -> ColumnName {
    ColumnName::try_new(n).unwrap()
}

fn column(n: &str, ty: FlussoType, array: bool) -> Field {
    Field {
        field: name(n),
        options: BTreeMap::new(),
        emit: true,
        source: FieldSource::Column(Column {
            column: col(n),
            ty,
            nullable: true,
            transforms: Vec::new(),
            default: None,
            enum_order: Vec::new(),
            subfields: true,
            indexed: true,
            array,
        }),
    }
}

fn aggregate(n: &str, op: AggregateOp, value_type: Option<FlussoType>) -> Field {
    Field {
        field: name(n),
        options: BTreeMap::new(),
        emit: true,
        source: FieldSource::Relation(Relation::Aggregate(Aggregate {
            table: TableName::try_new("parts").unwrap(),
            op,
            key: AggregateKey::Direct(col("owner_id")),
            value_type,
            filters: None,
            distinct: false,
            via: Vec::new(),
            on: Vec::new(),
        })),
    }
}

fn join(n: &str, kind: JoinKind, fields: Vec<Field>, indexed: bool) -> Field {
    Field {
        field: name(n),
        options: BTreeMap::new(),
        emit: true,
        source: FieldSource::Relation(Relation::Join(Join {
            table: TableName::try_new("items").unwrap(),
            kind,
            primary_key: col("id"),
            nullable: true,
            filters: None,
            order_by: None,
            limit: None,
            fields,
            indexed,
            on: Vec::new(),
        })),
    }
}

fn has_many() -> JoinKind {
    JoinKind::HasMany {
        foreign_key: col("owner_id"),
    }
}

fn belongs_to() -> JoinKind {
    JoinKind::BelongsTo {
        column: col("item_id"),
    }
}

/// Every scalar type but `decimal`, with a name for the case.
fn scalar_types() -> Vec<(&'static str, FlussoType)> {
    let map = |values: FlussoType| FlussoType::Map {
        values: Box::new(values),
    };
    vec![
        ("text", FlussoType::Text),
        ("identifier", FlussoType::Identifier),
        ("keyword", FlussoType::Keyword),
        ("enum", FlussoType::Enum),
        ("uuid", FlussoType::Uuid),
        ("boolean", FlussoType::Boolean),
        ("short", FlussoType::Short),
        ("integer", FlussoType::Integer),
        ("long", FlussoType::Long),
        ("float", FlussoType::Float),
        ("double", FlussoType::Double),
        ("date", FlussoType::Date),
        ("timestamp", FlussoType::Timestamp),
        ("binary", FlussoType::Binary),
        ("json", FlussoType::Json),
        ("geopoint", FlussoType::GeoPoint),
        (
            "custom",
            FlussoType::Custom {
                postgres: vec!["numeric".to_owned()],
                opensearch: "scaled_float".to_owned(),
            },
        ),
        ("map_text", map(FlussoType::Text)),
        ("map_keyword", map(FlussoType::Keyword)),
        ("map_long", map(FlussoType::Long)),
        ("map_double", map(FlussoType::Double)),
        ("map_date", map(FlussoType::Date)),
    ]
}

fn one(fields: Vec<Field>, key: &str, value: &str) -> (Vec<Field>, String) {
    (fields, format!(r#"{{"{key}":{value}}}"#))
}

fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    let mut push = |name: String, (fields, doc): (Vec<Field>, String)| {
        out.push(Case { name, fields, doc });
    };

    for (i, value) in VALUES.iter().enumerate() {
        for (ty_name, ty) in scalar_types() {
            push(
                format!("column/{ty_name}/{i}"),
                one(vec![column("f", ty.clone(), false)], "f", value),
            );
            push(
                format!("array/{ty_name}/{i}"),
                one(vec![column("f", ty.clone(), true)], "f", value),
            );
            push(
                format!("array_of/{ty_name}/{i}"),
                one(
                    vec![column("f", ty.clone(), true)],
                    "f",
                    &format!("[{value},null,{value}]"),
                ),
            );
        }

        let mut geo = column("f", FlussoType::GeoPoint, false);
        geo.source = FieldSource::Geo(Geo {
            lat: col("lat"),
            lon: col("lon"),
            nullable: true,
        });
        push(format!("geo/{i}"), one(vec![geo], "f", value));

        let mut script = column("f", FlussoType::Double, false);
        script.source = FieldSource::Script(Script {
            source: "1".to_owned(),
            ty: FlussoType::Double,
            nullable: true,
            array: false,
            enum_order: Vec::new(),
            subfields: true,
        });
        push(format!("script/{i}"), one(vec![script], "f", value));

        let mut constant = column("f", FlussoType::Keyword, false);
        constant.source = FieldSource::Constant(GenericValue::BigInt(5));
        push(format!("constant/{i}"), one(vec![constant], "f", value));

        let mut hidden = column("f", FlussoType::Double, false);
        hidden.emit = false;
        push(format!("emit_false/{i}"), one(vec![hidden], "f", value));

        let mut lax = column("f", FlussoType::Double, false);
        lax.options
            .insert("ignore_malformed".to_owned(), GenericValue::Bool(true));
        push(format!("options/{i}"), one(vec![lax], "f", value));

        for (op_name, op, value_type) in [
            ("count", AggregateOp::Count, None),
            ("exists", AggregateOp::Exists, None),
            ("avg", AggregateOp::Avg(col("v").into()), None),
            (
                "sum_long",
                AggregateOp::Sum(col("v").into()),
                Some(FlussoType::Long),
            ),
            (
                "sum_double",
                AggregateOp::Sum(col("v").into()),
                Some(FlussoType::Double),
            ),
            (
                "min_keyword",
                AggregateOp::Min(col("v").into()),
                Some(FlussoType::Keyword),
            ),
            ("max_none", AggregateOp::Max(col("v").into()), None),
            (
                "max_date",
                AggregateOp::Max(col("v").into()),
                Some(FlussoType::Date),
            ),
            (
                "ids_long",
                AggregateOp::Ids {
                    element_type: FlussoType::Long,
                },
                None,
            ),
            (
                "ids_keyword",
                AggregateOp::Ids {
                    element_type: FlussoType::Keyword,
                },
                None,
            ),
            (
                "ids_binary",
                AggregateOp::Ids {
                    element_type: FlussoType::Binary,
                },
                None,
            ),
            (
                "ids_double",
                AggregateOp::Ids {
                    element_type: FlussoType::Double,
                },
                None,
            ),
        ] {
            push(
                format!("aggregate/{op_name}/{i}"),
                one(
                    vec![aggregate("f", op.clone(), value_type.clone())],
                    "f",
                    value,
                ),
            );
            push(
                format!("aggregate_list/{op_name}/{i}"),
                one(
                    vec![aggregate("f", op, value_type)],
                    "f",
                    &format!("[{value},{value}]"),
                ),
            );
        }

        let children = || {
            vec![
                column("k", FlussoType::Keyword, false),
                column("n", FlussoType::Double, false),
                column("j", FlussoType::Json, false),
            ]
        };
        for (kind_name, kind) in [("has_many", has_many()), ("belongs_to", belongs_to())] {
            for indexed in [true, false] {
                let fields = vec![join("f", kind.clone(), children(), indexed)];
                let item = format!(r#"{{"k":{value},"n":{value},"j":{value},"extra":{value}}}"#);
                push(
                    format!("join/{kind_name}/{indexed}/object/{i}"),
                    one(fields.clone(), "f", &item),
                );
                push(
                    format!("join/{kind_name}/{indexed}/list/{i}"),
                    one(fields.clone(), "f", &format!("[{item},{item}]")),
                );
                push(
                    format!("join/{kind_name}/{indexed}/bare/{i}"),
                    one(fields, "f", value),
                );
            }
        }

        let mut group = column("g", FlussoType::Json, false);
        group.source = FieldSource::Group(children());
        push(
            format!("group/{i}"),
            one(
                vec![group],
                "g",
                &format!(r#"{{"k":{value},"n":{value},"j":{value}}}"#),
            ),
        );

        push(
            format!("unknown_key/{i}"),
            (
                vec![column("f", FlussoType::Keyword, false)],
                format!(r#"{{"other":{value},"f":"x"}}"#),
            ),
        );
        push(
            format!("root/{i}"),
            (
                vec![column("f", FlussoType::Keyword, false)],
                (*value).to_owned(),
            ),
        );
    }

    namesakes(&mut push);
    wide(&mut push);
    depth(&mut push);
    out
}

/// Two fields sharing a name, in either order and kind, with the query's keys
/// in field order, duplicated, or in the order `jsonb` sorts them.
fn namesakes(push: &mut impl FnMut(String, (Vec<Field>, String))) {
    let kinds = [
        ("keyword", FlussoType::Keyword),
        ("long", FlussoType::Long),
        ("double", FlussoType::Double),
        ("binary", FlussoType::Binary),
        ("date", FlussoType::Date),
        ("json", FlussoType::Json),
    ];
    let docs = [
        ("both", r#"{"ab":"\\x0102","k":"x","ab":"\\x0103"}"#),
        ("jsonb", r#"{"k":"x","ab":"\\x0102"}"#),
        ("numbers", r#"{"ab":"NaN","k":5.0,"ab":1.5}"#),
        ("first_only", r#"{"ab":5.0,"k":"x"}"#),
    ];
    for (first_name, first) in &kinds {
        for (last_name, last) in &kinds {
            for (doc_name, doc) in docs {
                let fields = vec![
                    column("ab", first.clone(), false),
                    column("k", FlussoType::Keyword, false),
                    column("ab", last.clone(), false),
                ];
                push(
                    format!("namesake/{first_name}/{last_name}/{doc_name}"),
                    (fields, doc.to_owned()),
                );
            }
        }
        let mut constant = column("ab", FlussoType::Keyword, false);
        constant.source = FieldSource::Constant(GenericValue::BigInt(5));
        for (doc_name, doc) in docs {
            push(
                format!("namesake/constant/{first_name}/{doc_name}"),
                (
                    vec![
                        constant.clone(),
                        column("k", FlussoType::Keyword, false),
                        column("ab", first.clone(), false),
                    ],
                    doc.to_owned(),
                ),
            );
            push(
                format!("namesake/{first_name}/constant/{doc_name}"),
                (
                    vec![
                        column("ab", first.clone(), false),
                        column("k", FlussoType::Keyword, false),
                        constant.clone(),
                    ],
                    doc.to_owned(),
                ),
            );
        }
    }
}

/// More than 50 fields: the query joins chunks with `||`, so `jsonb` sorts the
/// keys by length, then bytes, and keeps the last of a repeated key.
fn wide(push: &mut impl FnMut(String, (Vec<Field>, String))) {
    let types = scalar_types();
    let readable: Vec<&str> = VALUES.iter().copied().filter(|v| *v != "1e400").collect();
    let mut fields = Vec::new();
    let mut values = Vec::new();
    for i in 0..120 {
        let (_, ty) = &types[i % types.len()];
        let field_name = format!("f{}", 119 - i);
        fields.push(column(&field_name, ty.clone(), false));
        values.push((field_name, readable[i % readable.len()]));
    }
    fields.push(column("f3", FlussoType::Binary, false));
    fields.push(column("f30", FlussoType::Keyword, false));

    let mut in_order = String::from("{");
    for (i, (key, value)) in values.iter().enumerate() {
        if i > 0 {
            in_order.push(',');
        }
        write!(in_order, r#""{key}":{value}"#).unwrap();
    }
    in_order.push_str(r#","f3":"\\x0102","f30":5.0}"#);
    push("wide/field_order".to_owned(), (fields.clone(), in_order));

    let mut sorted: BTreeMap<(usize, String), &str> = BTreeMap::new();
    for (key, value) in &values {
        sorted.insert((key.len(), key.clone()), value);
    }
    sorted.insert((2, "f3".to_owned()), r#""\\x0102""#);
    sorted.insert((3, "f30".to_owned()), "5.0");
    let mut jsonb = String::from("{");
    for (i, ((_, key), value)) in sorted.iter().enumerate() {
        if i > 0 {
            jsonb.push(',');
        }
        write!(jsonb, r#""{key}":{value}"#).unwrap();
    }
    jsonb.push('}');
    push("wide/jsonb_order".to_owned(), (fields, jsonb));
}

/// Values nested near and past `serde_json`'s 128 levels, under each reader.
fn depth(push: &mut impl FnMut(String, (Vec<Field>, String))) {
    for levels in [120, 126, 127, 128, 140] {
        let nested = format!("{}1{}", "[".repeat(levels), "]".repeat(levels));
        for (ty_name, ty) in [
            ("keyword", FlussoType::Keyword),
            ("json", FlussoType::Json),
            ("double", FlussoType::Double),
        ] {
            push(
                format!("depth/{ty_name}/{levels}"),
                one(vec![column("f", ty.clone(), false)], "f", &nested),
            );
            push(
                format!("depth/array/{ty_name}/{levels}"),
                one(vec![column("f", ty, true)], "f", &nested),
            );
        }
        push(
            format!("depth/ids/{levels}"),
            one(
                vec![aggregate(
                    "f",
                    AggregateOp::Ids {
                        element_type: FlussoType::Long,
                    },
                    None,
                )],
                "f",
                &nested,
            ),
        );
    }
}
