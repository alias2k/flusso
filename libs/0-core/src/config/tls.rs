//! TLS settings for the **source** connection — the config vocabulary only.
//!
//! [`SourceTls`] carries what a deployment *declared* (in `flusso.toml`'s flat
//! `ssl_*` keys); it is not the effective TLS decision. The source backend
//! merges these settings over whatever the connection URL's own query
//! parameters (`sslmode`, `sslrootcert`, `sslcert`, `sslkey`) say — **config
//! keys win** — and defaults the mode to [`SslMode::Prefer`] when neither side
//! specifies one. Keeping the merge in the backend keeps this layer free of
//! URL parsing and of any concrete TLS stack.
//!
//! ```rust
//! use schema_core::{SourceTls, SslMode};
//!
//! let tls = SourceTls {
//!     mode: Some(SslMode::VerifyFull),
//!     root_cert: Some("/etc/ssl/rds-ca.pem".into()),
//!     ..SourceTls::default()
//! };
//! assert_eq!(tls.mode, Some(SslMode::VerifyFull));
//! ```

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// How the source connection negotiates TLS. The tokens match libpq's
/// `sslmode` values, and so do the semantics — in particular
/// [`Require`](Self::Require) encrypts but performs **no certificate
/// verification at all**; only the `Verify*` modes check anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    /// Never use TLS; fail if the server requires it.
    Disable,
    /// Try TLS first, fall back to plaintext if the server doesn't support it.
    /// The default when nothing specifies a mode.
    Prefer,
    /// Require TLS but accept **any** certificate — no chain or hostname
    /// verification. Protects against passive eavesdropping only.
    Require,
    /// Require TLS and verify the certificate chain, but not the hostname.
    VerifyCa,
    /// Require TLS, verify the chain **and** the hostname. The mode to use in
    /// production.
    VerifyFull,
}

/// The libpq-style token (`verify-full`, …), matching the serde form.
impl fmt::Display for SslMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token = match self {
            SslMode::Disable => "disable",
            SslMode::Prefer => "prefer",
            SslMode::Require => "require",
            SslMode::VerifyCa => "verify-ca",
            SslMode::VerifyFull => "verify-full",
        };
        write!(f, "{token}")
    }
}

/// The source's declared TLS settings, from `flusso.toml`'s flat `ssl_*` keys.
/// Every field is optional: an unset field defers to the connection URL's own
/// query parameters, and a set one overrides them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceTls {
    /// The `ssl_mode` key. Unset defers to the URL's `sslmode`, then to
    /// [`SslMode::Prefer`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<SslMode>,
    /// The `ssl_root_cert` key: a PEM file of trusted CA certificates for the
    /// `Verify*` modes. Unset defers to the URL's `sslrootcert`, then to the
    /// bundled Mozilla roots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_cert: Option<PathBuf>,
    /// The `ssl_cert` key: the client certificate chain PEM for mutual TLS.
    /// Must be paired with [`client_key`](Self::client_key). Unset defers to
    /// the URL's `sslcert`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_cert: Option<PathBuf>,
    /// The `ssl_key` key: the client private key PEM for mutual TLS. Must be
    /// paired with [`client_cert`](Self::client_cert). Unset defers to the
    /// URL's `sslkey`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_key: Option<PathBuf>,
    /// The `ssl_sni_hostname` key: the hostname sent during the TLS handshake
    /// when it differs from the connection host — connecting by IP or through
    /// a load balancer while the certificate names the real host. Config-only
    /// (no standard URL parameter) and replication-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sni_hostname: Option<String>,
}

impl SourceTls {
    /// `true` when no key is set — the connection URL alone decides.
    pub fn is_unset(&self) -> bool {
        self == &SourceTls::default()
    }
}
