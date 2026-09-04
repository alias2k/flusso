//! The Postgres source's own configuration: the `[source]` table with
//! `type = "postgres"`.
//!
//! [`PostgresConfig`] is what the composition root deserializes the entry's
//! options into ([`kernel::AdapterConfig`]); nothing outside this crate knows
//! its fields. The connection is a [`Connection`]: a URL (literal or
//! `{ env = "VAR" }`) or its parts. Resolution happens at run time, in
//! [`PostgresConfig::resolve_connection_url`], where the deployment override
//! `SOURCE_POSTGRES_CONNECTION_URL` applies.
//!
//! ```
//! use kernel::{AdapterConfig, Options};
//! use source_postgres::{Connection, PostgresConfig, SslMode};
//!
//! let options: Options = toml::from_str(r#"
//!     connection_url = { env = "PG_URL" }
//!     ssl_mode = "verify-full"
//!     slot = "search"
//! "#).unwrap();
//! let config = PostgresConfig::from_options(options).unwrap();
//! assert!(matches!(config.connection_url, Some(Connection::Url(_))));
//! assert_eq!(config.ssl_mode, Some(SslMode::VerifyFull));
//! assert_eq!(config.slot, "search");
//! assert_eq!(config.publication, "flusso");
//! assert!(config.manage_publication);
//! ```

use std::fmt;
use std::path::PathBuf;

use kernel::{AdapterConfig, Port, ResolveError, Secret, override_var};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::{self, Deserializer, MapAccess, Visitor};
use serde::{Deserialize, Serialize};

use crate::connection_url::ConnectionUrl;

/// The `[source]` options for `type = "postgres"`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = source, kind = "postgres")]
pub struct PostgresConfig {
    /// How the database is reached: a URL, literal or `{ env = "VAR" }`, or a
    /// table of parts (`host`, `port`, `user`, `password`, `database`). May be
    /// omitted when `SOURCE_POSTGRES_CONNECTION_URL` supplies it at run time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = Connection::Url(Secret::env("PG_URL")))]
    pub connection_url: Option<Connection>,
    /// Whether flusso may create or extend the publication to cover every table
    /// the indexes read, when the source role is privileged enough. `false`
    /// makes flusso only report coverage gaps and never issue publication DDL.
    #[serde(default = "default_true")]
    pub manage_publication: bool,
    /// The logical replication slot flusso consumes. Created on first run.
    #[serde(default = "default_flusso")]
    pub slot: String,
    /// The publication flusso subscribes to.
    #[serde(default = "default_flusso")]
    pub publication: String,
    /// TLS mode for the source connection. Overrides the URL's `sslmode`;
    /// omitted defers to it, then to `prefer`. `require` encrypts but verifies
    /// nothing; only the `verify-*` modes check the certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = SslMode::VerifyFull)]
    pub ssl_mode: Option<SslMode>,
    /// PEM file of trusted CA certificates for the `verify-*` modes. Overrides
    /// the URL's `sslrootcert`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[adapter(example = "/etc/ssl/certs/ca.pem")]
    pub ssl_root_cert: Option<PathBuf>,
    /// Client certificate chain PEM for mutual TLS; pairs with `ssl_key`.
    /// Overrides the URL's `sslcert`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_cert: Option<PathBuf>,
    /// Client private key PEM for mutual TLS; pairs with `ssl_cert`. Overrides
    /// the URL's `sslkey`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_key: Option<PathBuf>,
    /// SNI hostname sent in the TLS handshake when it differs from the
    /// connection host (IP connections, load balancers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssl_sni_hostname: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_flusso() -> String {
    "flusso".to_owned()
}

impl PostgresConfig {
    /// The entry name this singleton port is configured under, and the
    /// `<ENTRY>` of its override variables.
    pub const ENTRY: &'static str = "source";

    /// The variable that overrides (or supplies) the connection URL.
    pub fn connection_url_var() -> String {
        override_var(Self::ENTRY, Self::KIND, "connection_url")
    }

    /// Resolve the connection URL in the running environment. Precedence: an
    /// explicit `{ env = "VAR" }` URL names its own source and wins; otherwise
    /// `SOURCE_POSTGRES_CONNECTION_URL`, if set, overrides a literal URL, the
    /// parts, or an omitted connection; otherwise the literal URL or the
    /// assembled parts (whose `password` takes
    /// `SOURCE_POSTGRES_CONNECTION_URL_PASSWORD`).
    pub fn resolve_connection_url(&self) -> Result<ConnectionUrl, ResolveError> {
        let var = Self::connection_url_var();
        let invalid = |e: crate::connection_url::ConnectionUrlError| {
            ResolveError::Invalid(format!("invalid connection URL: {e}"))
        };
        match &self.connection_url {
            Some(Connection::Url(secret)) => {
                ConnectionUrl::try_new(secret.resolve(&var)?).map_err(invalid)
            }
            Some(Connection::Parts {
                host,
                port,
                user,
                password,
                database,
            }) => {
                if let Ok(url) = std::env::var(&var) {
                    return ConnectionUrl::try_new(url).map_err(invalid);
                }
                let password = Secret::resolve_optional(
                    password.as_ref(),
                    &override_var(Self::ENTRY, Self::KIND, "connection_url.password"),
                )?;
                ConnectionUrl::from_parts()
                    .username(user.clone())
                    .host(host.clone())
                    .port(*port)
                    .database(database.clone())
                    .maybe_password(password)
                    .call()
                    .map_err(invalid)
            }
            None => match std::env::var(&var) {
                Ok(url) => ConnectionUrl::try_new(url).map_err(invalid),
                Err(_) => Err(ResolveError::Missing(format!(
                    "the source has no `connection_url` and {var} is not set"
                ))),
            },
        }
    }

