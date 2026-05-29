mod cdc;
mod document;

pub use cdc::WalChangeCapture;
pub use document::PgDocumentBuilder;

// Re-exported so callers can build a capture without depending on
// `pgwire-replication` directly.
pub use pgwire_replication::{Lsn, ReplicationConfig, SslMode, TlsConfig};
