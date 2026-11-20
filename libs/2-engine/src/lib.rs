#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod budget;
mod error;
mod ingest;
mod observer;
mod policy;
mod progress;
mod sink_engine;
mod stop;

#[cfg(test)]
mod tests;

pub use budget::{Budget, Charge, FLOOR, Held, Reservation, document_bytes, value_bytes};
pub use error::*;
pub use ingest::{IngestEngine, KEY_PAGE, document_id};
pub use observer::*;
pub use policy::{Backoff, BackoffError, BatchPolicy, FailurePolicies, FailurePolicy};
pub use progress::{Holds, Level, Listener, Pin, Progress, Step, Taken, Token};
pub use sink_engine::{LaneHealth, ParkReason, ProbeOutcome, SinkControl, SinkEngine};
pub use stop::{Stop, drain_lanes};
