// Integration/e2e tests (in `tests/`) pull dev-dependencies the unit-test build
// doesn't touch; allow that only under `cfg(test)` — the normal build still
// enforces unused dependencies.
#![cfg_attr(test, allow(unused_crate_dependencies))]

mod cdc;
mod document;

pub use cdc::WalChangeCapture;
pub use document::PgDocumentBuilder;

// Re-exported so callers can build a capture without depending on
// `pgwire-replication` directly.
pub use pgwire_replication::{Lsn, ReplicationConfig, SslMode, TlsConfig};

/// Fuzzing entry point for the pgoutput decoder — feeds arbitrary bytes through
/// it and asserts (by not panicking) that malformed input is rejected rather
/// than crashing. Used by the `fuzz/` cargo-fuzz crate; gated behind the
/// `fuzzing` feature and not part of the stable API.
#[cfg(feature = "fuzzing")]
pub fn fuzz_pgoutput_decode(data: &[u8]) {
    cdc::fuzz_decode(data);
}
