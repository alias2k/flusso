# Roadmap

Work flusso intends to take on, not yet started. Each item states the problem
it solves and a sketch of the approach — neither is a committed API or a fixed
order. For how the system works today, see `CLAUDE.md`; for config and schema
keys, `SCHEMA.md`.

## Resumable backfill

Backfill streams a table through a server-side cursor
(`libs/1-sources/1-postgres/src/cdc/backfill.rs`), but progress is tracked only
as a per-index boolean, flipped to seeded *after* the whole snapshot lands. A
crash midway through a large table therefore restarts it from the first row — and
paired with stop-on-error, a single bad row late in a billion-row table can keep
a run from ever completing.

The fix is to checkpoint a keyset cursor (the last applied primary key per
table) as the snapshot drains, and resume from it instead of restarting the
scope. That same keyset structure also opens the door to **parallel backfill**
across tables and key ranges — today seeding is one cursor feeding one worker, so
large initial loads are strictly serial.

## High availability / failover

A single replication slot ties flusso to a single instance: the Helm chart
pins `replicas: 1` with a `Recreate` strategy, making the process a single point
of failure whose only recovery is a manual restart.

A warm standby with leader election — a Postgres advisory lock or a lease, so
exactly one instance owns the slot at a time — would cut failover from minutes of
manual intervention to seconds, while preserving the one-slot-per-deployment
invariant. This is failover, not sharding; horizontal scale-out is a separate,
larger question.

## Custom document transformations

Documents are shaped by a fixed transform vocabulary declared in `*.schema.yml`
(`schema_index_yaml::Transform`). It covers the common cases, but anything
outside the built-in set — bespoke normalization, derived fields computed from
several columns, enrichment, redaction — isn't expressible.

The plan is a transform stage users can extend through the same plugin model
flusso uses for backends: a registered transform that runs between document
assembly and the sink, addressable from a schema by name. Two delivery options,
likely both:

- **Compiled Rust transforms** registered in the composition root — zero
  overhead, fully trusted, the natural fit for in-tree and first-party logic.
- **WASM transforms** for sandboxed, language-agnostic, hot-loadable logic where
  recompiling the binary isn't acceptable — at the cost of a marshaling boundary
  on the hot path.

Either way the engine gains one well-defined hook (post-build, pre-sink) and the
transform vocabulary stops being closed.

## On-demand reindex over HTTP

Reindexing happens automatically when a schema changes (a new mapping hash
yields a fresh physical index), but there is no way to *trigger* a rebuild of an
unchanged index without restarting the process. After fixing data behind
quarantined documents (`on_error = "skip"`), correcting a source row en masse, or
recovering from an operator mistake, the only recourse today is a restart.

The operational HTTP surface (`apps/cli/src/http.rs`) is currently read-only —
`/healthz`, `/readyz`, `/status`, `/metrics`. A guarded, mutating control
endpoint (e.g. `POST /admin/reindex?index=…`) would re-run an index's backfill
on demand against the existing seed machinery (`Sink::is_seeded` /
`mark_seeded`, `ChangeCapture::snapshot`), reseeding into a fresh physical index
and swapping the alias on completion. Because it mutates, it needs an
authentication story the read-only endpoints don't — that gate is part of the
work, not an afterthought.

## Fuzz the WAL decoder

`libs/1-sources/1-postgres/src/cdc/pgoutput.rs` is a hand-written parser over
binary bytes arriving from the replication stream — the textbook target for
fuzzing, and a place where a panic is a denial of service on the whole pipeline.
There's already a `truncated_message_errors_without_panicking` unit test; the
instinct is right, it just needs to be made exhaustive.

A `cargo-fuzz` target that feeds arbitrary bytes to the message decoder and
asserts it never panics (returning an error is fine) would harden it cheaply. A
property test for the document-query SQL builder
(`libs/1-sources/1-postgres/src/document/query.rs`) is a natural companion — its
join, aggregate, and filter generation is intricate and currently covered only
by example-based tests.
