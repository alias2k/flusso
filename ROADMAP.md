# Roadmap

Work flusso intends to take on, not yet started. Each item states the problem and
a sketch of the approach — neither is a committed API. For how the system works
today see `CLAUDE.md`; for config and schema keys, `SCHEMA.md`.

## Resumable backfill

Backfill streams a table through a server-side cursor
(`libs/1-sources/1-postgres/src/cdc/backfill.rs`), but progress is tracked only as
a per-index boolean, flipped to seeded *after* the whole snapshot lands. A crash
midway through a large table restarts it from the first row — and paired with
stop-on-error, a single bad row late in a billion-row table can keep a run from
ever completing.

The fix is to checkpoint a keyset cursor (the last applied primary key per table)
as the snapshot drains, and resume from it instead of restarting the scope. That
same keyset structure also opens the door to **parallel backfill** across tables
and key ranges — today seeding is one cursor feeding one worker, so large initial
loads are strictly serial.

## High availability / failover

A single replication slot ties flusso to a single instance: the Helm chart pins
`replicas: 1` with a `Recreate` strategy, making the process a single point of
failure whose only recovery is a manual restart.

A warm standby with leader election — a Postgres advisory lock or a lease, so
exactly one instance owns the slot at a time — would cut failover from minutes to
seconds while preserving the one-slot-per-deployment invariant. This is failover,
not sharding; horizontal scale-out is a separate, larger question.

## Custom document transformations

Documents are shaped by a fixed transform vocabulary declared in `*.schema.yml`
(`schema_index_yaml::Transform`). Anything outside the built-in set — bespoke
normalization, derived fields computed from several columns, enrichment,
redaction — isn't expressible.

The plan is a transform stage users can extend through the same plugin model
flusso uses for backends: a registered transform that runs between document
assembly and the sink, addressable from a schema by name. Two delivery options,
likely both:

- **Compiled Rust transforms** registered in the composition root — zero
  overhead, fully trusted, the fit for in-tree and first-party logic.
- **WASM transforms** for sandboxed, language-agnostic, hot-loadable logic where
  recompiling the binary isn't acceptable — at the cost of a marshaling boundary
  on the hot path.

Either way the engine gains one well-defined hook (post-build, pre-sink) and the
transform vocabulary stops being closed.

## Write-side zero-downtime reindex

On-demand reindex ships today via alias-over-generations: a reindex seeds a fresh
generation while reads keep flowing to the current one, then swaps atomically (see
`CLAUDE.md`). The remaining gap is the brief **write** lag while the new
generation seeds.

Eliminating it would mean dual-writing live changes to both generations during the
seed, which reintroduces a snapshot-vs-live ordering race; the clean fix is
OpenSearch **external versioning** (a per-write version = WAL LSN, so a stale
snapshot write loses to a newer live write). That is deliberately deferred: it
taxes *every* write, forever, on the hot path to make a *rare* operation lag-free.
It stays a documented follow-on, purely additive on top of the alias scheme.
