---
status: proposed
---

# Documents are built once on the ingest side; one sink engine per sink

A deployment is one source, one stream, N sinks. The ingest engine captures changes, resolves and builds each affected document once, and publishes the envelope onto every sink's lane. Each sink has its own sink engine that receives, writes, flushes, and acknowledges, and that decides its own seeding and reindex. The fan-out sink is retired. Decided in #130 because a single shared engine made sinks dependent on each other: a stateless sink beside OpenSearch re-seeded OpenSearch on every restart, one reindex rebuilt every sink, and no flush ordering between sinks could be expressed (#128, #129).

## Considered options

- **Shared engine over a fan-out sink (status quo).** Builds once, but `is_seeded` is an AND across sinks, backfill writes to all, and stacking would need the fan-out to learn about stages.
- **One engine per sink, each building its own documents.** Independent sinks, but every document's joins are resolved N times, and a per-sink backfill has to rebuild through a per-sink builder.
- **Build once before the stream, one sink engine per sink (chosen).** Independent sinks receive identical documents; the join work is paid once; per-sink backfill is a snapshot into one lane.

## Consequences

- The source contract becomes `(Position, ChangeEvent)` plus `confirm(Position)`. A change no longer carries an in-process acknowledgement handle, which is what lets a lane cross a process boundary later.
- The ingest engine serializes snapshot and live builds and publishes in build order, so on any lane a document's later message is always the newer state. Sinks need no versioning.
- Reindex is per sink. The daemon hands the targeted sink engine a control message; it finishes the batch in hand, stages the fresh generation, and queues its own backfill request, so no engine restarts and no sibling is touched. Readers keep zero downtime from the alias swap; #6 shrinks to the writer stall on one sink.
- The sink port receives envelopes: `apply(&Envelope)` replaces `upsert`/`delete`, so an emitting sink forwards the envelope as-is and a destination sink reads index, id, and document from it.
- Stacking (#129) is a topology of lanes: `after = "primary"` feeds B's lane from A's engine with exactly the documents A accepted. Ordering and withholding fall out of the wiring; no cross-engine barrier exists. A downstream sink's backfill bypasses the upstream, so a document the upstream quarantined may reach the downstream during that backfill.
- The daemon becomes a supervisor: it wires lanes, restarts a failed engine with exponential backoff while the others keep running, and exposes operations (reindex) while engines expose primitives and the binary owns transport. A `stop` failure policy is not special: the sink engine stops, is restarted with backoff, and the lane redelivers the unacknowledged batch, so a poison document loops on that one sink while the others continue. A stalled sink engine pins WAL for the shared slot until it recovers; the per-sink lag metric is the alarm.
- The daemon runs `continuity()` first and hands the answer to every sink engine, which stages stale rebuilds and queues its backfill request before the ingest engine calls `prepare()` and starts draining requests. That preserves the #120 ordering with no shared engine.
- Status, the indexes endpoint, and metrics gain a sink dimension. Readiness means every engine is ready.
- With the in-process stream adapter, bounded lanes mean the slowest sink paces ingest. That is the accepted trade; a broker-backed stream adapter is what removes it.
