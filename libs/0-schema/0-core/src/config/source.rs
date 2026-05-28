use crate::common;

#[derive(Debug, Clone)]
pub struct Source {
    pub source_type: common::SourceType,
    pub connection_url: common::ConnectionUrl,
}
