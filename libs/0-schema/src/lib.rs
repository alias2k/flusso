//! Load a `flusso` configuration into a validated model.
//!
//! This is the front door to the configuration layer. [`load`] takes the path
//! to a `config.toml`, reads the source and sinks from it, resolves and parses
//! every index schema the file references, and hands back a single [`Config`].
//!
//! The format-specific crates (`schema-config-toml`, `schema-index-yaml`) and
//! the core model (`schema-core`) sit underneath. Downstream code depends only
//! on this crate and reaches the core types through its re-exports.
//!
//! # Example
//!
//! ```no_run
//! let config = schema::load("config.toml")?;
//!
//! for (name, index) in &config.indexes {
//!     println!("{name}: table {} ({} fields)", index.schema.table, index.schema.fields.len());
//! }
//! # Ok::<(), schema::LoadError>(())
//! ```

mod loader;

pub use loader::{LoadError, load};

// Re-export the canonical schema types so downstream crates depend only on
// `schema` rather than reaching into the sub-crates directly.
pub use schema_core::*;
