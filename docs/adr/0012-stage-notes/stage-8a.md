# Stage 8a design: one reader, one task per index

**Status:** design, against `main` at `a187cf91` (2026-11-20), revised after an independent review of
`61323c20`; the owner's decisions are in §6 (b). Slice 1 merged (#436). It replaces the note written before stages 3–7, stage 9's first half (#427), #434
(`engine::Backoff`) and #435 (backfill tickets, cut backfills) landed. The earlier implementation
branch is gone; 8a restarts from `main`.

**The owner's bar:** every slice correct, every guarantee kept, no slower than `main` on a same-runner
comparison. Speed-ups are welcome, not required. Concurrent by default; serial needs a reason.

## 1. The shape

Today one ingest task reads, resolves, batches, builds and publishes for every index, and walks every
backfill one index after another (`libs/2-engine/src/ingest.rs`'s `follow`). After 8a:

```text
LiveStream ─▶ reader ── resolve (own conn) ── mark run ──▶ Progress.pending[index] ──notify──┐
              │  routed, confirm (per run, 1 s tick), lead bound                             ▼
requests ─▶ dispatcher ── Backfill{index,…} (unbounded, per index) ──────▶ index task(index)
                                                         take · build · publish · walk · Complete
                         shared: Gate (builds permits) · key-page permit · Budget · pool · Progress
```

| Task | Owns | Never does |
| --- | --- | --- |
| **Reader** (one) | the live stream, the resolver connection, `routed`, confirmation, the 1 s tick, the lead bound | build, publish, wait on a lane |
| **Dispatcher** (one, the engine's own task) | the request consumer; routes each `Backfill` to its index task; supervises the others | block on a task, walk, build |
| **Index task** (one per index **with lanes**) | its `Builds` (slots, admission, grant order), its lanes' producers, its walk, its request queue | resolve, confirm |
| `Progress` (shared, outlives runs) | pending sets, item keys, lane registrations and marks | — |

**Which indexes get a task: the lanes' (`lanes.keys()`).** `main` builds whatever `resolve` returns and
drops an index's envelopes when it has no lane (`commit.rs`'s `publish_live`). The reader drops ids of
an index with no lane before marking: an id with no task to take it would pin a floor for ever (the
`TwoIndexes` harness in `ingest/tests.rs` resolves to an index with no lane). `backfill_indexes()` only
decides which requests are served.

What `main` already has and 8a reuses unchanged: the resolver on its own one-connection pool
(`with_resolver`, `backends.rs`), the `Gate`'s rule (live first, every fourth contested grant to
backfill), `Builds` (grant-order slots, admission by id hashes), the keyset `Walk` with 8b's segments
(`plan_walk`), tickets per lane (#435), one lane per `(index, sink)`, the sink engine as it is
(merging, parking, restaging, fences per lane), `engine::Backoff`.

## 2. The old note against `main`

| Old note | Now | Why |
| --- | --- | --- |
| §1 resolution builder, `with_resolver`, CLI wiring | **done on `main`** | merged with #422 |
| §1 reader marks per change | **changed:** per drained run (§3.1) | `main` cuts batches per drained run; one lock per touched index per run |
| §1 reader never waits except at a large pending bound | **changed:** a lead bound (§3.1) | `main` stops reading while a closed batch waits for a build (`ingest.rs`'s `open`); without pacing, a parked lane grows pending memory without bound and a stop has unbounded work |
| §1 register an item before sending it | **dropped:** register after each send, release after all (§3.3) | registering first leaves a phantom registration when a task is aborted between the two |
| §1 per-lane `newest` and `skipped` go | still right | the pending minimum and a global `routed` replace them |
| §2 pending set with a floor multiset | **simplified** (§3.2) | the minimum is `order`'s first key or the oldest of at most `2 × builds` builds |
| §2 `Batch.key`; `position` = the item's oldest | key right; **position changed** | the oldest breaks stdout's `seq`; decision (b)2 |
| §3 one task per `backfill_indexes()` index | **changed:** one per index with lanes | an id of an index without a task pins its floor for ever |
| §3 `Builds` per index, `2 × builds` slots, shared gate | still right | grant order stays: admission is released when a build *lands*, safe only because publication is in grant order |
| §3 gate with FIFO waiters | still needed, **with rules** (§3.5) | a permit granted to a task blocked on a lane is held unused; the first attempt was reverted (final review M2) |
| §3 a key page takes a build permit | **changed:** its own permit (§3.5) | `main` reads its one page beside the builds; the pool is sized for that (`backends.rs`) |
| §3 a page drops ids waiting or in flight live | **deferred** (recorded in the ADR) | not needed for correctness: admission plus grant order already order a page against a live build |
| §3 budget per-index half | still needed, **and more** | `Budget` assumes one reserver and one waiter (§3.5) |
| §3 coalescing for up to `max_delay`, inline | **changed:** a deadline arm | `collect_requests` waits inline (`timeout_at`); in an index task that would stall its live changes |
| §4 any failure fails the run | right until slice 8 | per-index restart is its own slice |
| §5 a stop leaves unbuilt ids for replay | **reversed** (§3.7, decision (b)1) | breaks `shutdown`'s `…a_restart_rewrites_none_of_it` |
| §7 `/status` `indexes.<name>` gains counts | **wrong place** | `indexes` maps a name to a string; decision (b)4 |
| §7 stall age from the lane's answer | **changed** (§3.2) | the index backlog would blame a healthy lane; decision (b)7 |
| §9, §10 migration and tracks | rewritten (§5) / dropped | the tracks are gone |
| §10b exit criteria vs stage 7 | **rebased on `main`** (§7) | the bar is `main`, same runner |

## 3. Design

### 3.1 The reader

A **run** is what `main`'s `drain_ready` buffers: the changes the stream has ready, cut at
`max_changes` changes, at `max_delay` from the run's first change, when the stream would block, or at
the stop. Per run:

1. For each change, in position order: `resolve`, drop ids of an index with no lane, add the rest to
   a local map per index keeping the first (oldest) position per id. `on_change_captured` as today.
2. For each touched index: one lock, `Pending::mark` every id, notify the index task.
3. `routed` = the run's last position, **after** every mark of the run.
4. Fire the fence listener once; confirm if `Progress::landed()` changed.

A change that resolves to nothing is only routed. The tick always confirms (as `main`). A reader
failure (source or resolve error) fails the whole engine.

**The lead bound.** `main` paces reading: a closed batch waiting for a build stops the reading. The
reader has the same pacing, counted in ids **waiting** (taken ones are building, not lead):

| Bound | Value (fixed, from existing settings) | At defaults |
| --- | --- | ---: |
| ids waiting on one index | `2 × max_changes` | 512 |
| ids waiting on every index together | `2 × builds × max_changes` | 2,048 |

Before each run the reader waits while either is reached, in a `select!` over the stop, the tick and
a notify each `take` sends. A run may overshoot by its own ids once.

- **What it buys.** What a stop must build stays close to `main`'s: about the total lead plus one run,
  against `main`'s one batch plus `2 × builds` slots. Pending memory stays small.
- **What it costs.** A stalled index stalls the reader once its lead fills: every index then waits,
  as on `main` once the budget fills. 6b relaxes it for an index the budget holds (below).
- **6b: an index held by the budget** (its task waits for a release) stops counting toward the lead.
  Its waiting ids count toward `max_pending_documents` instead (decision (b)3). So the other indexes
  go on, as ADR §5 decided. A stop with such an index can't build its backlog; it runs to the
  deadline and exits 1, as `main` does with a stalled sink.

**The end.** The reader is the first task to end, on a stop or the stream's end. It marks its last
run, raises `routed`, then **signals that it ended** and returns the `LiveStream` through its join
output, so the engine closes it as `main` does (`wind_down`'s `close_live` and the poll to the end).

### 3.2 Pending sets in `Progress`

One `Mutex<Pending>` per index inside `Progress` (it outlives every run). Keyed by row key: the index
is the set's own.

```rust
struct Pending {
    waiting: HashMap<Arc<RowKey>, (Position, u64)>,  // oldest unbuilt position, order counter
    order: BTreeMap<(u64, u64), Arc<RowKey>>,        // (seq, counter): oldest first; never an id hash
    in_flight: HashMap<Arc<RowKey>, Token>,          // live builds not yet published
    page_held: HashSet<Arc<RowKey>>,                 // ids in a running backfill page build
    builds: Vec<(Token, Position, Vec<(Arc<RowKey>, Position)>)>, // at most 2 × builds; oldest, take positions
}
// The item key counter is not here: it is `Progress::next_key`, one atomic per index (§3.3, slice 1).
```

**The minimum** (what holds confirmation back) is the least of `order`'s first key and each build's
oldest: no multiset to keep in step. Per lane, a build counts only until its item is registered on
that lane (each build keeps the sinks that registered it), so a full sibling lane still holds back
only itself (stage 6's guard; found while building slice 2).

| Operation | Who | Under the index lock |
| --- | --- | --- |
| `mark(ids)` | reader | add each id at its position unless waiting at an older one; **also when in flight** (that build may have read before the change) |
| `take(max) -> Taken` | index task | oldest first, skip ids in flight **or held by a running page**, move up to `max` to `in_flight` under a new token. A taken live batch is therefore always admissible, as `main` checks admission before a permit (`builds.rs`'s `choose`) |
| `hold_page(ids)` / `release_page(ids)` | index task | a page build's ids, at grant and at land |
| `published(token)` | index task, after every lane's send, **also for a build with no envelope** | drop the token's ids from `in_flight`; wake the task if one waits again |
| `give_back(token)` | a failed build; every token at a run's start | each id back to `waiting` at `min(take position, newer mark)` |

**The lane answer for confirmation and fences:**

| Lane state | Answer |
| --- | --- |
| an unlanded item, or its index's pending minimum for this lane | `Before(min(oldest unlanded, pending minimum))` |
| neither, something routed | `Through(routed)` |
| nothing routed | `Idle` |

**Read order.** Writers: the reader marks, then raises `routed`; an index task registers on each lane,
then publishes the token. A reader of `Progress` takes `routed` first, then each index's minimum, then
each lane's registrations, and never holds an index lock while taking the registry lock. So a routed
change it sees is pending, registered, or landed. `reached` and the port are unchanged; a quiet index
answers `Through(routed)` and pins nothing.

**Stall age and `pinned_by` blame no healthy lane for its index's backlog.** Confirmation and fences
use the pending minimum. Stall age (`stall.rs`) and `pinned_by` (`progress.rs`) use only the lane's own
unlanded items. When the pending minimum is what pins confirmation, `pinned_by` names the index
(`Pin::Lane(lane) | Pin::Backlog(index)`), and the backlog (ids waiting, its oldest's age) is reported
per index. Decision (b)7.

**The fence listener** (`Progress::on_registered`) fires after each publish, after each reader run and
on the tick: the three moments an answer can move without a sink mark.

### 3.3 Lane items: key and position

Live items can't be keyed by "the batch's last position": takes skip in-flight ids, so an id skipped
once is taken later with an older position.

- `Batch.key: Option<stream::ItemKey>`: the item's number on its index's lanes, from
  `Progress::next_key` (one atomic per index), taken at publish (grant order). In `Progress`, so a
  restarted task never reuses a key still unacknowledged. `None` for snapshot items; a live item
  without one fails the sink engine's run (`EngineError::UnkeyedLiveItem`). Built in slice 1.
- `Progress` registers `(key, oldest)` per lane; `applied(lane, key)` / `searchable(lane, key)`; the
  sink's `Fences` order by key. `Observer::on_position_applied` reports `Batch.position` (its one
  consumer, the fence observer, only wakes waiters).
- `Batch.position`, what the envelopes carry and an ordered sink orders by: the newest position marked
  on the index when the batch was taken. Decision (b)2.
- **Registration after each send succeeds**, the token published after every lane's send. Cancel-safe:
  a task aborted mid-publish leaves its ids in flight, given back at the next start. A full lane then
  holds its index's pending minimum, so it pins its sibling lanes of that index too; only at the
  channel's `LANE_BACKSTOP`, which the budget keeps out of reach.

**stdout after 8a**, exactly: `seq` is non-decreasing per index and may repeat; there is no order
across indexes. `docs/src/reference/sink-stdout.md`'s "a consumer that resumes from the last `seq` it
read skips nothing" and "across indexes the lines keep position order" must be reworded: a consumer
resumes per index, from the last `seq` inclusive. A per-index tie-break would make resume exact, but
`seq` is one opaque string compared as a number by readers; the cheap form is a second field (the
item key, `"key"`), which is a stdout format addition: decision (b)2.

### 3.4 An index task

Loop (`select!`; publishing is an arm, never an inline await):

| Arm | Does |
| --- | --- |
| pending notified, a build landed, a gate grant | grant (below) |
| a ready slot's publish finished | publish the next ready slot in grant order |
| a key page arrived | buffer keys, grant |
| a request (from the dispatcher), no walk running | start coalescing: a deadline arm at `max_delay` |
| the coalescing deadline, or every sink of the index asked | plan, start the walk |
| a 1 s tick | backstop grant (`main`'s tick arm, per index) |
| the stop, or the reader ended | §3.7 |

**Live grant.** One live build at a time; a second only while at least `max_changes` ids wait (ADR
§1). The task joins the gate's live line only when it can use a permit now (§3.5). With the permit and
the reservation, it `take`s that many ids.

**Builds per index.** `Builds` moves into the task: `2 × builds` slots per index, admission by id
hashes (now only pages against running live builds), publication in grant order, `SnapshotComplete`
as its own slot. **Why grant order stays:** admission is released when a build lands. A page landing
before an earlier slot waits in the queue; a live build of the same id admitted meanwhile gets a later
slot and so publishes after it. Publishing live builds as they land would need admission released at
publish.

**Publish** as `commit.rs` does, to the index's lanes only. `on_changes_settled` and
`Progress::skipped` go.

**Script failures (#198)** stay in the task: `skip` reports and publishes nothing for the id; `stop`
halts the task's grants, publishes nothing after the failed slot, and fails the task (§3.7).

**Backfill.**

- **Dispatcher.** Receives each `Request::Backfill` and forwards the delivery, ack handle included, to
  its index's task over an **unbounded** channel, in a `select!` with the stop: it never blocks on a
  task. An index with no lane, or one the builder doesn't build: warn and ack (as `main`). A dropped,
  unacknowledged delivery returns to the request lane (`ChannelAck`'s `Drop`).
- **Coalescing, per index.** `main`'s rule per index: once every sink with a lane of the index asked,
  start; else at `max_delay`. Sinks coalesce into one walk; indexes walk at once. Cost: an index whose
  seeded sibling sink never asks waits one `max_delay`, per index.
- **The walk** is `main`'s `Walk` over one index, with 8b's `plan_walk` segments and `lanes_now`.
- **Tickets and cuts (#435).** Each lane is answered under the greatest ticket it asked with before the
  walk started. A request of the same lane arriving mid-walk (a cut asks again from the first key)
  waits for the walk's end, then gets its own walk; the old walk's `SnapshotComplete`, naming the older
  ticket, seeds nothing. No preemption: `main`'s behaviour.
- **Restaging one lane (stage 9 §4)** is a request like any other: only that index's task serves it.
- **Acknowledgement** after the index's own `SnapshotComplete` is published (stage 7 review 12).

### 3.5 Shared: the gate, key pages, the budget, the pool

**Gate.** `builds` permits shared by every index task.

- A FIFO line per kind; a freed permit goes to the oldest live waiter, except every fourth contested
  grant, which goes to the oldest backfill waiter.
- **At most one waiter per task**, and only while it can use a permit now: a batch ready and not
  publishing a full lane's item. A task blocked on a lane never holds a granted permit.
- **The budget is asked at grant.** A waiter whose reservation the budget refuses keeps its place;
  the permit goes to the next waiter that fits. A release re-runs the grant.
- Cancel-safe: a waiter dropped after its grant hands the permit on.

**Key pages** don't take build permits: `main` reads its one page beside the builds. One key-page
permit shared by every walk (one page read at a time, as `main`); the builder pool stays `builds + 1`
plus the resolver. Raising it to several pages is a later, measured change (pool grows by as much).

**Budget**, from one reserver to many. **Built in slice 4** (as built, below the plan it replaced):

| Before | Slice 4 |
| --- | --- |
| check then add, "only the ingest engine reserves" | every count under one lock: `Budget::admit` decides, `Budget::reserve` checks again and adds in one step, so N reservers never pass the limit together |
| `want_release` + `notify_one`: one waiter | every release bumps a generation and calls `notify_waiters` after letting the lock go; a waiter passes its refusal's generation to `Budget::released_since`, which enables its `Notified` before reading the generation |
| an estimate per kind | an estimate per index and kind |
| one charge per batch, on every lane | one charge per index of a batch, on that index's lanes only |
| rule 4 per kind | per kind for backfill; per index for live (rule 5); the share (rule 3) bounds an index's live bytes |

**What slices 2 and 3 changed about this slice.** The plan assumed a task per index, each reserving for
its own batch. Until 6b one task builds one live batch across every index (slice 2's take), so:

- **The budget is reserved per index inside one build.** A build reserves one part per index, settles each
  part to that index's documents when it lands, and each index's charge rides only that index's lane items.
  Per-index accounting is exact with one build covering several indexes; nothing waited for 6b.
- **A live build takes the indexes that fit and defers the rest.** All-or-none per batch would let one
  index at its share hold every other index's changes back, which is what the share exists to stop. A
  deferred index's documents stay waiting, the live run keeps its position for a later grant, and the
  deferred index's lanes get **nothing** from that build, not even its step (slice 3): a step registered on
  a lane whose documents still wait would count those changes committed before they are built.
- **The deferral needs a bound, and one task needs it now.** With one reader, a live run is closed only by a
  grant; a run whose only documents are deferred would otherwise keep the reading stopped and the other
  indexes with it. So a grant that takes nothing still closes a run that read changes (an empty build), and
  reading goes on, which lets the deferred index's waiting set grow. It is bounded: an index may stay
  deferred while it has fewer than `2 × max_changes` documents waiting (6a's default for
  `max_waiting_per_index`, which the owner approved for 6a, brought forward as a constant); past that the
  live batch waits whole and a full run stops the reading, as on `main`. With one index nothing is
  deferred (`Builds::with_deferral(None)`): one index behaves exactly as before. 6a replaces the constant
  with the `max_waiting_per_index` setting. **Owner to confirm** the constant until then.

**The rules, as built after review round 3** (a build is granted when rules 1–3 allow it, or rule 4 or 5
does). The requirement: with two or more indexes, an index whose backfill or reindex runs keeps building its
steady live stream beside it no worse than `main` (where live had everything the backfill's half left:
`total + estimate ≤ limit`, backfill ≤ half); one index bursting **live** holds at most its share; one index
**stuck** (a parked lane holding its share and its backfill for hours) never stops the others; and the total
stays within a bound the manual states truthfully. The owner's standing rule settles the trade: memory in
the hundreds of MiB is cheap, latency is never traded for it, so the promise wins over a tighter bound.

| Rule | Bound |
| --- | --- |
| 1 | everything held, live and backfill, ≤ `max_built_mib` |
| 2 | backfill, over every index, ≤ half |
| 3 | with two or more indexes with lanes (decision (b)5), one index's **live** bytes ≤ `index_budget_share` of the limit |
| 4 | backfill holding nothing gets one batch, whatever 1 and 2 say |
| 5 | an index holding no live bytes gets one live batch, whatever 1 and 3 say (so live holding nothing takes every index waiting: `main`'s rule for live) |

Rules 1, 2, 4 and 5-with-one-index are `main`'s budget; rule 3 is the share. **An index's backfill bytes
don't count against its share**: backfill is capped as a whole by rule 2, as on `main`. **Rule 5 is what
keeps the promise while one index is stuck**: a healthy index acknowledges its live batches, so it holds no
live bytes again and again, and each time gets a batch whatever the stuck index holds. **`blocked`**
(oldest waiting first: the first index refused by the limit stops younger ones from taking room by it, so a
large batch isn't starved by small ones) applies only to indexes that hold live bytes; rule 5 is never held
back by it. A younger index thus gets at most its one rule-5 batch past an older one waiting for room, then
waits behind it like the rest: no starvation, and one stuck index can't turn into a stop for all.

**One index's batch** is everything a live grant takes of it: take-all. A cap at `max_changes` documents
per index per grant was considered and dropped: a partly taken index's lanes would be registered with the
build's step while some of its documents still wait, so its changes would count committed before they are
built (slice 3's invariant); keeping it correct needs a registration with no step for a partial take, more
machinery than the bound is worth. So the batch is stated in documents: an index left waiting keeps fewer
than `2 × max_changes` documents, and the grant that takes them also takes what the run being granted
marked, so with two or more indexes one index's batch holds the documents of up to about three live runs,
`3 × [batch] max_changes` changes' worth (with one index nothing waits: one run's, as on `main`).

**Bounds, derived:**

- **Total:** the limit, plus one live batch per index (rule 5), plus one backfill batch (rule 4), plus what
  each build in flight settles past its estimate. Guards: `four_indexes_pass_the_limit_by_at_most_a_live_batch_each_and_a_backfill_batch`
  (the reviewer's sequence: 224 KiB), `random_reservations_stay_within_the_stated_bounds` (seeded
  reservations and releases over four indexes; a guard, it doesn't tell rule sets apart), and the parked-lane
  engine test below, which checks the bound at every write for ten simulated minutes.
- **Backfill:** half, plus one batch when backfill held nothing, plus the same settle slack.
- **One index's live bytes:** its share, or one batch when that batch alone is larger, plus the same.
  Guard: the three-index schedule, which watches each index's live bytes at every write against share + one
  batch and fails with the share off.
- **One index's bytes in all:** its live bytes plus whatever backfill it holds within backfill's half.

**History of the rules** (kept short; the PR has the detail). Round 0 counted an index's backfill bytes
against its share and let an index holding nothing pass the limit. Round 1 made rule 4 for live look at live
bytes. Round 2 found a backfilling index's steady stream stopped behind its own backfill, and the per-index
bypass stacking once per kind per index; it counted live bytes only and passed no limit per index. Round 3
found that this let a stuck index stop every healthy one (case A: `a` reindexed and parked, backfill at
half, live at its share, `b` refused by the limit, then reading stopped; case B: three indexes, `a` parked at
its share, `x` oldest with a batch larger than the room left refused and `blocked` holding small `b` back
too): rule 5 now gives every index holding no live bytes its batch, past the limit, and `blocked` never
holds it back.

- **One stuck index never stops the others.** Budget guards: `a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch`
  (A), `a_large_batch_and_a_small_one_both_build_beside_a_parked_index` (B),
  `an_index_holding_no_live_bytes_gets_one_live_batch_past_the_limit` (the case round 2 left untested), all
  red on round 2's rules. Engine guard: `a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes`:
  `orders` reindexed with its lane parked (every document refused under `on_error = "stop"`) for ten
  simulated minutes, its backfill and live documents held; `users` writes bursts of sixteen distinct documents
  a second the whole time. Every `users` change is read and lands, the whole stays within the bound at every
  write, then `orders` is let through and converges. Without rule 5 `users` stops landing after about 25 s.
  It runs with lanes as large as a deployment's: with the harness's 64-item lanes the parked lane fills
  first and the publish blocks, which a deployment's lanes (1,048,576 items) never reach before the budget.
- **Per index, live keeps `main`'s room beside its own backfill.** Engine guard:
  `an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building`: the reviewer's scenario
  (4000 rows, `orders` backfilling 2000 of about 8 KiB behind a sink writing one item per 200 ms, `users` and
  200 distinct `orders` documents changing every 2 ms, budget 1 MiB, batch 16, builds 4), run twice on paused
  time: on the shared budget and on this branch's budget with the share off (`main`'s budget rules, not
  `main`'s code). It asserts `orders`' last live document is built no later than with the share off, and
  before its backfill completes; with the share counting backfill bytes it fails. The test sink costs a fixed
  time per item whatever its size, so the arms' other timings are not a performance comparison and nothing
  else is asserted. Budget guard: `an_index_backfilling_keeps_the_live_room_main_gave_it`.
- **The budget metrics keep their meaning** (review rounds 1–3). `flusso.memory.refused{work}` counts once
  per hold and `flusso.memory.wait{work}` reports one wait per hold and engine run, as on `main`: a live
  wait starts at the first refusal of live documents and ends at the grant that leaves no index deferred; a
  partial grant that defers an index neither ends the wait nor starts another. Whether a hold was counted
  belongs to the engine (`builds::Waits`), its clock to the engine run: when a run ends (an error, an abort,
  any drop of its `Builds`) an open wait is reported up to then, with wake `settle`, and only the "counted"
  flag is kept, so an outage before the next run doesn't stretch the wait, a refusal of the same hold in the
  next run starts a new clock without counting again, and with no refusal nothing more is reported. Guards:
  `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it` (11 before round 1, 1 after),
  `a_hold_across_an_engine_restart_is_one_wait`, `a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment`
  (an hour's outage: was reported as a wait of 3602 s), and `builds::tests`'
  `a_hold_refused_again_in_the_next_run_is_counted_once_and_timed_per_run`. Overlapping holds of two
  indexes are one wait. With one index nothing is deferred: the numbers are `main`'s.
- **An index with no lane** never has documents waiting (`Progress::mark` drops its ids), so it is never
  offered, deferred or counted in a wait; it holds nothing.
- **One index is `main`'s but for one case**: a live run that touched no document is now always granted (an
  empty `admit` never refuses), where `main`'s `fits(Live, 0)` refused it while live held something and a
  settle had left the total past the limit. An empty build holds nothing, so it changes no bound.
- **Order and starvation.** A live batch's indexes are offered oldest waiting first (by their oldest waiting
  position). An index refused by its own share doesn't hold others back. The first index refused by the
  limit stops every younger one that holds live bytes from taking room by the limit. So a stream of small
  batches can't starve a large one: room accumulates until the oldest fits, or it drains to no live bytes and
  rule 5 grants it.
- **No deadlock between an index at its share and the limit.** An index at its share holds live bytes, which
  come back when its lanes acknowledge; meanwhile every index holding no live bytes gets its batch, past
  the limit if need be (rule 5). A parked lane keeps its index at its share for as long as it is parked;
  the other indexes build on.
- **A backfill batch spanning two indexes** (a page ends one index's walk and starts the next) fits whole or
  not at all, by rules 1, 2 and 4 alone.
- **Which indexes share.** The indexes with lanes (`mappings`). Lanes per index don't matter: a batch's part
  is counted once, however many lanes carry it. The share is a fixed fraction of the limit, not divided by
  how many indexes have work.
- **Every byte back once.** A part's reservation gives its estimate back when dropped unsettled (a failed or
  aborted build, a stop, an engine run ending, a dropped future: the `Builds` slots are dropped with it);
  a settled charge when the last copy of its index's items drops. `Budget`'s `Drop` asserts nothing is
  counted, per index too, in every debug test.
- **Operable without per-index metrics.** `flusso.memory.*` and `/status`'s `memory` keep their meaning
  (totals per kind). An index held at its share shows as its backlog pinning confirmation
  (`pinned_by: {"index": …}`, slice 2) and as one live budget wait; bytes per index wait for slice 7.

## 4. Guarantees

| Invariant (CLAUDE.md) | After 8a | Existing guards | New test |
| --- | --- | --- | --- |
| At-least-once: flush-then-ack, landed is the minimum | lane answer adds the pending minimum; registration after send; token published after every send | `confirms_no_position_before_its_flush`, `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, `progress::tests` (`every_interleaving_answers_as_the_model_does`, `concurrent_registration_and_landing_never_answer_past_a_lane`), daemon `failed_*_restarts_*`, `wal_release` e2e | `a_marked_id_pins_confirmation_until_its_build_lands`; the model test with mark/take/publish/give_back; `an_aborted_publish_leaves_no_phantom_registration`; `a_take_with_no_envelope_still_releases_its_ids` |
| An emitted, unrouted change is never confirmed or fenced | `routed` after a run's marks; the source's `next_seq` test | `ack::tests`' `a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`, `the_tick_releases_idle_wal_while_routed_stays_put` | `a_run_half_marked_confirms_nothing_past_its_first_change` |
| Only indexes with lanes | ids of an index with no lane are dropped by the reader | `TwoIndexes` harness (`ingest/tests.rs`) | `an_id_of_an_index_without_lanes_pins_nothing` |
| Fences per sink and index | answers include the pending minimum; `Fences` keyed by item key | daemon `fence::tests`, `fence` e2e, `a_position_becomes_searchable_when_its_refresh_interval_passes`, `a_deferred_position_waits_for_a_caught_up_immediate_write` | `a_fence_waits_for_an_id_still_pending_on_its_index`, `a_fence_on_a_quiet_index_passes_while_another_index_builds` |
| Stall age and the pin | lane items only; backlog per index | `stall::tests`, `a_tie_at_one_source_position_names_the_lane_landed_is_decided_by` | `a_healthy_lane_is_not_stalled_by_its_index_backlog`, `a_backlog_pinning_confirmation_is_named_by_its_index` |
| Read-after-visible (#343) | unchanged: the hold is in the capture | `visibility` e2e, `a_fence_never_covers_a_commit_read_before_it_was_visible` | — |
| Built once | one resolve per change, one build per take for every lane | `builds_a_repeatedly_touched_document_once_per_batch`, `a_backfill_of_related_indexes_builds_each_document_once` | `a_document_marked_while_waiting_is_built_once` |
| Build order is delivery order, per index | take skips in-flight and page-held ids; admission + grant order; keys per index | `a_document_is_never_built_twice_at_once`, `random_schedules_keep_every_document_in_order`, `a_backfill_page_and_a_live_batch_of_one_document_build_one_after_the_other`, sink `a_document_is_never_written_twice_at_once_…` | `a_change_marked_while_its_document_builds_is_built_again_after`, `a_live_take_skips_ids_a_page_is_building`; the seeded schedule over three indexes |
| Backfill requests at-least-once, coalesced | per index; ack after its `SnapshotComplete`; dropped deliveries redelivered | `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`, `a_backfill_with_every_lane_asking_starts_without_waiting`, `a_straggling_reindex_request_within_max_delay_joins_the_snapshot`, `snapshots_go_only_to_the_requesting_lane`, `tests::resume`, `tests::gone` | `each_index_acknowledges_its_own_request_when_its_walk_completes`, `indexes_walk_at_once` (replaces `indexes_are_walked_one_after_another_…`), `coalescing_for_one_index_delays_no_live_change_of_it` |
| Tickets and cut backfills (#435) | per-lane greatest ticket per walk; a mid-walk request waits for the walk | `one_walk_answers_a_lanes_two_requests_under_the_later_ticket`, `a_completion_answering_an_older_request_seeds_nothing`, `a_resumed_request_and_a_fresh_one_of_a_lane_are_walked_once_from_the_first_key` | `a_cut_request_mid_walk_gets_its_own_walk_after_it` |
| A live stream that ends drains queued requests | tasks return after the reader's signal, with nothing pending or queued | `an_unseeded_sink_requests_a_snapshot_then_marks_seeded` | `a_request_queued_for_one_index_after_live_ends_is_served` |
| Graceful stop (#371) | the barrier; the reader returns the stream; pending built, bounded by the lead | `tests::stop`, daemon `tests::stop`, `shutdown` e2e incl. `…a_restart_rewrites_none_of_it` and `random_stops_under_writes_converge_after_a_restart` | `a_stop_mid_run_builds_what_the_reader_marked_last`, `a_stop_with_many_pending_ids_finishes_within_the_deadline`, `a_stop_with_one_index_stalled_lands_the_others` |
| Pacing | the lead bound (6a); the held-index exemption and `max_pending_documents` (6b) | `a_stalled_sink_paces_builds_and_they_resume_when_it_does` | `the_reader_waits_at_the_lead_and_resumes`, `a_parked_lane_bounds_pending_memory`, `an_index_held_by_the_budget_does_not_stop_the_reader` (6b) |
| #120 staging order | unchanged | `fresh_source_*`, daemon `fresh_source_rebuilds_seeded_sinks_before_preparing`, `every_index_of_every_sink_is_staged_before_the_source_prepares` | — |
| A lone change commits at once (#135) | `main`'s run cut; a waiting id is taken once a slot is free | `a_lone_change_on_a_quiet_stream_commits_without_waiting`, `max_delay_caps_a_batch_while_changes_keep_arriving` | — (rerun on the reader) |
| Script failures on the ingest side (#198) | in the index task | `skip_policy_leaves_a_failed_document_out_and_keeps_going`, `stop_policy_fails_the_ingest_engine_on_a_failed_document` | slice 8: `a_stop_failure_on_one_index_lets_the_others_land` |
| Item-level vs flush-wide; lane parking | sink side unchanged | `tests::parking`, `tests::merge`, daemon `a_stop_rejection_halts_its_lane_writing_several_items_at_once` | `a_parked_lane_fills_only_its_index_half_and_other_backfills_go_one_batch_at_a_time` |
| Memory budget | many reservers; rules 1–5 per §3.5 (slice 4) | `random_schedules_under_a_tiny_budget_stay_within_it_and_finish`, `live_keeps_room_beside_a_backfill_holding_its_half`, `a_batch_bigger_than_the_budget_still_builds` | `budget::tests` (`concurrent_reservations_never_pass_the_limit`, `a_release_wakes_every_waiting_task`, `a_release_between_the_refusal_and_the_wait_is_not_lost`, `an_index_holding_no_live_bytes_gets_one_live_batch_past_the_limit`, `the_oldest_index_refused_by_the_limit_holds_younger_ones_back`, `every_index_gets_every_byte_back_once`), `one_bursting_index_of_three_holds_its_share_and_the_others_build`, `a_deferred_index_is_never_counted_committed_before_its_documents_land`, `an_index_past_its_waiting_bound_holds_the_live_batch_whole`, `random_schedules_over_three_indexes_with_one_bursting_stay_within_their_shares` (binds: fails with the share off), `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it`, `an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building`, `an_index_backfilling_keeps_the_live_room_main_gave_it`, `four_indexes_pass_the_limit_by_at_most_a_live_batch_each_and_a_backfill_batch`, `a_hold_across_an_engine_restart_is_one_wait`, `a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment`, `a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes`, `a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch`, `a_large_batch_and_a_small_one_both_build_beside_a_parked_index` |
| Gate fairness | lines per kind; budget asked at grant | `gate::tests` | `a_task_blocked_on_a_full_lane_holds_no_permit` (two indexes, one lane full, `builds = 1`), `a_waiter_refused_by_the_budget_keeps_its_place` |
| `changes_committed` | computed from `Progress` (§3.6) | daemon status tests | daemon `tests::committed`, `progress::tests::committed` |
| Resume checkpoints (8b), reindex per sink | the walk and its marks move as they are | `tests::resume`, `a_killed_backfill_resumes_…` e2e, `reindex_control_stages_and_requests_a_snapshot_without_restarting`, `reindex_operation_targets_one_sink` | — |
| Independence (ADR §1) | new (6b) | — | `a_slow_build_of_one_index_delays_no_other_index`, `a_backfill_of_one_index_delays_no_live_change_of_another` |

## 5. Slices

Each compiles, passes the CI sequence in CLAUDE.md, and merges alone. Each carries its CLAUDE.md
lines; the manual is written by the documentation agent before its PR is ready.

| # | What moves | Tests written first | Could regress; checked by |
| --- | --- | --- | --- |
| 1 | **Item keys.** `Batch.key`, a key counter per index in `Progress`; `Progress` and the sink's `Fences` keyed by it. One task. | `progress::tests` on keys; `a_key_survives_an_ingest_restart` | nothing in behaviour; `live_drain_held` |
| 2 | **Pending sets in `Progress`, one task**, and the stall/pin attribution. The one task marks a run, then takes **across every index's set into one build**, as `main`'s batch spans indexes, published per lane. `newest`/`skipped` go. | pending unit tests (dedup, mark in flight, give back, the minimum, `(seq, counter)` order); the model test; `a_healthy_lane_is_not_stalled_by_its_index_backlog` | confirmation or fences early: model test, `fence` e2e, `wal_release`; lock cost: `live_drain_held` |
| 3 | **Changes committed from `Progress`** (§3.6, owner's option 1): every lane of a sink landed it, or had nothing for it. Moves when a change counts. One task. | daemon `tests::committed` (red on `main`), `progress::tests::committed` | `/status` and metric values; `drain_changes_per_s` steps once (window restarted) |
| 4 | **Budget for many reservers, per index** (§3.5, as built). Rule 3 (`index_budget_share`, one index's live bytes); a live build takes the indexes that fit and defers the rest, bounded at `2 × max_changes` waiting. Changes what a multi-index deployment holds: one index of three bursting live holds at most its share, the others keep building; an index's backfill or reindex leaves its live stream `main`'s room. One task. | budget tests of §4 | a burst on one index of three (§7); the tiny-budget schedule, and its three-index variant |
| 5 | **Gate with lines** (budget asked at grant), the key-page permit. After 4 (both touch `builds.rs`). One task. | gate tests of §4 | `snapshot/rows` at `builds = 1` and 4 |
| 6a | **Reader task + one builder task** over every pending set, the **lead bound** (`max_waiting_per_index`, `max_waiting`), the stop barrier, the stream handed back. No independence yet; `main`'s pacing. | `the_reader_waits_at_the_lead_and_resumes`, `a_stop_mid_run_builds_what_the_reader_marked_last`, `a_stop_with_many_pending_ids_finishes_within_the_deadline`, `an_id_of_an_index_without_lanes_pins_nothing` | the hop and the locks: the full in-process suite, `shutdown` e2e, the end measurements |
| 6b | **Per-index tasks, dispatcher, per-index walks and coalescing**, the held-index exemption and `max_pending_documents`. | the independence tests; `indexes_walk_at_once`; `a_task_blocked_on_a_full_lane_holds_no_permit`; the seeded schedule over three indexes | everything; the end measurements |
| 7 | **Per-index ingest status and metrics**, the backlog, `GET /indexes/<name>/status` (public), `POST /indexes/<name>/reindex` (private, replacing `POST /reindex`), `FlussoIndexBacklog`, the Grafana panel. | status and metric tests | — |
| 8 | **Per-index restart** (8b's other half). | `a_stop_failure_on_one_index_lets_the_others_land`, `a_restarted_index_task_gives_back_its_builds_and_serves_its_request_again`, daemon backoff tests per index | a poison index looping: backoff tests |

**Order and review.** 1 and 4 touch disjoint files and can be reviewed in either order; 2 and 3
follow 1; 5 follows 4. 6a needs 1–5; 6b follows 6a. 7 and 8 follow 6b and are independent of each
other. 1–5 change nothing a user sees except slice 4's per-index half and slice 2's `pinned_by`.

**Slice 2 as built** (where it differs from the plan above):

- **`newest` and `skipped` stay** until 6a. With one task they are still how a lane learns of a
  batch it has nothing in, and the model test keeps its meaning; 6a replaces them with the reader's
  `routed`.
- **Marked as read, granted all or none, admission as on `main`.** The one task marks its run
  (`mark_run`) and offers it to the gate only while no waiting id is in a running build, checked
  before a permit is taken, so no grant is ever given back. A build's ids leave admission when it
  **lands** (`build_landed`), as `main`'s `Builds::holding` did: a later build of one of them gets a
  later slot and publishes after (guard: `a_document_whose_build_landed_is_built_again_while_an_earlier_slot_waits`,
  which passes on `main`'s engine). The live estimate counts distinct documents. `take`'s skip of
  building ids is for 6a.
- **Three steps of a build** decide who answers for it: taken or landed, it is the index's
  **backlog** (`Pin::Backlog`, no lane's stall); once its batch is **sending** (`build_sending`), a
  lane that hasn't registered it owns it (`Pin::Lane`, its stall age); a lane's registration or skip
  settles it for that lane (`published_build` / `skipped_build`, the registry and the build changed
  before one notification, so a fence waiter never misses the move). On a tie, within a lane or across
  lanes, the lane is named.
- **What a failed run leaves behind is forgotten**, not given back (`forget_all` as a run starts).
  Confirmation stayed before it, so the replay marks it again, in batches of `max_changes`, as `main`
  rebuilt it. Giving it back built it all in one batch past `max_changes`, which the tiny-budget
  schedule caught (the budget's bound assumes one batch per build). Guard:
  `what_a_failed_run_left_behind_is_rebuilt_from_the_replay_in_batches` (three batches left behind
  are rebuilt two at a time; under give-back it sees one batch of six). Slice 8's per-index restart,
  which doesn't reopen the stream, needs give-back and brings it with its own bound.
- **A live batch carries its own live run's newest change**, as on `main`: every document it takes
  was marked by that live run (a document waits only between its live run's marking and grant, and
  what a failed engine run left is forgotten), so no fallback is needed, and the type says so
  (`Job::Live.position: Position`, carried out of the gate by `Grant::Live`; a live item whose index
  has no pending build is `EngineError::UnregisteredLiveItem`). Across engine runs what holds is the
  **position contract** (the `ChangeCapture` port): positions strictly increase within one stream, and
  over the capture's life a position never names a different change than it named before, because
  `Progress` keeps each lane's newest position for its whole life. The Postgres capture keeps
  numbering across reopened streams; the engine tests' sources give a replayed change the position it
  had. A floor at the newest position routed to any lane (review round 1) was removed in round 2. It
  did nothing for Postgres, whose replays are numbered past every earlier position; with a source that
  replays a change under the position it had, a replayed change's batch borrowed another lane's newer
  change, and a lane that never got that change answered through it
  (`a_replayed_batch_claims_no_change_another_lane_landed`, which models such a source).
- **A run whose changes build nothing is an empty build**, as on `main`: its slot tells every lane
  it was skipped and every sink its changes settled, in grant order (#204).
- **`pinned_by`** is `engine::Pin`. `/status` gives `{index, sink: null}` for a backlog,
  `flusso.slot.pinned_by` an `{index}` series, and the dev dashboard's WAL panel shows it with the
  slot's whole lag. It is non-null during every live build (the build is the backlog's). Stall age
  counts what a lane holds itself (`Progress::own_oldest`). `FlussoIndexBacklog` waits for slice 7.
- **`Fence::settled` needs no change:** a build left behind (an ingest engine that failed while
  stopping) holds both fence levels at its change, so the wait `settled` skips could never be
  answered (`a_build_left_behind_holds_both_levels_so_settled_cuts_off_no_answerable_wait`).
- **Existing tests changed:** `tests::lanes`' `a_lane_whose_live_item_is_blocked_answers_no_further_than_before`
  now expects `Before(0)` (the pending build holds the blocked lane) where it expected the lane's old
  `Idle`; `pinned_by` comparisons in
  `tests::parking`, `tests/lane_parking.rs` and `stall::tests` wrap the lane in `Pin::Lane`.

## 6. Decisions

### (a) Technical, with a recommendation

1. **Pending sets live in `Progress`**, one lock per index: confirmation and fences read them.
2. **Mark per drained run**, `main`'s batch cut.
3. **Register after each send, publish the token after all sends:** cancel-safe.
4. **Keep grant-order publication per index:** it is what makes releasing admission at landing safe.
5. **`take` skips page-held ids**, so a taken live batch is always admissible.
6. **No page dropping in 8a**: not needed for correctness (recorded in the ADR).
7. **One key-page permit, apart from the build permits;** the pool unchanged.
8. **Gate: one waiter per task, only while it can use a permit; the budget asked at grant.**
9. **Budget with many reservers** (CAS, wake-all, per-index estimate).
10. **`changes_committed` computed from `Progress`.**
11. **A failure fails the engine until slice 8;** a reader failure always does.
12. **A request mid-walk waits for the walk** (no preemption), as `main`.
13. **The dispatcher never blocks** (unbounded per-index channels, `select!` on the stop).
14. **The reader ends first and returns the stream;** tasks finish only after its signal.

### (b) Taken by the owner

All seven as recommended (2026-11-20, recorded on #386): a stop builds what is pending, capped by
the lead; stdout's wording only; the lead and the memory bound with 6b's exemption; the `ingest`
names; the per-index share only with two or more indexes; per-index restart as slice 8; stall age
from a lane's own items, `pinned_by` naming the index when its backlog pins, and a
`FlussoIndexBacklog` alert. Two additions:

**Every limit is a setting.** A `[concurrency]` key, a flag and a `FLUSSO_*` variable each (flag >
env > file), never baked into the lock. A key left unset takes its derived default. A value that makes
no sense is refused at start, naming where it came from, as `[backoff]` is (`daemon::backoff`).

| Key (flag / env) | Default | Refused at start | Lands in |
| --- | --- | --- | --- |
| `max_waiting_per_index` (`--max-waiting-per-index` / `FLUSSO_MAX_WAITING_PER_INDEX`) | `2 × [batch] max_changes` | 0 | 6a |
| `max_waiting` (`--max-waiting` / `FLUSSO_MAX_WAITING`) | `2 × builds × max_changes` | 0, or below `max_waiting_per_index` (as set or derived) | 6a |
| `max_pending_documents` (`--max-pending-documents` / `FLUSSO_MAX_PENDING_DOCUMENTS`) | 1,000,000 | 0, or below `max_waiting` | 6b |
| `index_budget_share` (`--index-budget-share` / `FLUSSO_INDEX_BUDGET_SHARE`) | 0.5 | outside (0, 1] (`nan` too), or not a number; a whole `1` is taken | 4 (built) |

- The two `max_waiting` keys are the reader's lead (§3.1); they also cap what a stop builds.
- `index_budget_share` is rule 2's fraction of `max_built_mib`. Decision 5 stands: with one enabled
  index it applies to nothing.
- The designer's config form renders from the `[concurrency]` schema: `just schema-gen` in each slice.

**Per-index HTTP paths** (slice 7, the owner's call; nothing before it):

| Route | Surface | What |
| --- | --- | --- |
| `GET /indexes/<name>/status` | public, unauthenticated like `/status` | the same JSON as `/status`'s `ingest.indexes.<name>`; an unknown or disabled index is `404` naming the known indexes |
| `POST /indexes/<name>/reindex?sink=…` | private, Basic auth | replaces `POST /reindex?index=…&sink=…`, which is removed (breaking); `flusso reindex` follows |
| `GET /indexes` | private, Basic auth | unchanged: every sink's seeding states |

So `/indexes/…` is served by both listeners: `…/status` only on the public one, `GET /indexes` and
`…/reindex` only on the private one. Each listener answers `404` for the other's routes, as now.

**`FlussoIndexBacklog`** needs the backlog's age (`flusso.index.backlog_age`), which exists from
slice 7, so the alert lands there; slice 2 only stops blaming lanes for it.

## 7. What to measure at the end

Nothing here was measured. After 6a and again after 6b, same runner, `main` against the branch,
interleaved:

| Where | What | Why it could move |
| --- | --- | --- |
| in-process `engine` `live_drain_held/sinks{1,2}/{1,64,256}` (the PR gate) | changes/s | the reader hop, the pending locks, the lead waits; `/1` shows the per-batch cost most |
| in-process `engine` `snapshot/rows`, at `builds = 1` and 4 | rows/s | a walk in its own task, the key-page permit |
| Docker `engine` `pipeline` (`change/item_update`, `change_burst`), `just review-ab` | latency, burst | real resolve and build under the new hop |
| scenario `reference`, `default`, `builds = 4`, `writes = 4` | visible latency p50/p99 | the hop and the wake per run |
| same | burst drain, writes and documents per write | batches cut per index; documents per write within ~15% of `main` |
| same, **a burst on one index of three** | drain, latency of the other two | slice 4's per-index half; 6b's independence |
| same | backfill documents/s, seed wall | walks at once; expected up, must not go down |
| same | peak RSS, CPU | pending sets, `2 × builds` slots per index |
| same, a stop under steady writes | stop duration | the lead built at the stop |
| scenario `complex` (one index) | latency, backfill, RSS | pure overhead; rule 2 with one index |
| scenario `sync-standby`, by hand | latency during a hold | the reader behind the visibility hold |
| `/metrics` stage attribution | `Resolve` busy share; `Build`/`Publish`/`Idle` per index | where a regression comes from |

Sanity only, expected flat: `pgoutput`, `resolve_no_io`, `documents`, `render`, `scripted`.
