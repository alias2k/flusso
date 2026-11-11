//! In-process benchmark of script fields: one batch of built documents finished
//! by [`Scripts::apply`], with no database or cluster involved. This is all the
//! CPU a script field adds to a build: the conversion into Rhai, the calls, the
//! typed conversion back, and the strip of input-only fields.
//!
//! Measured, each as throughput in documents:
//!
//! - `scripted/flat`: three root-level scripts reading `this`.
//! - `scripted/nested`: one script per element of a ten-element `has_many`,
//!   reading `this` and `root`.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-script --bench scripted
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Duration;

use config::yaml::SchemaYaml;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use kernel::{GenericValue, IndexName, IndexSchema, ParseFrom};
use script::Scripts;
use source::SourceSpec;

/// Documents finished per iteration.
const BATCH: usize = 1_000;

const FLAT: &str = "\
version: 1
table: products
fields:
  - keyword: first
    required: true
  - keyword: last
    required: true
    emit: false
  - double: price
    required: true
  - long: stock
    required: true
  - keyword: label
    required: true
    script: '`${this.first} ${this.last}`'
  - boolean: onSale
    required: true
    script: this.price < 10.0 && this.stock > 0
  - double: gross
    required: true
    script: this.price * 1.23
";

const NESTED: &str = "\
version: 1
table: products
fields:
  - keyword: currency
    required: true
  - has_many: variants
    table: variants
    primary_key: id
    foreign_key: product_id
    fields:
      - double: price
        required: true
      - keyword: shown
        required: true
        script: '`${this.price} ${root.currency}`'
";

fn index() -> IndexName {
    IndexName::try_new("products").unwrap()
}

fn compile(yaml: &str) -> Scripts {
    let schema = IndexSchema::try_from(SchemaYaml::try_parse(yaml).unwrap()).unwrap();
    Scripts::compile(&SourceSpec::new(BTreeMap::from([(index(), schema)]))).unwrap()
}

fn map(pairs: Vec<(&str, GenericValue)>) -> GenericValue {
    GenericValue::Map(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn flat_doc(i: usize) -> GenericValue {
    map(vec![
        ("first", GenericValue::String(format!("first{i}"))),
        ("last", GenericValue::String(format!("last{i}"))),
        ("price", GenericValue::Double(9.5)),
        ("stock", GenericValue::BigInt(3)),
        ("label", GenericValue::Null),
        ("onSale", GenericValue::Null),
        ("gross", GenericValue::Null),
    ])
}

fn nested_doc(_: usize) -> GenericValue {
    let variant = |p: f64| {
        map(vec![
            ("price", GenericValue::Double(p)),
            ("shown", GenericValue::Null),
        ])
    };
    map(vec![
        ("currency", GenericValue::String("EUR".to_owned())),
        (
            "variants",
            GenericValue::Array((0..10).map(|i| variant(f64::from(i))).collect()),
        ),
    ])
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("scripted");
    group.throughput(Throughput::Elements(BATCH as u64));
    group.measurement_time(Duration::from_secs(5));
    for (name, yaml, make) in [
        ("flat", FLAT, flat_doc as fn(usize) -> GenericValue),
        ("nested", NESTED, nested_doc),
    ] {
        let scripts = compile(yaml);
        let docs: Vec<GenericValue> = (0..BATCH).map(make).collect();
        let index = index();
        group.bench_function(name, |b| {
            b.iter_batched(
                || docs.clone(),
                |mut docs| {
                    for doc in &mut docs {
                        scripts.apply(&index, doc).unwrap();
                    }
                    black_box(docs)
                },
                criterion::BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
