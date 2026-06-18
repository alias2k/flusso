---
name: flusso-internals
description: Work on the flusso Rust codebase itself — add a source/sink backend, change the engine/pipeline, touch the config or schema layers, or extend the query derive. Use when contributing to or modifying flusso's implementation (not just authoring config). Covers crate layering, the sync pipeline, engine invariants, and where things live.
---

# Working on the flusso implementation

flusso keeps OpenSearch in sync with Postgres from declarative config. This skill is for changing the **Rust codebase**; for authoring config use **flusso-schema**, for the read client use **flusso-query**.

Before a non-trivial change, read the repo's `CLAUDE.md` — it's the living architecture index and the source of truth. This skill is the orientation; `CLAUDE.md` has the detail and stays current.

## Crate layering — the numeric prefix is the dependency layer

Crates under `libs/` and `apps/`. A crate depends **only on lower-numbered layers**. Keep it acyclic.

```
0-core  →  1-{queue,sources,sinks}  →  2-{engine,schema}  →  3-daemon  →  apps
```

- **`libs/0-core`** (`schema-core`) — the cross-cutting vocabulary every layer trades in: `GenericValue`, newtypes, `IndexMapping`, `IndexSchema`, `Field`/`Filter`, `FailurePolicy`, per-sink configs. The **only** layer-0 crate.
- **Layer 1** — concrete backends + abstractions: sources (`1-sources/0-core` traits, `1-sources/1-postgres`), sinks (`1-sinks/0-core`, `1-sinks/2-opensearch`), queue (`1-queue`).
- **Layer 2** — `2-engine` (the orchestrator) and `2-schema` (config loading: the file parsers `1-config-toml`/`1-index-yaml`, the assembled `Config`, conversion, loader). Parsers live at layer 2 **on purpose** so layer-1 backends can't reach config-loading machinery.
- **`libs/3-daemon`** — the domain: assembles the pipeline from a `Config` but names **no** concrete backend (backends arrive via the `Backends` trait). Telemetry-agnostic, owns no transport.
- **`apps/cli`** — the **composition root**: the one crate that names concrete backends, installs telemetry, serves HTTP, owns signals.

## The pipeline (`libs/2-engine/src/lib.rs`)

