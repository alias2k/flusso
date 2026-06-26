# flusso-engine

The `flusso` sync engine.

Wires the pluggable edges together and runs the pipeline:

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

A **capture** task drains the source's change stream into a bounded
in-process [`queue`](queue_channel) (back-pressure: capture blocks when the
queue is full). A **worker** pulls changes and, for the row each names,
resolves the affected document ids, assembles each one, and writes it to the
[`Sink`]'s buffer.

Writes are **batched**: the worker groups up to [`BatchPolicy::max_changes`]
changes (or whatever arrives within [`BatchPolicy::max_delay`], whichever
comes first) into a single [`flush`](Sink::flush), turning N changes into
⌈N / max_changes⌉ bulk round-trips instead of N. The source acks for a batch
are confirmed **only after** the flush that persisted their documents, so the
replication slot advances past a change exactly when its documents are
durable downstream — preserving at-least-once delivery: a crash before the
flush leaves the whole batch unconfirmed, so it is redelivered on restart and
re-applied idempotently (documents are rebuilt from the current row and
written by deterministic id).

Stopping on any error is therefore safe: unconfirmed changes are redelivered
when the run restarts.

Before anything else, the engine asks the [`DocumentBuilder`] for each
index's resolved mapping and tells the sink to create it
([`ensure_index`](Sink::ensure_index)) — so the destination uses the
configured field types instead of guessing. This is idempotent, so it runs
on every start, including resumes.

Before live capture, the engine runs an optional **backfill** phase. It asks
the [`DocumentBuilder`] which indexes exist and the sink whether each is
already seeded; for those that aren't, it asks the source to
[`snapshot`](ChangeCapture::snapshot) their root tables and drives that
finite stream through the same queue → resolve → build → sink path (scoped to
just the unseeded indexes), then records each as seeded. So "is a backfill
needed?" is the destination's call, not the source's.

The queue, source, sink, and document builder are all trait objects, so the
backend choices (WAL vs polling, stdout vs OpenSearch, channel vs a durable
broker) are swappable without touching this loop.

The implementation is split across modules: `policy` holds the run
configuration ([`BatchPolicy`], [`FailurePolicies`]); `pipeline` holds the
`Pipeline` execution machinery this `Engine` drives; `observer` the progress
trait; `error` the error type.
