# Architecture

flusso is a Rust workspace of small crates arranged in numbered dependency layers, with the `flusso` binary as the single place that names concrete backends.

## The model

```text
libs/0-core  →  libs/1-{queue,sources,sinks}  →  libs/2-{engine,schema}  →  libs/3-daemon  →  apps/
```

The numeric prefix on each directory **is** the dependency layer. A crate depends only on lower-numbered layers, and within a layer `0-core` holds the abstraction while higher numbers are concrete backends. The graph is acyclic, which is also what lets the workspace publish bottom-up to crates.io.

| Layer | Crate | Holds |
| --- | --- | --- |
| 0 | `schema-core` | The vocabulary every layer trades in: `GenericValue`, the validated newtypes, `IndexSchema`, `IndexMapping`, `Field`/`Filter`, `FailurePolicy`, the per-sink configs. |
| 1 | `queue-core`, `queue-channel` | The work-queue trait and its in-process bounded-channel implementation. |
| 1 | `sources-core`, `sources-postgres` | `ChangeCapture`, `DocumentBuilder`, `CaptureProvisioning`, `SchemaIntrospection`; and the Postgres implementation over logical replication. |
| 1 | `sinks-core`, `sinks-stdout`, `sinks-opensearch` | The `Sink` trait, JSON rendering, fan-out; and the two backends. |
| 2 | `engine` | The pipeline loop: capture, queue, resolve, build, sink, flush, ack. |
| 2 | `schema`, `schema-config-toml`, `schema-index-yaml` | The two file parsers and the assembled, validated `Config`; the `flusso.lock` compiler. |
| 3 | `daemon` | Wires a `Config` into a running pipeline and exposes live `Status`. Names no backend. |
| apps | `flusso-cli` | The binary: the `Backends` implementation, telemetry, HTTP surfaces, signals. |
| apps | `flusso-design` | The visual designer: axum API plus the embedded SPA. |
| apps | `flusso-query`, `flusso-query-derive` | The read side, depending only on the layer-0 vocabulary and the parsers. |

## Why layer 0 is one crate

`schema-core` is the cross-cutting vocabulary and nothing else. Everything that turns config *files* into that vocabulary lives a layer up in the `schema` group. That placement is deliberate: a layer-1 backend depends only on the vocabulary and cannot reach the assembled `Config` or the file parsers. The Postgres source takes a `SourceSpec` (the enabled indexes and their schemas, in core types), never the whole `Config`; the OpenSearch sink takes `IndexMapping`s.

## Everything the engine drives is a trait object

`ChangeCapture`, `DocumentBuilder`, `Sink`, and the queue are all `#[async_trait]` objects. The engine is the only orchestrator and knows no backend, so WAL versus polling, stdout versus OpenSearch, and channel versus durable broker swap without touching the loop. Tests mock them.

## The daemon owns the domain, the binary owns the transport

The daemon assembles the pipeline from a `Config`, but backend construction is a seam: the `Backends` trait is supplied to `Daemon::new`, so the daemon depends only on `sources-core` and `sinks-core`. It reports progress through the engine's `Observer` trait into a shared `Status`, and owns no HTTP server, no metrics exporter, no signals.

The CLI is the **composition root**. It implements `Backends` (the one `match` over source and sink types), installs the OpenTelemetry meter provider, records metrics from `Observer` events, serves the two HTTP surfaces, and handles `SIGINT`/`SIGTERM`. Adding a backend is a new match arm plus its crate; the daemon and engine are untouched.

## Package name versus extern name

Every crate publishes under a `flusso-*` package name but is used by a short extern name: `flusso-schema-core` on crates.io, `schema_core` in `use`. Each lib sets `[lib] name = "<extern>"`, and `[workspace.dependencies]` keeps the short key with `package = "flusso-…"`, so a rename needs no source change.

## Where this shows up

- [The pipeline](pipeline.md) for the engine's invariants.
- [The config layer](config-layer.md) for parse-then-convert and the lock.
- `libs/README.md` in the repository for the crate table with paths, and `CLAUDE.md` for the agent-facing index of the same material.