    /// The declared TLS settings, grouped for the connection helpers.
    pub fn tls(&self) -> Tls {
        Tls {
            mode: self.ssl_mode,
            root_cert: self.ssl_root_cert.clone(),
            client_cert: self.ssl_cert.clone(),
            client_key: self.ssl_key.clone(),
            sni_hostname: self.ssl_sni_hostname.clone(),
        }
    }

    /// Which port this adapter implements, for callers that hold the type
    /// without the trait in scope.
    pub const fn port() -> Port {
        Port::Source
    }
}

/// How the source database is reached: a full URL (literal or `{ env = "VAR" }`)
/// or the individual connection parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Connection {
    /// A full connection URL.
    Url(Secret),
    /// The parts of a connection URL; `password` may come from the environment.
    Parts {
        host: String,
        port: u16,
        user: String,
        password: Option<Secret>,
        database: String,
    },
}

const CONNECTION_EXPECTED: &str = "a connection URL string, an env reference `{ env = \"VAR\" }`, \
     or a table of connection parts (`host`, `port`, `user`, `password`, `database`)";

impl Serialize for Connection {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Connection::Url(secret) => secret.serialize(serializer),
            Connection::Parts {
                host,
                port,
                user,
                password,
                database,
            } => {
                let mut out = serializer.serialize_struct("Parts", 5)?;
                out.serialize_field("host", host)?;
                out.serialize_field("port", port)?;
                out.serialize_field("user", user)?;
                if let Some(password) = password {
                    out.serialize_field("password", password)?;
                }
                out.serialize_field("database", database)?;
                out.end()
            }
        }
    }
}

/// The parts table, used only to deserialize the `Parts` arm so serde's field
/// defaults and unknown-field rejection apply without re-deriving them by hand.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Parts {
    /// Database host. Default: `localhost`.
    #[serde(default = "default_host")]
    host: String,
    /// Database port. Default: `5432`.
    #[serde(default = "default_port")]
    port: u16,
    /// Database role. Default: `postgres`.
    #[serde(default = "default_user")]
    user: String,
    /// The role's password, literal or `{ env = "VAR" }`.
    #[serde(default)]
    password: Option<Secret>,
    /// Database name.
    database: String,
}

fn default_host() -> String {
    "localhost".to_owned()
}

fn default_port() -> u16 {
    5432
}

fn default_user() -> String {
    "postgres".to_owned()
}

struct ConnectionVisitor;

impl<'de> Visitor<'de> for ConnectionVisitor {
    type Value = Connection;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(CONNECTION_EXPECTED)
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Connection::Url(Secret::Value(value.to_owned())))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(Connection::Url(Secret::Value(value)))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut entries: Vec<(String, serde_json::Value)> = Vec::new();
        while let Some(key) = map.next_key::<String>()? {
            let value: serde_json::Value = map.next_value()?;
            entries.push((key, value));
        }
        if entries.is_empty() {
            return Err(de::Error::custom(format!(
                "expected {CONNECTION_EXPECTED}, found an empty table"
            )));
        }
        if entries.iter().any(|(key, _)| key == "env") {
            if let Some((extra, _)) = entries.iter().find(|(key, _)| key != "env") {
                return Err(de::Error::custom(format!(
                    "unexpected key `{extra}` in env reference — write it as `{{ env = \"VAR\" }}`"
                )));
            }
            let env = entries
                .into_iter()
                .find_map(|(key, value)| (key == "env").then_some(value))
                .and_then(|value| value.as_str().map(str::to_owned))
                .ok_or_else(|| de::Error::custom("`env` must be a string"))?;
            return Ok(Connection::Url(Secret::Env(env)));
        }
        let object: serde_json::Map<String, serde_json::Value> = entries.into_iter().collect();
        let parts: Parts = serde_json::from_value(serde_json::Value::Object(object))
            .map_err(|e| de::Error::custom(format!("{e} — expected {CONNECTION_EXPECTED}")))?;
        Ok(Connection::Parts {
            host: parts.host,
            port: parts.port,
            user: parts.user,
            password: parts.password,
            database: parts.database,
        })
    }
}

