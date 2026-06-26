# `libs/` — the flusso library crates

This is the contributor's map of flusso's internals. If you're *using* flusso, you want
the [manual](https://alias2k.github.io/flusso/) instead; if you're hacking on the code,
start here, then read [`CLAUDE.md`](https://github.com/alias2k/flusso/blob/main/CLAUDE.md)
for the architecture tour and house rules.

## The numeric prefix is the dependency layer

Every crate sits in a numbered layer, and **a crate may only depend on lower-numbered
layers**. The dependency graph is kept strictly acyclic — that's the whole point of the
numbering, and it's what lets the workspace publish bottom-up to crates.io.

```text
0-core  →  1-{queue,sources,sinks}  →  2-{engine,schema}  →  3-daemon  →  apps/
```

Within a layer, **`0-core` is the abstraction/domain** and higher numbers are concrete
backends. So `libs/1-sinks/0-core` is the `Sink` trait; `libs/1-sinks/2-opensearch` is an
implementation of it. A backend depends on its layer's `0-core`, never the reverse, and
never on a sibling backend.

Layer 0 is a single crate, `schema-core` — the cross-cutting vocabulary (`GenericValue`,
the newtypes, `IndexMapping`, `Field`/`Filter`, the per-sink configs) that every other
layer trades in. Keeping it alone at the bottom is deliberate: the layer-1 backends depend
only on this vocabulary and *cannot* see the assembled `Config` or the file parsers, which
live a layer up in the `2-schema` group.

## The crates

| Layer | Crate (extern name) | Path | Role |
| --- | --- | --- | --- |
| 0 | `schema-core` | [`0-core`](0-core) | The validated domain model + vocabulary every crate produces and consumes. |
| 1 | `queue-core` | [`1-queue/0-core`](1-queue/0-core) | The work-queue abstraction, generic over the payload. |
| 1 | `queue-channel` | [`1-queue/1-channel`](1-queue/1-channel) | In-process queue over a bounded `tokio` mpsc channel. |
| 1 | `sources-core` | [`1-sources/0-core`](1-sources/0-core) | Source abstractions: `ChangeCapture`, `DocumentBuilder`, `CaptureProvisioning`. |
| 1 | `sources-postgres` | [`1-sources/1-postgres`](1-sources/1-postgres) | Postgres source: WAL capture, backfill, document building, publication management. |
| 1 | `sinks-core` | [`1-sinks/0-core`](1-sinks/0-core) | The `Sink` trait, JSON rendering, and a fan-out sink. |
| 1 | `sinks-stdout` | [`1-sinks/1-stdout`](1-sinks/1-stdout) | Writes each operation to stdout (NDJSON or pretty). |
| 1 | `sinks-opensearch` | [`1-sinks/2-opensearch`](1-sinks/2-opensearch) | OpenSearch sink: bulk API, typed mappings, hashed index names + alias, seeding, reindex. |
| 2 | `engine` | [`2-engine`](2-engine) | The sync engine — capture → queue → resolve → build → sink, batched, at-least-once. |
| 2 | `schema` | [`2-schema`](2-schema) | Config front door: `load()` reads a `flusso.toml` + its schemas into one validated `Config`. |
| 2 | `schema-config-toml` | [`2-schema/1-config-toml`](2-schema/1-config-toml) | Parses `flusso.toml` → neutral entities. |
| 2 | `schema-index-yaml` | [`2-schema/1-index-yaml`](2-schema/1-index-yaml) | Parses `*.schema.yml` → core types. |
| 3 | `daemon` | [`3-daemon`](3-daemon) | Wires a `Config` into a running pipeline (domain only) and exposes live `Status`. |

The `flusso` binary and the query-side crates live one level up, under
[`apps/`](https://github.com/alias2k/flusso/tree/main/apps).

## Package name vs extern name

A crate's published **package name** differs from the **extern name** code uses:
`flusso-schema-core` on crates.io, `schema_core` in `use`. Each lib sets
`[lib] name = "<extern>"`, and `[workspace.dependencies]` keeps the short key plus
`package = "flusso-…"`. So `use schema_core::…` works with no rename in source.

## Adding a crate

1. Pick the layer: does it depend only on lower-numbered layers? If it'd need a sibling
   backend or a higher layer, the design is wrong.
2. Put abstractions in that layer's `0-core`; put a concrete backend at a higher number in
   the same group.
3. Keep the graph acyclic. Update the table above, the root
   [`README.md`](https://github.com/alias2k/flusso/blob/main/README.md) "Project layout",
   and `CLAUDE.md`'s layering section in the same change.
4. Give it a `README.md` wired as the crate root doc (`#![doc = include_str!("../README.md")]`)
   so crates.io and docs.rs get a real landing page.

## House rules

The workspace lints are strict and **fail the build**: no `.unwrap()`/`.expect()` outside
tests, no `println!`/`eprintln!` (use `tracing`), no `slice[i]` indexing, every public type
derives `Debug`. See [`CLAUDE.md`](https://github.com/alias2k/flusso/blob/main/CLAUDE.md)
for the full list and the rationale.
