#![doc = include_str!("../README.md")]
// Integration/e2e tests (in `tests/`) pull dev-dependencies the unit-test build
// doesn't touch; allow that only under `cfg(test)` — the normal build still
// enforces unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod cdc;
mod config;
mod connection_url;
mod document;
mod tls;

pub use cdc::WalChangeCapture;
pub use config::{Connection, PostgresConfig, SslMode, Tls};
pub use connection_url::{ConnectionUrl, ConnectionUrlError, Scheme};
pub use document::PgDocumentBuilder;
pub use tls::{replication_config, sql_connection_url};

// Re-exported so callers can build a capture without depending on
// `pgwire-replication` directly.
pub use pgwire_replication::{Lsn, ReplicationConfig, TlsConfig};

/// Fuzzing entry point for the pgoutput decoder — feeds arbitrary bytes through
/// it and asserts (by not panicking) that malformed input is rejected rather
/// than crashing. Used by the `fuzz/` cargo-fuzz crate; gated behind the
/// `fuzzing` feature and not part of the stable API.
#[cfg(feature = "fuzzing")]
pub fn fuzz_pgoutput_decode(data: &[u8]) {
    cdc::fuzz_decode(data);
}
