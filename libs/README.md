# `libs/` — the flusso library crates

Every library crate, its layer, and what it holds. The numeric prefix on each directory **is** the dependency layer: a crate depends only on lower-numbered layers, and the graph stays acyclic.

```text
0-core  →  1-{queue,sources,sinks}  →  2-{engine,schema}  →  3-daemon  →  apps/
```

| Layer | Crate (extern name) | Path | Role |
| --- | --- | --- | --- |
| 0 | `schema-core` | [`0-core`](0-core) | The validated domain model and vocabulary every crate produces and consumes. |
| 1 | `queue-core` | [`1-queue/0-core`](1-queue/0-core) | The work-queue abstraction, generic over the payload. |
| 1 | `queue-channel` | [`1-queue/1-channel`](1-queue/1-channel) | In-process queue over a bounded `tokio` mpsc channel. |
| 1 | `sources-core` | [`1-sources/0-core`](1-sources/0-core) | Source abstractions: `ChangeCapture`, `DocumentBuilder`, `CaptureProvisioning`, `SchemaIntrospection`. |
| 1 | `sources-postgres` | [`1-sources/1-postgres`](1-sources/1-postgres) | Postgres source: WAL capture, backfill, document building, publication management. |
| 1 | `sinks-core` | [`1-sinks/0-core`](1-sinks/0-core) | The `Sink` trait, JSON rendering, and a fan-out sink. |
| 1 | `sinks-stdout` | [`1-sinks/1-stdout`](1-sinks/1-stdout) | Writes each operation to stdout (NDJSON or pretty). |
| 1 | `sinks-opensearch` | [`1-sinks/2-opensearch`](1-sinks/2-opensearch) | OpenSearch sink: bulk API, typed mappings, hash alias over generations, seeding, reindex. |
| 2 | `engine` | [`2-engine`](2-engine) | The sync engine: capture → queue → resolve → build → sink, batched, at-least-once. |
| 2 | `schema` | [`2-schema`](2-schema) | Config front door: `load()` reads a `flusso.toml` + its schemas into one validated `Config`; compiles `flusso.lock`. |
| 2 | `schema-config-toml` | [`2-schema/1-config-toml`](2-schema/1-config-toml) | Parses `flusso.toml` → neutral entities. |
| 2 | `schema-index-yaml` | [`2-schema/1-index-yaml`](2-schema/1-index-yaml) | Parses `*.schema.yml` → core types. |
| 3 | `daemon` | [`3-daemon`](3-daemon) | Wires a `Config` into a running pipeline (domain only) and exposes live `Status`. |

The `flusso` binary, the designer, and the query crates live under [`apps/`](https://github.com/alias2k/flusso/tree/main/apps).

## Read next

- [Architecture](https://alias2k.github.io/flusso/contribute/architecture.html): why layer 0 is one crate, the trait-object seams, the daemon/binary split, package versus extern names.
- [The pipeline](https://alias2k.github.io/flusso/contribute/pipeline.html): the engine's invariants and their guard tests.
- [`CLAUDE.md`](https://github.com/alias2k/flusso/blob/main/CLAUDE.md): the agent-facing index and house rules. The workspace lints are strict and fail the build: no `.unwrap()`/`.expect()` outside tests, no `println!`, no `slice[i]`, every public type derives `Debug`.

## Adding a crate

1. Pick the layer: it must depend only on lower-numbered layers. Needing a sibling backend or a higher layer means the design is wrong.
2. Abstractions go in that layer's `0-core`; a concrete backend at a higher number in the same group.
3. Update the table above, the Architecture chapter, and `CLAUDE.md`'s layering section in the same change.
4. Give it a `README.md` wired as the crate root doc (`#![doc = include_str!("../README.md")]`) so crates.io and docs.rs get a landing page.
