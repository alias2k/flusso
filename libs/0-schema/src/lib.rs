//! Load a `flusso` configuration into a validated model.
//!
//! This is the front door to the configuration layer. [`load`] takes the path
//! to a `flusso.toml`, reads the source and sinks from it, resolves and parses
//! every index schema the file references, and hands back a single [`Config`].
//!
//! The format-specific crates (`schema-config-toml`, `schema-index-yaml`) and
//! the core model (`schema-core`) sit underneath. Downstream code depends only
//! on this crate and reaches the core types through its re-exports.
//!
//! # Example
//!
//! ```no_run
//! let config = schema::load("flusso.toml")?;
//!
//! for (name, index) in &config.indexes {
//!     println!("{name}: table {} ({} fields)", index.schema.table, index.schema.fields.len());
//! }
//! # Ok::<(), schema::LoadError>(())
//! ```

// `serde_json` / `serde_yaml` are dev-dependencies used only by the
// `schema_drift` integration test; allow them to look unused in the lib's own
// test build (see `tests/schema_drift.rs`).
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod compiled;
mod loader;

pub use compiled::{
    CompileError, Compiled, FORMAT_VERSION, compile, from_bytes, load_compiled, to_bytes, write,
};
pub use loader::{LoadError, load};

// Re-export the canonical schema types so downstream crates depend only on
// `schema` rather than reaching into the sub-crates directly.
pub use schema_core::*;

// The embedded editor-assist JSON Schemas, owned by the format crates that
// define each file's shape. Exposed here so the CLI (and any consumer) reaches
// them through `schema`.
pub use schema_config_toml::CONFIG_SCHEMA;
pub use schema_index_yaml::INDEX_SCHEMA;
