use schema_core::{OpensearchSink, StdoutSink};
use serde::{Deserialize, Serialize};

/// A destination for built documents: an OpenSearch cluster, or `stdout` for
/// inspecting output during development.
///
/// The per-backend settings ([`OpensearchSink`]/[`StdoutSink`]) are vocabulary
/// the sink backends read directly; this enum is the composition glue that
/// selects between them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sink {
    Opensearch(OpensearchSink),
    Stdout(StdoutSink),
}
