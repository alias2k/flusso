//! The `[server]` table: bind addresses for the operational HTTP surfaces.

use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

/// Bind addresses for the two operational HTTP surfaces, from a `flusso.toml`
/// `[server]` table. Both are optional — the binary layers `FLUSSO_*` env vars
/// and CLI flags on top (which win), falling back to built-in defaults when all
/// are absent. Each is a `host:port` socket address, parsed and validated at
/// config-read time by [`SocketAddr`]'s own deserializer.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Server {
    /// Public, read-only surface (`/healthz`, `/readyz`, `/status`, `/metrics`),
    /// e.g. `0.0.0.0:9464`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_address: Option<SocketAddr>,
    /// Private, Basic-auth control surface (`/indexes`, `/reindex`), e.g.
    /// `0.0.0.0:9465`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_address: Option<SocketAddr>,
}
