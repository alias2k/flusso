---
status: accepted
---

> **Implementation status (the first merge, PR #422).** In: stages 1–2 (already on `main`) and 3–7, 8b's backfill resume, 8c (stall age, WAL behind, the alert), and #379 (a fence holds while an index is seeded or rebuilt). Some 8a groundwork is in as well: the build permits and resolution on its own connection. `Progress`'s pending sets live on `feat/concurrency-8a` with the rest of 8a. **Not yet:** 8a's reader and index tasks, the per-index budget half, per-index restart (8b), and stage 9 (per-lane failure). Until then one ingest task builds every index, and a failure stops a sink's whole run.

# Each index runs on its own task; order is kept per document

Today one ingest task builds every index in turn, and each sink writes one batch at a time (issue #386). A backfill of one index holds up live changes of every other index. On `reference` the sink's flushes take about twice the build time, while the cluster could take four requests at once.

**Decision.**

- **One reader task** reads changes, resolves each one once on its own connection, and marks the documents it touches as *pending* on their index.
- **One task per index** builds its pending documents and its backfill in batches, and publishes each batch to one lane per `(index, sink)`.
- **One task per sink** writes its lanes with several bulk requests in flight, live items first.
- **Order holds per document**: never built twice at once, never written twice at once, a later build lands after an earlier one. No versioned writes.
- **A position describes itself**, so the source keeps no map of every change. The engine's `Progress` confirms the slot up to the oldest change some lane hasn't landed.
- **Fences and "caught up" are per `(index, sink)`.**
- **A stalled lane pins the slot**, as a stalled sink does today, and alerts. The other indexes keep going while memory allows. Dropping and rebuilding a stalled index is left to its own design.

```text
                          ┌─ pending(users) ─▶ index task(users) ──┬─ lane(users,primary) ──┐
ChangeCapture ─▶ reader ──┼─ pending(orders) ▶ index task(orders) ─┼─ lane(users,audit) ────┤
 read · resolve once      └─ pending(…) ─────▶ index task(…) ──────┘  lane(orders,…) …      │
 (own connection)              ▲ key pages      build · publish                             │
                               └─ backfill ◀── request(index) ◀─── sink engines ◀───────────┤
                                                                                             ▼
 confirm ◀── Progress: oldest outstanding position per (index, sink) ◀── written ── SinkEngine(primary): writes ×4, live first
                  └──▶ fence per (index, sink): applied / searchable                 SinkEngine(audit):   writes ×4, live first
```

## 1. The shape

**Reader** (one task, owns the replication stream).

- Reads changes as today. The visibility hold (ADR 0010) and the read-ahead stay in the capture.
- Resolves each change once with `DocumentBuilder::resolve`, on a resolution connection of its own.
- Marks each resulting id pending on its index, then records the change as *routed* in `Progress`. A change that resolves to nothing is landed once routed.
- Waits only when the pending budget is full (section 5), never on a lane.

**Pending set** (one per index; shared state, not a queue).

- A map from document id to the oldest position not yet built for it.
- A second change to a pending id adds nothing: a build reads the database as it is now, so one build covers every earlier change.
- A mark is unconditional, even while the id's build is in flight (that build may have read before the change).
- A failed build gives its ids back at `min(old, current)`.

**Index task** (one per enabled index).

- Takes pending ids, oldest first, up to `max_changes` per build, skipping ids whose earlier build is in flight.
- Runs one live build at a time; a second only while at least `max_changes` ids are pending. So a trickle still batches itself.
- Publishes each built batch to every live lane of the index.
- **Backfill:**
  - Reads root keys in keyset pages and builds them like pending ids, publishing to the lanes that asked.
  - A page drops ids that are pending or in flight live (their live build goes to every lane).
  - It resumes from the last key it published after a restart.
  - The request is acknowledged after `SnapshotComplete`, as today.
- Builds a fresh copy of given ids for one lane, on request (section 6), through the backfill path.

**Sink engine** (one per sink, as today).

- Keeps up to `writes` bulk requests in flight. Each write is one lane item: one index's documents.
- Picks the next item in this order:
  1. live before backfill, except every fourth grant (section 5);
  2. lanes take turns;
  3. within a lane, the earliest admissible item (section 2) first.

**Backfill requests** are per index (`Request::Backfill { sink }`) and coalesce across sinks, as today.

### Why a task per index building batches, not a task per document

`build_many` fetches up to 512 documents in one query, so the joins run once per batch. All three `reference` indexes (62,000 documents), load average 22–26, three runs, documents/s:

| | 1 caller | 8 callers | 32 callers |
| --- | ---: | ---: | ---: |
| one document per `build_many` | 1,079 / 1,056 / 3,328 | 8,091 / 9,515 / 14,085 | 16,255 / 19,474 / 18,651 |
| 256 documents per `build_many` | 11,005 / 14,087 / 17,809 | 78,965 / 62,945 | — |

32 per-document builders on 32 connections do about as well as one batched caller on one connection. A task per document would also lose the deduplication of a burst.

## 2. Ordering

> **For each document, builds never overlap, and a later build reaches every sink after an earlier one.**

**Why that is enough:**

- A build reads the current, visible state. A build started after another ended sees a superset: Read Committed takes a new snapshot per statement, and later snapshots on the primary see more.
- So per document, build order is state order. Builds of different documents may land in any order.

**One admission rule at both stages:** an item starts only when no earlier unfinished item shares a document id with it.

| Stage | Item | Finished when |
| --- | --- | --- |
| Index task | a pending batch, a backfill page, a fresh build | published to every lane it goes to |
| Sink engine, per lane | a lane item | every document in it is written or rejected for good |

How the hard cases come out:

- **An item partly answered 429 stays unfinished** until its retried documents are written. Otherwise a newer build of the same id would be admitted, and the stale retry would overwrite it.
- **A sink engine restarts.** It waits for writes in flight (up to the request timeout), then the lane redelivers only unacknowledged items, in order.
- **An index task or the reader restarts.** Unpublished ids go back to pending, or are re-marked; that costs rebuilds, never a wrong document.
- **A reindex, a re-stage, or `SnapshotComplete`** is a barrier on its lane: it starts when nothing before it is unfinished, and nothing after it starts until it is done.

**Residual risk, as today.** A bulk request the client gave up on can still run on OpenSearch and land after a newer build of the same id.

**Versioned writes: rejected.** A build sequence isn't snapshot order across connections, a Postgres snapshot isn't totally ordered, a backfill build has no position, and an OpenSearch delete keeps its version only for `index.gc_deletes` (60 s), after which an older upsert revives the document.

## 3. Confirmation and fences

> **Nothing is confirmed or fenced past a change the capture emitted until every lane that needs it has landed it.**

**Stamps.** The capture stamps each position as it emits the change. It knows the commit boundaries then, because the visibility hold keeps whole transactions.

- `fence_before`: the change's registered commit LSN − 1.
- `confirm_before`: the newest commit LSN registered before this change's commit.
- `confirm_through`: the change's commit LSN if it is its commit's last change, else `confirm_before`.

**What the capture keeps:**

- `next_seq` (it outlives a reopened stream, as today);
- the horizon, moved by keepalives and filtered commits while nothing is buffered;
- the confirmed LSN.

`entries` goes. `read_through` no longer confirms on its own.

**The port takes one of three cases**, from `Progress`:

| Case | Confirm to | Fence at |
| --- | --- | --- |
| `Before(o)`: `o` is the oldest outstanding position | `o.confirm_before` | `o.fence_before` |
| `Through(p)`: nothing outstanding, *routed* is `p`, and `p.seq + 1 == next_seq` | the live horizon | the live horizon |
| `Through(p)`: nothing outstanding, but a later change was emitted | `p.confirm_through` | `p.confirm_through` |
| `Idle`: nothing routed yet | the live horizon while `next_seq` is still the capture's first seq, else the confirmed LSN | the same |

**Why the `next_seq` test.** An emitted change the reader hasn't routed is in no lane, so the horizon alone would fence or confirm past it, and a crash would lose it. Using only `p`'s stamps would stop idle WAL being released (#111/#204). `next_seq` outlives a reopened stream, so the test also holds after an ingest restart. `Idle` is decided by the capture for the same reason: in a fresh process, change 0 can be emitted, with the horizon past it, before the reader routes it.

**Calls.**

- The ingest side confirms after each routed commit, skipping a value it already sent.
- The one-second tick **always** calls `confirm`, because the answer can change while *routed* stays put (the horizon moves).

**`Progress`** (engine crate; one per deployment; outlives every task, like `Status`):

- *Routed*: the newest routed position.
- *Outstanding per lane*: a multiset of positions (oldest of each pending id, build in flight, unacknowledged lane item). Each lane keeps its minimum incrementally.
- The case is taken over every lane.
- The reader marks ids and records *routed* under one lock.

**Lanes.**

- A lane item carries the oldest position among its ids; acknowledging it removes that position.
- Backfill items carry none.
- Empty batches disappear, so an idle index never pins the slot.

**Fences per `(index, sink)`** (ADR 0009, at a finer grain).

- *Applied* comes from the table, taken over the named lanes. *Searchable* is the same, over items whose write is searchable.
- `/wait` takes `index` (repeatable); without it, every index, as today. Compatible both ways: an old daemon ignores `index`, and an old client waits on every index.
- `Client::after` passes its root's index; `_msearch` and a combined search pass the union of their roots.
- The client's once-per-level cache (`passed` in `sdk/query/src/fence.rs`) is keyed by `(level, index)`. A union fence that passes marks each of its indexes.
- A disabled index gets `FenceRefused`.
- **A lane re-staging after `index_not_found` answers `503`** with a reason until it is seeded again. It has nothing outstanding, so the table would otherwise pass it while nothing is written.
- "Caught up" is per lane: a write is caught up when its lane has nothing else queued or in flight. A `Deferred` refresh of an index runs when its own lane empties.

## 4. Where concurrency pays

From #389, on `reference` under load ≈32: key fetch 0.3 s, build 5.8–7.4 s, flushes 12.6–16.8 s.

**Prototype** (throwaway, not committed) on the `reference` config and data at `default` scale: `build_many` with *N* callers on one pool, pre-built batches written through *N* `OpensearchSink`s into fresh indexes, and a counting allocator for a document's heap. Postgres 16 and OpenSearch 2 (one node, 1 GiB heap, one shard per index), local containers, data in cache, 24 cores, three runs.

`build_many`, 256 keys per call, all three indexes (62,000 documents), load average 2.4–5.2, documents/s:

| Calls | run 1 | run 2 | run 3 | vs 1 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 42,320 | 38,743 | 32,900 | 1× |
| 2 | 67,939 | 53,276 | 58,892 | 1.5× |
| 4 | 112,959 | 121,032 | 112,787 | 3.0× |
| 8 | 191,783 | 190,468 | 188,317 | 5.0× |
| 16 | 243,791 | 247,853 | 250,283 | 6.4× |

Per index at 8 calls, same load: `users` (10,000 documents) 13–15k → 57–65k; `orders` (50,000) 52–69k → 300–325k.

Bulk writes, load average 2.4–5.2, documents/s:

| In flight | `orders` (50,000) | three indexes, one after another (62,000) | three indexes, interleaved (62,000) |
| ---: | ---: | ---: | ---: |
| 1 | 24,061 / 24,622 / 24,900 | 14,464 / 16,613 / 16,778 | 16,775 / 16,730 / 17,168 |
| 2 | 42,429 / 45,878 / 42,395 | 32,574 / 32,416 / 32,115 | — |
| 3 | — | — | 45,101 / 44,295 / 41,487 |
| 4 | 73,478 / 79,112 / 72,082 | 53,323 / 51,276 / 53,093 | — |
| 6 | — | — | 68,794 / 53,801 / 56,424 |
| 8 | 87,641 / 100,494 / 86,250 | 58,148 / 63,716 / 62,628 | — |
| 16 | — | 40,711 / 46,957 / 41,490 | — |

Heap of a built document against its JSON, per index, load average 22–24 (the allocator's overhead isn't counted, so these are lower bounds):

| Index | JSON | heap | factor |
| --- | ---: | ---: | ---: |
| `users` | 1,523 B | 16,076 B | 10.6× |
| `products` | 910 B | 7,394 B | 8.1× |
| `orders` | 367 B | 3,674 B | 10.0× |

What it says:

- OpenSearch takes overlapping bulk requests as well on one index as across several. 4 in flight gives about 3×, 8 gives 3.5–4×, and 16 is slower than 8. So `writes` is per sink.
- Postgres scales to 8 calls and flattens by 16, on cached data. Serially, writing takes about 2.3× the build time, as in #389.
- Budgets count heap bytes, not JSON. 256 `users` documents hold about 4 MiB.
- **Expected:** about 3× faster backfill with 4 builds and 4 writes. No index's backfill delays another index's live changes.

### Where resolution runs

Chosen: **once, on the reader.** The alternative is to publish the row change and let each index's task resolve its own documents.

| | On the reader (chosen) | Per index |
| --- | --- | --- |
| Work | one `resolve` per change | one per change per index reading the table |
| Correctness | the pre-image travels with the change | also correct: a later lookup reads later state, whose changes are in the WAL |
| Backlog of a stalled index | document ids, each held once | every row change with its pre-image, bounded by WAL |
| Independence | a slow lookup delays every index | it delays only its own index |

**What would flip it:** #387 showing the reader busy resolving most of a burst while index tasks wait. Even then, the first step is concurrent resolution on the reader, with marks kept in order. Per-index resolution pays only if one index's lookups dominate.

## 5. Bounds

**Postgres connections: 9, whatever the number of indexes.**

| Pool | Connections | Used by |
| --- | ---: | --- |
| builder | `builds` | `build_many`, key pages |
| resolution | 1 | reverse lookups, root-key reads |
| visibility, admin, replication | 1 + 2 + 1 | as today |

**The build gate** (`builds` permits; the sink's writer slots use the same rule):

- Live waiters go first.
- Every fourth grant goes to the oldest backfill waiter, if one waits. So a steady trickle across 24 indexes doesn't starve backfill, and `builds = 1` still runs one.
- A key page takes a backfill permit.

**Keyset backfill.**

- `WHERE key > $last ORDER BY key LIMIT 1024`, with `$last` bound through `sql::key_cast`, one page of an index's root keys at a time, up to the greatest key the walk read when it started.
- The page reads the bare key. Root filters and soft-delete stay in the build, which turns a filtered-out row into a tombstone, as the cursor did. A filter on the page would make the page's plan depend on the filter, and a raw filter can read anything (#386 stage 3).
- No shared snapshot is needed: the slot predates the backfill (#120), so every change made during it is in the WAL and rebuilt live. A page gives only ids: the document is read when it is built, and per document build order is lane order (section 2).

**Memory: four rules** over heap bytes of built documents not yet acknowledged by every lane:

1. All built documents together hold at most `max_built_mib`.
2. One index holds at most half of it.
3. Backfill, over all indexes, holds at most half of it.
4. An index that holds nothing is always granted one batch.

Plus one, for a lane parked by `on_error = stop` (section 6): the parked item's bytes are released when its fresh build is asked for. Without it, the fresh build would wait on the parked item's own bytes.

A build reserves its estimate (its ids × the index's running average) before it starts, and settles to the real size after. An index over its limit after settling reserves nothing more until it is back under. **The bound is `max_built_mib` plus one batch per index.**

Checked against:

| Case | Outcome |
| --- | --- |
| 1 index | half the budget unused. A live build may wait for one backfill write to be acknowledged (the gate serves live first). |
| 24 indexes, two lanes stalled | the two indexes hold up to the whole budget. Every other index still progresses one batch at a time (rule 4). Slower, no deadlock. |
| 200 indexes | the bound holds, but "one batch per index" can be large: 200 × ~4 MiB of `users` documents. Lower `max_built_mib` or `max_changes` for such a deployment. |
| one 5 MB document × 256 | granted alone by rule 4, once its index holds nothing. |
| two backfills behind a slow sink | they hold at most half (rule 3), so live builds keep the other half. |

**Pending ids:**

- `max_pending_documents` deployment-wide; past it the reader waits, and every index stalls.
- An id costs 150–250 B with its position (more for long keys).

**Realistic RSS with the defaults: about 0.8–1.1 GiB at worst, up to about 24 indexes.** 200 indexes add about 800 MiB (one batch per index).

| Part | Size |
| --- | --- |
| built documents | 512 MiB plus one batch per index |
| pending ids | up to ~250 MiB |
| bulk bodies in flight | a few MiB |
| read-ahead (`max_changes × 16` row images; bounded in count only, as today) | tens of MiB |
| allocator overhead | 10–20% |
| baseline | ~100 MiB |

**A stalled lane or index, in this version:**

- It pins the slot at its oldest outstanding position, as a stalled sink does today.
- `/status` and metrics give each lane's stall age and WAL behind; an alert fires past 10 minutes or 1 GiB.
- The other indexes keep going while memory allows. Once the pending cap or the built budget is reached, the reader waits and every index stalls, as backpressure does today.
- A busy index under a bulk update paces the reader the same way, and triggers nothing.
- The reader never waits on age or WAL: that doesn't stop the primary writing WAL.

A long stall has two costs:

- **Replay.** A restart replays the pinned WAL for every index.
- **Slot invalidation.** Past `max_slot_wal_keep_size`, the slot is invalidated and every index on every sink is rebuilt.

**Capping those costs by dropping and rebuilding one index is deferred to its own issue and design.** Three review rounds found holes in it; the next design starts from them:

- record the drop in the sink's meta before it takes effect, or a restart reads the sink as seeded and loses the changes made meanwhile (so an unreachable sink is never dropped);
- recover into a new generation, or documents deleted meanwhile survive;
- call a lane stalled when it has acknowledged nothing for a window, not when its pending count is high (a busy index is healthy);
- discard the lane's queued items, or they keep holding budget.

## 6. Failure and lifecycle

**An index task that fails** (a `stop` build failure, a query error) restarts alone, with backoff. Its in-flight ids go back to pending, and its backfill resumes from the last key it published.

**At the sink**, per lane:

| What happens | What the sink engine does |
| --- | --- |
| a document rejected under `skip` | drops it; the position advances, as now |
| a document rejected under `stop` | parks the lane, which retries with backoff while other lanes go on. Each retry asks the index task for a fresh build of the parked ids for that lane only; the parked position stays outstanding, and lane items stay immutable |
| an item answered 429 or 503 | retries it with backoff; it never reaches `on_error` |
| `index_not_found` | re-stages that `(index, sink)` at a lane barrier: `ensure_index`, a backfill request, then redelivery. This replaces today's sink-wide restart (#123) |
| a transport failure | fails every lane: the sink is down |

**Stop (#371):** the reader stops reading; index tasks publish their builds in flight and leave pending ids unconfirmed for replay; backfills are abandoned unacknowledged; sinks finish live items and skip backfill items; `Progress` confirms as acknowledgements arrive, then the stream closes.

**Staging (#120)**, unchanged in order: `continuity()`, then every sink staged, then `prepare()`, then the reader and index tasks start.

**Unchanged:** `--skip-backfill`, `backfill = false`, and the visibility hold (#343).

**Sinks:** the OpenSearch sink's per-flush state is made safe for concurrent `write(&self)`. The stdout sink holds its output lock for a whole `write`.

**Observability:**

- `/status` gains `indexes.<name>`: phase, pending, builds, bytes held, and per sink the stall age, WAL behind and items in flight.
- Metrics gain an `index` label.
- The dev dashboard and `dev/prometheus` get the panels and the stall alert.

## 7. The ports

| Port | Change |
| --- | --- |
| kernel `Position` | gains `fence_before`, `confirm_before`, `confirm_through`, opaque to the engine |
| `ChangeCapture` | `confirm` and `reached` take `Before(o) \| Through(p) \| Idle`; `entries` goes; `snapshot` goes; the daemon calls `prepare` |
| `Stream` | `lane(index, sink)`, `requests(index)`; `watermark` and `is_idle` move to `Progress` (supersedes ADR 0003's "the stream owns the watermark") |
| `LaneItem::Batch` | `index`, `envelopes`, heap `bytes`, `oldest: Option<Position>`; `SnapshotComplete` names one index |
| `Request` | `Backfill { sink }` per index lane, plus a fresh build of ids for one lane |
| `DocumentBuilder` | already shareable; gains `keys(index, after, limit)`; `Document::Upsert` gains its heap size; a second pool for resolution |
| `Sink` | `write(&self, &[Envelope], caught_up) -> WriteReport` replaces `apply` + `flush` (stage 2); concurrent calls allowed; busy items retried inside the write, never reported. `caught_up` makes every write that returned before the call searchable, not only its own documents. The `index` argument comes with per-`(index, sink)` lanes (stage 7): until then a lane item spans indexes |
| channel adapter | releases per ticket, not by moving a cursor back (today it re-serves items still being written); lanes bounded by bytes alone |
| `/wait`, `flusso-query` | `index` on `/wait`; `Client::after` passes its index; the fence cache keyed by `(level, index)` |

## 8. Guards

**Ordering** (mock builder returning "database version at build start", random finish order; recording sinks with random delay):

- `a_document_is_never_built_or_written_twice_at_once`
- per id and sink, versions written never go down

**Confirmation and fences:**

- `a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`, with a keepalive in between
- `the_tick_releases_idle_wal_while_routed_stays_put`
- `a_fresh_process_never_confirms_its_first_unrouted_change`
- `an_idle_stream_after_an_ingest_restart_still_releases_wal`
- `a_later_build_finishing_first_confirms_nothing_past_an_earlier_one` (stage 4)

**Seeded random** (`FLUSSO_SCHEDULE_SEED`):

- random delays, backfills, reindexes, 429s, a deleted generation;
- tasks killed at random;
- at every confirmation and fence answer, every covered change is on the named sinks at its version or newer;
- at the end, every sink equals the table.

**Live, against Postgres and OpenSearch**, `builds = 8`, `writes = 8`:

- Backfill 50,000 rows while writers bump a counter on random rows, delete some and re-key some. Per id, counters never go down. The end state equals the table.
- `live_latency_during_a_backfill_stays_within_one_build_and_one_write`

**Fixes, failures and bounds:**

- `a_released_ticket_redelivers_only_itself`, `an_item_refused_with_429_is_retried_not_rejected`, `a_partly_refused_item_admits_no_newer_build_of_its_ids`
- `an_idle_index_never_pins_the_slot`, `a_failed_index_holds_up_no_other_index`
- `a_stalled_lane_never_deadlocks_the_budget`, `a_parked_lane_with_a_full_budget_unparks`, `an_oversized_batch_is_granted_alone`
- `backfill_never_blocks_a_live_build`, `backfill_gets_permits_under_a_steady_trickle`, `live_goes_first_with_one_build_and_one_write`, `a_trickle_is_built_one_batch_at_a_time`
- `a_missing_generation_on_one_lane_is_rebuilt_while_the_others_write`, `a_restarted_backfill_resumes_from_its_last_key`
- `positions_stay_bounded_while_one_index_is_stalled`; a 24-index backfill holding at most 9 Postgres connections

**Client:**

- `a_fenced_client_waits_again_for_another_index`
- a union fence marks each of its indexes
- old and new client against old and new daemon

**Rewritten:** `wal_release`'s empty-batch guard. **Kept:** the `fence`, `shutdown`, `visibility`, `pipeline` and `backfill` e2e.

## 9. Stages

Each stage leaves `main` correct and is measured on `reference` before and after.

1. **Fixes any concurrency needs:** item 429/503 retried, with the lane item unfinished until written; the channel releasing per ticket.
2. **Concurrent writes per sink**, the measured bottleneck: about 2–3× on a backfill.
   - `Sink::write`, safe for concurrent calls;
   - per-id admission and live-first selection on the sink's one lane;
   - the lane watermark taken before its oldest unacknowledged item (from the predecessor recorded at delivery), with fence reports following it;
   - in-flight writes awaited on a restart.

   Settled in its PR (#386 stage 2):
   - **The window.** The sink engine takes an item from the lane only to use it: for a free write slot, or, with only backfill waiting, to find a live item behind it, up to 32 waiting. The channel's ledger holds delivered items outside the lane's bound, so a sink holds at most `writes + 32` delivered items beside its lane's `capacity`. Live-first therefore sees only the window until stage 7. Reading ahead regardless (the first version) cost the in-process bench at `writes = 1`: an item read early was acknowledged after the ingest engine had dropped its copy, so the sink freed the batch while the next one was built.
   - **Every fourth grant** at the writer slots counts only *contested* grants (live and backfill both admissible). Uncontested grants don't move the counter.
   - **Caught up** is decided at grant: nothing else is queued or waiting on the lane. A caught-up write makes every write that returned before it searchable. When the engine goes idle with a written position not yet searchable, it sends an empty caught-up write. With several writes in flight, the last one granted isn't the last to finish, and the default `refresh_interval` is 10 s.
   - **A failed write** stops new grants; the writes in flight are awaited, the successful ones acknowledged, then the error ends the run.
   - **Admission hashes lazily.** An item's document hashes are computed only when a comparison is needed. With nothing in flight and one kind waiting, the front item is granted as it is, so `writes = 1` with live traffic hashes nothing. Hashing every document on the sink's task doubled the in-process `live_drain` bench at `writes = 1`.
   - **Queued items are not merged into one write (#397).** Measured on `reference` at default scale, three runs each, with #409's per-stage timings: in a burst the ingest task's `resolve` is busy 0.71 of the wall at every `writes` setting, while the sink is idle 0.25–0.30 of it. The burst is bound by resolution on the ingest side (#387), not by the number of writes, so merging has nothing to win. An earlier run of the merge at `writes = 4` moved the burst drain 2,069 → 2,156 changes/s (within the spread) and the writes per burst 157 → 152. Revisit with per-`(index, sink)` lanes (stage 7), whose items are one index's and smaller.
   - **Measured** (`main` with #409 against this stage, release builds, load average 4–18 on 24 cores, medians of three): backfill 9,270 → 13,171 documents/s at `writes = 2` (12,910 at 4, 11,942 at 8, 9,185 at 1); the sink's `write` runs 1.6, 3.1 and 4.9 deep at 2, 4 and 8, and the ingest task's `idle` falls from 0.38 to 0, so the build on the one ingest task is now the backfill's bottleneck (stage 4). Peak RSS 251 → 131 MiB at `writes = 4`: the sink keeps up, so built batches no longer pile up in the lane. Burst drain (2,225 → 2,235 changes/s at 4) and live latency p99 (38.6 → 31.0 ms) unchanged within the spread.
3. **Keyset backfill on the builder pool:** no long transaction, one connection fewer. It doesn't depend on stage 2, so concurrent writes land first.

   Settled in its PR (#386 stage 3):
   - **The port.** `ChangeCapture::snapshot` and `SnapshotTable` go. `DocumentBuilder` gains `keys(index, after, limit)` and `backfill_indexes()`, both required, so a decorator can't silently seed with nothing. The engine holds the cursor (the last key it read), so 8b only has to store it.
   - **One index at a time, from its own root.** Pages are per index, not per root table: two indexes on one table each read it (one key scan more, about 0.3 s on `reference`). That also stops the backfill routing rows by table name alone, the backfill half of #390.
   - **`ORDER BY key USING <`**, not a plain `ORDER BY`. The `>` of the next page resolves its operator through the search path, the index's order doesn't: for a type whose operators are off the path (`citext` in `ext`), a plain `ORDER BY` walked in `citext` order and compared as `text`, and skipped rows. `USING <` makes both the same operator; where it is the index's own (every other case) the plan is unchanged.
   - **The bare key on the page.** Filters and soft-delete stay in the build (section 5).
   - **The same roots as the cursor read:** a single-column catalog primary key that is the declared one. #391's roots still get nothing.
   - **A walk is bounded** (review of stage 3). It reads the index's greatest key once when it starts (`DocumentBuilder::last_key`) and pages up to it (`keys`' `through`). A row whose key is past the bound got that key after the walk started, so after the slot existed: an insert, or a key changed to past it, reaches the live stream. A row at or below it is paged, or changed later and delivered live. The bound is compared by value, so deleting its row mid-walk still ends the walk. Unbounded, writers inserting greater keys kept the walk going for N/(B − R) pages, never ending when they kept pace, and built every row they added twice.
   - **The bound sits in the select list** (`(key <= $2) AS within`), not the `WHERE`: as a second range condition, a generic plan took `key > $1 AND key <= $2` for a small range and switched to a bitmap scan and a sort on every page.
   - **The key index's own operators.** `<`, `>` and `<=` are read from the primary key index's opclass and written schema-qualified (`OPERATOR(ext.<)`), so the index serves the order whatever the search path. A role that may not use their schema gets the bare names with `USING <`: correct, but every page sorts the rest of the table, and a warning says so once per index.
4. **The build gate and concurrent builds on the one ingest task.** Builds are published in position order (a small reorder buffer), because stage 2's watermark still trusts lane order.
5. **The memory budget:** the global limit, the backfill half and the one-batch grant. The per-index half lands with stage 8a.
6. **Self-describing positions and `Progress`,** still with one lane per sink. `entries` goes; the tick always confirms.
7. **Per-`(index, sink)` lanes, fences and caught-up;** per-index requests; `/wait?index` and the client cache.
8. **Index tasks:**
   - 8a. The reader, pending sets, index tasks, the resolution connection, and the per-index budget half.
   - 8b. Per-index restart and backfill resume.
   - 8c. Stall age, WAL behind, and the alert.
9. **Per-lane failure:** parking with fresh builds; re-staging on `index_not_found`.

**Beside the code**, in the stage that changes each: `CLAUDE.md` and `CONTEXT.md`; ADR 0003 (superseded in part) and 0009 (amended); the Reference pages, `contribute/pipeline.md` and the query fence chapter; the upgrade page; `config.schema.json`; the designer (`ConfigPanel.tsx`, every locale, dist); the `flusso-query` README and `plugin/**`; the Helm chart, Grafana, `dev/prometheus`, `dev/bench`'s `/metrics` parsing and the `engine` bench baseline.

**Settled in the stage's PR, not here:**

- the build gate's counter (does it reset when no backfill waits);
- the reorder buffer's shape;
- `Progress`'s lock layout, and the atomic move from pending to in flight;
- the `/wait` index for a raw `Client` request;
- the keyset SQL;
- `Retry-After` on a re-staging lane;
- metric names, and the Helm values beyond question 8;
- a build's set of lanes, taken when it publishes, not when it starts;
- what an index-task failure does in 8a before 8b exists, and whether pending sets survive it;
- where unpositioned backfill pages sit in stage 4's reorder buffer;
- which stage removes the item-count bound on lanes;
- the overshoot bound with up to `builds` reservations at once;
- `confirm_before` defined as the horizon at emission;
- the channel ledger keeping a parked item's `Arc` (its accounting released, its memory not);
- which backoff the 429 retry and the parked-lane retry use.

**Later, when a measurement asks:**

- skipping indexes that already landed a replayed change after a reader restart;
- retrying a failed reverse lookup in place;
- a slow-down of `writes` after 429s;
- concurrent resolution on the reader (#387).

## 10. Open questions for the owner

The reviewers' recommendation is the proposed default.

1. **`builds = 4`, `writes = 4`.** *Yes.* Cost: 8 measured faster (5× and 3.5–4×), but on cached data, so that speed is left to the setting.
2. **A stalled lane pins the slot and alerts; dropping and rebuilding it is a later design.** *Yes.* Cost: a long stall costs replay and, past `max_slot_wal_keep_size`, a full rebuild, as today.
3. **Fences per `(index, sink)`.** *Yes.* Cost: a `/wait` parameter and a query-train release.
4. **Item 429s retried as transient, plain retry first.** *Yes.* Cost: a document refused with 429 for good retries for ever, and the stall alert shows it.
5. **At most two live builds per index, a second only for a backlog.** *Yes.* Cost: a hot index uses at most two of the `builds` permits for live changes.
6. **A parked lane asks for a fresh build of its ids.** *Yes.* Cost: one more request kind on the index task.
7. **A stop leaves unbuilt ids for replay.** *Yes.* Cost: the next start rebuilds them.
8. **Helm memory defaults.** Today `resources: {}`. *Ship `requests.memory: 512Mi`, and leave the limit commented out with the formula beside it* (`max_built_mib` + one batch per index + pending + ~100 MiB). Cost: no OOM-kill protection by default; a `1536Mi` limit could OOM-kill a deployment with many indexes or large documents that runs today.

## Settings

All in `flusso.toml`, never baked into the lock. Flag beats env, env beats file.

| Setting (flag / env) | Default | Bounds |
| --- | ---: | --- |
| `[concurrency] builds` (`--builds` / `FLUSSO_BUILDS`) | 4 | builds and key pages at once; the builder pool |
| `[concurrency] writes` (`--writes` / `FLUSSO_WRITES`) | 4 | bulk requests in flight per sink |
| `[concurrency] max_built_mib` (`--max-built-mib` / `FLUSSO_MAX_BUILT_MIB`) | 512 (64 was tried and reverted, `stage-5.md` "The default") | heap of built documents not yet acknowledged |
| `[concurrency] max_pending_documents` (`--max-pending-documents` / `FLUSSO_MAX_PENDING_DOCUMENTS`) | 1,000,000 | pending ids before the reader waits |
| `[batch] max_changes` (existing) | 256 | documents per build and per write |
| `[batch] max_delay_ms` (existing) | 50 | now only how long backfill requests coalesce |
| `[stream] capacity` (existing) | — | deprecated: still loads, ignored, warns |
| `max_restart_backoff` (existing) | 60 s | restart backoff of a failed task |

## Fixed constants

| Constant | Value | Where |
| --- | ---: | --- |
| confirm tick | 1 s | ingest side; always confirms |
| backfill share at the build gate and writer slots | every 4th grant | section 5 |
| one index's share of `max_built_mib` | half | section 5 |
| backfill's share of `max_built_mib` | half | section 5 |
| live builds in flight per index | 1, a 2nd while ≥ `max_changes` pending | section 1 |
| keys per backfill page | 1,024 | section 5 |
| documents per `build_many` query | up to 512 | as today |
| read-ahead | `max_changes × 16` changes | as today |
| wait for writes in flight on a sink restart | up to the request timeout | section 2 |
| stall alert | 10 min or 1 GiB of WAL | `dev/prometheus` rule |
