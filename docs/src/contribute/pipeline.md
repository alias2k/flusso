# The pipeline

Two generic engines over three ports, and the invariants that make them correct: build once, flush-then-ack with a watermark, sink-decided backfill, continuity before trust, at-least-once requests, and item rejections separated from flush errors. Each is guarded by a named test.

## The shape

```text
                 ┌─ lane ─▶ SinkEngine(primary) ─▶ apply ─▶ flush ─▶ ack ─┐
ChangeCapture ─▶ IngestEngine ─┤                                             ├─▶ watermark ─▶ confirm
  resolve · build once · publish└─ lane ─▶ SinkEngine(audit)   ─▶ apply ─▶ flush ─▶ ack ─┘
                 ◀─ request lane ─ Backfill { sink, indexes } ─────────────────────
```

One source, one stream, N sinks. The **ingest engine** (`libs/2-engine/src/ingest.rs`) owns the source side: capture, resolve, build, publish. One **sink engine** per sink (`sink_engine.rs`) owns delivery to that sink and that sink's seeding. The **stream** (`libs/1-ports/stream`) carries one lane per sink downward and a request lane upward, and owns the watermark. The **daemon** (`libs/3-daemon/src/supervise.rs`) stages every sink engine, then runs all engines as independent tasks. Everything an engine drives is a trait object, so backends swap without touching either loop.

## Documents are built once

The ingest engine buffers live changes per `BatchPolicy` (default 256 changes or 50 ms; `max_changes: 1` reproduces flush-per-change), resolves each to the `DocumentId`s it touches, dedups them, and `build_many` assembles each touched document **once** even when several changes hit it and however many sinks there are. The result is published as one `Batch` of kernel `Envelope`s, tagged with the position of the last change, to every lane. Snapshot rows for a backfill flow through the same resolve → build path but are published without a position and only to the lanes that requested them, ending with a `SnapshotComplete` marker. Both kinds interleave on one task, so lane order is build order: a document's later message is always its newer state and sinks need no versioning. Guards: `builds_a_repeatedly_touched_document_once_per_batch`, `batches_changes_into_a_single_build_and_flush`, `snapshots_go_only_to_the_requesting_lane`.

## At-least-once via flush-then-ack and the watermark

A sink engine applies a batch's envelopes, flushes once, and only then acks the lane item. Acking a positioned batch records that position for the lane; the stream's watermark is the **minimum** over lanes of each lane's last acked position, and the ingest engine hands that watermark to `ChangeCapture::confirm` after each commit (and on a one-second tick), so the replication slot advances past a change exactly when **every** sink has made its documents durable. A sink engine that dies before its flush leaves the item unacked; its lane redelivers it to the restarted engine, and the rebuild is idempotent (deterministic ids, current state). Positions are monotonic for the life of the source, across reopened streams, so a watermark the lanes still hold when the ingest engine restarts can never name a change of the new stream. The status phase drops to `starting` while the ingest engine restarts and recovers when it follows the source again; only a clean end is `stopped`. Guards: `confirms_no_position_before_its_flush`, `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, the channel adapter's `watermark_is_the_minimum_over_lanes` and `several_deliveries_can_be_outstanding_and_ack_independently`, the daemon's `failed_sink_engine_restarts_and_redelivers` and `failed_ingest_engine_restarts_and_resumes_the_stream`, the Postgres adapter's `a_reopened_stream_continues_the_numbering`.

## Backfill is each sink's decision

Staging (`SinkEngine::stage`) `ensure_index`es every mapping, asks the sink `is_seeded` per index, and sends one `Backfill { sink, indexes }` request up the request lane for the unseeded set. The ingest engine snapshots the union of the requested root tables and publishes the rows to the requesting lanes only; a seeded sibling sees nothing, so a stateless sink beside OpenSearch no longer re-seeds it on every restart. `backfill = false` on a sink skips staging's request entirely and never marks anything seeded. Requests coalesce into one snapshot fanned to every requester: whatever is already queued is taken with no wait (every startup request is, since the daemon stages every sink before the ingest engine runs), and the engine holds for stragglers only while some lane has not asked, for at most the batch policy's `max_delay`, so a reindex fanned to every sink is one pass over the table and a single-sink backfill starts at once. Requests are at-least-once: the ingest engine acks a request only after its `SnapshotComplete` is published, so a crash mid-snapshot redelivers it; and a sink engine remembers which indexes it has requested and not yet seen complete, so re-staging after its own restart does not ask twice. Guards: `an_unseeded_sink_requests_a_snapshot_then_marks_seeded`, `a_seeded_sink_requests_nothing`, `backfill_false_makes_a_stateless_sink_live_only`, `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`, the daemon's `unseeded_sink_is_backfilled_without_touching_its_sibling`.

