//! In-process benchmark of the pgoutput decoder over a recorded WAL stream —
//! the pure-CPU half of live capture, with no server involved.
//!
//! The input is `benches/fixtures/pgoutput.bin`: the raw `XLogData` payloads a
//! real Postgres 16 sent for a mix of inserts, updates (key-only and full old
//! tuples), deletes, and a truncate across tables keyed by `int`, `bigint`,
//! `uuid`, and a composite key. It is recorded, not synthesized, so type OIDs,
//! relation messages, and message ordering are what production sees.
//! `tests/record_pgoutput.rs` re-records it (needs Docker).
//!
//! Measured: decoding every message in the fixture, as throughput in messages.
//!
//! Run with:
//!
//! ```text
//! cargo bench -p flusso-source-postgres --bench pgoutput --features bench
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    unused_crate_dependencies,
    missing_debug_implementations
)]

use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};

/// The recorded stream: `u32` little-endian length prefix, then the message.
const FIXTURE: &[u8] = include_bytes!("fixtures/pgoutput.bin");

fn frames(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at + 4 <= bytes.len() {
        let len = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        at += 4;
        out.push(&bytes[at..at + len]);
        at += len;
    }
    out
}

fn bench(c: &mut Criterion) {
    let messages = frames(FIXTURE);
    assert!(
        messages.len() > 1_000,
        "fixture holds {} messages; re-record it",
        messages.len()
    );

    let mut group = c.benchmark_group("decode");
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.throughput(Throughput::Elements(messages.len() as u64));
    group.bench_function("fixture", |b| {
        b.iter(|| {
            messages
                .iter()
                .filter(|m| source_postgres::bench_pgoutput_decode(m))
                .count()
        });
    });
    group.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
