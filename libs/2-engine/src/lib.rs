#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod error;
mod ingest;
mod observer;
mod policy;
mod progress;
mod sink_engine;
mod stop;

#[cfg(test)]
mod tests;

pub use error::*;
pub use ingest::{IngestEngine, KEY_PAGE, document_id};
pub use observer::*;
pub use policy::{BatchPolicy, FailurePolicies, FailurePolicy};
pub use progress::{Level, Listener, Progress};
pub use sink_engine::{SinkControl, SinkEngine};
pub use stop::{Stop, drain_lanes};