```
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

The engine is the only orchestrator; everything it drives is a **trait object**. Invariants to preserve (each has a guard test — keep it passing):

- **At-least-once via flush-then-confirm.** Source acks are confirmed *only after* `Sink::flush` makes documents durable. A crash before flush redelivers the batch; re-applied idempotently by deterministic id. (`confirms_no_ack_before_its_flush`)
- **Two-step resolve → build, deduped per batch.** Buffer changes → resolve each to `DocumentId`s → dedup → `build_many` assembles each touched doc **once**. (`builds_a_repeatedly_touched_document_once_per_batch`)
- **Backfill is the sink's decision.** On start `ensure_index` every mapping, then ask each sink `is_seeded`; snapshot root tables for the unseeded, then `mark_seeded`.
- **Item-level rejection vs flush-wide error.** `Sink::flush` returns a `FlushReport`: `Err` is flush-wide (always stops); an `Ok` report lists per-document rejections. `FailurePolicies` (`Stop`/`Skip`, global + per-index by **logical** name) decide each in `commit`. (`skip_policy_*`/`stop_policy_*`/`per_index_stop_*`)
- **Observability is a trait.** The engine reports to an `Observer` (`observer.rs`) — sync, cheap, no-op by default. The daemon is the consumer, fanning to metrics + status. (`reports_lifecycle_and_progress_to_the_observer`)

`BatchPolicy` (default 256 changes / 50ms) controls flush grouping.

## The seams (where to add things)

- **Add a source/sink backend** → a new match arm in `apps/cli/src/backends.rs` (`FlussoBackends`) plus its crate. The daemon and engine are untouched. The source builder takes a `SourceSpec` (enabled indexes + schemas, layer-0 types), not the whole `Config` — translation is `backends.rs`'s `source_spec` helper.
- **Change the daemon wiring** → `libs/3-daemon/src/` (`lib.rs` `Daemon`/`RunningDaemon`, `observer.rs`, `status.rs`, `lag.rs`). It exposes *data*: `Observer` events + a `Status` handle.
- **Transport/telemetry/signals** → `apps/cli/src/` (`telemetry/`, `http/` public+private routers + auth, `commands/run.rs` restart loop).

## Config layer — two-stage parse then convert

`schema::load(path)` is the front door. Each file parser works in two stages: **parse** (serde → permissive entity types, unknown fields rejected) then **convert** (lift into the model, apply rules the format can't express). `*.schema.yml` → `IndexSchema` conversion lives in `schema-index-yaml`; `flusso.toml` → `Config` conversion lives in the `schema` crate (`deployment/conversion.rs`), next to `Config`, so the toml parser stays free of `Config`.

**Secrets are deferred** — `{ env = "VAR" }` becomes a `Secret`, resolved in the running environment, never at parse time. A compiled `flusso.lock` carries no baked secret.

Schema YAML is **type-first**: `- <type>: <name>`. Joins split by verb (`belongs_to`/`has_one`/`has_many`/`many_to_many`), aggregates by op (`count`/`sum`/`avg`/`min`/`max`/`ids`). The `ids` op is `AggregateOp::Ids { element_type }` — a flat array of the related table's PK; it reuses the aggregate `Relation`/`AggregateKey` machinery (so CDC/reverse-resolution/publication are automatic) and sets `ResolvedField.array` so the mapping is the bare element type and the query derive expects `Vec<T>`. Parsing: `libs/2-schema/1-index-yaml/src/entities/field.rs`; core model `schema_core::FieldSource`; reverse resolution per kind in `libs/1-sources/1-postgres/src/document/resolve.rs`.

## Query derive (`apps/query` + `apps/query-derive`)

A deep subsystem — read the `flusso-query-derive` memory note before changing it. In short: `#[derive(FlussoDocument)]` discovers `flusso.toml`, resolves the index mapping DB-free, validates the struct (exists/leaf-type/nullability), and generates the query surface (handles for all schema fields + `get`/`query` + schema hash). Scope tagging keeps nested queries honest (`Query<Root>` vs `Query<Order>`, lifted via `User::orders().any(...)`). Sibling derives: `FlussoMultiDocument` (blended union, purely syntactic), `FlussoValue` (custom scalar value types via a type-level marker trait). UI tests are trybuild snapshots — re-bless with `TRYBUILD=overwrite`.

## The lints are strict — they fail the build

`Cargo.toml [workspace.lints]` **denies** (not warns): `unwrap_used`/`expect_used` (return `Result`; tests opt out), `print_stdout`/`print_stderr` (use `tracing`), `indexing_slicing` (use `.get(i)`), `warnings`, `unsafe_code`, `missing_debug_implementations`, `unused_*`. Code that trips these won't compile. `clippy` runs **without** `--all-targets` because test code uses `.unwrap()`.

## Match CI before assuming green

In order: `cargo fmt --all --check` → `cargo clippy --workspace` (no `--all-targets`) → `cargo check --workspace --all-targets` → `cargo nextest run --profile ci --run-ignored all` → `cargo test --doc` → `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items`. A separate job runs a 60s `pgoutput_decode` smoke fuzz on nightly.

Test notes:
- `cargo nextest run` for fast tests; `--run-ignored all` adds the Postgres e2e (needs Docker; `testcontainers`).
- `cargo test --doc` — nextest does **not** run doctests.
- The `schema` crate's `flusso.toml` env-var tests mutate process-wide env — under plain `cargo test` use `-p schema -- --test-threads=1` (nextest is fine: process per test).
- The toolchain is pinned in `rust-toolchain.toml` because the derive's trybuild `.stderr` snapshots are compiler-version-sensitive.

## Keep `CLAUDE.md` current

It's a living index. When a change makes it wrong — a crate moved/renamed, a new engine invariant/guard test, a lint or format change — update it in the **same** change. The "Where things live" table in `CLAUDE.md` is the fastest jump-to map; start there.
