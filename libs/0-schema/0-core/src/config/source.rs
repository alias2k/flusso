use serde::Serialize;

use crate::common;

/// The database documents are read from. Today that's always Postgres.
#[derive(Debug, Clone, Serialize)]
pub struct Source {
    pub source_type: common::SourceType,
    /// The `connection_url` serializes with its password redacted (see
    /// [`ConnectionUrl`](common::ConnectionUrl)), so a serialized `Source` is
    /// safe to print.
    pub connection_url: common::ConnectionUrl,
}
