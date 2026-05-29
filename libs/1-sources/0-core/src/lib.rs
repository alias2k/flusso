//! The source abstraction for `pg_sync_rs`.
//!
//! A *source* answers one question for the engine — *what changed?* — as a
//! single async stream. Everything downstream of that stream (re-reading rows,
//! assembling documents, writing to sinks, recording checkpoints) belongs to
//! the engine, not here.
//!
//! - [`ChangeCapture`] is the one trait that varies by mechanism. Logical
//!   replication (WAL) is the first implementation; polling or trigger-based
//!   capture can follow without the engine knowing the difference.
//! - [`Change`] is what the stream yields: a thin [`ChangeEvent`] — table and
//!   primary key, never row contents — paired with an [`Ack`].
//! - [`Ack`] / [`AckSink`] carry confirmation back to the mechanism so it can
//!   advance its own durable resume point. Delivery is at-least-once.
//!
//! The design rests on three decisions:
//!
//! 1. **Resume is internal.** No position or cursor crosses this API. Each
//!    mechanism owns its resume state — trivially so for WAL, where the
//!    replication slot is durable on the server.
//! 2. **Backfill is the head of the stream.** [`ChangeCapture::start`] emits
//!    the initial snapshot first, then transitions to live changes as one
//!    continuous stream, with a consistent boundary between the two.
//! 3. **Events are thin.** A change names a row; it does not carry its data.
//!    The engine re-reads the current row at assembly time, so every mechanism
//!    looks identical and nothing depends on `REPLICA IDENTITY`.
//!
//! Reading rows — the initial scan and per-document join/aggregate resolution —
//! is deliberately *not* part of this trait. It is plain SQL, the same whatever
//! the capture mechanism is, and lives as a concrete reader in the source
//! implementation crate.

mod ack;
mod capture;
mod change;
mod error;

pub use ack::*;
pub use capture::*;
pub use change::*;
pub use error::*;