## A seed is only as good as the stream behind it

Run order is `source.continuity()` → stage every sink → `source.prepare()` → serve requests and live. `continuity` is read-only (Postgres: does the slot exist?) and answers `Resumed` or `Fresh`; `prepare` creates the resume point and is the ingest engine's first act. On `Fresh`, every change since the last seed is unobservable, so each sink engine warns and stages `sink.reindex` plus `ensure_index` for every index still `is_seeded` (a fresh generation, so rows gone from the source drop on the swap, never an in-place reseed), and the normal backfill refills them.

The order is load-bearing twice. Rebuilds are staged **before** `prepare` creates the slot, so a crash in between comes back `Fresh` and re-stages instead of `Resumed` trusting stale seeds. `prepare` runs **before** any snapshot, so a write between snapshot and first live read is covered. The daemon enforces it by finishing every sink's first staging before it starts the ingest engine. Only seeded indexes are staged; an unseeded one is already being rebuilt and staging again would orphan a generation. Under `--skip-backfill` staging only warns. Guards: `fresh_source_rebuilds_seeded_indexes_before_prepare_then_snapshots`, `fresh_source_leaves_an_unseeded_index_to_the_normal_backfill`, `skip_backfill_with_a_fresh_source_stages_nothing`, the daemon's `fresh_source_rebuilds_seeded_sinks_before_preparing`, the sink's `deleted_generation_is_recreated_and_reported_unseeded` e2e, the Postgres `continuity` e2e, and the two `restart_*` cases in the engine's `pipeline` e2e.

## Reindex is an operation on one sink

A reindex reaches a running sink engine as a control message from the daemon (`DaemonControl::reindex(index, Option<sink>)`, behind `POST /reindex`). Between two batches the engine stages a fresh generation (`reindex` + `ensure_index`) and sends a new `Backfill` request; the current generation keeps serving until the snapshot completes and `SnapshotComplete` lets the sink swap. Nothing restarts, and untargeted sinks are untouched; forcing every sink coalesces into one snapshot. A sink with `backfill = false` ignores it (nothing to rebuild), as does an index whose snapshot is still outstanding. Guards: `reindex_control_stages_and_requests_a_snapshot_without_restarting`, the daemon's `reindex_operation_targets_one_sink`.

## Item-level rejections versus flush-wide errors

`Sink::flush` returns a `FlushReport`. `Err` is flush-wide and stops **that sink engine**, which the daemon restarts with exponential backoff (one second doubling to `max_restart_backoff`, default 60 s) while the ingest engine and the other sinks keep running; its lane fills meanwhile, so with the in-process stream a stalled sink eventually paces ingest and pins WAL, and its per-sink lag and in-flight numbers are the alarm. An `Ok` report lists documents the destination applied the batch but rejected individually. `FailurePolicies` (a global default plus per-index overrides, resolved by logical name) decides each in `commit`: `Stop` errors the engine out with the batch unacked (redelivered after the restart); `Skip` quarantines it (`Observer::on_document_quarantined`, with the sink) and acks the batch so the poison isn't redelivered. A single `Stop` halts the whole batch before any quarantine event is emitted. The OpenSearch sink never retries a rejection. Guards: `skip_policy_*`, `stop_policy_*`, `per_index_stop_*`.

## Observability is a trait

Both engines report lifecycle and progress to an `Observer` (`observer.rs`): sync, cheap, no-op by default, set with `with_observer`. Every sink-side event carries the `SinkName`; engine errors carry an `EngineId` (`Ingest` or `Sink(name)`). The daemon fans events to its per-sink `Status`; the binary attaches an OpenTelemetry observer. Guard: `reports_lifecycle_and_progress_to_the_observer`.

## Where this shows up

- [How flusso works](../start/how-it-works.md) for the user-facing telling.
- [Reindex without downtime](../operate/reindex.md), [Recover from a dropped slot](../operate/dropped-slot.md), and [Handle rejected documents](../operate/rejected-documents.md) for the operational consequences.
- [Stream: channel](../reference/stream-channel.md) for the in-process lanes and their backpressure.
