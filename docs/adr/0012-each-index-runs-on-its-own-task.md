---
status: proposed
---

# Each index runs on its own task; order is kept per document

Today one ingest task builds every index in turn, and each sink writes one batch at a time (issue #386). A backfill of one index holds up live changes of every other index, and on `reference` the sink's flushes take about twice the build time while the cluster could take four requests at once.

**Decision.**

- **One reader task** reads changes, resolves each one once on a connection of its own, and marks the documents it touches as *pending* on their index.
- **One task per index** builds that index's pending documents and its backfill in batches, and publishes each built batch to one lane per `(index, sink)`.
- **One task per sink** writes its lanes with several bulk requests in flight.
- **Order holds per document, not per lane.** A document is never built twice at once and never written twice at once, and a later build of it reaches every sink after an earlier one. Nothing else is serialized. Sinks need no versioned writes.
- **A position describes itself.** It carries what confirmation and fences need, so the source keeps no map of every change. The engine's `Progress` finds the oldest change some index hasn't landed on some sink; the slot is confirmed up to just before it.
- **Fences and "caught up" are per `(index, sink)`.** A stalled index holds up no `/wait` and no refresh on another index.

```text
                          ┌─ pending(users) ─▶ index task(users) ──┬─ lane(users,primary) ──┐
ChangeCapture ─▶ reader ──┼─ pending(orders) ▶ index task(orders) ─┼─ lane(users,audit) ────┤
 read · resolve once      └─ pending(…) ─────▶ index task(…) ──────┘  lane(orders,…) …      │
 (own connection)              ▲ key pages      build · publish                             │
                               └─ backfill ◀── request(index) ◀─── sink engines ◀───────────┤
                                                                                             ▼
 confirm ◀── Progress: oldest outstanding position per (index, sink) ◀── written ── SinkEngine(primary): writes ×4
                  └──▶ fence per (index, sink): applied / searchable                 SinkEngine(audit):   writes ×4
```

## 1. The shape

**Reader** (one task; owns the replication stream).

- Reads changes as today. The visibility hold (ADR 0010) and the read-ahead stay in the capture.
- Resolves each change once with `DocumentBuilder::resolve`, on a resolution connection of its own (section 5), so reverse lookups never queue behind builds.
- For every document id it gets, marks the id pending on that index, with the change's position. Then records the change as *routed* in `Progress`. A change that resolves to nothing is landed once routed.
- Waits only when the deployment's pending budget is full (section 5). It never waits on a lane.

**Pending set** (one per index; shared state, not a queue).

- A map from document id to the *oldest* position not yet built for it.
- A second change to a pending id adds nothing: a build reads the database as it is now, so one build after the newest change covers every earlier one. Deduplication spans the whole backlog, not one batch.
- A mark is unconditional, even for an id whose build is in flight: that build may have read before this change.
- A build that fails gives its ids back with `min(old, current)` as their oldest position, so no change is forgotten.

**Index task** (one per enabled index).

- Takes pending ids, oldest first, up to `max_changes` per build, skipping ids whose earlier build is still in flight.
- **One live build in flight per index; a second only while at least `max_changes` ids are pending.** A trickle stays one build of whatever piled up, so "a burst batches itself" holds. Concurrency within an index is for backlogs and backfills only.
- Calls `build_many` on the shared builder and publishes the batch to every lane of that index.
- Runs the index's backfill: reads root keys in keyset pages, builds them like pending ids, and publishes to the lanes that asked. A page drops ids that are pending or in flight live: their live build goes to every lane, the requester's included.
- Remembers the last key it published per backfill, so a restarted index task resumes the backfill from there. The request stays unacknowledged until `SnapshotComplete`, as today.

**Sink engine** (one per sink, as today).

- Reads every lane of its sink and keeps up to `writes` bulk requests in flight across them.
- Each write carries one lane's item, so one index's documents.

**A change touching several indexes** is read and resolved once, on the reader. Its ids are split by index into the pending sets, and each index builds only its own.

**Backfill.** A sink asks per index (`Request::Backfill { sink }` on that index's request lane). The index task coalesces requests from several sinks as today, pages the keys, builds them, publishes to the requesting lanes, then publishes `SnapshotComplete` and acknowledges. Two indexes rooted at one table each page its keys; keys cost 0.3 s on `reference`, so sharing that read isn't worth coupling the tasks.

### Why a task per index building batches, not a task per document

The owner's framing: the reader publishes that something changed, the builders interested in it wake up, build in parallel and publish. That is this shape; the open choice is the size of a builder's unit of work.

`build_many` fetches up to 512 documents in one query, so the joins run once per batch instead of once per document. Measured on the same data and machine at the same load (average 22–26), documents/s over three runs:

| | 1 caller | 8 callers | 32 callers |
| --- | ---: | ---: | ---: |
| one document per `build_many` | 1,079 / 1,056 / 3,328 | 8,091 / 9,515 / 14,085 | 16,255 / 19,474 / 18,651 |
| 256 documents per `build_many` | 11,005 / 14,087 / 17,809 | 78,965 / 62,945 | — |

32 concurrent per-document builds, on 32 connections, do about as well as one batched caller on one connection, and 8 batched callers do 4× better than them. A task per document also loses the deduplication of a burst, and it needs a connection or a queue in front of the pool, which is the batching back again. Batches per index keep the query count low and the per-document order easy to keep.

## 2. Ordering

Today "build order is delivery order" holds because one task builds everything. It is replaced by:

> **For each document, builds never overlap, and a later build reaches every sink after an earlier one.**

Why that is enough:

- `build_many` reads the current, visible state. A build that starts after another one ended sees at least what the first saw (Read Committed takes a new snapshot per statement, and later snapshots on the primary see a superset). So per document, build order is state order.
- Builds of *different* documents can land in any order: they never overwrite each other.

**One admission rule at each stage: an item starts only when no earlier unfinished item shares a document id with it.**

| Stage | Items | "Finished" means |
| --- | --- | --- |
| Index task | pending batches and backfill pages | built and published to every lane |
| Sink engine, per lane | lane items | the bulk request returned |

How the cases the issue lists come out:

- **Backfill against live changes of one index.** A page and a live batch run in either order or at once if they share no id; a page skips ids live already covers. A live batch waits only for an earlier item sharing an id, so on an index under backfill a live change waits about one build plus one write.
- **An index task restarts.** Ids taken but not published go back to pending; published ones are in the lanes.
- **A sink engine restarts, or an item is redelivered.** First, writes still in flight are waited for, up to the request timeout, so no orphaned bulk request lands after its redelivery. Then the lane redelivers only the unacknowledged items, in lane order, and admission applies again.
- **The reader restarts.** The stream reopens at the confirmed position and marks ids again; that costs rebuilds, never a wrong document.
- **A reindex of one sink while another sink of the index is live.** Pages go to the reindexing lane only, live batches to both. Staging the new generation is a barrier on that index's lane: it starts when nothing is in flight there, because a write in flight picked its generation before the swap.
- **Two sinks at different speeds.** Each lane is admitted on its own. The index task waits only when its memory budget is full (section 5).
- **`SnapshotComplete`** is a barrier on its lane: it starts when everything before it has finished, and nothing after it starts until `mark_seeded` returns.
- **A lane parked by `on_error = stop`** (section 6) holds an old build of some ids. When a newer build of the same id is published to that lane, it replaces the parked item's copy of that id (open question 6).

**Versioned writes: not needed, and rejected.**

- *Build sequence as an external version.* Start order isn't snapshot order once two connections race; with the per-id rule it adds nothing.
- *Postgres snapshot (`xmin`/`xmax`).* Not a total order.
- *Change position.* A backfill build has none, and two builds after one position can see different states.
- *Cost.* Every operation carries `version_type=external`, and a delete keeps its version only for `index.gc_deletes` (60 s), after which an older upsert brings the document back.

## 3. Confirmation and fences

**A position describes itself.** Today `Positions` (`cdc/ack.rs`) keeps one `entries` row per emitted change until it is confirmed. Today a full lane stops the reader, which bounds the map. A reader that doesn't wait on lanes would let it grow without bound behind a stalled index: 1,000 hot rows taking 10,000 updates/s keep 1,000 ids pending and add 10,000 rows a second. So the capture stamps each position with what it knows when it emits the change. It already knows commit boundaries then, because the visibility hold keeps whole transactions:

- `fence_before`: the change's commit LSN − 1. A fence can claim this while the change is outstanding.
- `confirm_before`: the end of the last commit complete before this change. Confirmation may go this far while the change is outstanding.
- `confirm_through`: the same once this change is landed (its own commit end if it is the commit's last change).

The capture then keeps only its horizon (keepalives, filtered commits) and the confirmed LSN. `reached` and `confirm` take the position, not a sequence to look up. The rules don't change: never confirm into an unprocessed commit's record, and fence strictly below the next outstanding change.

**`Progress`** (engine crate, one per deployment, outlives every task, like `Status`):

- *Routed:* the newest routed position.
- *Outstanding per `(index, sink)`:* a multiset of positions: the oldest position of each pending id, of each build in flight, and of each lane item not yet acknowledged.
- *Landed for a lane:* its oldest outstanding position's `confirm_before`, or *routed*'s `confirm_through` when nothing is outstanding.
- *Confirmable:* the minimum over every lane. The ingest side hands it to `ChangeCapture::confirm` after each change it marks and on the one-second tick, as today.

The slot can never pass a change some index hasn't landed on some sink: that change's position is in some multiset until then.

Memory: one entry per pending id, build in flight and lane item. Bounded by the pending and lane budgets (section 5), not by WAL.

**Batches and lanes.**

- A lane item carries the oldest position among its ids instead of a batch position. Acknowledging it removes that position from the lane's multiset.
- **Empty batches disappear.** A change that touches nothing of an index costs that index nothing, so an idle index never pins the slot. The `wal_release` guard that relies on empty batches is rewritten to assert the same release bounds without them.
- Backfill items carry no position and never move `Progress`, as today.

**Fences, per `(index, sink)`** (ADR 0009 keeps its meaning, at a finer grain):

- *Applied* for a lane is its oldest outstanding position's `fence_before`, else the capture's `reached` for *routed*. *Searchable* is the same over items whose write has become searchable.
- `/wait` gains `index` (one or more). Without it, it waits on every index, as today. `Client::after` passes its root's index, and every index of a combined search.
- A failed or lagging index blocks only fences that name it.
- **"Caught up" is per lane.** A write is caught up when its lane has nothing else queued or in flight. A `Deferred` refresh of an index runs when that index's own lane empties, so a long backfill of one index no longer keeps another index unrefreshed.

## 4. Where concurrency pays

From #389, on `reference` under load ≈32: key fetch 0.3 s, build 5.8–7.4 s, flushes 12.6–16.8 s. The sink sets the pace.

**Prototype.** A throwaway example, not committed. It loaded the `reference` config and seeded data (`default` scale, 62,000 documents) and did three things:

- called `PgDocumentBuilder::build_many` with *N* concurrent callers on one pool;
- built every document once, then wrote the batches through *N* `OpensearchSink`s (one buffer each, `flush(false)`), each run into fresh indexes;
- counted the heap a built document holds, with a counting allocator.

Setup: Postgres 16 and OpenSearch 2 (single node, 1 GiB heap, one shard per index) in local containers, data in cache, 24 cores. Three runs each. Load average 2.4–5.2 for the first two tables; 22–24 for the memory and per-document runs, because another agent was measuring.

`build_many`, 256 keys per call, all three indexes, documents/s:

| Concurrent calls | run 1 | run 2 | run 3 | vs 1 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 42,320 | 38,743 | 32,900 | 1× |
| 2 | 67,939 | 53,276 | 58,892 | 1.5× |
| 4 | 112,959 | 121,032 | 112,787 | 3.0× |
| 8 | 191,783 | 190,468 | 188,317 | 5.0× |
| 16 | 243,791 | 247,853 | 250,283 | 6.4× |

Within one index the scaling is the same: `users` 13–15k → 57–65k documents/s at 8, `orders` 52–69k → 300–325k.

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

- **OpenSearch takes overlapping bulk requests as well on one index as across several.** 4 in flight give about 3×, 8 about 3.5–4×, 16 is slower than 8. The limit is the cluster's, so `writes` is per sink.
- **Postgres scales to 8 and flattens by 16** on cached data; a real database with cold pages and other clients flattens sooner.
- Serially, writing takes about 2.3× the build time, as in #389. With 4 writes in flight the sink drops below the build, so concurrent builds are the next limit.
- **Expected:** a backfill on `reference` about 3× faster with 4 builds and 4 writes. Another index's backfill no longer delays live changes; on the same index a live change waits about one build plus one write.

**Memory a built document holds** (counted heap of the `Document`s against their JSON text):

| Index | JSON per document | heap per document | factor |
| --- | ---: | ---: | ---: |
| `users` | 1,523 B | 16,076 B | 10.6× |
| `products` | 910 B | 7,394 B | 8.1× |
| `orders` | 367 B | 3,674 B | 10.0× |

The allocator's own overhead isn't counted, so these are lower bounds. A batch of 256 `users` documents holds about 4 MiB. Budgets therefore count heap bytes, not JSON length (section 5).

Not parallelised: the reader, and backfill key paging (0.3 s of 17). See below.

### Where resolution runs

The owner's framing suggests the alternative: the reader publishes the row change, and each index's task resolves its own affected documents in parallel.

| | Resolve once on the reader (chosen) | Each index resolves its own |
| --- | --- | --- |
| Work | one `resolve` per change | one per change per index that reads the table; most of it the same lookups |
| Correctness | the pre-image travels with the change; lookups run as the change is routed | also correct: the pre-image travels too, and a lookup run later reads later state, whose own changes are in the WAL and resolve in turn; same guarantee, later |
| Backlog of a stalled index | document ids, each held once at its oldest position | row changes: every change, with its pre-image, kept until resolved, because each pre-image may name a different old parent. Bounded by WAL, not documents |
| Independence | one slow reverse lookup delays every index | a slow lookup delays only its index |

**Decision: resolve once on the reader.** It bounds the backlog by documents and does each lookup once.

**What would flip it.** #387 is measuring the reader's share of the time. If the reader is busy resolving most of the time in a burst while index tasks wait for work, the first step is concurrent resolution *on the reader*: several changes resolved at once, marked in order, on a small resolution pool. That keeps the id backlog. Moving resolution to the indexes pays only if one index's lookups dominate the reader and hold up indexes whose own lookups are cheap.

## 5. Bounds and settings

**Postgres connections.** Today: builder pool (sqlx default 10, about one used), backfill pool 2 (one held in a `REPEATABLE READ` transaction for the whole snapshot), visibility 1, admin 2, replication 1.

With index tasks:

| Pool | Connections | Used by |
| --- | ---: | --- |
| builder | `builds` (4) | `build_many`, backfill key pages, column-type lookups during builds |
| resolution | 1 | `resolve`'s reverse lookups and root-key reads, column-type lookups during resolution |
| visibility | 1 | as today |
| admin | 2 | as today |
| replication | 1 | as today |
| **total** | **9** | whatever the number of indexes |

- A semaphore of `builds` permits bounds builds deployment-wide. **A key page takes a backfill build permit**, so it waits its turn like a build.
- **Live first, without a reserved permit.** Live waiters on the semaphore are served before backfill waiters, and backfill never holds more than the permits it got. With `builds = 1` or `writes = 1` a backfill still runs, between live builds. Both settings are tested.
- **Backfill keys are read in keyset pages** (`WHERE key > $last ORDER BY key LIMIT 1024`) on the builder pool, not through a cursor in a held transaction, so 24 indexes backfilling at once hold no extra connection.
- **No shared snapshot is needed** (the issue's question 5). The slot is created before any backfill (#120), so every insert, delete or key change during the backfill is in the WAL and rebuilt live. A row that exists for the whole backfill is reached by the keyset walk. Documents are built from the current state anyway. Keyset paging needs a single-column ordered key, which the backfill already requires.

**Memory.**

- **Built documents share one deployment budget, counted in heap bytes:** `max_built_mib`, default 512.
  - The builder reports each document's heap size; it builds the tree, so adding up its allocations costs little.
  - An index task reserves `max_changes ×` its running average before it builds, and settles to the real size after. A build's output is therefore inside the budget while it runs, waits for publish, and sits in lanes.
  - A batch is counted once while any lane holding it hasn't acknowledged it, since every lane holds the same `Arc`.
  - Live reservations are served first. One index's backfill may hold at most half the budget.
- **Pending ids share one deployment budget:** `max_pending_documents`, default 1,000,000 (about 100 MiB of keys plus their positions). Past it the reader waits.
- **Not in a budget, but bounded:**
  - per sink, `writes` bulk bodies in flight (NDJSON, about the JSON size of one batch, 0.1–0.4 MiB here) and their responses;
  - the reader's read-ahead (`max_changes × 16` changes).
- **Worst case:** `max_built_mib` + `max_pending_documents × ~100 B` + `sinks × writes × one batch's JSON`. With the defaults and two sinks: 512 + ~100 + ~4 MiB, about 0.6 GiB. Today a single lane can hold 1,024 × 256 documents.

**Backpressure.**

- A slow sink fills its lanes. Their batches stay counted, so the budget fills, and new builds of the indexes that publish there wait.
- Other indexes keep building while budget is left. A sink stalled for long enough fills the budget and pauses every index, which is also when its slot position stops moving: the alarm is the per-`(index, sink)` stall age.

**A stalled index** (failed, parked or lagging) pins the slot at its oldest outstanding position, and on the primary that costs WAL. If `max_slot_wal_keep_size` invalidates the slot, every index on every sink is rebuilt.

- `/status` and metrics report, per `(index, sink)`, the age of the oldest outstanding position and the WAL behind it.
- A default alert fires past 10 minutes or 1 GiB.
- The reader's wait is tied to memory (the pending budget), not to WAL: the reader stopping doesn't stop the primary writing WAL.
- Dropping a stalled index's backlog and reindexing it once it recovers is the follow-up (open question 2).

**Settings.** In `flusso.toml`, never baked into the lock, like `[batch]`; flag > env > file:

| Key | Flag / env | Default | What it bounds |
| --- | --- | ---: | --- |
| `[concurrency] builds` | `--builds` / `FLUSSO_BUILDS` | 4 | concurrent builds and key pages, deployment-wide; the builder pool size |
| `[concurrency] writes` | `--writes` / `FLUSSO_WRITES` | 4 | bulk requests in flight per sink (lowered for a while after a 429; section 6) |
| `[concurrency] max_built_mib` | `--max-built-mib` / `FLUSSO_MAX_BUILT_MIB` | 512 | heap of built documents not yet acknowledged by every lane |
| `[concurrency] max_pending_documents` | `--max-pending-documents` / `FLUSSO_MAX_PENDING_DOCUMENTS` | 1,000,000 | pending ids, deployment-wide, before the reader waits |

- `[batch] max_changes` keeps its meaning: documents per build and per write.
- `[batch] max_delay_ms` now only bounds how long backfill requests coalesce.
- `[stream] capacity` is deprecated: it still loads (format freeze), is ignored, and warns. A lane is bounded by bytes alone.

## 6. Failure and lifecycle

**An index task that fails** (a `stop` build failure, a query error) is restarted alone, with backoff from 1 s doubling to `max_restart_backoff`.

- Its ids in flight go back to pending, and its backfill resumes from the last key it published.
- Other indexes keep going. Its outstanding positions pin the slot, and the stall alert is the alarm.

**`on_error` and the sink.**

- `skip` drops the document and the position still advances, as now.
- A `stop` rejection parks that `(index, sink)` lane: it is retried with backoff while the sink's other lanes go on. A newer build of a parked id replaces it (open question 6).
- A write carries one index's documents, so a missing target index fails that lane only. A transport failure fails every lane of the sink, which means the sink is down.

**Transient refusals are not rejections.** Today `bulk_rejected` (`bulk.rs`) turns every item with status ≥ 400 into a `RejectedDocument`, except a missing index. That includes `es_rejected_execution_exception` (429, a full write queue), which with `skip` is acknowledged and lost. Several requests in flight make it likely on a busy cluster. The fix lands first:

- An item answered 429 or 503 is retried with backoff in a later request and never reaches `on_error`.
- A request-level 429 is already retried by the transport. It also lowers that sink's writes in flight to one, which climbs back by one per clean minute up to `writes`.

**Stop (#371).**

1. The reader reads nothing more.
2. Index tasks finish the builds in flight, publish them, and take nothing new. Pending ids never taken are left unconfirmed and replayed on the next start (open question 7).
3. Backfills are abandoned unacknowledged, as now.
4. Sink engines finish their live items and skip backfill items, as now; `Progress` is confirmed as acknowledgements arrive.
5. The live stream closes with the final position. The deadline and exit code stay the CLI's.

**Staging (#120)** keeps its order:

1. `continuity()`.
2. Every sink staged: rebuilds staged, requests queued.
3. The daemon calls `prepare()`.
4. The reader and the index tasks start, so no backfill can start before `prepare`.

**Reindex of one sink** works as today, as a barrier on that index's lane (section 2), with the request on that index's request lane.

**Unchanged:** `--skip-backfill`; a `backfill = false` sink (no request; it still gets and acknowledges live items); the visibility hold (#343), which stays in the capture before the reader.

**The stdout sink** holds its output lock for a whole `write`, so concurrent writes never interleave their lines.

**Observability.**

- `/status` gains `indexes.<name>`: phase (`live`, `backfilling`, `restarting`), pending documents, builds in flight, budget held, and per sink the stall age, WAL behind, items and writes in flight.
- Metrics gain an `index` label on build time, pending documents and stall age.
- The dev dashboard gets the panels, plus a `FlussoIndexStalled` alert.
- `/readyz` still means every task is ready.

## 7. The ports

| Port | Change |
| --- | --- |
| kernel `Position` | Gains the source's `fence_before`, `confirm_before` and `confirm_through`, opaque to the engine (section 3). |
| `ChangeCapture` | `confirm` and `reached` take a position; the `entries` map goes. `snapshot` goes (the builder pages keys). The daemon calls `prepare`. |
| `Stream` | `lane(index, sink)` and `requests(index)`. `watermark` and `is_idle` move to `Progress`, superseding ADR 0003's "the stream owns the watermark": one position now fans out to lanes only the engine knows about. Redelivery and the request lane stay. |
| `LaneItem::Batch` | `index`, `envelopes`, heap `bytes`, `oldest: Option<Position>` (`None` for backfill). No batch position, no `changes`. `SnapshotComplete` names one index. |
| `Request` | `Backfill { sink }`, one lane per index. |
| `DocumentBuilder` | `build_many` already takes `&self` behind an `Arc` on a pool, so it is shareable as is. Gains `keys(index, after, limit)` for keyset pages. `Document::Upsert` gains its heap size. The builder gets a second pool for resolution. |
| `Sink` | `apply` + `flush(caught_up)` become `write(&self, index, &[Envelope], caught_up) -> FlushReport`: no buffer in the sink, safe to call concurrently. `FlushReport` separates retryable items from rejected ones. |
| channel adapter | `release` today moves a cursor back and redelivers every item after it, including items whose writes are still running. That breaks "never written twice at once", so it becomes a per-ticket "due again" flag. Bytes bound, no item cap. |

## 8. Guards

- **Ordering, with mocks.** A builder returns each document's "database version at build start" and finishes builds in random order; sinks record what they write and take random time. `a_document_is_never_built_or_written_twice_at_once`; per id per sink, the versions written never go down.
- **Seeded random**, in the style of `random_stops_under_writes_converge_after_a_restart`, replayed with `FLUSSO_SCHEDULE_SEED`:
  - random delays, backfills and reindexes started at random points, items answered 429;
  - index tasks, sink engines and the reader killed and restarted at random;
  - at every confirmation and every fence answer, every change at or before it is on the named sinks at its version or newer;
  - at the end, every sink equals the table.
- **No lost or reordered update during a backfill, live.** Postgres + OpenSearch, `builds = 8`, `writes = 8`, small batches. Backfill 50,000 rows while writers bump a counter column on random rows, delete some and re-key some. A recording wrapper around the real sink asserts per id that counters never go down. Afterwards every document equals its row and no deleted row has a document.
- **The fixes that land first.** `a_released_ticket_redelivers_only_itself` (channel). `an_item_refused_with_429_is_retried_not_rejected` (bulk unit test, and a live one against a cluster with a one-slot write queue).
- **Independence.**
  - `an_idle_index_never_pins_the_slot`
  - `a_failed_index_pins_the_slot_and_holds_up_no_other_index`
  - `a_fence_on_one_index_ignores_a_stalled_sibling_index`
  - `a_backfill_of_one_index_leaves_anothers_deferred_refresh_on_time`
  - `a_parked_lane_leaves_the_sinks_other_lanes_running`
- **Bounds.**
  - `positions_stay_bounded_while_one_index_is_stalled`: hot rows at a high rate, one failed index.
  - `the_built_budget_is_never_exceeded`
  - `live_goes_first_and_backfill_still_runs_with_one_build_and_one_write`
  - `a_trickle_is_built_one_batch_at_a_time`
  - `a_restarted_backfill_resumes_from_its_last_key`
  - a 24-index e2e counting `pg_stat_activity` while every index backfills: at most 9 connections.
- **Kept as they are:** the `fence` e2e, the `shutdown` e2e, `visibility`, `pipeline`'s restart cases, the request coalescing tests, the `backfill` e2e from #389. **Rewritten:** `wal_release`'s empty-batch guard.

## 9. Stages

Each leaves `main` correct and is measured on the `reference` scenario before and after.

1. **Fixes needed by any concurrency.** Item 429/503 retried instead of rejected, with the 429 slow-down. The channel releasing per ticket. *Gain:* no lost documents on a busy cluster.
2. **Keyset backfill on the builder pool.** Drops the long-held `REPEATABLE READ` transaction and its pool. *Gain:* one connection and no long transaction; the same speed.
3. **Concurrent writes per sink.** `Sink::write`; per-id admission on the sink's one lane; `writes` in flight.
   - The lane's watermark becomes the position before its oldest unacknowledged item, using the predecessor position recorded at delivery.
   - Fence reports follow that watermark, not each item.
   - The stdout sink serializes a write. In-flight writes are awaited on a restart.
   - *Gain:* the measured bottleneck, about 2–3× on a backfill.
4. **Concurrent builds on the one ingest task.** Build permits with live priority, per-id admission, one live build plus backlog builds. *Gain:* builds stop being the next limit.
5. **The memory budget.** Heap size from the builder; `max_built_mib` replaces `[stream] capacity`. *Gain:* bounded memory.
6. **Self-describing positions and `Progress`.** Per-`(index, sink)` lanes, fences and caught-up; per-index request lanes; `/wait?index`, `Client::after` passing its index. *Gain:* a stalled index blocks no other index's fences or refreshes.
7. **The reader and index tasks.** Pending sets, the resolution connection, per-index restart with backfill resume, stall age and the alert. *Gain:* per-index independence; live changes never queue behind another index's backfill.
8. **Per-lane failure.** Parking a lane on `stop`, with newer builds replacing the parked copy.

**What each stage updates beside the code:**

- `CLAUDE.md`'s invariants and guards.
- The manual's Reference pages: `config-toml.md` (`[concurrency]`), the CLI flags, `/status`, `/wait`, metrics. Plus `docs/src/contribute/pipeline.md`.
- `libs/1-config/config.schema.json` (`just schema-gen`).
- The designer: `ConfigPanel.tsx` authors `[batch]`, so `[concurrency]` needs its controls, i18n in every locale, and the dist rebuilt.
- The upgrade page: the changed `Sink` trait and `Position`, `max_delay_ms`'s narrower meaning, `[stream] capacity` deprecated, `/wait`'s `index`.
- The Grafana panels and the stall alert.
- `CONTEXT.md`: lane, position, pending, `Progress`.
- ADR 0003 marked superseded in part.
- A new baseline for the in-process `engine` bench.

## 10. Open questions for the owner

Each has a recommendation and its cost. The reviewer's recommendation is the proposed default.

1. **Defaults `builds = 4`, `writes = 4`.** *Recommended: yes.* 8 measured faster (5× and 3.5–4×), but on cached data and an idle database. *Cost:* part of the measured speed is left to the setting.
2. **What a stalled index does.** *Recommended:* now, the reader waits only at the memory budgets, and a stall past 10 minutes or 1 GiB of WAL alerts. Next, dropping the stalled index's backlog and reindexing it on every sink once it recovers, which unpins the slot. *Cost:* until the follow-up, an index stalled long enough still risks the slot (`max_slot_wal_keep_size`).
   - *Where this differs from the review:* the review asks to cap the reader's wait by age or WAL. Waiting doesn't stop the primary writing WAL, so it caps nothing the stall consumes. It only stalls the healthy indexes as well. The cap is therefore memory, and WAL and age drive the alert and, later, the drop.
3. **Fences per `(index, sink)`**, `/wait?index`, and `Client::after` passing its index. *Recommended: yes.* *Cost:* a `/wait` parameter and a query-train change.
4. **Item 429s retried as transient; `writes` fixed, with a temporary reduction after a request-level 429.** *Recommended: yes.* *Cost:* a document refused for good with 429 retries forever; the lane's stall age shows it.
5. **Concurrency within one index only for a backlog, never for a trickle.** *Recommended: yes.* *Cost:* a single hot index under a steady trickle uses one build at a time.
6. **A lane parked by `on_error = stop`: resend the stale build, or let a newer build of the same id replace it.** *Recommended: replace.* *Cost:* the lane item becomes mutable, so the channel adapter gains a replace-by-id.
7. **A stop with a backlog pending leaves unbuilt ids for replay.** *Recommended: yes.* *Cost:* the next start rebuilds them; they were never confirmed, so nothing is lost.
