//! The source abstractions for `flusso`.
//!
//! A source has two **independent** responsibilities, each its own module.
//! Neither module references the other; the engine is the only thing that
//! bridges them.
//!
//! - [`cdc`] — *what changed?* A pluggable change-capture mechanism that yields
//!   a stream of thin [`Change`](cdc::Change)s and confirms progress via an
//!   [`Ack`](cdc::Ack). Logical replication (WAL) is the first mechanism;
//!   polling or triggers can follow.
//! - [`document`] — *what to build?* Turns a changed row (named by table and
//!   key) into the target documents it affects, and assembles each one.
//!
//! Alongside them, [`enrich`](crate::enrich) is the source-independent half of a
//! third job every source shares: filling the gaps a thin config leaves (field
//! types, nullability) into a complete [`IndexMapping`](schema_core::IndexMapping).
//! A source supplies only the one store-specific piece — a [`Catalog`] over its
//! column types — and gets the whole resolution for free.
//!
//! Both build on two shared, mechanism-neutral primitives that belong to
//! neither concern:
//!
//! - [`RowKey`] — a row's primary key as ordered column/value pairs.
//! - [`SnapshotTable`] — a schema-qualified table the engine asks a mechanism
//!   to snapshot when seeding an index.
//! - [`SourceError`] / [`Result`] — the common error type.
//!
//! Keeping the two abstractions apart means a deployment can mix any change
//! mechanism with any document builder, and either can be implemented, tested,
//! or replaced without touching the other.

mod enrich;
mod error;
mod row_key;
mod snapshot_table;

pub mod cdc;
pub mod document;

pub use enrich::*;
pub use error::*;
pub use row_key::*;
pub use snapshot_table::*;
