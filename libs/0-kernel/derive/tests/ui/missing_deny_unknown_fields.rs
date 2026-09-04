use kernel::AdapterConfig;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[adapter(port = sink, kind = "demo")]
struct Lenient {
    #[serde(default)]
    pretty: bool,
}

fn main() {}
