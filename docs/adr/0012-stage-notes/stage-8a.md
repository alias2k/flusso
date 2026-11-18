# Stage 8a design: the reader, pending sets, one task per index

Today one ingest task reads, resolves, batches, builds and publishes for every index
(`libs/2-engine/src/ingest.rs`'s `follow`). After this stage:

- **One reader task** reads the live stream, resolves each change on a resolution connection of its
  own, marks the ids it touches pending on their index, and confirms.
- **One task per enabled index** takes its pending ids in batches, builds them through its own
  `Builds`, publishes to its `(index, sink)` lanes, and walks its own backfill.
- Shared between them: `Progress` (which now holds the pending sets), the build gate (`builds`
  permits), the budget, the builder pool.

Per-index restart and backfill resume are 8b. Stall age and the alert are 8c.

```text
LiveStream ─▶ reader ── resolve (own conn) ── mark(index, ids, p) ──▶ Progress.pending[index]
              │  confirm (after marks, 1 s tick)                         │ notify
              │                                                          ▼
requests ─────┴─▶ dispatch Backfill{index,sink} ──────────────▶ IndexTask(index): take → build → publish
                                                                 shared: Gate(builds), Budget, builder pool
```

## 1. What the reader does per change

In position order, one change at a time (as `buffer_live` does today):

1. `resolve(table, key, before, after)` on the **resolution builder**: a second `DocumentBuilder` the
   composition root opens with **one** connection, sharing the column-type cache (`ColumnTypes`)
   with the build pool, so a `Relation` message still invalidates both.
   `IngestEngine::with_resolver(Arc<dyn DocumentBuilder>)`; without one, `documents` resolves (tests,
   embedders).
2. Group the ids by index, and for each index `Progress::mark(index, ids, position)`: one lock per
   touched index (section 2).
3. `Progress::routed(position)`: the newest routed, under its own lock, **after** the marks. A change
   that resolves to nothing is only routed, so it is landed once routed (as today).
4. Confirm: after a run of marks when the answer changed, and on the 1 s tick always (as today, moved
   from the ingest loop to the reader).

**Ordering of the writes and reads in `Progress`** (what keeps every answer conservative):

| Writer | Order |
| --- | --- |
| reader | mark on the index's pending set, then raise `routed` |
| index task, publish | register the item on **every** lane of the batch, then release the batch from the index's floor, then send to each lane |
| sink engine | raise its lane's marks (unchanged) |

A reader of `Progress` (confirm, a fence) copies `routed` first, then reads each index's floor, then
each lane's registrations. So a position it sees as routed is either still in a floor or already
registered on its lanes: registration precedes release, and the floor is read before the lanes.

**Registering before sending** replaces stage 6's "register as each send succeeds": an item
registered on a full lane pins that lane only (its sink can't land what it never got), the healthy
lanes land theirs. The per-lane `newest` and `Progress::skipped` go: a lane's answer is
`Before(min(its unlanded, its index's floor))` else `Through(routed)` else `Idle`. An index with
nothing pending and nothing unlanded passes every routed change, so a quiet index pins nothing (no
skip call needed).

**What "routed" means now:** marked. Confirmation and fences already treat an unbuilt marked id as
outstanding through the floor, so `routed` can move at mark time.

### The pending bound: the reader waits

`max_pending_documents` (deployment-wide, default 1,000,000; `[concurrency]`, flag and env as the other
three). The reader keeps a count of ids in pending sets (an `AtomicUsize` in `Progress`; in flight
doesn't count). Before marking a change it waits while the count is at or past the bound, woken by a
`Notify` each take signals. It then marks the change's ids whole, so one change touching 50,000
parents overshoots by them once. Past the bound **every index stalls**: the reader stops reading, the
capture's read-ahead fills, the stream waits (ADR §5). The reader never waits on a lane, on age or on
WAL.

While it waits it still answers the stop and still confirms on the tick (a `select!` over the stop,
the tick and the notify).

## 2. The pending set, per index

Inside `Progress`, one `Mutex<Pending>` per index (Progress already outlives every run, which is
what the restart rules below need):

```rust
struct Pending {
    waiting: HashMap<DocumentId, Position>,   // id → oldest position not yet built
    order: BTreeSet<(u64, IdHash)>,           // (seq, id) for oldest-first takes
    in_flight: HashMap<DocumentId, u64>,      // id → build token
    floor: BTreeMap<u64, usize>,              // multiset of seqs: waiting ids + each build's oldest
    newest_marked: Option<Position>,
}
```

- **Mark** (`mark(index, ids, p)`): an id not waiting is added at `p`; an id already waiting keeps
  its older position (one build covers both changes: a build reads the database as it is now). **A
  mark is unconditional, in flight or not**: an id whose build is running is added to `waiting` too,
  because that build may have read before the change.
- **Take** (`take(index, max, live_in_flight) -> Option<Taken { ids, oldest, token }>`), the
  **atomic move from pending to in flight**, under the one lock: walk `order` oldest first, skip an id
  in `in_flight`, move up to `max` ids from `waiting` to `in_flight` under a new token. The build's
  oldest (the least seq of the ids taken) is added to `floor` as that build's entry, and each taken
  id's own entry leaves it. So the floor never drops below an outstanding change.
- **A change arriving while its id builds** lands in `waiting` (not lost), and that id is not taken
  again until its build is published (the take skips in-flight ids), so the next build starts after
  the previous one ended, reads newer state, and lands later. That is ADR §2's rule, enforced where
  the ids are, so the index task's own admission check (stage 4's hashes in `Builds`) stays only for
  backfill pages against live builds.
- **Publish** (`published(index, token, item key, lanes)`): register the item on each lane (key,
  `oldest`), then under the index lock drop the token's ids from `in_flight` and its entry from
  `floor`, then wake the index task if some of them wait again.
- **Give back** (`give_back(index, token)`): a build that failed, or a run that ended with builds
  unpublished: each id returns to `waiting` at `min(its position at take, any newer mark)` (its take
  position comes from the build's record), the token's floor entry is replaced by the ids' own.
- **The floor** answers `min` in O(log n); the lane answer reads it under the index lock.

Cost per id: `DocumentId` (index + `RowKey`) twice plus a `Position` (32 B) and two map entries:
the ADR's 150–250 B estimate holds; documented with the setting.

### Positions on lane items

Live item keys can no longer be "the batch's last position": a batch takes ids oldest first and
skips ids in flight, so the newest position among a batch's ids doesn't grow along a lane (an id
skipped while it built is taken later with an older position). So:

- `stream::Batch` gains **`key: Option<u64>`**: the item's number on its index's lanes, from a counter
  per index, assigned at publish (an index task publishes to its lanes in grant order). Snapshot
  items: `None`, as their position is today.
- `Batch.position` becomes the item's **oldest** position (what an emitting sink writes as `seq`, and
  what an ordered sink orders live fronts by). Envelopes carry it too.
- `Progress` registers `(key, oldest)`; the sink engine reports `applied(lane, key)` /
  `searchable(lane, key)` and `Fences` orders by key. `Observer::on_position_applied(lane, position)`
  keeps the oldest position (the fence observer only wakes waiters).

The exact per-index `oldest` (ADR "a batch's oldest per index") falls out: it is the least position
among the ids the build took.

## 3. An index task

`ingest/index_task.rs`, one per index of `DocumentBuilder::backfill_indexes()` (the enabled
indexes; the daemon already builds their lanes). It owns: the producers of its lanes, its `Builds`
(reorder buffer and admission), its backfill (track A's `Walk` and the `Snapshot` state, one index),
its request handles. Loop (`select!`):

| Arm | Does |
| --- | --- |
| pending notified, or a build landed, or the budget released | grant (below) |
| a build landed | settle the budget, publish ready slots in grant order |
| a snapshot page | buffer keys, grant |
| a backfill request for this index (from the dispatcher) | coalesce (below), start the walk |
| the stop | section 5 |

**Grant.** Live first, as ADR §1: one live build at a time, a second only while at least
`max_changes` ids wait. Take up to `max_changes` ids (`Progress::take`): whatever waits when a slot is
free, so a lone change is built at once and a trickle batches itself while a build runs. A backfill
batch competes for the same slots, every fourth contested grant (the gate below).

**The gate, shared.** `builds` permits across every index task: `Gate` (extracted from `Builds`'
`choose`): a `Mutex` with the free count, a FIFO of live waiters and one of backfill waiters, and the
every-fourth counter, each waiter woken by its own `oneshot`. A key page takes a backfill permit (ADR
§5). A task asks for a permit only when it has a batch ready; it holds it for the build and gives it
back when the build lands. The task's `Builds` keeps per-index admission and the reorder buffer,
bound `2 × builds` slots **per index** (each index publishes in its own grant order; one index's slow
build holds back only its own slots).

**Publish** (`commit.rs`'s path, moved): register on every lane, release from the floor, send to each
lane, `on_changes_settled` no longer needed for skipped sinks (each sink's lanes of this index get the
item; another index's item is another index's). `changes` on an item: the number of distinct changes
whose marks the build covered is not known per index (a change marks several indexes), so
`changes_committed` moves to the reader: a change counts as committed for a sink when every lane of
that sink has it landed. **Proposed:** keep `/status` `changes_committed` as "routed changes whose
positions every lane of the sink has passed", computed from `Progress` at snapshot time, and drop the
per-item `changes` count. Say if you'd rather keep the per-item count (it would be approximate).

**Independence.** One index's slow build holds its own slots and one permit; a full lane blocks only
its own task's sends (its pending grows, its floor pins confirmation, its fences wait); another
index's task keeps taking, building and publishing. Shared and therefore still coupling: the permits
(a slow build holds one of `builds`), the budget (rule 2 below caps one index at half), the reader
(the pending bound: everything stalls once it is reached), the builder pool.

**Backfill per index.** The dispatcher (the engine's own task, which also supervises the others)
receives each `Backfill { index, sink }` and forwards it with its ack handle to that index's task. The
task coalesces: requests already queued, then for up to `max_delay` while some sink of the index has
not asked (today's rule, per index). Its walk is track A's `Walk` over one index. A page drops ids
waiting or in flight live (ADR §1: their live build goes to every lane). `SnapshotComplete { index }`
to the requesting lanes, then the acks.

**Pool.** Build pool: `builds` connections (a key page takes a build permit, so it holds one of them).
Resolution: 1. So today's `builds + 2` becomes `builds + 1`, plus visibility (1), admin (2) and
replication (1) as today: 9 at `builds = 4`, ADR §5's count, whatever the number of indexes.

**The budget's per-index half** (rule 2, track C's `Budget`): `try_reserve(work, index, estimate)`;
an index's held bytes at most half the limit; rule 4: an index holding nothing is granted one batch
(replaces "a kind holding nothing" for live; backfill keeps its own half). Track C owns `budget.rs`;
I'd ask them for the per-index accounting API or do it with their agreement.

## 4. Failure and restart in 8a

- **Any task failing fails the ingest side.** An index task's error (a build or query error, an
  `on_error = stop` script failure, #198) or the reader's ends the engine's run with that error; the
  dispatcher aborts the other tasks and awaits them (their builds are aborted with them, as
  `Builds::abandon` does). The daemon restarts the whole ingest engine with backoff, as today. 8b makes
  an index task restart alone.
- **Pending sets survive the run** (they are in `Progress`). At the end of a run, and again at the
  start of the next (idempotent, covering an abort), every in-flight id is given back. The resumed
  stream replays from the confirmed position, which is at or before every floor entry, so replayed
  changes re-mark ids already waiting (dedup keeps the older position). Nothing is lost: an id is in
  `waiting`, in flight with a floor entry, or registered on its lanes, at every instant.
- **At-least-once** holds as today: confirmation stops before the least of every floor and every
  lane's unlanded item.
- **Script failures** (`on_error`, ingest side): `skip` reports and publishes nothing for the id (as
  today, in the index task); `stop` fails the index task and therefore the run (above).

## 5. The stop (#371) across N tasks

1. The reader stops reading (its `select!` checks the stop before the stream, as `next_live` does).
2. Every index task finishes its builds running and publishes them; it takes **no** new pending ids
   (ADR question 7: unbuilt ids stay for replay); a snapshot is abandoned unacknowledged; its
   queued snapshot batches are skipped by the sinks (unchanged).
3. The dispatcher waits for every index task, then `drain_lanes` confirms as acknowledgements arrive.
   With ids still waiting, the confirmed position stops before the oldest of them, so the next start
   replays and builds them.
4. `close_live`, then the stream polled to its end (unchanged).

Behaviour change: today a stop builds and publishes the buffered batch. With pending sets the
unbuilt ids are left for replay. `shutdown`'s e2e (everything at or before `confirmed_flush_lsn`
indexed; a restart converges) still holds; a stop indexes slightly less before exiting.

## 6. Unchanged

Read-after-visible (the hold stays in the capture), `--skip-backfill`, `backfill = false`, staging
order (`continuity`, stage, `prepare`, then the reader and index tasks), the request lane's
at-least-once acks, the sink side (including track A's concurrent seeding: a lane's `SnapshotComplete`
runs `mark_seeded` as a flight holding one `writes` slot; nothing here depends on how the sink side
seeds, only on `SnapshotComplete { index }` arriving after the index's last snapshot item).

## 7. Observability

- `on_stage`: the reader reports `Resolve` and `Idle` with `index: None`; an index task reports
  `Build`, `Publish`, `LaneWait`, `Page`, `Snapshot`, `Idle` with `index: Some(i)`. `StageTiming`
  already has `index`; the metric `flusso.stage.duration` gains the `index` label where set, and
  #409's attribution table groups by `(stage, index)` (`dev/bench` parses it).
- `/status`: `indexes.<name>` gains `pending` (ids waiting), `in_flight` (ids building), `builds`
  (running), `bytes` (held, with the per-index budget). Metrics: `flusso.index.pending{index}`,
  `flusso.index.in_flight{index}`, read at scrape.
- `/status` `pending_documents` total, against `max_pending_documents`; `flusso.reader.waits`
  counter (times the reader waited on the bound).

## 8. Per-item cost

| Path | Today (stage 7) | 8a |
| --- | --- | --- |
| task hops, live change → sink write | capture → ingest task → lane → sink task: 2 | capture → reader → index task → lane → sink task: 3 (one `Notify` wake per batch, not per change) |
| locks per change, reader | none in the ingest task until publish | one pending lock per index it touches + `routed` |
| locks per batch, index side | registry per lane at publish | one pending lock (take) + one per lane (register) + one pending lock (release) |
| lone change latency | resolve, build, publish on one task | resolve, mark, wake, take, build, publish: one task switch more, tens of µs |

The in-process `engine` bench (`live_drain`, `lone_change`) measures it; I'll report counts (lock
acquisitions, wakes per change) with the same counting as stage 6 §9, then rates.

## 9. Migration, in commits that each compile and pass

1. **`Batch.key` and the floors.** `stream::Batch` gains `key`; `Progress` gains the pending sets
   (`mark`, `take`, `published(index, token, …)`, `give_back`, `routed`) and drops per-lane `newest`
   and `skipped`; the one ingest task uses them (it marks, then takes at once). Red tests first: the
   pending unit tests (dedup, mark in flight, give back, floor never below an outstanding change), the
   seeded `every_interleaving_answers_as_the_model_does` extended with marks and takes.
2. **The resolution builder.** `with_resolver`; the CLI opens a 1-connection `PgDocumentBuilder`
   sharing `ColumnTypes`; the build pool goes to `builds + 1`, then `builds` in step 4.
3. **The shared gate.** `Gate` out of `Builds::choose`; still one task. Its tests (live first, every
   fourth, a permit per key page).
4. **Index tasks.** `IndexTask` holding `Builds`, publish, the walk; the engine becomes reader +
   dispatcher + tasks. The seeded schedule test runs several indexes; new guards
   `a_slow_build_of_one_index_delays_no_other_index`, `a_full_lane_of_one_index_delays_no_other_index`,
   `a_change_marked_while_its_document_builds_is_built_again`, `a_stop_leaves_unbuilt_ids_for_replay`.
5. **`max_pending_documents`.** Config, flag, env, the reader's wait; guard
   `the_reader_waits_at_the_pending_bound_and_resumes`.
6. **The per-index budget half** (with track C).
7. **Observability**, then docs (CLAUDE.md, CONTEXT.md, the pipeline chapter, config/HTTP/metrics
   reference, upgrade page, Helm/Grafana if a panel is added).

## 10. What I need from tracks A and C

- **Track C** (`ingest/builds.rs`, `ingest/commit.rs`, `budget.rs`): frozen from step 3 until step 6
  lands. Steps 1–2 touch `commit.rs`'s publish (registration and the item key) and the engine's
  constructor only. For step 6, the per-index accounting in `Budget`: I propose C adds
  `try_reserve(work, Option<&IndexName>, estimate)` and the rule-2/rule-4 checks, and I wire it.
- **Track A** (`ingest/snapshot.rs`, `Walk`): frozen from step 4. The walk moves into the index task
  as it is, over one index (`Walk::new(documents, [index])`); the coalescing moves to the task.

## 11. Open points for the review

- `changes_committed` per sink: computed from `Progress` (proposed) or an approximate per-item count.
- The reorder buffer per index at `2 × builds` (could be smaller, `2 × 2 live + backfill share`).
- Whether pending sets are kept across a run's end (proposed: yes, in `Progress`), or cleared and
  rebuilt from replay (simpler, but a fence would pass between the clear and the replayed marks: no).
