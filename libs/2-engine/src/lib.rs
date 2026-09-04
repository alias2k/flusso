#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod error;
mod ingest;
mod observer;
mod policy;
mod sink_engine;

#[cfg(test)]
mod tests;

pub use error::*;
pub use ingest::{IngestEngine, document_id};
pub use observer::*;
pub use policy::{BatchPolicy, FailurePolicies, FailurePolicy};
pub use sink_engine::{SinkControl, SinkEngine};
