use schema_core::{ConnectionSpec, ConnectionUrl, ResolveError, SourceType, resolve_connection_url};
use serde::{Deserialize, Serialize};

/// The database documents are read from. Today that's always Postgres.
///
/// The connection is stored unresolved (a literal or an environment reference)
/// and resolved at runtime by [`resolve_connection_url`](Source::resolve_connection_url),
/// so a compiled config carries no secret it wasn't given literally.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Source {
    pub source_type: SourceType,
    /// How to reach the database. `None` means "rely entirely on
    /// `DATABASE_URL`", checked when the connection is resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<ConnectionSpec>,
}

impl Source {
    /// Resolve the connection URL now, in the current environment, applying the
    /// `DATABASE_URL` deployment override. Call this at connect time, not at
    /// load time, so the value is read where the pipeline runs.
    pub fn resolve_connection_url(&self) -> Result<ConnectionUrl, ResolveError> {
        resolve_connection_url(self.connection.as_ref())
    }
}
