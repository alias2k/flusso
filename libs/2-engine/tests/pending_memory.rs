//! What a pending document costs (ADR 0012 stage 8a, §3.9 §10): an exempt
//! index's changes are kept as pending documents, ids only, up to
//! `max_pending_documents` (default 1,000,000), outside the build budget. A
//! counting global allocator measures what `Progress` holds for a million
//! documents of one index keyed by one integer column, the common shape:
//! the live bytes once marked, per document, and the most held at once
//! while the maps grew. A correctness bound, not a benchmark: no timing.
#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::print_stderr,
    unused_crate_dependencies
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering};

use engine::Progress;
use kernel::{ColumnName, GenericValue, IndexName, Position, SinkName};
use source::RowKey;
use source::document::DocumentId;

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
static PEAK: AtomicIsize = AtomicIsize::new(0);

fn moved(by: isize) {
    let now = LIVE.fetch_add(by, Ordering::Relaxed) + by;
    PEAK.fetch_max(now, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        moved(layout.size() as isize);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        moved(-(layout.size() as isize));
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        moved(size as isize - layout.size() as isize);
        unsafe { System.realloc(ptr, layout, size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// A million documents.
const DOCUMENTS: usize = 1_000_000;

/// The bytes one pending document holds, steady: its key behind an `Arc`
/// (the `RowKey`'s `Vec` of one `(ColumnName, GenericValue)` and the column
/// name's string), its `waiting` entry (the key, its position, the order
/// counter, its read number) and its `order` entry, with the tables' slack.
/// The note's estimate, about 300 B, is the ceiling; measured 266 B (Linux,
/// x86_64, 1,000,000 documents): about 266 MB at the default cap.
const STEADY_PER_DOCUMENT: usize = 300;

/// The most held per document at once: the `waiting` table doubles as it
/// grows, both tables held for a moment. The note's estimate, 360 B, is the
/// ceiling; measured 314 B: about 314 MB at the default cap.
const PEAK_PER_DOCUMENT: usize = 360;

#[test]
fn a_pending_document_costs_what_the_note_says() {
    let orders = IndexName::try_new("orders").unwrap();
    let progress = Progress::of([orders.clone()], [SinkName::try_new("primary").unwrap()]);
    let column = ColumnName::try_new("id").unwrap();
    let before = LIVE.load(Ordering::SeqCst);
    PEAK.store(before, Ordering::SeqCst);
    // Marked a run at a time, as the reader marks them, each document's key
    // made as a decoder makes it.
    for run in 0..(DOCUMENTS / 1000) {
        progress.mark_reads((0..1000).map(|at| {
            let n = run * 1000 + at;
            (
                DocumentId {
                    index: orders.clone(),
                    key: RowKey(vec![(column.clone(), GenericValue::BigInt(n as i64))]),
                },
                Position::new(n as u64),
                n as u64 + 1,
            )
        }));
    }
    let steady = (LIVE.load(Ordering::SeqCst) - before) as usize / DOCUMENTS;
    let peak = (PEAK.load(Ordering::SeqCst) - before) as usize / DOCUMENTS;
    eprintln!("a pending document: {steady} B held, {peak} B at the peak");
    assert_eq!(progress.waiting(), DOCUMENTS);
    assert!(
        steady <= STEADY_PER_DOCUMENT,
        "{steady} B per pending document, over {STEADY_PER_DOCUMENT}"
    );
    assert!(
        peak <= PEAK_PER_DOCUMENT,
        "{peak} B per pending document at the peak, over {PEAK_PER_DOCUMENT}"
    );
}
