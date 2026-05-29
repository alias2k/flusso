//! Postgres change capture over logical replication (WAL / pgoutput).
//!
//! Implements [`sources_core::cdc::ChangeCapture`] on top of `pgwire-replication`.
//! [`WalChangeCapture::start`] connects to a replication slot and yields a
//! stream of thin [`Change`](sources_core::cdc::Change)s — table name and primary
//! key per committed row change — that the engine re-reads and assembles.
//!
//! What this crate does:
//!
//! - Decodes the pgoutput messages `pgwire-replication` leaves raw — `Relation`,
//!   `Insert`, `Update`, `Delete`, `Truncate` (see [`pgoutput`]) — tracking
//!   relation metadata so it can extract each changed row's key.
//! - Buffers a transaction's changes and emits them on `Commit`, tagged with
//!   the commit LSN, so acknowledgements map to clean commit boundaries.
//! - Translates the per-change [`Ack`](sources_core::cdc::Ack) into a contiguous LSN
//!   watermark and reports it to the server, advancing the slot only as far as
//!   the engine has durably confirmed (at-least-once).
//!
//! Configuration and prerequisites live on [`WalChangeCapture`]. The relevant
//! `pgwire-replication` types are re-exported below for convenience.

mod ack;
mod capture;
mod pgoutput;
mod stream;

pub use capture::WalChangeCapture;

// Re-exported so callers can build a capture without depending on
// `pgwire-replication` directly.
pub use pgwire_replication::{Lsn, ReplicationConfig, SslMode, TlsConfig};
