# flusso-engine

The `flusso` sync engines — build every document once on the source side, deliver it to each sink independently, and advance the source only past what every sink has made durable.

```text
                 ┌─ lane ─▶ SinkEngine(primary) ─▶ apply ─▶ flush ─▶ ack ─┐
ChangeCapture ─▶ IngestEngine ─┤                                             ├─▶ watermark ─▶ confirm
  resolve · build once · publish└─ lane ─▶ SinkEngine(audit)   ─▶ apply ─▶ flush ─▶ ack ─┘
                 ◀─ request lane ─ Backfill { sink, indexes } ─────────────────────
```

## At a glance

| Edge (trait object) | Job | Swap to… |
| --- | --- | --- |
| [`ChangeCapture`](source::cdc::ChangeCapture) | establish the resume point, stream + snapshot the source, confirm positions | WAL, polling, … |
| [`DocumentBuilder`](source::document::DocumentBuilder) | resolve a row → document ids, assemble each | per source |
| [`Stream`](stream::Stream) | one lane per sink, a request lane, the watermark | in-process channel, durable broker |
| [`Sink`](sink::Sink) | apply envelopes, flush, report seeding, stage rebuilds | stdout, OpenSearch |

| Invariant | What it buys you | Guarded by |
| --- | --- | --- |
| **At-least-once** — a lane acks a batch only *after* the flush that persisted it; the source is confirmed only the watermark | crash before flush → the batch is redelivered to that sink, re-applied idempotently | flush-then-ack, watermark = min over lanes |
| **Built once** — resolve → dedup → `build_many` per batch, published to every lane | a doc touched N times in a batch is built once; N sinks cost one build | dedup per batch |
| **Build order is delivery order** | on any lane a later message is the newer state, so sinks need no versioning | one ingest task serializes snapshot and live builds |
| **Backfill is each sink's call** | a stateless sink beside OpenSearch never re-seeds it; `backfill = false` opts a sink out | `is_seeded` per (sink, index), `Backfill` requests |
| **A seed is only as good as the stream behind it** | a missing resume point invalidates every earlier seed — those indexes are rebuilt, staged *before* the point is created | `continuity` → stage → `prepare` → snapshot |
| **Requests are at-least-once** | a crash mid-snapshot redelivers the request; ten concurrent requests for one index are one pass over the table | ack after `SnapshotComplete`, coalescing |
| **Item rejections vs flush errors** | one poison doc doesn't have to stop the run — or the other sinks | [`FailurePolicies`] per sink engine |

## The ingest engine

[`IngestEngine`] owns the source side. It buffers live changes per
[`BatchPolicy`] ([`max_changes`](BatchPolicy::max_changes) or
[`max_delay`](BatchPolicy::max_delay), whichever first), resolves each to the
documents it touches, deduplicates, builds them once, and publishes one
`Batch` — the envelopes plus the position of the last change — to every lane.
Snapshot rows for a backfill flow through the same resolve → build path but
are published only to the lanes that requested them, without a position, and
end with a `SnapshotComplete` marker. Both kinds interleave on one task, so
lane order is build order.

After each commit it reads the stream's watermark — the lowest position every
lane has acknowledged — and hands it to the source as
[`confirm`](source::cdc::ChangeCapture::confirm). Nothing else moves the
resume point.

## The sink engine

[`SinkEngine`], one per sink, owns that sink's seeding and delivery.
**Staging** ([`stage`](SinkEngine::stage)) ensures every index, retires
stale seeds under a `Fresh` source, and sends one `Backfill` request for
whatever is still unseeded. The **loop** ([`run`](SinkEngine::run)) receives
a batch, applies each envelope, flushes once, decides item-level rejections by
its failure policies (`stop` → the engine errors out with the batch
unacknowledged; `skip` → quarantine and continue), and acks. A
`SnapshotComplete` marks its indexes seeded. A reindex reaches a running sink
engine as a control message and is staged between two batches: `reindex` +
`ensure_index`, then a fresh `Backfill` request. Nothing restarts.

## Continuity first, then stage, then prepare

The daemon asks [`continuity`](source::cdc::ChangeCapture::continuity) once,
stages **every** sink engine, and only then runs the ingest engine, whose
first act is [`prepare`](source::cdc::ChangeCapture::prepare). When
continuity reports [`Fresh`](source::cdc::Continuity::Fresh) every seed is
stale, so each still-seeded index is rebuilt from scratch
([`reindex`](sink::Sink::reindex), a fresh target so rows gone from the
source drop on the swap) while the old copy keeps serving. The order is
load-bearing twice: rebuilds are staged before the resume point exists (a
crash in between comes back `Fresh` and re-stages), and the resume point
exists before any snapshot runs (a write between snapshot and first live read
is covered).

> 💡 **Did you know** — the source, stream, sink, and document builder are all
> trait objects, so the backend choices (WAL vs polling, channel vs broker,
> stdout vs OpenSearch) swap without touching either loop.

## Module map

| Module | Holds |
| --- | --- |
| `ingest` | [`IngestEngine`] |
| `sink_engine` | [`SinkEngine`], [`SinkControl`] |
| `policy` | run configuration — [`BatchPolicy`], [`FailurePolicies`] |
| `observer` | the progress trait, with the sink dimension |
| `error` | the error type |
