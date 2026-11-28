//! The meta index's documents and what the sink does to them, as pure values
//! (the review of the closing round, item 1).
//!
//! **Not a stable interface.** It is public only for flusso's own test
//! models (the daemon's world), hidden from the docs, and may change in any
//! release. The sink's HTTP code and
//! `memory` (feature `testing`) run the same plans, so a test stand-in built
//! on it decides exactly as the sink does.

pub use crate::detachment::plan::{
    Kept, Plan, Step, begin_rebuild, demote, detach, gaps_update, is_tombstone, put_back, read,
    reindex_moves, rewritten, seeded, tombstone, whole_marker,
};

#[cfg(feature = "testing")]
pub mod memory;
