use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = sink, kind = "demo")]
struct NoExample {
    url: String,
}

fn main() {}
