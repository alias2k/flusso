use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct PerIndex {}

#[derive(Debug, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = source, kind = "demo", index_options = PerIndex)]
struct Source {
    #[serde(default)]
    pretty: bool,
}

fn main() {}
