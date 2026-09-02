# The pipeline

The engine's loop and the five invariants that make it correct: flush-then-confirm, dedup per batch, sink-decided backfill, continuity before trust, and item rejections separated from flush errors. Each is guarded by a named test.

## The loop

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

A **capture** task drains the source's change stream into a bounded queue; a full queue blocks capture, which is the back-pressure. A **worker** pulls changes and, for each, resolves the affected document ids, builds each document, and hands it to the sink's buffer. `BatchPolicy` (default 256 changes or 50 ms) decides when a batch flushes; `max_changes: 1` reproduces flush-per-change. The code is `libs/2-engine/src/pipeline.rs`; `lib.rs` is the `Engine` builder.

## At-least-once via flush-then-confirm

Source acks for a batch are confirmed **only after** the `Sink::flush` that made its documents durable, so the replication slot advances past a change exactly when its documents have landed. A crash before the flush redelivers the whole batch, and redelivery is idempotent: documents are rebuilt from the current row and written by deterministic id. Stopping on any error is therefore always safe. Guard: `confirms_no_ack_before_its_flush`.

## Two-step resolve then build, deduped

A batch buffers changes, resolves each to affected `DocumentId`s, dedups them, then `build_many` assembles each touched document **once** even when several changes hit it. Writes within a batch are keyed and idempotent, so reordering them is safe. Guard: `builds_a_repeatedly_touched_document_once_per_batch`.

## Backfill is the sink's decision

On start the engine `ensure_index`es every mapping, then asks each sink `is_seeded`. Unseeded indexes get their root tables snapshotted through the same queue, resolve, build, sink path, scoped so a seeded index sharing a table isn't rewritten, then `mark_seeded`. Whether a backfill is needed is the destination's call, not the source's.

## A seed is only as good as the stream behind it

Run order is `source.continuity()`, `ensure_index` all, on `Fresh` stage rebuilds, `source.prepare()`, backfill, live. `continuity` is read-only (Postgres: does the slot exist?) and answers `Resumed` or `Fresh`; `prepare` creates the resume point. On `Fresh`, every change since the last seed is unobservable, so the engine warns and stages `sink.reindex` plus `ensure_index` for every index still `is_seeded` (a fresh generation, so rows gone from the source drop on the swap, never an in-place reseed), and the normal backfill refills them.

The order is load-bearing twice. Rebuilds are staged **before** `prepare` creates the slot, so a crash in between comes back `Fresh` and re-stages instead of `Resumed` trusting stale seeds. `prepare` runs **before** the snapshot, so a write between snapshot and first live read is covered. Only seeded indexes are staged; an unseeded one is already being rebuilt and staging again would orphan a generation. Under `--skip-backfill` it only warns. `FanOutSink` forwards `reindex`, since with the no-op default `is_seeded` would stay true and nothing would rebuild. Guards: the `fresh_source_*`, `resumed_source_*`, and `skip_backfill_with_a_fresh_source_*` unit tests, the sink's `deleted_generation_is_recreated_and_reported_unseeded` e2e, the Postgres `continuity` e2e, and the two `restart_*` cases in the engine's `pipeline` e2e.

## Item-level rejections versus flush-wide errors

`Sink::flush` returns a `FlushReport`. `Err` is flush-wide and always stops the run. An `Ok` report lists documents the destination applied the batch but rejected individually. `FailurePolicies` (a global default plus per-index overrides, resolved by logical name) decides each in `commit`: `Stop` halts with the batch unconfirmed; `Skip` quarantines it (`Observer::on_document_quarantined`) and acks the batch so the poison isn't redelivered. A single `Stop` halts the whole batch before any quarantine event is emitted. The OpenSearch sink never retries a rejection. Guards: `skip_policy_*`, `stop_policy_*`, `per_index_stop_*`.

## Observability is a trait

The engine reports lifecycle and progress to an `Observer` (`observer.rs`): sync, cheap, no-op by default, set with `with_observer`. It depends on the trait, never on a metrics backend. The daemon fans events to its `Status`; the binary attaches an OpenTelemetry observer. Guard: `reports_lifecycle_and_progress_to_the_observer`.

## Where this shows up

- [How flusso works](../start/how-it-works.md) for the user-facing telling.
- [Recover from a dropped slot](../operate/dropped-slot.md) and [Handle rejected documents](../operate/rejected-documents.md) for the operational consequences.
