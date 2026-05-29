//! Change data capture: *what changed?*
//!
//! A [`ChangeCapture`] mechanism yields a stream of thin [`Change`]s — each a
//! [`ChangeEvent`] (table and key, never row contents) paired with an [`Ack`].
//! This module is self-contained: it knows nothing about how documents are
//! built from a change.
//!
//! The design rests on three decisions:
//!
//! 1. **Resume is internal.** No position or cursor crosses this API. Each
//!    mechanism owns its resume state — trivially so for WAL, where the
//!    replication slot is durable on the server.
//! 2. **Backfill is the head of the stream.** [`ChangeCapture::start`] emits
//!    the initial snapshot first, then transitions to live changes as one
//!    continuous stream, with a consistent boundary between the two.
//! 3. **Events are thin.** A change names a row; it does not carry its data, so
//!    every mechanism looks identical and nothing depends on `REPLICA IDENTITY`.

mod ack;
mod capture;
mod change;

pub use ack::*;
pub use capture::*;
pub use change::*;
