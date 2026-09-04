#![doc = include_str!("../README.md")]
// `serde_json` is a dev-dependency used only by the integration tests; allow it
// to look unused in the lib's own test build.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod compiled;
mod deployment;
mod loader;
pub mod toml;
pub mod yaml;

pub use compiled::{
    CompileError, Compiled, FORMAT_VERSION, compile, from_bytes, load_compiled, to_bytes, write,
    write_if_changed,
};
pub use deployment::{Config, DEFAULT_STREAM_KIND, Index, ServerConfig};
pub use loader::{LoadError, load};

// Re-export the kernel vocabulary so downstream crates depend only on `config`
// rather than reaching for `kernel` as well. The assembled `Config` family
// (above) lives in this crate; everything else — the identifiers, `IndexSchema`,
// `IndexMapping`, `FailurePolicy` — is the cross-cutting kernel vocabulary.
pub use kernel::*;

pub use crate::toml::CONFIG_SCHEMA;
pub use crate::yaml::INDEX_SCHEMA;
