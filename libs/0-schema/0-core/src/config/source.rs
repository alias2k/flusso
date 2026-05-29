use crate::common;

/// The database documents are read from. Today that's always Postgres.
#[derive(Debug, Clone)]
pub struct Source {
    pub source_type: common::SourceType,
    pub connection_url: common::ConnectionUrl,
}
