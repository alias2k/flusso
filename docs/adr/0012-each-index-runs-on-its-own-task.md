---
status: proposed
---

# Each index runs on its own task; order is kept per document

Today one ingest task builds every index in turn, and each sink writes one batch at a time (issue #386). A backfill of one index holds up live changes of every other index, and on `reference` the sink's flushes take about twice the build time while the cluster could take four requests at once.

**Decision.**

- **One reader task** reads changes, resolves each one once, and marks the documents it touches as *pending* on their index. It never waits on an index.
- **One task per index** builds that index's pending documents and its backfill, and publishes each built batch to one lane per sink.
- **One task per sink** writes its lanes with several bulk requests in flight.
- **Order holds per document, not per lane.** A document is never built twice at once, and never written twice at once; a later build of it is published and written after an earlier one. Nothing else is serialized. Sinks need no versioned writes.
- **The position confirmed to Postgres is the oldest change some index has not landed on some sink, minus one**, kept by a new in-memory `Progress` in the engine. The stream port no longer computes a watermark.

```text
                         ┌─ pending(users) ──▶ index task(users) ──┬─ lane(users,primary) ─┐
ChangeCapture ─▶ reader ─┼─ pending(orders) ─▶ index task(orders) ─┼─ lane(users,audit) ───┤
 read · resolve once     └─ pending(…) ──────▶ index task(…) ──────┘  lane(orders,…) …     │
       │                      ▲ keys pages        build · publish                          │
       │                      └── own backfill ◀── request(index) ◀── sink engines ◀───────┤
       │                                                                                   ▼
       └──── confirm ◀── Progress (oldest change not landed on every sink) ◀── applied ── SinkEngine(primary): write ×4 in flight
                               └──▶ fence: applied / searchable per sink                   SinkEngine(audit):   write ×4 in flight
```

## 1. The shape

**Reader** (one task, owns the replication stream).

- Reads changes as today: `ChangeCapture::live`, the visibility hold (ADR 0010) and the read-ahead stay in the capture.
- Resolves each change once with `DocumentBuilder::resolve`, as today.
- For every document id it gets, marks the id pending on that id's index, with the change's position.
- Then records "routed through this position" in `Progress`. A change that resolves to nothing is landed the moment it is routed.

**Pending set** (one per index, shared state, not a queue).

- A map from document id to the *oldest* position not yet built for it.
- A second change to an id already pending adds nothing: a build reads the database as it is now, so one build after the newest change covers every earlier one. Deduplication now spans the whole backlog, not one batch.
- Its memory is bounded by the index's document count, not by the length of the WAL. That is what lets a stalled or failed index fall behind without stopping the reader (bounded further in section 5).

**Index task** (one per enabled index).

- Takes pending ids, oldest first, up to `max_changes` per build, skipping any id whose earlier build is still in flight.
- Calls `build_many` on the shared builder and publishes the batch to every sink lane of that index.
- Runs that index's backfill: reads root keys in pages and builds them like pending ids, publishing only to the lanes that asked.
- A lone change on a quiet stream is built at once. A burst batches itself: whatever piles up while one build runs is the next batch. `max_delay` is no longer needed to group live changes.

**Sink engine** (one per sink, as today).

- Reads every lane of its sink: one lane per `(index, sink)`.
- Keeps up to `writes` bulk requests in flight across them.

**Where a change touching several indexes goes.** It is read and resolved once, on the reader. Its ids are split by index into the pending sets. Each index then builds only its own ids.

**Backfill.** A sink asks per index (`Request::Backfill { sink }` on that index's request lane). The index task coalesces requests from several sinks as today, reads its root table's keys in pages, builds them, publishes to the requesting lanes, then publishes `SnapshotComplete` and acknowledges the requests. Two indexes rooted at one table each read its keys; keys cost 0.3 s on `reference`, so sharing the read isn't worth coupling the tasks.

## 2. Ordering

Today's rule is "build order is delivery order", because one task builds everything. Its replacement:

> **For each document, builds never overlap, and a later build reaches every sink after an earlier one.**

Why that is enough:

- `build_many` reads the current, visible state of the database. A build that starts after another one ended sees at least what the first saw (Read Committed takes a new snapshot per statement, and later snapshots on the primary see a superset). So, per document, build order is state order.
- Builds of *different* documents can run and land in any order. Different documents never overwrite each other.

How each stage keeps it, with one rule, **in-order admission with conflicts**: an item may start only when no earlier unfinished item shares a document id with it.

| Stage | Items | "Finished" means |
| --- | --- | --- |
| Index task | pending batches and backfill pages | built and published to every lane |
| Sink engine, per lane | lane items | the bulk request returned |

The rule holds across every case the issue lists:

- **A backfill against live changes of the same index.** A backfill page and a live batch may run in either order or at once, as long as they share no id. If they do, the later one waits. Whichever lands last read the newer state. A live change never waits for a backfill page it doesn't share an id with, so live latency on an index under backfill is one build plus one write.
- **Restart of an index task.** Ids taken but not yet published go back to pending. Published ones are in the lanes.
- **Restart of a sink engine, or a redelivery.** The lane redelivers its unacknowledged items in lane order, and admission applies again. Two items of one id are never in flight together, so a redelivered item is never older than an acknowledged one of the same id.
- **Restart of the reader.** The stream reopens at the confirmed position and marks ids pending again. Re-marking costs a rebuild, never a wrong document.
- **A reindex of one sink while another sink of that index is live.** The index task publishes backfill pages to the reindexing sink's lane only and live batches to both. The rule is per id at the index task, so each lane still gets one id's builds in order.
- **Two sinks at different speeds.** Each lane is admitted on its own. The index task waits only when a lane is full (section 5).
- **`SnapshotComplete`** is a barrier on its lane: it starts when everything before it has finished, and nothing after it starts until `mark_seeded` returns.

**Versioned writes: not needed, and rejected.** Options weighed:

- *A build sequence number as an external version.* The sequence in which builds start is not the order of their database snapshots once two connections race, so it would order wrongly exactly where it is needed. With the per-id rule it adds nothing.
- *The Postgres snapshot (`xmin`/`xmax`).* Not a total order: two snapshots with the same `xmax` can see different transactions.
- *The change position.* A backfill build has none, and two builds after one position can see different states.
- *Cost on OpenSearch.* Every operation must carry `version_type=external`. A delete keeps its version only for `index.gc_deletes` (60 s), after which an older upsert brings a deleted document back.

One gap stays as it is today: a bulk request that timed out on the client but still runs on the cluster can land after its own retry. The per-id rule makes it no worse (the next build of that id waits for the retry).

## 3. Confirmation and fences

**`Progress`** (engine crate, one per deployment, outlives every task, like `Status`):

- *Routed:* the newest position the reader has marked.
- *Per `(index, sink)`:* the oldest position still pending, building, or written but unacknowledged on that lane.
- *Landed for a sink:* the oldest such position across that sink's lanes, minus one; if none, *routed*.
- *Confirmable:* the minimum of *landed* over every sink.

The ingest side hands *confirmable* to `ChangeCapture::confirm` after each change it marks and on the one-second tick, as today. The watermark can never pass a change some index has not landed on some sink, because that change's oldest position is still counted.

What changes for batches and lanes:

- **A lane item no longer carries a position.** It carries the token `Progress` gave its build, which `Progress` maps to the oldest position among its ids. Acknowledging the item releases it.
- **An empty batch disappears.** Today every lane gets an empty batch so it acknowledges a position. Now a change that touches no document of an index costs that index nothing: an idle index contributes *routed* and never pins the slot. A change that touches nothing at all is landed once routed (the `wal_release` cases).
- **Backfill items** carry no token and never move `Progress`, as today.

**Fences** keep their meaning (ADR 0009). The two levels per sink:

- *Applied* is that sink's *landed*.
- *Searchable* is the same computation over items whose flush has become searchable. The sink engine keeps, per lane, the items applied but not yet searchable, with their due time or the next caught-up flush, as it does now per sink.

`ChangeCapture::reached` is unchanged: the fence still asks the source to translate a position, and the source still answers strictly below the next outstanding change. A fence covers neither a backfill nor an index mid-reindex, as today.

## 4. Where concurrency pays

From #389 on `reference` (under load ≈32): key fetch 0.3 s, build 5.8–7.4 s, flushes 12.6–16.8 s. The sink sets the pace.

**Prototype.** A throwaway example (not committed) loaded the `reference` config and seeded data (`default` scale: 62,000 documents), then:

- called `PgDocumentBuilder::build_many` over 256-key batches with *N* concurrent callers on one pool;
- built every document once, then wrote the batches through *N* `OpensearchSink`s (one buffer each, `flush(false)`), each run into fresh indexes.

Setup: Postgres 16 and OpenSearch 2 (single node, 1 GiB heap, one shard per index) in local containers, data in cache, 24 cores. Three runs each, load average 2.4–5.2.

`build_many`, all three indexes, documents/s:

| Concurrent builds | run 1 | run 2 | run 3 | vs 1 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 42,320 | 38,743 | 32,900 | 1× |
| 2 | 67,939 | 53,276 | 58,892 | 1.5× |
| 4 | 112,959 | 121,032 | 112,787 | 3.0× |
| 8 | 191,783 | 190,468 | 188,317 | 5.0× |
| 16 | 243,791 | 247,853 | 250,283 | 6.4× |

Within one index the same holds: `users` 13–15k → 57–65k documents/s at 8, `orders` 52–69k → 300–325k.

Bulk writes, documents/s:

| In flight | one index (`orders`, 50k) | three indexes, in turn (62k) | three indexes, interleaved |
| ---: | ---: | ---: | ---: |
| 1 | 24,061 / 24,622 / 24,900 | 14,464 / 16,613 / 16,778 | 16,775 / 16,730 / 17,168 |
| 2 | 42,429 / 45,878 / 42,395 | 32,574 / 32,416 / 32,115 | — |
| 3 | — | — | 45,101 / 44,295 / 41,487 |
| 4 | 73,478 / 79,112 / 72,082 | 53,323 / 51,276 / 53,093 | — |
| 6 | — | — | 68,794 / 53,801 / 56,424 |
| 8 | 87,641 / 100,494 / 86,250 | 58,148 / 63,716 / 62,628 | — |
| 16 | — | 40,711 / 46,957 / 41,490 | — |

What it says:

- **OpenSearch takes overlapping bulk requests about as well on one index as across several.** 4 in flight give about 3×, 8 about 3.5–4×, 16 is slower than 8. The limit is the cluster's, so the setting is per sink, not per index.
- **Postgres keeps scaling to 8 and flattens by 16** on cached data. A real database with cold pages and other clients will flatten sooner.
- The serial ratio matches #389's: flush about 2.3× build. With 4 writes in flight the sink drops below the build, so builds become the limit next, which is why the index tasks build concurrently too.
- **Expected:** a backfill on `reference` about 3× faster with 4 and 4, live changes of other indexes no longer behind it, and live changes of the same index one build plus one write behind it. The headline numbers come from the scenario harness in each stage.

Not parallelised: the reader (key reading and resolution are cheap on `reference`, and one reader is what makes positions simple; concurrent resolution with ordered marking is the next step if a scenario shows the reader saturated), and backfill key reading (0.3 s of 17).

## 5. Bounds and settings

**Postgres connections.** Today: builder pool (sqlx default, 10, about one used), backfill pool 2 (one held in a `REPEATABLE READ` transaction for the whole snapshot), visibility 1, admin 2, replication 1.

With index tasks:

- **One builder pool, sized `builds + 1`**, shared by every index task. A semaphore of `builds` permits bounds concurrent `build_many` calls deployment-wide. A per-index pool would cost 24+ connections on the owner's deployment for nothing the semaphore doesn't give.
- **Backfill keys are read in keyset pages** (`WHERE key > $last ORDER BY key LIMIT 1024`) on that pool, not through a cursor in a held transaction. So 24 indexes backfilling at once hold no extra connection.
- **Why no shared snapshot is needed** (the issue's question 5): the slot is created before any backfill (#120), so every insert, delete or key change made during the backfill is in the WAL and rebuilt live. A row that exists for the whole backfill is reached by the keyset walk. Documents are built from the current state anyway. The keys need a single-column ordered key, which the backfill already requires.
- Total with defaults: 5 + 1 + 2 + 1 = **9**, whatever the number of indexes.

**Memory in flight.**

- **Lanes are bounded by bytes.** Each `(index, sink)` lane holds at most `lane_mib` of documents (default 8) and at most `capacity` items, whichever comes first. One item larger than the bound is admitted alone. Size is the length of the document JSON text Postgres returned, which the builder already has; it travels in `EnvelopeMeta.bytes`. A built batch is shared between sinks (`Arc`), so memory is at most `indexes × lane_mib` (192 MiB for 24 indexes), not × sinks, and only lanes behind a slow sink fill. This replaces the 1,024 × 256 documents per lane #389 found.
- **Pending ids are bounded per index** at `max_pending_documents` (default 1,000,000, about 100 MiB of keys). Past it the reader waits. That is the one place one index can pace the others, and only after it is a million distinct documents behind (open question 2).
- **A build holds its permit only while `build_many` runs**, so a full lane never idles the database. Built batches waiting on a lane are at most one live and two backfill per index.

**Backpressure.** A slow sink fills its lanes. A full lane stops the publishing index task; other indexes keep going, and so does that index on its other sinks' lanes once the slow one has room. The reader never waits on a lane.

**Live first.** A backfill never takes the last permit: backfill builds use at most `builds − 1` permits, and backfill writes at most `writes − 1` per sink. A live change always finds a build and a write slot.

**Settings** (`flusso.toml`, never baked into the lock, like `[batch]`; flag > env > file):

| Key | Flag / env | Default | What it bounds |
| --- | --- | ---: | --- |
| `[concurrency] builds` | `--builds` / `FLUSSO_BUILDS` | 4 | concurrent `build_many` calls, deployment-wide; the builder pool is this + 1 |
| `[concurrency] writes` | `--writes` / `FLUSSO_WRITES` | 4 | bulk requests in flight per sink |
| `[concurrency] lane_mib` | `--lane-mib` / `FLUSSO_LANE_MIB` | 8 | document bytes queued per `(index, sink)` lane |
| `[concurrency] max_pending_documents` | `--max-pending-documents` / `FLUSSO_MAX_PENDING_DOCUMENTS` | 1,000,000 | pending ids per index before the reader waits |

`[batch] max_changes` keeps its meaning (documents per build and per write). `[batch] max_delay_ms` now only bounds how long requests coalesce. `[stream] capacity` stays as the item cap per lane.

## 6. Failure and lifecycle

- **An index task that fails** (a `Stop` build failure, a query error) is restarted alone with backoff (1 s doubling to `max_restart_backoff`), as a sink engine is today. Its ids in flight go back to pending, so the change is retried. Other indexes keep going. Its pending set keeps filling, and its oldest pending position pins the slot: the same trade as a stalled sink today, with the per-index pending age as the alarm.
- **`on_error`** is resolved per index as now. `skip` drops the document and the position still advances. A `stop` *rejection at the sink* no longer stops the sink engine: it parks that `(index, sink)` lane, which is retried with backoff while the sink's other lanes go on. A write carries one index's documents, so a missing target index is a lane failure too. A transport failure fails every lane of the sink, which is the sink being down.
- **Stop (#371).** The reader reads nothing more. Index tasks finish the builds in flight, publish them and take nothing new; pending ids that were never taken are left, unconfirmed, and read again on the next start. Backfills are abandoned unacknowledged, as now. Sink engines finish their lanes' live items and skip backfill items as now; `Progress` is confirmed as acknowledgements arrive; then the live stream closes with the final position. The deadline and exit code stay the CLI's.
- **Staging (#120).** Unchanged order: `continuity()`, every sink staged (rebuilds and requests queued), then the daemon calls `prepare()`, then it spawns the reader and the index tasks. No index task can start a backfill before `prepare`.
- **Reindex of one sink.** As today, the sink engine stages the new generation between items and requests a backfill, now on that index's request lane. Other sinks and other indexes see nothing.
- **`--skip-backfill`** and **`backfill = false`** are unchanged: no request is sent, and a `backfill = false` sink still gets and acknowledges live items.
- **The visibility hold (#343)** stays in the capture, before the reader. A build reads only after its change was emitted, and the per-id rule doesn't depend on which connection a build used.
- **Observability.** `/status` gains `indexes.<name>`: phase (`live`, `backfilling`, `restarting`), pending documents, age of the oldest pending change, builds in flight, and per sink the lane's bytes, items and writes in flight. Metrics gain an `index` label on build time, pending documents and lane bytes. `/readyz` still means every task is ready.

## 7. The ports

| Port | Change |
| --- | --- |
| `Stream` | `lane(index, sink)` and `requests(index)`; `watermark` and `is_idle` move to `Progress`. ADR 0003's "the stream owns the watermark" is superseded: one position now fans out to lanes only the engine knows about. The request lane and redelivery stay the stream's. |
| `LaneItem::Batch` | `index`, `envelopes`, `bytes`, and `kind: Live { token } \| Backfill`; no `position`, no `changes`. `SnapshotComplete` names one index. |
| `Request` | `Backfill { sink }`, one per index lane. |
| `DocumentBuilder` | `build_many` already takes `&self` behind an `Arc` and runs on a pool, so it is shareable as is. Gains `keys(index, after, limit)` for keyset pages; `Document::Upsert` gains the document's size. |
| `ChangeCapture` | `snapshot` goes (keys come from the builder). `prepare` is called by the daemon. |
| `Sink` | `apply` + `flush(caught_up)` become one `write(&self, index, &[Envelope], caught_up) -> FlushReport`, with no buffer in the sink, safe to call concurrently. "Caught up" means nothing else is queued or in flight on the sink. The OpenSearch sink's remembered-to-refresh set stays behind its own lock. |
| channel adapter | One ticketed ledger per lane, as now; out-of-order acknowledgement already works. Adds the byte bound. |

## 8. Guards

- **Ordering, mocks.** A builder that returns each document's "database version at build start" and finishes builds in random order; sinks that record what they write and take random time. `a_document_is_never_built_or_written_twice_at_once`, and per id per sink the versions written never go down.
- **Seeded random, in the style of `random_stops_under_writes_converge_after_a_restart`.** `FLUSSO_SCHEDULE_SEED` replays. Random delays, backfills and reindexes started at random points, index tasks, sink engines and the reader killed and restarted at random. At every confirmation: every change at or before the confirmed position is on every sink at its version or newer. At the end: every sink equals the table.
- **No lost or reordered update during a backfill, live.** Postgres + OpenSearch, `builds = 8`, `writes = 8`, small batches. Backfill 50,000 rows while writers bump a counter column on random rows, delete some and re-key some. A recording wrapper around the real sink asserts per id that counters never go down; afterwards every document equals its row and no deleted row has a document.
- **Independence.** `an_idle_index_never_pins_the_watermark`; `a_failed_index_pins_the_slot_and_holds_up_no_other_index`; `a_slow_lane_paces_only_its_index`; `a_parked_lane_leaves_the_sinks_other_lanes_running`.
- **Bounds.** `lane_memory_is_bounded_by_bytes`; `a_backfill_never_takes_the_last_build_or_write`; `pending_ids_past_the_cap_make_the_reader_wait`; a 24-index e2e counting `pg_stat_activity` while all of them backfill (≤ builds + 5).
- **Kept as they are.** The `wal_release` bounds, the `fence` e2e, the `shutdown` e2e, `visibility`, `pipeline`'s restart cases, the request coalescing tests, the backfill e2e from #389.

## 9. Stages

Each leaves `main` correct and is measured with the `reference` scenario before and after.

1. **Concurrent writes per sink.** `Sink::write`, conflict admission on the sink's one lane, `writes` in flight, and the lane's watermark computed as the newest position before its oldest unacknowledged positioned item, instead of its newest acknowledged one (with acknowledgements out of order, the newest one can pass an item still in flight). *Gain:* the measured bottleneck; about 2–3× on a backfill.
2. **Byte-bounded lanes.** Document size from the builder, `lane_mib`. *Gain:* memory, not speed.
3. **`Progress` and per-`(index, sink)` lanes,** still one ingest task. Watermark leaves the stream; per-index request lanes. *Gain:* none by itself; it makes stage 4 a change to the engine only.
4. **Index tasks.** Pending sets, the reader, build permits, keyset backfill on the builder pool, per-index restart. *Gain:* concurrent builds and per-index independence; live changes no longer queue behind another index's backfill.
5. **Per-lane failure and per-index status.** Parking a lane on a `stop` rejection, `/status` and metrics per index, the manual and `CLAUDE.md`'s invariants.

## 10. Open questions

1. **Defaults `builds = 4`, `writes = 4`.** Measured about 3× each. 8 measured faster (5× and 3.5–4×), but on cached data and an otherwise idle database. *Recommendation:* 4 and 4. *Cost:* leaves part of the measured speed for the setting.
2. **What a stalled index does once `max_pending_documents` is reached.** (a) The reader waits; every index stalls, after a million documents (recommended: bounded memory, simple). (b) No cap; memory grows with the distinct documents touched while the index is down. (c) Drop the pending set and schedule a reindex of that index on every sink once it recovers, which rebuilds deletes correctly through a new generation. *Cost of (c):* a full rebuild per overflow and a new code path.
