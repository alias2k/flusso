# `libs/` — the flusso library crates

Every library crate, its layer, and what it holds. The numeric prefix on each directory **is** the dependency layer: a crate depends only on lower-numbered layers, and the graph stays acyclic. The five seams are named in [`CONTEXT.md`](../CONTEXT.md): **kernel**, **ports**, **adapters**, **engine**, **daemon**.

```text
0-kernel  →  1-{ports/*, config}  →  2-{adapters/*, engine}  →  3-daemon  →  apps/, sdk/
```

| Layer | Seam | Crate (extern name) | Path | Role |
| --- | --- | --- | --- | --- |
| 0 | kernel | `kernel` | [`0-kernel`](0-kernel) | The vocabulary every crate trades in: `GenericValue`, the validated newtypes, `IndexSchema`/`IndexMapping`, `FailurePolicy`, `Secret`, the `Options` tree, `AdapterConfig`. Names no adapter and no file format. |
| 0 | kernel | `kernel_derive` | [`0-kernel/derive`](0-kernel/derive) | `#[derive(AdapterConfig)]`, re-exported by `kernel` behind its `derive` feature. |
| 1 | port | `source` | [`1-ports/source`](1-ports/source) | The source port: `ChangeCapture`, `DocumentBuilder`, `SourceSpec`, `CaptureProvisioning`, `SchemaIntrospection`. |
| 1 | port | `stream` | [`1-ports/stream`](1-ports/stream) | The stream port: the producer/consumer contract between the ingest side and each sink. |
| 1 | port | `sink` | [`1-ports/sink`](1-ports/sink) | The sink port: the `Sink` trait and the JSON rendering of documents. |
| 1 | config | `config` | [`1-config`](1-config) | The `flusso.toml` and `*.schema.yml` parsers (as modules), the assembled `Config`, and the deterministic `flusso.lock`. Depends only on the kernel. |
| 2 | adapter | `source_postgres` | [`2-adapters/source-postgres`](2-adapters/source-postgres) | Postgres source: WAL capture, backfill, document building, publication management, and its own config type. |
| 2 | adapter | `stream_channel` | [`2-adapters/stream-channel`](2-adapters/stream-channel) | In-process stream: one bounded `tokio` channel per sink lane plus a request lane, with a ticketed in-flight ledger and the watermark. |
| 2 | adapter | `sink_opensearch` | [`2-adapters/sink-opensearch`](2-adapters/sink-opensearch) | OpenSearch sink: bulk API, typed mappings, hash alias over generations, seeding, reindex. |
| 2 | adapter | `sink_stdout` | [`2-adapters/sink-stdout`](2-adapters/sink-stdout) | Emits each operation to stdout (NDJSON or pretty). |
| 2 | engine | `engine` | [`2-engine`](2-engine) | The two generic loops that drive the ports: the ingest engine (capture → resolve → build once → publish) and one sink engine per sink (receive → apply → flush → ack); batched, deduplicated, at-least-once. |
| 3 | daemon | `daemon` | [`3-daemon`](3-daemon) | Supervises one deployment from a `Config` through the `Backends` seam: stages every sink engine, runs the engines as independent tasks with restart backoff, exposes per-sink `Status` and the reindex operation. Knows no adapter name. |

The `flusso` binary and the designer live under [`apps/`](https://github.com/alias2k/flusso/tree/main/apps); the consumer-facing query crates under [`sdk/`](https://github.com/alias2k/flusso/tree/main/sdk).

## Read next

- [Architecture](https://alias2k.github.io/flusso/contribute/architecture.html): why the kernel is one crate, the port seams, the daemon/binary split, package versus extern names.
- [The pipeline](https://alias2k.github.io/flusso/contribute/pipeline.html): the engine's invariants and their guard tests.
- [`CLAUDE.md`](https://github.com/alias2k/flusso/blob/main/CLAUDE.md): the agent-facing index and house rules. The workspace lints are strict and fail the build: no `.unwrap()`/`.expect()` outside tests, no `println!`, no `slice[i]`, every public type derives `Debug`.

## Adding a crate

1. Pick the seam. A port's contract goes in `1-ports/`; a concrete implementation of a port goes in `2-adapters/<port>-<technology>` and depends only on the kernel and its port. Needing a sibling adapter or a higher layer means the design is wrong.
2. An adapter owns its configuration: a `#[derive(AdapterConfig)]` struct in its crate, registered once in the composition root (`apps/cli/src/adapters.rs`). The config crate and the daemon never learn its name.
3. Update the table above, the Architecture chapter, and `CLAUDE.md`'s layering section in the same change.
4. Give it a `README.md` wired as the crate root doc (`#![doc = include_str!("../README.md")]`) so crates.io and docs.rs get a landing page.
