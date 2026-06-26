#![doc = include_str!("../README.md")]
// `serde_json` / `serde_yaml` are dev-dependencies used only by the
// `schema_drift` integration test; allow them to look unused in the lib's own
// test build (see `tests/schema_drift.rs`).
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod compiled;
mod deployment;
mod loader;

pub use compiled::{
    CompileError, Compiled, FORMAT_VERSION, compile, from_bytes, load_compiled, to_bytes, write,
};
pub use deployment::{Config, Index, ServerConfig, Sink, Source};
pub use loader::{LoadError, load};

// Re-export the canonical schema vocabulary so downstream crates depend only on
// `schema` rather than reaching into the sub-crates directly. The assembled
// `Config` family (above) lives in this crate; everything else — the
// identifiers, `IndexSchema`, `IndexMapping`, `FailurePolicy`, the per-sink
// configs — is the cross-cutting vocabulary from `schema-core`.
pub use schema_core::*;

pub use schema_config_toml::CONFIG_SCHEMA;
pub use schema_index_yaml::INDEX_SCHEMA;
