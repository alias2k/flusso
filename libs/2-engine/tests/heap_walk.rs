//! The memory budget's size walk ([`engine::value_bytes`]) against what the
//! allocator really holds for a document: a counting global allocator
//! measures the live bytes a document's tree owns, and the walk must agree
//! within a fifth, for documents shaped like the `reference`
//! and `complex` indexes'. The walk counts no allocator bookkeeping, so the
//! process's RSS stays above it by the allocator's own overhead.
#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::print_stderr,
    unused_crate_dependencies
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, Ordering};

use kernel::GenericValue;

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
/// One measurement at a time: the count is process-wide.
static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size() as isize, Ordering::Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        LIVE.fetch_add(size as isize - layout.size() as isize, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// The bytes `make`'s value holds on the heap once built, measured by the
/// allocator, and the walk's count of it.
fn measure(make: impl Fn() -> GenericValue) -> (isize, usize) {
    let _one = ONE_AT_A_TIME.lock().unwrap();
    let before = LIVE.load(Ordering::SeqCst);
    let value = Box::new(make());
    let held = LIVE.load(Ordering::SeqCst) - before - std::mem::size_of::<GenericValue>() as isize;
    let walked = engine::value_bytes(&value);
    drop(value);
    (held, walked)
}

fn text(s: &str) -> GenericValue {
    GenericValue::String(s.to_owned())
}

/// A map built one insert at a time, in key order, as a decoder reading
/// `jsonb` (which sorts its keys) builds it.
fn inserted(entries: usize) -> GenericValue {
    let mut map = BTreeMap::new();
    for n in 0..entries {
        map.insert(format!("key{n:04}"), GenericValue::BigInt(n as i64));
    }
    GenericValue::Map(map)
}

fn map(entries: Vec<(&str, GenericValue)>) -> GenericValue {
    GenericValue::Map(
        entries
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

/// A `users`-like document: scalars, an object, and a nested array of orders.
fn user(id: i64) -> GenericValue {
    let orders = (0..5)
        .map(|n| {
            map(vec![
                ("id", GenericValue::BigInt(id * 1000 + n)),
                ("status", text("paid")),
                ("total", GenericValue::Double(42.5 * n as f64)),
                ("placedAt", text("2024-01-02T03:04:05.123Z")),
            ])
        })
        .collect();
    map(vec![
        ("id", GenericValue::BigInt(id)),
        ("email", text(&format!("user{id}@example.com"))),
        ("fullName", text(&format!("Customer Number {id}"))),
        ("active", GenericValue::Bool(id % 2 == 0)),
        ("orderCount", GenericValue::BigInt(5)),
        (
            "account",
            map(vec![("tier", text("pro")), ("country", text("PL"))]),
        ),
        ("orders", GenericValue::Array(orders)),
        (
            "tags",
            GenericValue::Array(vec![text("a"), text("bb"), text("ccc")]),
        ),
    ])
}

/// A `complex`-like document: wide, deep, with bytes and long text.
fn complex(id: i64) -> GenericValue {
    let wide = (0..60)
        .map(|n| (format!("field{n:02}"), text(&"x".repeat(20 + n))))
        .collect::<BTreeMap<_, _>>();
    let deep = (0..6).fold(text("leaf"), |inner, level| {
        map(vec![
            ("level", GenericValue::BigInt(level)),
            ("child", inner),
            ("note", text("some words at each level")),
        ])
    });
    map(vec![
        ("id", GenericValue::BigInt(id)),
        ("wide", GenericValue::Map(wide)),
        ("deep", deep),
        ("blob", GenericValue::Bytes(vec![7u8; 300])),
        ("description", text(&"lorem ipsum ".repeat(40))),
    ])
}

#[test]
fn the_walk_agrees_with_the_allocator_within_a_fifth() {
    for (name, make) in [
        (
            "user",
            Box::new(|| user(42)) as Box<dyn Fn() -> GenericValue>,
        ),
        ("complex", Box::new(|| complex(7))),
        ("text", Box::new(|| text(&"y".repeat(500)))),
        ("inserted 3", Box::new(|| inserted(3))),
        ("inserted 40", Box::new(|| inserted(40))),
        ("inserted 500", Box::new(|| inserted(500))),
        (
            "array",
            Box::new(|| GenericValue::Array((0..100).map(GenericValue::BigInt).collect())),
        ),
    ] {
        let (held, walked) = measure(make);
        let ratio = walked as f64 / held as f64;
        eprintln!("{name}: walk {walked} B, allocator {held} B, ratio {ratio:.2}");
        assert!(
            (0.8..=1.2).contains(&ratio),
            "{name}: the walk counts {walked} B, the allocator holds {held} B (ratio {ratio:.2})"
        );
    }
}
