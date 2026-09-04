# flusso-kernel-derive

`#[derive(AdapterConfig)]`: declare an adapter's configuration once and get its kind, port, JSON schema, example, and override variables from that one declaration.

```rust,ignore
use kernel::{AdapterConfig, Secret};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where the documents go.
#[derive(Debug, Serialize, Deserialize, JsonSchema, AdapterConfig)]
#[serde(deny_unknown_fields)]
#[adapter(port = sink, kind = "demo")]
pub struct DemoConfig {
    /// The cluster URL.
    #[adapter(example = "https://search:9200")]
    pub url: Secret,
    /// Documents per request.
    #[serde(default = "default_batch_size")]
    pub batch_size: u32,
}

fn default_batch_size() -> u32 { 1000 }
```

| Attribute | On | Meaning |
| --- | --- | --- |
| `#[adapter(port = source \| stream \| sink, kind = "…")]` | the struct | Which port, and the `type = "…"` token that selects the adapter |
| `#[adapter(example = …)]` | a field | The value `example()` uses; a string literal goes through `Into`, anything else is used verbatim, an `Option<T>` is wrapped in `Some` |

A field with no example falls back to its serde default; an `Option` with neither is `None`; a required field with no example is a compile error. The struct must carry `#[serde(deny_unknown_fields)]`, also checked at compile time.

Re-exported by `flusso-kernel` behind its `derive` feature; depend on the kernel, not on this crate.
