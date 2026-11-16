//! In-process benchmark of the document reader: `coerce_document` over the JSON
//! text the document query returns, then the sink's render of the body
//! (`sink::Json`), as every build does once per document.
//!
//! Numbers are what it measures (issue #366): `serde_json`'s `float_roundtrip`
//! and the exact `Decimal` conversion of an untyped number both sit on this path.
//!
//! - `doubles`: 64 `double` columns, each written as Postgres writes a `float8`.
//! - `double_array`: one `double[]` column of 512 elements.
//! - `untyped`: one `json` column, an object of 64 numbers with no declared type.
//! - `keywords`: 64 `keyword` columns, the control with no number at all.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-source-postgres --bench documents --features bench
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::fmt::Write as _;
use std::hint::black_box;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use kernel::{Column, ColumnName, Field, FieldName, FieldSource, FlussoType};

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("documents");
    group.measurement_time(Duration::from_secs(5));
    let mut numbers = Numbers(0x9e37_79b9_7f4a_7c15);

    let doubles: Vec<Field> = (0..64)
        .map(|i| column(&format!("d{i}"), FlussoType::Double, false))
        .collect();
    let mut doc = String::from("{");
    for i in 0..64 {
        let sep = if i == 0 { "" } else { "," };
        write!(doc, r#"{sep}"d{i}":{}"#, float8_text(numbers.next())).unwrap();
    }
    doc.push('}');
    case(&mut group, "doubles", doubles, doc);

    let mut doc = String::from(r#"{"ds":["#);
    for i in 0..512 {
        let sep = if i == 0 { "" } else { "," };
        write!(doc, "{sep}{}", float8_text(numbers.next())).unwrap();
    }
    doc.push_str("]}");
    case(
        &mut group,
        "double_array",
        vec![column("ds", FlussoType::Double, true)],
        doc,
    );

    let mut doc = String::from(r#"{"j":{"#);
    for i in 0..64 {
        let sep = if i == 0 { "" } else { "," };
        write!(doc, r#"{sep}"n{i}":{}"#, float8_text(numbers.next())).unwrap();
    }
    doc.push_str("}}");
    case(
        &mut group,
        "untyped",
        vec![column("j", FlussoType::Json, false)],
        doc,
    );

    let keywords: Vec<Field> = (0..64)
        .map(|i| column(&format!("k{i}"), FlussoType::Keyword, false))
        .collect();
    let mut doc = String::from("{");
    for i in 0..64 {
        let sep = if i == 0 { "" } else { "," };
        write!(doc, r#"{sep}"k{i}":"value-{i}-{:x}""#, numbers.0).unwrap();
        numbers.next();
    }
    doc.push('}');
    case(&mut group, "keywords", keywords, doc);

    group.finish();
}

fn case(
    group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>,
    name: &str,
    fields: Vec<Field>,
    doc: String,
) {
    let read = source_postgres::bench_document_reader(fields);
    let body = read(doc.as_bytes()).expect("the document reads");
    assert!(!serde_json::to_vec(&sink::Json(&body)).unwrap().is_empty());
    group.throughput(Throughput::Bytes(doc.len() as u64));
    group.bench_function(name, |b| {
        b.iter(|| {
            let body = read(black_box(doc.as_bytes())).unwrap();
            serde_json::to_vec(&sink::Json(&body)).unwrap()
        });
    });
}

/// Doubles of every magnitude a column holds in practice: half everyday
/// values below a million with 15 to 17 digits, half spread over ±1e±30.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        let unit = (self.0 >> 11) as f64 / (1u64 << 53) as f64;
        if self.0 & 1 == 0 {
            (unit - 0.5) * 2e6
        } else {
            let exponent = ((self.0 >> 1) % 61) as i32 - 30;
            (unit - 0.5) * 10f64.powi(exponent)
        }
    }
}

/// `value` as Postgres's `float8out` writes it (PG 12 on): its shortest
/// round-trip digits, with an exponent below 1e-4 and from 1e15.
fn float8_text(value: f64) -> String {
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if (-4..15).contains(&exponent) {
        value.to_string()
    } else {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exponent.abs())
    }
}

fn column(name: &str, ty: FlussoType, array: bool) -> Field {
    Field {
        field: FieldName::try_new(name).unwrap(),
        options: Default::default(),
        emit: true,
        source: FieldSource::Column(Column {
            column: ColumnName::try_new(name).unwrap(),
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

criterion_group!(benches, bench);
criterion_main!(benches);
