# Architecture

flusso is a Rust workspace of small crates arranged along five named seams, kernel, ports, adapters, engine, daemon, with the `flusso` binary as the single place that names a concrete adapter.

## The model

```text
libs/0-kernel  →  libs/1-{ports/*, config}  →  libs/2-{adapters/*, engine}  →  libs/3-daemon  →  apps/, sdk/
```

The numeric prefix on each directory **is** the dependency layer. A crate depends only on lower-numbered layers, and the graph is acyclic, which is also what lets the workspace publish bottom-up to crates.io. The vocabulary for the seams is in the repository's `CONTEXT.md`.

| Layer | Seam | Crate | Holds |
| --- | --- | --- | --- |
| 0 | kernel | `kernel` (+ `kernel_derive`) | The vocabulary every layer trades in: `GenericValue`, the validated newtypes, `IndexSchema`, `IndexMapping`, `Field`/`Filter`, `FailurePolicy`, `Secret`, the `Options` tree, `PortEntry`, `AdapterConfig`. Names no adapter and no file format. |
| 1 | port | `source` | `ChangeCapture`, `DocumentBuilder`, `SourceSpec`, `CaptureProvisioning`, `SchemaIntrospection`. |
| 1 | port | `stream` | The producer/consumer contract between the source side and the sinks. |
| 1 | port | `sink` | The `Sink` trait and the JSON rendering of documents. |
| 1 | config | `config` | The `flusso.toml` and `*.schema.yml` parsers as modules, the assembled `Config`, the `flusso.lock` compiler. Depends only on the kernel. |
| 2 | adapter | `source_postgres`, `stream_channel`, `sink_opensearch`, `sink_stdout` | One implementation of one port for one technology, each owning its configuration type. |
| 2 | engine | `engine` | The generic loop that drives the ports: capture, buffer, resolve, build, sink, flush, ack. |
| 3 | daemon | `daemon` | Assembles one deployment from a `Config` through the `Backends` seam and exposes live `Status`. Names no adapter. |
| apps | | `flusso-cli` | The binary: the adapter registry and the `Backends` implementation, telemetry, HTTP surfaces, signals. |
| apps | | `flusso-design` | The visual designer: axum API plus the embedded SPA. Renders its forms from the adapter descriptions the CLI hands it. |
| sdk | | `flusso-query`, `flusso-query-derive` | The read side, depending only on the kernel vocabulary and the config crate. |

## Why the kernel is one crate

`kernel` is the cross-cutting vocabulary and nothing else. Everything that turns config *files* into that vocabulary lives a layer up in `config`, and everything an adapter needs to know about its own settings lives in the adapter. That placement is deliberate: an adapter depends only on the vocabulary and its port, and cannot reach the assembled `Config` or the file parsers. The Postgres source takes a `SourceSpec` (the enabled indexes and their schemas, in kernel types), never the whole `Config`; the OpenSearch sink takes `IndexMapping`s.

## Adapters own their configuration

Every port table in `flusso.toml` (`[source]`, `[stream]`, `[sinks.<name>]`) is a `PortEntry`: a `type` plus an uninterpreted `Options` tree. The config crate carries it through without knowing its shape. The adapter for that `type` declares a config struct with `#[derive(AdapterConfig)]`, and the CLI's registry (`apps/cli/src/adapters.rs`) turns the options into that struct with `deny_unknown_fields`, before anything connects. From the same declaration the CLI renders the editor JSON schema, the Reference option tables, and the descriptions the designer builds its forms from. Adding an adapter is its crate plus one registry entry. The decision and its trade-offs are ADR 0001 in `docs/adr/`.

Secrets are `Secret` values, resolved by the adapter in the running environment with one override rule for every adapter: `<ENTRY>_<TYPE>_<KEY>`, so `PRIMARY_OPENSEARCH_URL` and `SOURCE_POSTGRES_CONNECTION_URL` follow the same rule.

## Everything the engine drives is a trait object

`ChangeCapture`, `DocumentBuilder`, `Sink`, and the stream are all `#[async_trait]` objects. The engine is the only orchestrator and knows no adapter, so WAL versus polling, stdout versus OpenSearch, and channel versus durable broker swap without touching the loop. Tests mock them.

## The daemon owns the domain, the binary owns the transport

The daemon assembles the pipeline from a `Config`, but adapter construction is a seam: the `Backends` trait is supplied to `Daemon::new`, so the daemon depends only on the `source` and `sink` ports. `Backends::validate` runs first, so a bad option fails before any connection. The daemon reports progress through the engine's `Observer` trait into a shared `Status`, and owns no HTTP server, no metrics exporter, no signals.

The CLI is the **composition root**. It holds the adapter registry, implements `Backends`, lays the adapter-specific flags (`--slot`, `--queue-capacity`, `--pretty`) over the file's entries as overrides, installs the OpenTelemetry meter provider, records metrics from `Observer` events, serves the two HTTP surfaces, and handles `SIGINT`/`SIGTERM`.

## Package name versus extern name

Every crate publishes under a `flusso-*` package name but is used by a short extern name: `flusso-kernel` on crates.io, `kernel` in `use`; `flusso-source-postgres` and `source_postgres`. Each lib sets `[lib] name = "<extern>"`, and `[workspace.dependencies]` keeps the short key with `package = "flusso-…"`, so a rename needs no source change. The names were changed once, at 0.16, to match the seams (ADR 0004).

## Where this shows up

- [The pipeline](pipeline.md) for the engine's invariants.
- [The config layer](config-layer.md) for parse-then-convert, adapter validation, and the lock.
- `libs/README.md` in the repository for the crate table with paths, and `CLAUDE.md` for the agent-facing index of the same material.
