//! In-process benchmark of the sink's rendering path: kernel documents →
//! JSON → the NDJSON bulk body. This is every byte of CPU work `apply` and
//! `flush` do before a request leaves the process, with no cluster involved.
//!
//! Measured, each as throughput in envelopes:
//!
//! - `render/mid`: a realistic mid-size document (a dozen scalars, an object,
//!   a five-element nested array) — the everyday shape.
//! - `render/wide`: a heavy document (fifty scalars, twenty nested orders of
//!   four items each) — the worst-case shape.
//! - `render/delete`: tombstones, the metadata-only line.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-sink-opensearch --bench render --features bench
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::collections::BTreeMap;
use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use kernel::{Envelope, GenericValue, IndexName, Position};

/// Envelopes rendered per iteration.
const BATCH: usize = 1_000;

fn index() -> IndexName {
    IndexName::try_new("users").unwrap()
}

fn order(id: i64, items: usize) -> GenericValue {
    let mut order = BTreeMap::new();
    order.insert("id".to_owned(), GenericValue::BigInt(id));
    order.insert("status".to_owned(), GenericValue::String("paid".into()));
    order.insert("total".to_owned(), GenericValue::Decimal((id % 997).into()));
    order.insert(
        "placedAt".to_owned(),
        GenericValue::TimestampTz(chrono::Utc::now()),
    );
    if items > 0 {
        let items = (0..items as i64)
            .map(|k| {
                let mut item = BTreeMap::new();
                item.insert("productId".to_owned(), GenericValue::Int(k as i32));
                item.insert("quantity".to_owned(), GenericValue::Int(1 + k as i32));
                item.insert(
                    "unitPrice".to_owned(),
                    GenericValue::Decimal((k * 3).into()),
                );
                GenericValue::Map(item)
            })
            .collect();
        order.insert("items".to_owned(), GenericValue::Array(items));
    }
    GenericValue::Map(order)
}

fn mid(id: i64) -> GenericValue {
    let mut account = BTreeMap::new();
    account.insert("tier".to_owned(), GenericValue::String("pro".into()));
    account.insert("country".to_owned(), GenericValue::String("PL".into()));
    account.insert(
        "createdAt".to_owned(),
        GenericValue::TimestampTz(chrono::Utc::now()),
    );
    let mut map = BTreeMap::new();
    map.insert("id".to_owned(), GenericValue::Int(id as i32));
    map.insert(
        "email".to_owned(),
        GenericValue::String(format!("user{id}@example.com")),
    );
    map.insert(
        "fullName".to_owned(),
        GenericValue::String(format!("Customer Number {id}")),
    );
    map.insert("active".to_owned(), GenericValue::Bool(id % 2 == 0));
    map.insert("orderCount".to_owned(), GenericValue::BigInt(5));
    map.insert(
        "lifetimeValue".to_owned(),
        GenericValue::Decimal(425.into()),
    );
    map.insert("avgOrderValue".to_owned(), GenericValue::Double(85.0));
    map.insert("account".to_owned(), GenericValue::Map(account));
    map.insert(
        "orders".to_owned(),
        GenericValue::Array((0..5).map(|n| order(id * 1000 + n, 0)).collect()),
    );
    GenericValue::Map(map)
}

fn wide(id: i64) -> GenericValue {
    let GenericValue::Map(mut map) = mid(id) else {
        unreachable!()
    };
    for n in 0..50 {
        map.insert(
            format!("attribute{n}"),
            GenericValue::String(format!("value {n} of document {id}")),
        );
    }
    map.insert(
        "orders".to_owned(),
        GenericValue::Array((0..20).map(|n| order(id * 1000 + n, 4)).collect()),
    );
    GenericValue::Map(map)
}

fn upserts(body: fn(i64) -> GenericValue) -> Vec<Envelope> {
    (0..BATCH as i64)
        .map(|i| {
            Envelope::upsert(
                index(),
                i.to_string(),
                body(i),
                Some(Position(i as u64)),
                chrono::Utc::now(),
            )
        })
        .collect()
}

fn deletes() -> Vec<Envelope> {
    (0..BATCH as i64)
        .map(|i| {
            Envelope::delete(
                index(),
                i.to_string(),
                Some(Position(i as u64)),
                chrono::Utc::now(),
            )
        })
        .collect()
}

fn bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("render");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(BATCH as u64));
    for (name, envelopes) in [
        ("mid", upserts(mid)),
        ("wide", upserts(wide)),
        ("delete", deletes()),
    ] {
        group.bench_function(name, |b| {
            b.iter(|| sink_opensearch::bench_render_bulk(&envelopes).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
