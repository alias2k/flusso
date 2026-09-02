# flusso-engine

The `flusso` sync engine — drives changes from a source through to a sink, exactly the once they're durable.

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

## At a glance

| Edge (trait object) | Job | Swap to… |
| --- | --- | --- |
| [`ChangeCapture`] | establish the resume point, stream + snapshot the source | WAL, polling, … |
| [`queue`](queue_channel) | bounded buffer, back-pressures capture | channel, durable broker |
| [`DocumentBuilder`] | resolve a row → document ids, assemble each | per source |
| [`Sink`] | buffer, flush, ack, report seeding | stdout, OpenSearch |

| Invariant | What it buys you | Guarded by |
| --- | --- | --- |
| **At-least-once** — acks confirmed only *after* the flush that persisted their docs | crash before flush → whole batch redelivered, re-applied idempotently | flush-then-confirm |
| **Two-step resolve → build, deduped** | a doc touched N times in a batch is built once | dedup per batch |
| **Backfill is the sink's call** | the destination decides what needs seeding, not the source | `is_seeded` per index |
| **A seed is only as good as the stream behind it** | a missing resume point invalidates every earlier seed — those indexes are rebuilt, not trusted; rebuilds are staged *before* the point is created, so a crash in between re-stages | `continuity` → [`Continuity`](sources_core::cdc::Continuity), then `prepare` |
| **Item rejections vs flush errors** | one poison doc doesn't have to stop the run | [`FailurePolicies`] |

## The loop

A **capture** task drains the source's change stream into a bounded
in-process [`queue`](queue_channel) (back-pressure: capture blocks when the
queue is full). A **worker** pulls changes and, for the row each names,
resolves the affected document ids, assembles each one, and writes it to the
[`Sink`]'s buffer.

## Batching

Writes are **batched**: the worker groups up to [`BatchPolicy::max_changes`]
changes (or whatever arrives within [`BatchPolicy::max_delay`], whichever
comes first) into a single [`flush`](Sink::flush), turning N changes into
⌈N / max_changes⌉ bulk round-trips instead of N.

## At-least-once

The source acks for a batch are confirmed **only after** the flush that
persisted their documents, so the replication slot advances past a change
exactly when its documents are durable downstream. A crash before the flush
leaves the whole batch unconfirmed, so it's redelivered on restart and
re-applied idempotently — documents are rebuilt from the current row and
written by deterministic id.

Stopping on any error is therefore safe: unconfirmed changes are redelivered
when the run restarts.

## Continuity first

The very first call of a run is [`continuity`](ChangeCapture::continuity): a
read-only question to the source — did its durable resume point (the
replication slot) survive from the last run? The answer decides, below,
whether the sink's seed markers can be trusted.

## Mapping next

Then the engine asks the [`DocumentBuilder`] for each index's resolved mapping
and tells the sink to create it ([`ensure_index`](Sink::ensure_index)) — so
the destination uses the configured field types instead of guessing. This is
idempotent, so it runs on every start, including resumes.

## Backfill

Before live capture, the engine runs an optional **backfill** phase. It asks
the [`DocumentBuilder`] which indexes exist and the sink whether each is
already seeded; for those that aren't, it asks the source to
[`snapshot`](ChangeCapture::snapshot) their root tables and drives that
finite stream through the same queue → resolve → build → sink path (scoped to
just the unseeded indexes), then records each as seeded. So "is a backfill
needed?" is the destination's call, not the source's.

The one thing that overrides a seed marker is the stream behind it. When
`continuity` reports [`Continuity::Fresh`](sources_core::cdc::Continuity) —
the resume point is gone — every change since the seed is unobservable, so no
seeded index can be trusted. The engine warns, stages a from-scratch rebuild of
each one ([`reindex`](Sink::reindex), a fresh target so rows that no longer
exist at the source are dropped on the swap), and lets the backfill refill them
while the old copy keeps serving. Under `skip_backfill` it only warns.

Only *then* does the engine call [`prepare`](ChangeCapture::prepare) to create
the resume point. The order is load-bearing twice: after the rebuilds are
staged, so a crash in between comes back as `Fresh` and stages them again
rather than as `Resumed` with the stale seeds trusted; and before the
snapshot, so a write that lands between the snapshot and the first live read
is already covered.

A sink whose marker contradicts its *own* destination (the OpenSearch sink
finds the seed marker but not the index it names) reconciles that inside
`ensure_index`, and reports unseeded.

> 💡 **Did you know** — the queue, source, sink, and document builder are all
> trait objects, so the backend choices (WAL vs polling, stdout vs OpenSearch,
> channel vs a durable broker) swap without touching this loop.

## Module map

| Module | Holds |
| --- | --- |
| `policy` | run configuration — [`BatchPolicy`], [`FailurePolicies`] |
| `pipeline` | the `Pipeline` execution machinery this `Engine` drives |
| `observer` | the progress trait |
| `error` | the error type |
