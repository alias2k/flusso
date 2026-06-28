#![doc = include_str!("../README.md")]
// `proptest` (the round-trip property test) and other test-only crates are dev
// dependencies the library itself doesn't use; allow that in the test build.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod assets;

pub mod api;
pub mod codegen;
pub mod preview;
pub mod server;

pub use server::{DesignOptions, serve};
