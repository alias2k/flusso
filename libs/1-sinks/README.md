# 1-sinks — the sink layer

Where the engine writes each built document: `0-core` is the `Sink` trait, the higher-numbered crates are concrete destinations.

| Crate | Path | Role |
| --- | --- | --- |
| sinks-core | [0-core](0-core) | The `Sink` trait, `to_json` rendering, the fan-out sink, and the shared `SinkError`/`Result`. |
| sinks-stdout | [1-stdout](1-stdout) | Emits each operation as a self-describing JSON envelope (NDJSON or pretty) — for watching or piping into `jq`. |
| sinks-opensearch | [2-opensearch](2-opensearch) | Writes to OpenSearch via the bulk API: owns its indexes with `dynamic: strict` mappings, alias-over-generations addressing, reindex, and backlog-adaptive refresh. |

`0-core` also holds the neutral-value→JSON rendering and a fan-out sink that multiplexes one stream to several destinations.

Part of [the flusso library crates](../README.md).
