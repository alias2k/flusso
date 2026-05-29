//! The validated domain model for `pg_sync_rs`.
//!
//! Every other crate produces or consumes these types. They are the canonical
//! shape of a configuration: already validated, carrying no trace of the file
//! format they were parsed from.
//!
//! - [`common`] holds the validated primitives — newtypes such as [`TableName`]
//!   and [`ColumnName`] that enforce Postgres identifier rules at construction.
//! - [`config`] holds the structures built from them: [`Config`],
//!   [`IndexSchema`], [`Field`], [`Join`], [`Aggregate`], [`Filter`], and the rest.
//! - [`traits`] defines the conversions the format crates implement —
//!   [`ParseFrom`] (text into entities) and [`ContentHasher`].
//!
//! Identifier types are built with [`nutype`]: they can only be constructed
//! through `try_new`, so an invalid name never reaches the model.
//!
//! [`nutype`]: https://docs.rs/nutype

pub mod common;
pub mod config;
pub mod traits;

pub use common::*;
pub use config::*;
pub use traits::*;
