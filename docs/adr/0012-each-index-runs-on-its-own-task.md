---
status: proposed
---

# Each index runs on its own task; order is kept per document

Today one ingest task builds every index in turn, and each sink writes one batch at a time (issue #386). A backfill of one index holds up live changes of every other index. On `reference` the sink's flushes take about twice the build time, while the cluster could take four requests at once.

**Decision.**

- **One reader task** reads changes and resolves each one once, on a connection of its own. It marks the documents the change touches as *pending* on their index.
- **One task per index** builds that index's pending documents and its backfill in batches. It publishes each built batch to one lane per `(index, sink)`.
- **One task per sink** writes its lanes with several bulk requests in flight, live items first.
- **Order holds per document, not per lane.** A document is never built twice at once and never written twice at once. A later build of it reaches every sink after an earlier one. Nothing else is serialized, and sinks need no versioned writes.
- **A position describes itself.** It carries what confirmation and fences need, so the source keeps no map of every change. The engine's `Progress` finds the oldest change some index hasn't landed on some sink, and the slot is confirmed up to just before it.
- **Fences and "caught up" are per `(index, sink)`.** A stalled index holds up no `/wait` and no refresh of another index.
- **An index stalled past a set limit is dropped and rebuilt**, so one index can't pin the slot for ever.

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
- Resolves each change once with `DocumentBuilder::resolve`, on a resolution connection of its own (section 5). A failed reverse lookup is retried in place, without reopening the stream.
- Marks every resulting id pending on its index, with the change's position, then records the change as *routed* in `Progress`. A change that resolves to nothing is landed once routed.
- After the reader restarts within a running process, a replayed change is not marked on an index whose lanes have all landed past its commit. `Progress` outlives the reader, so the replay costs index I nothing it already has.
- Waits only when the deployment's pending budget is full (section 5). It never waits on a lane.

**Pending set** (one per index; shared state, not a queue).

- A map from document id to the *oldest* position not yet built for it. A second change to a pending id adds nothing: a build reads the database as it is now, so one build after the newest change covers every earlier one.
- A mark is unconditional, even for an id whose build is in flight, because that build may have read before this change.
- A build that fails gives its ids back with `min(old, current)` as their oldest position.

**Index task** (one per enabled index).

- Takes pending ids, oldest first, up to `max_changes` per build, skipping ids whose earlier build is still in flight.
- **One live build in flight; a second only while at least `max_changes` ids are pending.** A trickle stays one build of whatever piled up, so a burst still batches itself.
- Builds with `build_many` on the shared builder, within its memory share (section 5), and publishes the batch to every lane of the index.
- Runs the index's backfill: reads root keys in keyset pages and builds them like pending ids, publishing to the lanes that asked.
  - A page drops ids that are pending or in flight live, since their live build goes to every lane.
  - It remembers the last key it published, so a restart resumes from there. The request stays unacknowledged until `SnapshotComplete`, as today.
- Also builds a fresh copy of given ids for one lane through the same path, when that lane asks (section 6).

**Sink engine** (one per sink, as today).

- Reads every lane of its sink and keeps up to `writes` bulk requests in flight. Each write is one lane's item, so one index's documents.
- **It picks the next item in this order:**
  1. Admissible live items before admissible backfill items.
  2. Within each kind, lanes take turns.
  3. Within a lane, the earliest admissible item first.

  "Admissible" is the per-id rule of section 2. So a live change waits for at most one write per writer slot, never for a lane's queue of backfill pages.

**A change touching several indexes** is read and resolved once, on the reader. Its ids are split by index into the pending sets, and each index builds only its own.

**Backfill.**

- A sink asks per index (`Request::Backfill { sink }` on that index's request lane).
- The index task coalesces requests from several sinks as today, pages the keys, builds them and publishes to the requesting lanes. Then it publishes `SnapshotComplete` and acknowledges the requests.
- Two indexes rooted at one table each page that table's keys. Keys cost 0.3 s on `reference`, not worth coupling the tasks.

### Why a task per index building batches, not one per document

The owner's framing: the reader publishes that something changed, the interested builders wake up, build in parallel and publish. That is this shape. The open choice is the builder's unit of work.

`build_many` fetches up to 512 documents in one query, so the joins run once per batch instead of once per document. Measured over all three `reference` indexes (62,000 documents), same machine, load average 22–26, three runs, in documents/s:

| | 1 caller | 8 callers | 32 callers |
| --- | ---: | ---: | ---: |
| one document per `build_many` | 1,079 / 1,056 / 3,328 | 8,091 / 9,515 / 14,085 | 16,255 / 19,474 / 18,651 |
| 256 documents per `build_many` | 11,005 / 14,087 / 17,809 | 78,965 / 62,945 | — |

So 32 per-document builds on 32 connections do about as well as one batched caller on one connection. 8 batched callers do 4× better than the 32. A task per document would also lose the deduplication of a burst. And it would need a connection, or a queue in front of the pool, which is the batching back again.

## 2. Ordering

> **For each document, builds never overlap, and a later build reaches every sink after an earlier one.**

**Why that is enough:** `build_many` reads the current, visible state. A build that starts after another ended sees at least what the first saw: Read Committed takes a new snapshot per statement, and on the primary a later snapshot sees a superset. So per document, build order is state order. Builds of different documents can land in any order.

**One admission rule at each stage:** an item starts only when no earlier unfinished item shares a document id with it.

| Stage | Items | "Finished" means |
| --- | --- | --- |
| Index task | pending batches, backfill pages, fresh builds for one lane | built and published to every lane it goes to |
| Sink engine, per lane | lane items | every document in it written, or rejected for good |

How the cases come out:

- **Backfill and live changes of one index.** They run in either order or at once if they share no id, and a page skips ids that live will cover.
- **An item partly answered 429 stays unfinished** until its retried documents are written. Otherwise a newer build of the same id could start and the stale retry would overwrite it.
- **An index task restarts.** Ids taken but not published go back to pending; published ones are in the lanes.
- **A sink engine restarts, or an item is redelivered.**
  - First, writes in flight are waited for, up to the request timeout.
  - Then the lane redelivers only its unacknowledged items, in lane order.
  - Admission applies again.
- **The reader restarts.** The stream reopens at the confirmed position and marks ids again; that costs rebuilds, never a wrong document.
- **A reindex of one sink while another sink of the index is live.** Pages go to the reindexing lane only, live batches to both. Staging the new generation is a barrier on that lane: a write in flight picked its generation before the swap.
- **`SnapshotComplete`** is a barrier on its lane. It starts when everything before it has finished, and nothing after it starts until `mark_seeded` returns.

**Residual risk, as today.** A bulk request the client gave up on (it timed out) can still run on OpenSearch and land after a newer build of the same id. Waiting out in-flight writes on a restart narrows this; it doesn't close it. Only versioned writes would close it, and they were rejected.

**Versioned writes: rejected.**

- *Build sequence as an external version.* Start order isn't snapshot order once two connections race.
- *Postgres snapshot.* Not a total order.
- *Change position.* A backfill build has none, and two builds after one position can see different states.
- *Cost.* Every operation would carry `version_type=external`, and a delete keeps its version only for `index.gc_deletes` (60 s). After that, an older upsert brings the document back.

## 3. Confirmation and fences

> **Nothing is confirmed or fenced past a change the capture has emitted until every lane that needs it has landed it.**

**A position describes itself.** Today `Positions` (`cdc/ack.rs`) keeps one `entries` row per emitted change until it is confirmed. A full lane stops the reader, which keeps that map small. A reader that doesn't wait on lanes would let it grow behind a stalled index. So the capture stamps each position as it emits the change; it knows the commit boundaries then, because the visibility hold keeps whole transactions:

- `fence_before`: the change's registered commit LSN (its end LSN, as registered today) − 1.
- `confirm_before`: the newest commit LSN registered before this change's commit.
- `confirm_through`: this change's commit LSN once it is the commit's last change, else `confirm_before`.

**What the capture keeps:**

- `next_seq`, still needed across a reopen.
- The horizon (keepalives, filtered commits; moved only while nothing is buffered, as today).
- **The last emitted change's seq and stamps.** Updated under the same lock as the horizon when a change is emitted.
- The confirmed LSN. `read_through` no longer moves it on its own.

**How `confirm(p)` and `reached(p)` answer:**

| What is outstanding | Confirm to | Fence at |
| --- | --- | --- |
| a change `o` is outstanding | `o.confirm_before` | `o.fence_before` |
| nothing outstanding, and `p` is the last emitted change | the live horizon | the live horizon |
| nothing outstanding, but `p` is older than the last emitted change | `p.confirm_through` | `p.confirm_through` |

**Why the last-emitted check.** Without `entries`, the horizon alone races. T commits at LSN 500 touching index I, the stream emits it and the horizon moves to 500. But the reader hasn't taken T from the read-ahead yet, and lane I is idle, so *routed* is the change before T. A `/wait` on I for 500 would get the horizon and pass, and its read would miss T. The tick would confirm 500, and a crash would lose T. Using only the routed change's stamps would be safe, but keepalives and filtered commits would then stop releasing idle WAL (#111/#204), and an idle fence would never pass. The live horizon is used only when the reader has routed everything the capture emitted.

**`Progress`** (engine crate, one per deployment, outlives every task, like `Status`):

- *Routed:* the newest routed position.
- *Outstanding per `(index, sink)`:* a multiset of positions. It holds the oldest position of each pending id, of each build in flight, and of each lane item not yet acknowledged.
- *Landed for a lane:* its oldest outstanding position, or *routed* when nothing is outstanding. Confirmable is the minimum over lanes, handed to `confirm`.
- **Consistency:** the reader marks the ids, then records *routed*, under one lock. A reader of `Progress` never sees *routed* past a change whose ids aren't in the multisets yet.
- **Cost:** each lane keeps its minimum incrementally (a `BTreeMap` of counts). The ingest side confirms once per routed commit and on the one-second tick, as today, not once per marked change.
- **Memory:** one entry per pending id, build in flight and lane item, bounded by the budgets (section 5), not by WAL.

**Batches and lanes.**

- A lane item carries the oldest position among its ids. Acknowledging it removes that position from its lane's multiset.
- **Empty batches disappear.** An idle index never pins the slot. The `wal_release` guard that relies on empty batches is rewritten to assert the same release bounds without them.
- Backfill items carry no position, as today.

**Fences per `(index, sink)`** (ADR 0009, at a finer grain):

- *Applied* for a lane comes from the table above. *Searchable* is the same over items whose write has become searchable.
- `/wait` gains `index` (repeatable). Without it, it waits on every index, as today. Both directions stay compatible: an old daemon ignores `index`, and an old client gets every index.
- `Client::after` passes its root's index. `_msearch` and a combined search pass the union of their roots.
- The client's once-per-level cache (`passed: AtomicU8` in `sdk/query/src/fence.rs`) is keyed by level and index set.
- A client whose root's index is disabled in the deployment gets `FenceRefused` (404, unknown index).
- **"Caught up" is per lane.** A write is caught up when its lane has nothing else queued or in flight. A `Deferred` refresh of an index runs when that index's own lane empties.

## 4. Where concurrency pays

From #389, on `reference` under load ≈32: key fetch 0.3 s, build 5.8–7.4 s, flushes 12.6–16.8 s. The sink sets the pace.

**Prototype.** A throwaway example, not committed. It loaded the `reference` config and seeded data (`default` scale), then:

- called `PgDocumentBuilder::build_many` with *N* concurrent callers on one pool;
- built every document once, then wrote the batches through *N* `OpensearchSink`s into fresh indexes;
- counted a built document's heap with a counting allocator.

Setup: Postgres 16 and OpenSearch 2 (single node, 1 GiB heap, one shard per index), local containers, data in cache, 24 cores, three runs each.

`build_many`, 256 keys per call, all three indexes (62,000 documents), **load average 2.4–5.2**, documents/s:

| Concurrent calls | run 1 | run 2 | run 3 | vs 1 |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 42,320 | 38,743 | 32,900 | 1× |
| 2 | 67,939 | 53,276 | 58,892 | 1.5× |
| 4 | 112,959 | 121,032 | 112,787 | 3.0× |
| 8 | 191,783 | 190,468 | 188,317 | 5.0× |
| 16 | 243,791 | 247,853 | 250,283 | 6.4× |

Per index, same load:

- `users` (10,000 documents): 13–15k → 57–65k at 8.
- `orders` (50,000 documents): 52–69k → 300–325k at 8.

The single-caller figures in section 1 (11–18k) are lower because they were taken at load 22–26.

Bulk writes, **load average 2.4–5.2**, documents/s:

| In flight | `orders` only (50,000) | all three, index after index (62,000) | all three, interleaved (62,000) |
| ---: | ---: | ---: | ---: |
| 1 | 24,061 / 24,622 / 24,900 | 14,464 / 16,613 / 16,778 | 16,775 / 16,730 / 17,168 |
| 2 | 42,429 / 45,878 / 42,395 | 32,574 / 32,416 / 32,115 | — |
| 3 | — | — | 45,101 / 44,295 / 41,487 |
| 4 | 73,478 / 79,112 / 72,082 | 53,323 / 51,276 / 53,093 | — |
| 6 | — | — | 68,794 / 53,801 / 56,424 |
| 8 | 87,641 / 100,494 / 86,250 | 58,148 / 63,716 / 62,628 | — |
| 16 | — | 40,711 / 46,957 / 41,490 | — |

Heap held by a built document against its JSON text, **load average 22–24**:

| Index | JSON per document | heap per document | factor |
| --- | ---: | ---: | ---: |
| `users` | 1,523 B | 16,076 B | 10.6× |
| `products` | 910 B | 7,394 B | 8.1× |
| `orders` | 367 B | 3,674 B | 10.0× |

The allocator's own overhead isn't counted, so these are lower bounds.

What it says:

- **OpenSearch takes overlapping bulk requests as well on one index as across several.** 4 in flight give about 3×, 8 about 3.5–4×, and 16 is slower than 8. The limit is the cluster's, so `writes` is set per sink.
- **Postgres scales to 8 and flattens by 16** on cached data.
- Serially, writing takes about 2.3× the build time, as in #389. With 4 writes in flight, the sink drops below the build.
- **Budgets count heap, not JSON:** 256 `users` documents hold about 4 MiB.
- **Expected:** a backfill on `reference` about 3× faster with 4 builds and 4 writes. Another index's backfill no longer delays live changes.

### Where resolution runs

The owner's framing suggests the alternative: the reader publishes the row change, and each index's task resolves its own affected documents.

| | Resolve once on the reader (chosen) | Each index resolves its own |
| --- | --- | --- |
| Work | one `resolve` per change | one per change per index reading the table |
| Correctness | the pre-image travels with the change | also correct: the pre-image travels too. A later lookup reads later state, and that state's own changes are in the WAL |
| Backlog of a stalled index | document ids, each held once | every row change with its pre-image, bounded by WAL |
| Independence | a slow reverse lookup delays every index | a slow lookup delays only its index |

**What would flip it:** #387 showing the reader busy resolving most of a burst while index tasks wait. Even then, the first step is concurrent resolution on the reader: a small resolution pool, with marks still made in order. Moving resolution to the index tasks pays only if one index's lookups dominate the reader.

## 5. Bounds and settings

**Postgres connections.** Today: builder pool (sqlx default 10, about one in use), backfill pool 2 (one held in `REPEATABLE READ` for a whole snapshot), visibility 1, admin 2, replication 1.

| Pool | Connections | Used by |
| --- | ---: | --- |
| builder | `builds` (4) | `build_many`, backfill key pages, column-type lookups during builds |
| resolution | 1 | `resolve`'s reverse lookups and root-key reads |
| visibility | 1 | as today |
| admin | 2 | as today |
| replication | 1 | as today |
| **total** | **9** | whatever the number of indexes |

**The build gate.** A gate of `builds` permits, with two queues:

- **Live first, but backfill isn't starved.** A freed permit goes to a live waiter, except every fourth grant, which goes to the oldest backfill waiter if there is one. So backfill gets at least a quarter of the permits under a steady trickle across 24 indexes, and runs at all with `builds = 1`. The sink's writer slots use the same rule.
- **A key page takes a backfill permit.**
- tokio's `Semaphore` has no priority, so this is a small custom gate.

**Keyset backfill.**

- Pages are read with `WHERE key > $last ORDER BY key LIMIT 1024`, with `$last` bound through `sql::key_cast`.
- The root filters and the soft-delete predicate apply on the page, as on the cursor today.
- A root with no index on its key sorts each page, which costs a scan per page. `check` warns about it.
- **No shared snapshot is needed** (the issue's question 5):
  - The slot exists before any backfill (#120), so every insert, delete and key change during the backfill is in the WAL and rebuilt live.
  - A row that exists for the whole backfill is reached by the walk.
  - Documents are built from the current state anyway.

**Memory: the built budget** (`max_built_mib`, default 512, counted in heap bytes).

> **A stalled lane pauses only its own index.** Every index keeps a share no other index can take, and live keeps a reserve that backfill can't take.

- **Split:**
  - half the budget is divided equally among the indexes as guaranteed shares;
  - the other half is shared, and one index may hold at most half of it.
- **Live reserve:** within an index's guaranteed share, half is live-only. Backfill uses the rest of its share and the shared half.
- **A stalled lane** (parked, failing, or not acknowledging for 30 s): its index can't take from the shared half. What it holds stays until acknowledged. That index stops building once its share is full, and its changes wait as pending ids. Other indexes may run slower without the shared half, but never stop.
- **Reservations:**
  - Each build reserves `ids × the index's running average` before it starts.
  - An index's first build reserves its whole free share.
  - When `max_changes ×` the average doesn't fit the share, the index builds smaller batches.
  - A reservation larger than all that is free is granted when the index holds nothing, like a lane admitting one oversized item.
- **Overshoot:** settling can exceed a reservation. The excess is held, and the index's next reservation waits until it is back under its limit. So the overshoot is at most one batch per running build.
- **Counted once:** a batch is counted until every lane holding it has acknowledged it, since every lane holds the same `Arc`.

**Memory: pending ids** (`max_pending_documents`, default 1,000,000, deployment-wide). An id costs 150–250 B with its position, more for long keys (ids up to 512 B). Past the cap, the reader waits.

**What the reader's wait means.** A stalled index with wide churn keeps adding pending ids. It eventually fills the deployment-wide cap, and then the reader stops: every index stalls. That is the point where the drop below steps in, before the cap is reached.

**Bounded elsewhere:**

- per sink, `writes` bulk bodies in flight (about one batch's JSON each, 0.1–0.4 MiB here) and their responses;
- the read-ahead, `max_changes × 16` changes, whose row images are bounded in count but not in bytes (as today).

**Realistic RSS with the defaults:** about 0.8–1.1 GiB at worst.

| Part | Size |
| --- | --- |
| built budget plus overshoot | 512 + ~16 MiB |
| pending | up to ~250 MiB |
| write bodies | a few MiB |
| read-ahead | tens of MiB with wide rows |
| allocator overhead | 10–20% |
| baseline | ~100 MiB |

The Helm chart gets defaults to match: `requests.memory: 512Mi`, `limits.memory: 1536Mi` (today `resources: {}`).

**A stalled index** (a failing task, a parked or failing lane, or a slow sink) pins the slot at its oldest outstanding position. The costs:

- **WAL on the primary.** If `max_slot_wal_keep_size` invalidates the slot, every index on every sink is rebuilt.
- **Restart replay.** A process restart replays all the WAL the stalled index pins, re-resolved one change at a time, for every index. Within a running process, the replay skip in section 1 avoids rebuilding what other indexes already have.

The guard against both:

- **Alert:** `/status` and metrics report, per `(index, sink)`, the age of the oldest outstanding position and the WAL behind it. A `FlussoIndexStalled` alert fires past 10 minutes or 1 GiB.
- **Drop and rebuild:** past `drop_stalled_after` (default 1 hour) or `drop_stalled_wal_mib` (default 8 GiB), the stalled `(index, sink)` lanes are dropped.
  - Their pending ids and outstanding positions leave `Progress`, so the slot moves on.
  - Each dropped lane is marked for rebuild. Once it recovers, the sink stages a reindex (a new generation) and a backfill, so deletes made meanwhile are not missed.
  - `/status` shows the lane as `dropped` until it is seeded again.
- **The reader never waits on age or WAL.** Stopping the reader doesn't stop the primary writing WAL; it would only stall the healthy indexes too.

**Settings.** In `flusso.toml`, never baked into the lock, like `[batch]`. Flag beats env, env beats file.

| Key | Flag / env | Default | What it bounds |
| --- | --- | ---: | --- |
| `[concurrency] builds` | `--builds` / `FLUSSO_BUILDS` | 4 | builds and key pages at once; the builder pool size |
| `[concurrency] writes` | `--writes` / `FLUSSO_WRITES` | 4 | bulk requests in flight per sink |
| `[concurrency] max_built_mib` | `--max-built-mib` / `FLUSSO_MAX_BUILT_MIB` | 512 | heap of built documents not yet acknowledged by every lane |
| `[concurrency] max_pending_documents` | `--max-pending-documents` / `FLUSSO_MAX_PENDING_DOCUMENTS` | 1,000,000 | pending ids before the reader waits |
| `[concurrency] drop_stalled_after_secs` | `--drop-stalled-after-secs` / `FLUSSO_DROP_STALLED_AFTER_SECS` | 3600 | stall age before an `(index, sink)` lane is dropped and rebuilt; `0` never |
| `[concurrency] drop_stalled_wal_mib` | `--drop-stalled-wal-mib` / `FLUSSO_DROP_STALLED_WAL_MIB` | 8192 | WAL behind before the same; `0` never |

Other keys:

- `[batch] max_changes` keeps its meaning: documents per build and per write.
- `[batch] max_delay_ms` now only bounds how long backfill requests coalesce.
- `[stream] capacity` is deprecated: it still loads (format freeze), is ignored, and warns.

## 6. Failure and lifecycle

**An index task that fails** (a `stop` build failure, a query error) is restarted alone, with backoff from 1 s doubling to `max_restart_backoff`.

- Ids in flight go back to pending, and its backfill resumes from the last published key.
- Other indexes keep going. Its outstanding positions pin the slot until it recovers, or until the drop.

**On the sink side**, a write carries one lane's documents.

- **`skip`** drops the rejected document and the position still advances, as now.
- **`stop` parks the lane.** It is retried with backoff while the sink's other lanes go on. Each retry first asks the index task for a fresh build of the parked ids for that lane only, through the path a backfill page uses. The parked item's position stays outstanding until the fresh build is written, so lane items stay immutable.
- **A 429 or 503 on an item is transient.** The item is retried with backoff in a later request and never reaches `on_error`, and the lane item stays unfinished until it is written. A request-level 429 is already retried by the transport. A slow-down of `writes` is added only if a measurement shows retries alone fall short.
- **`index_not_found` re-stages that index on that sink.** This replaces today's flush-wide restart (#123, `generation_deleted_while_running_fails_the_flush_and_is_rebuilt`). The sink engine does three things at a barrier on the lane:
  1. `ensure_index` recreates the generation and retracts the seed marker;
  2. it sends a backfill request for that `(index, sink)`;
  3. it redelivers the lane's items into the new generation.

  A lane that only retried would never recover.
- **A transport failure** fails every lane of the sink, which is the sink being down.

**Stop (#371).**

1. The reader reads nothing more.
2. Index tasks finish the builds in flight, publish them and take nothing new. Pending ids never taken are left unconfirmed and replayed on the next start.
3. Backfills are abandoned unacknowledged.
4. Sink engines finish their live items and skip backfill items.
5. `Progress` is confirmed as acknowledgements arrive, and the live stream closes with the final position.

The deadline and exit code stay the CLI's.

**Staging (#120)** keeps its order:

1. `continuity()`.
2. Every sink staged.
3. The daemon calls `prepare()`.
4. The reader and index tasks start.

**Reindex of one sink:** as today, as a barrier on that index's lane, with the request on that index's request lane.

**Unchanged:** `--skip-backfill`; a `backfill = false` sink; the visibility hold (#343), which stays in the capture.

**The OpenSearch sink under concurrent writes:**

- its per-flush state (the alias targets read per write, the deferred-refresh set) sits behind locks that `write(&self)` may take concurrently;
- a generation swap is a lane barrier, so no write races it.

**The stdout sink** holds its output lock for a whole `write`.

**Observability.**

- `/status` gains `indexes.<name>`: phase (`live`, `backfilling`, `restarting`, `dropped`), pending documents, builds in flight, budget held. Per sink it adds the stall age, WAL behind, items and writes in flight.
- Metrics gain an `index` label.
- The dev dashboard and `dev/prometheus` get the panels and the `FlussoIndexStalled` alert.

## 7. The ports

| Port | Change |
| --- | --- |
| kernel `Position` | Gains the source's `fence_before`, `confirm_before` and `confirm_through`, opaque to the engine. |
| `ChangeCapture` | `confirm` and `reached` take a position (section 3); the capture keeps the last emitted change instead of `entries`. `snapshot` goes. The daemon calls `prepare`. |
| `Stream` | `lane(index, sink)` and `requests(index)`. `watermark` and `is_idle` move to `Progress`, superseding ADR 0003's "the stream owns the watermark". Redelivery and the request lane stay. |
| `LaneItem::Batch` | `index`, `envelopes`, heap `bytes`, `oldest: Option<Position>` (`None` for backfill). `SnapshotComplete` names one index. |
| `Request` | `Backfill { sink }`, one request lane per index, plus a fresh build of ids for one lane (section 6). |
| `DocumentBuilder` | Already shareable (`&self` behind an `Arc`, on a pool). Gains `keys(index, after, limit)`. `Document::Upsert` gains its heap size. A second pool for resolution. |
| `Sink` | `apply` + `flush(caught_up)` become `write(&self, index, &[Envelope], caught_up) -> FlushReport`, safe to call concurrently. `FlushReport` keeps retryable items apart from rejected ones. |
| channel adapter | `release` today rewinds a cursor and serves every later item again, including items still being written. It becomes a per-ticket "due again" flag. Lanes are bounded by bytes alone. |
| `/wait`, `flusso-query` | `index` on `/wait`; `Client::after` passes its index; the once-per-level cache is keyed by level and index set. |

## 8. Guards

**Ordering, with mocks.** A builder returns each document's "database version at build start" and finishes builds in random order; sinks record writes and take random time.

- `a_document_is_never_built_or_written_twice_at_once`
- per id and sink, the versions written never go down

**Confirmation and fences.**

- `a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`, with a keepalive arriving in between.
- `a_later_build_finishing_first_confirms_nothing_past_an_earlier_one` (stage 4).
- `an_idle_stream_still_releases_wal_and_passes_a_fence` (the #111/#204 cases on the new capture).

**Seeded random**, in the style of `random_stops_under_writes_converge_after_a_restart`, replayed with `FLUSSO_SCHEDULE_SEED`:

- random delays, backfills and reindexes, items answered 429, a deleted generation;
- index tasks, sink engines and the reader killed at random;
- at every confirmation and fence answer, every covered change is on the named sinks at its version or newer;
- at the end, every sink equals the table.

**Live, against Postgres and OpenSearch.** `builds = 8`, `writes = 8`, small batches. Backfill 50,000 rows while writers bump a counter column on random rows, delete some and re-key some.

- A recording wrapper around the real sink asserts that per id, counters never go down.
- Afterwards, every document equals its row and no deleted row has a document.
- `live_latency_during_a_backfill_stays_within_one_build_and_one_write`: p99 of a probe on an index under backfill and on one beside it.

**The fixes that land first.**

- `a_released_ticket_redelivers_only_itself`
- `an_item_refused_with_429_is_retried_not_rejected`
- `a_partly_refused_item_admits_no_newer_build_of_its_ids`

**Independence and budgets.**

- `an_idle_index_never_pins_the_slot`
- `a_failed_index_pins_the_slot_and_holds_up_no_other_index`
- `a_stalled_lane_pauses_only_its_index_and_the_budget_never_deadlocks`
- `an_oversized_batch_is_granted_alone`
- `backfill_holding_the_shared_half_never_blocks_a_live_build`
- `backfill_gets_permits_under_a_steady_trickle`
- `live_goes_first_with_one_build_and_one_write`
- `a_trickle_is_built_one_batch_at_a_time`
- `a_fence_on_one_index_ignores_a_stalled_sibling`
- `a_backfill_of_one_index_leaves_anothers_deferred_refresh_on_time`
- `a_missing_generation_on_one_lane_is_rebuilt_while_the_others_write`
- `a_stalled_lane_past_its_limit_is_dropped_and_rebuilt`
- `a_restarted_backfill_resumes_from_its_last_key`

**Bounds.**

- `positions_stay_bounded_while_one_index_is_stalled`
- a 24-index e2e counting `pg_stat_activity` during a full backfill (at most 9 connections)

**Client.**

- `a_fenced_client_waits_again_for_another_index`
- `a_disabled_index_is_refused`
- old client against new daemon, and new client against old daemon

**Rewritten:** `wal_release`'s empty-batch guard. **Kept:** the `fence`, `shutdown`, `visibility`, `pipeline` and `backfill` e2e, and request coalescing.

## 9. Stages

Each stage leaves `main` correct and is measured on `reference` before and after.

1. **The fixes needed by any concurrency.**
   - item 429/503 retried, with the lane item unfinished until written;
   - the channel releasing per ticket.

   *Gain:* no lost documents on a busy cluster.
2. **Keyset backfill on the builder pool.** Key casting, filters and soft-delete on the page. *Gain:* one connection fewer and no long transaction.
3. **Concurrent writes per sink.**
   - `Sink::write`, safe for concurrent calls in the OpenSearch sink;
   - per-id admission and live-first selection on the sink's one lane;
   - the lane watermark taken before the oldest unacknowledged item, from the predecessor recorded at delivery, with fence reports following it;
   - the stdout sink serialized; in-flight writes awaited on a restart.

   *Gain:* about 2–3× on a backfill (the measured bottleneck).
4. **The build gate and concurrent builds on the one ingest task.** Builds are published **in position order** through a small reorder buffer, because the lane watermark of stage 3 still trusts lane order. *Gain:* builds stop being the next limit.
5. **The memory budget:**
   - heap size from the builder;
   - shares, the shared half and the live reserve;
   - `max_built_mib` replaces `[stream] capacity`.

   *Gain:* bounded memory.
6. **Self-describing positions and `Progress`,** still on one ingest task with one lane per sink. The capture keeps the last emitted change; `read_through` stops confirming. *Gain:* the capture's map goes; the base for 7.
7. **Per-`(index, sink)` lanes, fences and caught-up.** Per-index request lanes, `/wait?index`, and the client's per-index cache. *Gain:* a stalled index blocks no other index's fences or refreshes.
8. **The reader and index tasks.**
   - pending sets and the resolution connection;
   - in-place lookup retries and the replay skip;
   - per-index restart with backfill resume;
   - stall age and the alert.

   *Gain:* live changes never queue behind another index's backfill.
9. **Per-lane failure:** parking with fresh builds, and re-staging a lane on `index_not_found`.
10. **Dropping and rebuilding a stalled index** past its configured limits. *Gain:* one index can no longer pin the slot for ever.

**What the stages update beside the code**, each in the stage that changes it:

- `CLAUDE.md`'s invariants and guards; `CONTEXT.md` (lane, position, pending, `Progress`).
- ADR 0003 superseded in part, and ADR 0009 amended (per-index fences).
- The manual's Reference pages: `config-toml.md` (`[concurrency]`), the CLI flags, `/status`, `/wait`, metrics. Also `docs/src/contribute/pipeline.md` and the query manual's fence chapter.
- The upgrade page:
  - the `Sink` trait and `Position` changes;
  - `max_delay_ms`'s narrower meaning;
  - `[stream] capacity` deprecated;
  - `/wait`'s `index`;
  - per-lane `index_not_found`.
- `libs/1-config/config.schema.json` (`just schema-gen`).
- The designer: `ConfigPanel.tsx` gets `[concurrency]`, every locale, and the dist rebuilt.
- The `flusso-query` README, and the `plugin/**` skills that describe `/wait` or `Client::after`.
- The Helm chart's memory defaults; Grafana panels; `dev/prometheus` alert rules.
- `dev/bench`'s `/metrics` parsing (the added `index` label); a new baseline for the in-process `engine` bench.

## 10. Open questions for the owner

Each has a recommendation and its cost. The reviewers' recommendation is the proposed default.

1. **Defaults `builds = 4`, `writes = 4`.** *Recommended: yes.* *Cost:* 8 measured faster (5× and 3.5–4×), but on cached data and an idle database, so part of that speed is left to the setting.
2. **A stalled index.**
   - *Recommended:* the reader waits only at the memory budgets. An alert fires past 10 minutes or 1 GiB of WAL. The lane is dropped and rebuilt past 1 hour or 8 GiB, a committed stage.
   - *Cost:* a dropped lane is rebuilt from scratch.
   - The defaults should sit below the deployment's `max_slot_wal_keep_size`, which flusso can read and warn about.
3. **Fences per `(index, sink)`.** *Recommended: yes.* *Cost:* a `/wait` parameter and a query-train release.
4. **Item 429s retried as transient, `writes` fixed.** *Recommended: yes, plain retry with backoff first; add a slow-down only if measured.* *Cost:* a document refused with 429 for good retries for ever; the stall alert shows it.
5. **Concurrency within one index only for a backlog.** *Recommended: yes.* *Cost:* a single hot index under a steady trickle uses one build at a time.
6. **A lane parked by `on_error = stop`.** *Recommended:* the parked item asks for a fresh build of its ids for that lane, and its position stays outstanding. *Cost:* one more request kind on the index task; lane items stay immutable.
7. **A stop with a backlog pending leaves unbuilt ids for replay.** *Recommended: yes.* *Cost:* the next start rebuilds them; nothing is lost.
