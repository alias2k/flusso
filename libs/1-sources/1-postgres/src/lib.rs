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
