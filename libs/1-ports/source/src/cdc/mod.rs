//! Change data capture: *what changed?*
//!
//! A [`ChangeCapture`] mechanism yields a stream of thin [`ChangeEvent`]s —
//! table and key, never row contents — each paired with the
//! [`Position`](kernel::Position) the source assigned it. This module is
//! self-contained: it knows nothing about how documents are built from a
//! change.
//!
//! The design rests on three decisions:
//!
//! 1. **Resume is the source's, confirmation is cumulative.** No cursor crosses
//!    this API on the way *out*; each mechanism owns its resume state —
//!    trivially so for WAL, where the replication slot is durable on the
//!    server. What crosses on the way *back* is a [`Position`](kernel::Position):
//!    [`ChangeCapture::confirm`] says every change up to it is durable
//!    downstream. The stream computes that position as the lowest one every sink
//!    has acknowledged, so a live change carries no in-process acknowledgement
//!    handle and can cross a process boundary. [`Continuity`] is the other
//!    crossing: [`ChangeCapture::continuity`] says, read-only, whether the
//!    resume point survived, because a seed a sink recorded earlier is only
//!    valid while the point that fed it does; [`ChangeCapture::prepare`] then
//!    establishes it before any snapshot.
//! 2. **Capture streams live changes only.** [`ChangeCapture::live`] streams
//!    ongoing changes. A backfill reads current rows through the document
//!    builder ([`DocumentBuilder::keys`](crate::document::DocumentBuilder::keys)),
//!    which needs no snapshot shared with the stream: every change made after
//!    [`prepare`](ChangeCapture::prepare) reaches the live stream.
//! 3. **Events are thin.** A change names a row; it does not carry its data, so
//!    every mechanism looks identical and nothing depends on `REPLICA IDENTITY`.

mod capture;
mod change;

pub use capture::*;
pub use change::*;
