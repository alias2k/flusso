#![doc = include_str!("../README.md")]
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod budget;
mod error;
mod index_states;
mod ingest;
mod observer;
mod policy;
mod progress;
mod rereads;
mod sink_engine;
mod stop;

#[cfg(test)]
mod tests;

pub use budget::{
    Admission, Budget, Charge, FLOOR, Held, Refusal, Reservation, document_bytes, value_bytes,
};
pub use error::*;
pub use index_states::{
    ExemptionState, HeldBy, IndexStates, IndexView, LaneFull, PacingReader, Walking,
};
pub use ingest::{DEFAULT_MAX_PENDING_DOCUMENTS, IngestEngine, KEY_PAGE, STUCK_AFTER, document_id};
pub use observer::*;
pub use policy::{Backoff, BackoffError, BatchPolicy, FailurePolicies, FailurePolicy, MaxWaiting};
pub use progress::{Attach, Holds, Level, Listener, Pin, Progress, Step, Taken, Token};
pub use rereads::{CapturePositions, ReadBefore, Reread, RereadListener, Rereads, SourcePositions};
pub use sink_engine::{
    LaneHealth, ParkReason, ProbeOutcome, ReattachVerdict, Reply, SinkControl, SinkEngine,
};
pub use stop::{Stop, drain_lanes};
