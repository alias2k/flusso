//! Connection-URL → replication config translation, including the TLS
//! decision.
//!
//! One resolved connection URL drives two kinds of connections — the
//! logical-replication stream (`pgwire-replication`) and the SQL pools
//! (`sqlx`) — and both must agree on TLS. The effective settings come from two
//! places, merged here in one spot:
//!
//! - the URL's own libpq-style query parameters — `sslmode`, `sslrootcert`,
//!   `sslcert`, `sslkey` (what sqlx reads natively);
//! - the deployment's declared [`SourceTls`] (the flat `ssl_*` keys in
//!   `flusso.toml`), which **override** the URL's parameters.
//!
//! When neither side specifies a mode, the default is `prefer` — try TLS,
//! fall back to plaintext — matching libpq's and sqlx's own default, so the
//! replication stream never silently diverges from the query connections.
//! `sslmode=allow` (which the replication stack doesn't model) is treated as
//! `prefer`. The SNI hostname override is config-only and applies to the
//! replication stream only — sqlx has no such knob.
//!
//! [`replication_config`] builds the stream side; [`sql_connection_url`]
//! projects the same decision back onto the URL handed to every sqlx pool.
//!
//! ```rust
//! use kernel::SourceTls;
//! use source_postgres::replication_config;
//!
//! let config = replication_config(
//!     "postgres://app:pw@db.example.com/appdb?sslmode=verify-full",
//!     &SourceTls::default(),
//!     "flusso",
//!     "flusso_pub",
//! )
//! .unwrap();
//! assert!(config.tls.mode.requires_tls());
//! ```

use std::path::{Path, PathBuf};

use kernel::{SourceTls, SslMode};
use pgwire_replication::{ReplicationConfig, SslMode as PgSslMode, TlsConfig};
use source::SourceError;
use url::Url;

/// Build the logical-replication connection config from a resolved connection
/// URL and the deployment's declared TLS settings (config keys win over the
/// URL's `ssl*` parameters; no mode anywhere means `prefer`).
pub fn replication_config(
    connection_url: &str,
    tls: &SourceTls,
    slot: &str,
    publication: &str,
) -> Result<ReplicationConfig, SourceError> {
    let url = parse_url(connection_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| setup("connection URL has no host"))?
        .to_owned();
    let port = url.port().unwrap_or(5432);
    let user = url.username();
    if user.is_empty() {
        return Err(setup("connection URL has no user"));
    }
    let password = url.password().unwrap_or_default();
    let database = url.path().trim_start_matches('/');
    // Postgres defaults the database to the user when the URL omits it.
    let database = if database.is_empty() { user } else { database };

    let effective = resolve_tls(&url, tls)?;
    Ok(
        ReplicationConfig::new(host, user, password, database, slot, publication)
            .with_port(port)
            .with_tls(effective.into_replication_tls()),
    )
}

/// The connection URL to hand to the SQL pools (sqlx): the deployment's
/// config-declared TLS settings written onto the query string as the libpq
/// parameters sqlx reads (`sslmode`, `sslrootcert`, `sslcert`, `sslkey`),
/// overriding any the URL already carries. With no config key set the URL is
/// returned unchanged — its own parameters (or sqlx's `prefer` default)
/// already say everything. The SNI override is not representable here; it
/// applies to the replication stream only.
pub fn sql_connection_url(connection_url: &str, tls: &SourceTls) -> Result<String, SourceError> {
    let overrides: Vec<(&str, String)> = [
        ("sslmode", tls.mode.map(|m| m.to_string())),
        ("sslrootcert", tls.root_cert.as_ref().map(path_string)),
        ("sslcert", tls.client_cert.as_ref().map(path_string)),
        ("sslkey", tls.client_key.as_ref().map(path_string)),
    ]
    .into_iter()
    .filter_map(|(key, value)| value.map(|v| (key, v)))
    .collect();
    if overrides.is_empty() {
        return Ok(connection_url.to_owned());
    }

    let mut url = parse_url(connection_url)?;
    // Same validation as the replication side, so a bad merge fails here too.
    resolve_tls(&url, tls)?;

    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !overrides.iter().any(|(k, _)| k == key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    {
        let mut pairs = url.query_pairs_mut();
        pairs.clear();
        pairs.extend_pairs(kept);
        pairs.extend_pairs(&overrides);
    }
    Ok(url.into())
}

/// The effective TLS decision after merging config over URL parameters.
struct EffectiveTls {
    mode: SslMode,
    root_cert: Option<PathBuf>,
    client_cert: Option<PathBuf>,
    client_key: Option<PathBuf>,
    sni_hostname: Option<String>,
}

impl EffectiveTls {
    fn into_replication_tls(self) -> TlsConfig {
        TlsConfig {
            mode: match self.mode {
                SslMode::Disable => PgSslMode::Disable,
                SslMode::Prefer => PgSslMode::Prefer,
                SslMode::Require => PgSslMode::Require,
                SslMode::VerifyCa => PgSslMode::VerifyCa,
                SslMode::VerifyFull => PgSslMode::VerifyFull,
            },
            ca_pem_path: self.root_cert,
            sni_hostname: self.sni_hostname,
            client_cert_pem_path: self.client_cert,
            client_key_pem_path: self.client_key,
        }
    }
}

/// Merge the declared settings over the URL's `ssl*` query parameters —
/// config wins per field — and validate the result.
fn resolve_tls(url: &Url, tls: &SourceTls) -> Result<EffectiveTls, SourceError> {
    let mode = match &tls.mode {
        Some(mode) => *mode,
        None => match last_param(url, "sslmode") {
            Some(token) => parse_mode(&token)?,
            None => SslMode::Prefer,
        },
    };
    let effective = EffectiveTls {
        mode,
        root_cert: merged_path(&tls.root_cert, url, "sslrootcert"),
        client_cert: merged_path(&tls.client_cert, url, "sslcert"),
        client_key: merged_path(&tls.client_key, url, "sslkey"),
        sni_hostname: tls.sni_hostname.clone(),
    };
    if effective.client_cert.is_some() != effective.client_key.is_some() {
        return Err(setup(
            "mutual TLS needs both the client certificate and the client key \
             (ssl_cert/sslcert and ssl_key/sslkey) — only one is set",
        ));
    }
    Ok(effective)
}

fn merged_path(configured: &Option<PathBuf>, url: &Url, param: &str) -> Option<PathBuf> {
    configured
        .clone()
        .or_else(|| last_param(url, param).map(PathBuf::from))
}

/// The last occurrence wins, like libpq treats repeated parameters.
fn last_param(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .last()
}

fn parse_mode(token: &str) -> Result<SslMode, SourceError> {
    match token {
        "disable" => Ok(SslMode::Disable),
        // `allow` (plaintext first, TLS on demand) has no replication-side
        // equivalent; `prefer` is the closest opportunistic mode.
        "allow" | "prefer" => Ok(SslMode::Prefer),
        "require" => Ok(SslMode::Require),
        "verify-ca" => Ok(SslMode::VerifyCa),
        "verify-full" => Ok(SslMode::VerifyFull),
        other => Err(setup(&format!(
            "invalid sslmode '{other}' in the connection URL — expected one of \
             disable, allow, prefer, require, verify-ca, verify-full"
        ))),
    }
}

fn parse_url(connection_url: &str) -> Result<Url, SourceError> {
    Url::parse(connection_url).map_err(|e| setup(&format!("invalid connection URL: {e}")))
}

fn setup(message: &str) -> SourceError {
    SourceError::Setup(message.to_owned())
}

fn path_string<P: AsRef<Path>>(path: P) -> String {
    path.as_ref().display().to_string()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
