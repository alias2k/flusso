# flusso-engine

The `flusso` sync engine — drives changes from a source through to a sink, exactly the once they're durable.

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

## At a glance

| Edge (trait object) | Job | Swap to… |
| --- | --- | --- |
| [`ChangeCapture`] | stream + snapshot the source | WAL, polling, … |
| [`queue`](queue_channel) | bounded buffer, back-pressures capture | channel, durable broker |
| [`DocumentBuilder`] | resolve a row → document ids, assemble each | per source |
| [`Sink`] | buffer, flush, ack, report seeding | stdout, OpenSearch |

| Invariant | What it buys you | Guarded by |
| --- | --- | --- |
| **At-least-once** — acks confirmed only *after* the flush that persisted their docs | crash before flush → whole batch redelivered, re-applied idempotently | flush-then-confirm |
| **Two-step resolve → build, deduped** | a doc touched N times in a batch is built once | dedup per batch |
| **Backfill is the sink's call** | the destination decides what needs seeding, not the source | `is_seeded` per index |
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

## Mapping first

Before anything else, the engine asks the [`DocumentBuilder`] for each
index's resolved mapping and tells the sink to create it
([`ensure_index`](Sink::ensure_index)) — so the destination uses the
configured field types instead of guessing. This is idempotent, so it runs
on every start, including resumes.

## Backfill

Before live capture, the engine runs an optional **backfill** phase. It asks
the [`DocumentBuilder`] which indexes exist and the sink whether each is
already seeded; for those that aren't, it asks the source to
[`snapshot`](ChangeCapture::snapshot) their root tables and drives that
finite stream through the same queue → resolve → build → sink path (scoped to
just the unseeded indexes), then records each as seeded. So "is a backfill
needed?" is the destination's call, not the source's.

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