impl<'de> Deserialize<'de> for Connection {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ConnectionVisitor)
    }
}

impl JsonSchema for Connection {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Connection")
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let secret = generator.subschema_for::<Secret>();
        let parts = generator.subschema_for::<Parts>();
        json_schema!({
            "description": "A connection URL (literal or `{ env = \"VAR\" }`), or a table of connection parts.",
            "anyOf": [secret, parts]
        })
    }
}

/// `sslmode` for the source connection, in libpq's terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SslMode {
    /// Never use TLS.
    Disable,
    /// Use TLS if the server offers it, without verifying anything.
    Prefer,
    /// Require TLS, without verifying the certificate.
    Require,
    /// Require TLS and verify the certificate against the CA bundle.
    VerifyCa,
    /// Require TLS, verify the certificate, and check the hostname.
    VerifyFull,
}

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

/// The declared TLS settings, as the connection helpers consume them. Every
/// field may be `None`: the URL's own `ssl*` parameters then apply, then the
/// libpq defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tls {
    pub mode: Option<SslMode>,
    pub root_cert: Option<PathBuf>,
    pub client_cert: Option<PathBuf>,
    pub client_key: Option<PathBuf>,
    pub sni_hostname: Option<String>,
}

impl Tls {
    /// Whether nothing was declared.
    pub fn is_unset(&self) -> bool {
        self == &Tls::default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use kernel::Options;

    fn config(toml: &str) -> PostgresConfig {
        let options: Options = toml::from_str(toml).unwrap();
        PostgresConfig::from_options(options).unwrap()
    }

    #[test]
    fn url_forms_parse() {
        assert_eq!(
            config("connection_url = \"postgres://u@h/d\"").connection_url,
            Some(Connection::Url(Secret::Value("postgres://u@h/d".into())))
        );
        assert_eq!(
            config("connection_url = { env = \"PG\" }").connection_url,
            Some(Connection::Url(Secret::Env("PG".into())))
        );
    }

    #[test]
    fn parts_parse_with_defaults() {
        let parsed =
            config("connection_url = { database = \"shop\", password = { env = \"PW\" } }");
        assert_eq!(
            parsed.connection_url,
            Some(Connection::Parts {
                host: "localhost".into(),
                port: 5432,
                user: "postgres".into(),
                password: Some(Secret::Env("PW".into())),
                database: "shop".into(),
            })
        );
    }

    #[test]
    fn wrong_connection_shape_lists_the_three_forms() {
        let options: Options = toml::from_str("connection_url = 5").unwrap();
        let error = PostgresConfig::from_options(options)
            .unwrap_err()
            .to_string();
        assert!(error.contains("connection URL string"), "{error}");
        let options: Options =
            toml::from_str("connection_url = { env = \"X\", oops = 1 }").unwrap();
        let error = PostgresConfig::from_options(options)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unexpected key `oops`"), "{error}");
    }

    #[test]
    fn unknown_field_is_rejected() {
        let options: Options =
            toml::from_str("connection_url = \"postgres://u@h/d\"\nslott = \"x\"").unwrap();
        let error = PostgresConfig::from_options(options)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unknown field `slott`"), "{error}");
    }

    #[test]
    fn example_is_complete_and_round_trips() {
        let example = PostgresConfig::example();
        let options = Options::from_serialize(&example).unwrap();
        assert_eq!(PostgresConfig::from_options(options).unwrap(), example);
        let description = PostgresConfig::description();
        assert_eq!(description.kind, "postgres");
        assert!(
            description
                .secrets
                .iter()
                .any(|path| path == "connection_url"),
            "{:?}",
            description.secrets
        );
    }

    #[test]
    fn parts_round_trip_through_serialize() {
        let parsed = config(
            "connection_url = { host = \"db\", port = 5433, user = \"app\", database = \"shop\" }",
        );
        let text = toml::to_string(&parsed).unwrap();
        assert!(
            text.contains(
                "[connection_url]\nhost = \"db\"\nport = 5433\nuser = \"app\"\ndatabase = \"shop\""
            ),
            "{text}"
        );
        let again: PostgresConfig = toml::from_str(&text).unwrap();
        assert_eq!(again, parsed);
    }

    #[test]
    fn tls_groups_the_ssl_fields() {
        let parsed = config("ssl_mode = \"verify-ca\"\nssl_root_cert = \"/ca.pem\"");
        let tls = parsed.tls();
        assert_eq!(tls.mode, Some(SslMode::VerifyCa));
        assert_eq!(tls.root_cert, Some(PathBuf::from("/ca.pem")));
        assert!(config("").tls().is_unset());
    }

    #[test]
    fn override_variable_names() {
        assert_eq!(
            PostgresConfig::connection_url_var(),
            "SOURCE_POSTGRES_CONNECTION_URL"
        );
    }
}
