# flusso-engine

The `flusso` sync engines — build every document once on the source side, deliver it to each sink independently, and advance the source only past what every sink has made durable.

```text
                 ┌─ lane ─▶ SinkEngine(primary) ─▶ write ×4 ─▶ ack ─┐
ChangeCapture ─▶ IngestEngine ─┤                                       ├─▶ watermark ─▶ confirm
  resolve · build once · publish└─ lane ─▶ SinkEngine(audit)   ─▶ write ×4 ─▶ ack ─┘
                 ◀─ request lane ─ Backfill { sink, indexes } ─────────────────────
```

## At a glance

| Edge (trait object) | Job | Swap to… |
| --- | --- | --- |
| [`ChangeCapture`](source::cdc::ChangeCapture) | establish the resume point, stream + snapshot the source, confirm positions | WAL, polling, … |
| [`DocumentBuilder`](source::document::DocumentBuilder) | resolve a row → document ids, assemble each | per source |
| [`Stream`](stream::Stream) | one lane per sink, a request lane, the watermark | in-process channel, durable broker |
| [`Sink`](sink::Sink) | write envelopes (several writes at once), report seeding, stage rebuilds | stdout, OpenSearch |

| Invariant | What it buys you | Guarded by |
| --- | --- | --- |
| **At-least-once** — a lane acks a batch only *after* the write that persisted it; the source is confirmed only the watermark | crash before a write returns → the batch is redelivered to that sink, rewritten idempotently | write-then-ack; watermark = min over lanes of the position before each lane's oldest unacknowledged item |
| **Built once** — resolve → dedup → `build_many` per batch, published to every lane | a doc touched N times in a batch is built once; N sinks cost one build | dedup per batch |
| **Build order is delivery order, per document** | a document's later build always lands after its earlier one, with several writes in flight, so sinks need no versioning | one ingest task serializes builds; the sink engine never writes two items sharing a document at once |
| **Backfill is each sink's call** | a stateless sink beside OpenSearch never re-seeds it; `backfill = false` opts a sink out | `is_seeded` per (sink, index), `Backfill` requests |
| **A seed is only as good as the stream behind it** | a missing resume point invalidates every earlier seed — those indexes are rebuilt, staged *before* the point is created | `continuity` → stage → `prepare` → snapshot |
| **Requests are at-least-once** | a crash mid-snapshot redelivers the request; ten concurrent requests for one index are one pass over the table | ack after `SnapshotComplete`, coalescing |
| **Item rejections vs write errors** | one poison doc doesn't have to stop the run — or the other sinks | [`FailurePolicies`] per sink engine |
| **A stop drains, never aborts** | a requested [`Stop`] publishes the batch in flight, lands every lane, confirms the watermark and closes the source's stream before the run ends | [`Stop`] checked between the stream's items, [`drain_lanes`], [`ChangeCapture::close_live`](source::cdc::ChangeCapture::close_live) |
| **Applied, then searchable, per sink** | a fence can tell "a get sees it" from "a search sees it" without anyone forcing a refresh | [`Observer::on_position_applied`] before the ack, [`Observer::on_position_searchable`] from each write's reported visibility, in position order |

## The ingest engine

[`IngestEngine`] owns the source side. It buffers every live change the stream
has ready and commits the moment the stream would block — a lone change on a
quiet stream is built at once — or once [`max_changes`](BatchPolicy::max_changes)
have accumulated, with [`max_delay`](BatchPolicy::max_delay) as the cap while
changes keep arriving ([`BatchPolicy`]). Each change is resolved to the
documents it touches, deduplicated, built once, and published as one
`Batch` — the envelopes plus the position of the last change — to every lane.
Snapshot rows for a backfill go through the same build path but are not
resolved: each row is its own document, once per requested index rooted at its
table. They are published only to the lanes that requested them, without a
position, and end with a `SnapshotComplete` marker. Both kinds interleave on one task, so
lane order is build order.

After each commit it reads the stream's watermark — the lowest position every
lane has behind its oldest unacknowledged item — and hands it to the source as
[`confirm`](source::cdc::ChangeCapture::confirm). Nothing else moves the
resume point.

## The sink engine

[`SinkEngine`], one per sink, owns that sink's seeding and delivery.
**Staging** ([`stage`](SinkEngine::stage)) ensures every index, retires
stale seeds under a `Fresh` source, and sends one `Backfill` request for
whatever is still unseeded. The **loop** ([`run`](SinkEngine::run)) writes up
to [`with_writes`](SinkEngine::with_writes) lane items at once, each with one
[`write`](sink::Sink::write): an item starts only when no earlier unfinished
item shares a document with it, and live items go before backfill items
except every fourth grant both could take. When a write returns, the engine
decides item-level rejections by its failure policies (`stop` → no new write
starts, the ones in flight finish, then the engine errors out with the item
unacknowledged; `skip` → quarantine and continue) and acks. A
`SnapshotComplete` marks its indexes seeded once everything before it is
written. A reindex reaches a running sink engine as a control message and is
staged once the writes in flight returned: `reindex` + `ensure_index`, then a
fresh `Backfill` request. Nothing restarts.

The loop also reports where the sink stands, in position order whatever order
the writes finish in. The last position before the oldest one still being
written is **applied**, reported before the acknowledgement that lets the
watermark pass it. Each write says when its documents become searchable; the
engine reports a position **searchable** once it, and every position before
it, is due. Both go to the [`Observer`], which is how the daemon's fence
learns of them.

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
