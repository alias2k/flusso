//! Postgres change capture over logical replication (WAL / pgoutput).
//!
//! Implements [`source::cdc::ChangeCapture`] on top of `pgwire-replication`.
//! [`WalChangeCapture`]'s `live` connects to a replication slot and yields a
//! stream of positioned [`ChangeEvent`](source::cdc::ChangeEvent)s — table
//! name and primary key per committed row change — that the engine re-reads
//! and assembles.
//!
//! What this crate does:
//!
//! - Decodes the pgoutput messages `pgwire-replication` leaves raw — `Relation`,
//!   `Insert`, `Update`, `Delete`, `Truncate` (see [`pgoutput`]) — tracking
//!   relation metadata so it can extract each changed row's key.
//! - Buffers a transaction's changes and emits them on `Commit`, tagged with
//!   the commit LSN, so acknowledgements map to clean commit boundaries.
//! - Maps each [`Position`](kernel::Position) the engine
//!   [`confirm`](source::cdc::ChangeCapture::confirm)s back to its commit LSN
//!   and reports a contiguous watermark to the server, advancing the slot only
//!   as far as every sink has durably confirmed (at-least-once).
//!
//! Configuration and prerequisites live on [`WalChangeCapture`]. The relevant
//! `pgwire-replication` types are re-exported below for convenience.

mod ack;
mod backfill;
mod capture;
mod introspection;
mod pgoutput;
mod publication;
mod stream;

pub use capture::WalChangeCapture;

/// Run the pgoutput decoder over arbitrary bytes, discarding the result.
///
/// The decoder must never panic on malformed input (an `Err` is the correct
/// outcome) — a panic here is a denial of service on the replication stream.
/// This wrapper exists only so the `fuzz/` crate can reach the otherwise
/// crate-private [`pgoutput::decode`]; gated behind the `fuzzing` feature.
#[cfg(feature = "fuzzing")]
pub(crate) fn fuzz_decode(data: &[u8]) {
    let _ = pgoutput::decode(data);
}
