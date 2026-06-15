use std::fmt;

use serde::{Deserialize, Serialize};

use crate::common::{ConnectionUrl, ConnectionUrlError, HttpUrl, HttpUrlError, SinkName};

/// The reserved environment variable that supplies / overrides the source
/// connection URL. The source is a singleton, so one well-known name (the
/// 12-factor convention) is unambiguous.
pub const SOURCE_URL_VAR: &str = "DATABASE_URL";

/// A value resolved at **runtime**: either a literal baked into the config or a
/// reference to an environment variable read when the pipeline runs. Deferring
/// resolution is what lets a compiled config travel without its secrets — a
/// literal is carried as-is, an [`Env`](Self::Env) reference carries only the
/// variable name, and the real value is read in the environment that runs it.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Secret {
    /// A literal value, stored verbatim.
    Value(String),
    /// Read from this environment variable at resolution time.
    Env(String),
}

impl Secret {
    /// Read this secret's value from its own source — a literal as-is, an `Env`
    /// from the environment. Does not consult any reserved variable.
    fn read(&self) -> Result<String, ResolveError> {
        match self {
            Secret::Value(value) => Ok(value.clone()),
            Secret::Env(var) => {
                std::env::var(var).map_err(|_| ResolveError::EnvNotSet(var.clone()))
            }
        }
    }
}

/// Redacted, so a debug-printed config never leaks a literal. An `Env` reference
/// shows its variable name (not a secret); a literal shows `***`.
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Secret::Value(_) => write!(f, "Secret(***)"),
            Secret::Env(var) => write!(f, "Secret(env {var})"),
        }
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Secret::Value(_) => write!(f, "***"),
            Secret::Env(var) => write!(f, "${{{var}}}"),
        }
    }
}

/// How the source connection is specified: a full URL (literal or from env) or
/// the parts to assemble one. Resolution happens at runtime, so a configured
/// value can be overridden by [`SOURCE_URL_VAR`] in the running environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionSpec {
    /// A full connection URL.
    Url(Secret),
    /// The parts of a connection URL; `password` may come from the environment.
    Parts {
        host: String,
        port: u16,
        user: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        password: Option<Secret>,
        database: String,
    },
}

/// Resolve the source connection URL, with [`SOURCE_URL_VAR`] as the deployment
/// override. Precedence, highest first:
///
/// 1. An explicit `Url(Env)` names its own source and wins — the reserved
///    variable is not consulted.
/// 2. [`SOURCE_URL_VAR`], if set — overriding a configured value or filling an
///    omitted one.
/// 3. The configured literal URL or assembled parts.
/// 4. Otherwise an error.
pub fn resolve_connection_url(
    spec: Option<&ConnectionSpec>,
) -> Result<ConnectionUrl, ResolveError> {
    // An explicit `{ env = "X" }` reference wins and is not overridden.
    if let Some(ConnectionSpec::Url(env @ Secret::Env(_))) = spec {
        return Ok(ConnectionUrl::try_new(env.read()?)?);
    }

    // Otherwise the reserved variable overrides a configured value or fills an
    // omitted one.
    if let Ok(url) = std::env::var(SOURCE_URL_VAR) {
        return Ok(ConnectionUrl::try_new(url)?);
    }

    match spec {
        Some(ConnectionSpec::Url(secret)) => Ok(ConnectionUrl::try_new(secret.read()?)?),
        Some(ConnectionSpec::Parts {
            host,
            port,
            user,
            password,
            database,
        }) => Ok(ConnectionUrl::from_parts()
            .username(user.clone())
            .host(host.clone())
            .port(*port)
            .database(database.clone())
            .maybe_password(password.as_ref().map(Secret::read).transpose()?)
            .call()?),
        None => Err(ResolveError::MissingConnection),
    }
}

/// Resolve a **required** sink value, with `reserved` as the deployment override
/// variable. Same precedence as [`resolve_connection_url`]: an explicit `Env`
/// reference wins; otherwise `reserved` overrides the literal; otherwise the
/// literal.
pub fn resolve_required(secret: &Secret, reserved: &str) -> Result<String, ResolveError> {
    match secret {
        env @ Secret::Env(_) => env.read(),
        Secret::Value(literal) => Ok(literal_or_override(literal, reserved)),
    }
}

/// Resolve an **optional** sink value. Same precedence as
/// [`resolve_required`], plus: when the config omits it, `reserved` fills it if
/// set, otherwise `None`.
pub fn resolve_optional(
    secret: Option<&Secret>,
    reserved: &str,
) -> Result<Option<String>, ResolveError> {
    match secret {
        Some(env @ Secret::Env(_)) => env.read().map(Some),
        Some(Secret::Value(literal)) => Ok(Some(literal_or_override(literal, reserved))),
        None => Ok(std::env::var(reserved).ok()),
    }
}

/// The `reserved` variable if set, else the literal.
fn literal_or_override(literal: &str, reserved: &str) -> String {
    std::env::var(reserved).unwrap_or_else(|_| literal.to_owned())
}

/// The per-sink reserved-variable prefix: the sink's name, uppercased, so
/// several OpenSearch sinks never collide (`<NAME>_OPENSEARCH_URL`, …).
pub fn sink_var_prefix(name: &SinkName) -> String {
    name.to_string().to_uppercase()
}

#[derive(thiserror::Error, Debug)]
pub enum ResolveError {
    #[error("environment variable '{0}' is not set")]
    EnvNotSet(String),
    #[error("source has no connection_url and {SOURCE_URL_VAR} is not set")]
    MissingConnection,
    #[error("invalid connection URL: {0}")]
    ConnectionUrl(#[from] ConnectionUrlError),
    #[error("invalid HTTP URL: {0}")]
    HttpUrl(#[from] HttpUrlError),
}

/// Build an `HttpUrl` from a resolved string, mapping the validation error into
/// a [`ResolveError`]. Used by sink resolution.
pub fn http_url(value: String) -> Result<HttpUrl, ResolveError> {
    Ok(HttpUrl::try_new(value)?)
}
