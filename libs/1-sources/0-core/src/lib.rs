//! The source abstractions for `pg_sync_rs`.
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
//! Both build on two shared, mechanism-neutral primitives that belong to
//! neither concern:
//!
//! - [`RowKey`] — a row's primary key as ordered column/value pairs.
//! - [`SourceError`] / [`Result`] — the common error type.
//!
//! Keeping the two abstractions apart means a deployment can mix any change
//! mechanism with any document builder, and either can be implemented, tested,
//! or replaced without touching the other.

mod error;
mod row_key;

pub mod cdc;
pub mod document;

pub use error::*;
pub use row_key::*;
