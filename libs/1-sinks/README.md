# 1-sinks — the sink layer

`0-core` defines the `Sink` trait, the neutral-value→JSON rendering, and a fan-out sink that
multiplexes one stream to several destinations; the higher-numbered crates are concrete
backends. The engine writes each built document to a `Sink`.

- [sinks-core](0-core) — the `Sink` trait, `to_json` rendering, the fan-out sink, and the shared `SinkError`/`Result`.
- [sinks-stdout](1-stdout) — a `Sink` that emits each operation as a self-describing JSON envelope (NDJSON or pretty) for watching or piping into `jq`.
- [sinks-opensearch](2-opensearch) — a `Sink` that writes to an OpenSearch cluster via the bulk API: owns its indexes with explicit `dynamic: strict` mappings, alias-over-generations addressing, reindex, and backlog-adaptive refresh.

Part of [the flusso library crates](../README.md).
