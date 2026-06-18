use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize};

use crate::EnvOrValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    Postgres(PostgresSource),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PostgresSource {
    pub connection_url: Option<ConnectionUrl>,
    /// Whether flusso may auto-create/extend the publication to cover every
    /// table the indexes read (when the source role is privileged enough).
    /// Omitted means enabled; set `false` to make flusso only report coverage
    /// gaps and never issue publication DDL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manage_publication: Option<bool>,
}

/// How the source database is reached: a full URL (literal or `{ env = "VAR" }`)
/// or the individual connection parts.
///
/// `Deserialize` is hand-written (not `#[serde(untagged)]`) so a malformed value
/// reports the three accepted shapes instead of serde's opaque "data did not
/// match any variant of untagged enum".
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ConnectionUrl {
    Url(EnvOrValue),
    Parts {
        host: String,
        port: u16,
        user: String,
        password: Option<EnvOrValue>,
        database: String,
    },
}

/// The connection parts, used only to deserialize the `Parts` arm — keeping
/// serde's field defaults and unknown-field rejection without re-deriving them
/// by hand.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Parts {
    #[serde(default = "default_host")]
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default = "default_user")]
    user: String,
    password: Option<EnvOrValue>,
    database: String,
}

const EXPECTED: &str = "a connection URL string, an env reference `{ env = \"VAR\" }`, \
                        or a connection-parts table `{ host, port, user, password, database }`";

impl<'de> Deserialize<'de> for ConnectionUrl {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Dispatch on the value's shape, then re-deserialize the matching arm so
        // its own (clear) errors surface, instead of `#[serde(untagged)]`'s opaque
        // "data did not match any variant".
        let value = toml::Value::deserialize(deserializer)?;
        match value {
            toml::Value::String(url) => Ok(ConnectionUrl::Url(EnvOrValue::Value(url))),
            toml::Value::Table(table) if table.contains_key("env") => {
                let env: EnvOrValue = table.try_into().map_err(de::Error::custom)?;
                Ok(ConnectionUrl::Url(env))
            }
            toml::Value::Table(table) => {
                let parts: Parts = table.try_into().map_err(de::Error::custom)?;
                Ok(ConnectionUrl::Parts {
                    host: parts.host,
                    port: parts.port,
                    user: parts.user,
                    password: parts.password,
                    database: parts.database,
                })
            }
            other => Err(de::Error::custom(format!(
                "expected {EXPECTED}, found {}",
                value_kind(&other)
            ))),
        }
    }
}

/// A human name for a TOML scalar, for the "found …" half of an error.
fn value_kind(value: &toml::Value) -> &'static str {
    match value {
        toml::Value::String(_) => "a string",
        toml::Value::Integer(_) => "an integer",
        toml::Value::Float(_) => "a float",
        toml::Value::Boolean(_) => "a boolean",
        toml::Value::Datetime(_) => "a datetime",
        toml::Value::Array(_) => "an array",
        toml::Value::Table(_) => "a table",
    }
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    5432
}

fn default_user() -> String {
    "postgres".to_owned()
}
