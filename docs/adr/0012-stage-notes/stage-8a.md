# Stage 8a design: one reader, one task per index

**Status:** design, against `main` at `a187cf91` (2026-11-20), revised after an independent review of
`61323c20`; the owner's decisions are in §6 (b). Slices 1–6a merged (6a: #446, `7e10c43c`); 6b's
design is §3.9, for review before code. Slice 1 merged (#436). It replaces the note written before stages 3–7, stage 9's first half (#427), #434
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

**Since 6a, and in 6b's design (§3.8, §3.9):** the reader, the dispatcher and the index builders are
futures of the engine run's one task, not spawned tasks; builds and key reads run on tasks of their own.
Read "task" below as "builder" where it names an index's.

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
  **Acceptance test** (recorded in slice 4): `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap`
  with the reading **not** stopping: `orders` parked at its share and changing one of fifty documents a
  second for ten minutes, `users` read and landing the whole time. In slice 4 the reading stops at second 61,
  when `orders` has `2 × max_changes` documents waiting.

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

**Gate.** `builds` permits shared by every index task. **Rules as built in slice 5**, written before the
code; the bounds below follow from them.

| Rule | |
| --- | --- |
| G1 | at most `builds` permits are out; a build holds one from its grant until it lands (the `Permit` drops with the build's slot entry, failed or aborted builds included) |
| G2 | one FIFO line per kind (live, backfill); a waiter joins the back of its kind's line and keeps its place until it is granted or dropped |
| G3 | a waiter is **eligible** unless the budget refused it **at the budget's current generation**; a refusal made while behind (G6) stands only while the waiter still is behind. While any waiter is in a line the budget moves its generation on every change of what it holds, not only a release (`Budget::watch`: a release, a grant, a settle larger than its estimate), so a refusal at the current generation is exactly what the budget would answer now. Standing is computed from every waiter's refusal and the generation at the moment of asking, under the lock, never from a mark another waiter has to refresh (review round 2) |
| G4 | a free permit is offered to the oldest eligible live waiter, except on every fourth **contested** grant (both lines have an eligible waiter), when it goes to the oldest eligible backfill waiter; a grant only one kind could take doesn't move the count (`main`'s rule, per waiter) |
| G5 | **the budget is asked at grant**, under the gate's lock (gate lock, then the budget's; the budget never takes the gate's): the offered waiter reserves its batch from the gate's own budget (`Gate::new` takes it; `acquire` takes the batch's parts and asks the budget itself, so every refusal carries that budget's generation). It reserves only at the generation it chose the waiter at (`Budget::reserve_at`): when a release or an add came between the choice and the reservation, nothing is reserved or recorded and the waiter is chosen again at the new generation (review round 3). That needs a real change each time, so it can't spin; refused, it keeps its place, becomes ineligible (G3) and the offer moves on by G4 |
| G6 | a waiter is **behind** while an older waiter of its kind is ineligible with a refusal **by the limit** at the current generation; it reserves *behind*: it takes no room by the limit (for live only rule 5, for backfill only rule 4 can grant it), so small batches can't starve a large one (the budget's oldest-first, across waiters). A refusal by an index's **share**, or one the budget has changed since, holds no one back: one stuck index never blocks another's line. A waiter refused **while behind** is eligible again as soon as nothing holds it back: the one ahead left (granted or dropped), was refused by its share, or the budget changed (review rounds 1 and 2) |
| G7 | every change of the gate's state (a permit freed, a waiter leaving, a refusal, a grant) wakes every waiter, after the gate's lock is let go (a grant's own add wakes the budget's waiters with it held, which only schedules them). A change of the budget wakes only the waiters holding a refusal, through `Budget::released_since`, and the gate adds no wake-up of its own (review round 3): those are the only waiters whose standing it changes. It makes refused waiters eligible, which can only take an offer away from a waiter without a refusal (an older waiter becomes the oldest eligible of its kind, or a contested turn goes to the other kind), never give one to it. a waiter registers its wake-up before it reads the state, so no wake-up is lost. A permit given back wakes only when someone is in a line, decided under the lock (a waiter joins under it before reading the state, so it either sees the permit free or is woken): with no waiter, as today, a landed build costs no wake-up |
| G8 | cancel-safe: a waiter dropped before its grant leaves its line and wakes the others; a permit dropped gives itself back, once. A waiter watches the budget from joining its line until it leaves it, granted or dropped, so a dropped waiter leaves no watch behind |
| G9 | **every build waits in a line** (slice 6a; until then the one task asked `Gate::choose`, which yielded to eligible waiters but never reserved behind, and is gone): in a grant pass the builder joins the line of each kind it has a batch ready for and a slot to run (`Gate::join`), asks once (`Waiter::try_grant`), and leaves before the pass ends. With both kinds ready it is in both lines at once, so G4's count moves as it did; every reservation obeys G5 and G6. So the order of grants, the every-fourth rule, a second live build only while the waiting batch is full, and `builds` as the cap are `main`'s |

**Key pages** don't take build permits: `main` reads its one page beside the builds. **K1**: one key-page
permit shared by every walk (`tokio::sync::Semaphore`, FIFO), held from the start of a key read (a page, or
an index's bound) until it returns or its walk is dropped (it lives inside the walk's stored fetch, so a
dropped walk or a stop gives it back). With one walk at a time (this slice) it is always free. The builder
pool stays `builds + 1` plus the resolver. Raising it to several pages is a later, measured change (the pool
grows by as much).

**Bounds, from the rules.**

- Builds at once ≤ `builds` (G1); key reads at once ≤ 1 (K1); builder connections ≤ `builds + 1`, as now.
- **Passed over, eligible.** The oldest eligible backfill waiter is passed over by at most three contested
  live grants in a row (G4). The oldest eligible live waiter is passed over only by every fourth contested
  grant. A waiter further back waits for the ones ahead of it, FIFO (G2).
- **Passed over, refused.** While the budget refuses it, a waiter is passed over by any eligible one (G3,
  G5): one waiter the budget can't fit never holds a permit idle. Once the budget changes it is eligible
  again (G3) and offered before every younger waiter of its kind: one chosen before the change reserves
  only at the generation it was chosen at, so it gives the offer back (G5). While it is refused by the
  limit, the younger waiters of its kind take no room by the limit (G6); a waiter of the other kind may,
  on a grant G4 gives that kind. By the budget's rules it waits at most until its index holds no live
  bytes (rule 5) or backfill holds nothing (rule 4).
- **The stuck case.** A stuck index's waiter refused by its share holds no other waiter back (G6); one refused
  by the limit holds others only from room by the limit, never from rule 5 (a healthy index holding no live
  bytes still gets its batch). A full lane or a sink down holds no permit: a permit is held by a running
  build only (G1), and from 6a a task joins a line only while it can use a permit now.
- **A stop while waiting** drops the waiter (G8): it leaves its line at once.
- **Refused while behind.** A waiter refused only because an older one the limit refused was ahead is
  eligible again, and woken, as soon as nothing holds it back (G6): the one ahead left, was refused again
  by its share, or the budget changed. A waiter is held back only by an older one whose refusal by the
  limit is what the budget would answer now (G3), so it never waits behind an answer that is out of date.
- **What a waiter reads of another.** Checked in review round 2, for each fact one waiter reads that
  another's actions set, after a release, a settle that adds bytes, a grant and a drop:
  - *eligibility and "behind"*: computed at each ask from the refusals and the budget's current
    generation, which moves on every change of what is held while anyone waits (G3); a grant or drop
    removes the waiter from its line under the lock, so the next computation no longer sees it;
  - *the selected waiter*: computed at each ask from the lines (G4), never stored;
  - *the every-fourth count*: one counter, moved only under the lock by a contested grant (G4);
  - *a refusal's reason* (`by_limit`): decided by the budget at a generation; read only while that
    generation is current, so a settle that turns a limit refusal into a share refusal moves the
    generation and the reason is asked again;
  - *free permits*: one counter under the lock (G1).
  Each case has a test: `a_waiter_behind_reads_the_older_ones_standing_after_a_release_not_its_old_refusal`,
  `a_waiter_behind_is_granted_when_the_older_one_is_refused_by_its_share_after_a_release`,
  `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again`,
  `a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves`,
  `a_selected_waiter_dropped_before_it_runs_wakes_the_next`.
- **Cost of watching.** While a waiter is in a line, every add (a grant, a settle larger than its estimate)
  moves the generation and wakes the budget's waiters, who ask again. With no waiter, as today, an add
  costs one comparison under the budget's lock and no wake-up (`with_no_waiter_an_add_moves_no_generation`,
  also after a waiter was granted and another dropped). One case to measure in 6b: a refused backfill
  waiter whose turn has come (the count at three) is asked again after every live grant, since each
  grant's add makes its refusal out of date; it is refused again without moving the count, and the next
  grant is live. So while it waits, every live grant costs one more budget decision.
- **Metrics** don't move: the gate records nothing of its own in this slice; budget waits stay
  `Builds`' (slice 4), and with one task (G9) every grant is decided where it was.

**Mutations of the gate's rules** (re-run in slice 6a's review round 1, over every engine unit test, with
`acquire`'s body in `Waiter::try_grant`, `Gate::choose` gone and the builder on `Gate::pass`: every entry
below fails as listed or more (`G1` 13 tests, `G2` 17, `G4` "always live" 7 with the `a_pass_alone_*`
tests), `G3/G6` still equivalent; the fast pass's own mutations are R5's in §3.8). Each mutation
compiles (`std::hint::black_box` keeps a constant from being folded away) and is run against
`gate::tests` and `snapshot::tests`' `walks_read_one_key_page_at_a_time`, with a per-test limit of 10 s
(nextest `terminate-after`), since a waiter that spins without yielding stops a paused-time runtime and
its tokio timeout with it. "Hang" marks a test the limit ended.

| Mutation | Tests that fail |
| --- | --- |
| G1: no check of free permits | 12 gate tests |
| G2: a waiter joins the front of its line | 15 gate tests |
| G3: a refusal stands after the budget changed | `a_waiter_behind_reads_the_older_ones_standing_after_a_release_not_its_old_refusal`, `a_waiter_behind_is_granted_when_the_older_one_is_refused_by_its_share_after_a_release`, `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again`, `a_younger_waiter_behind_a_limit_refusal_takes_no_room_by_the_limit`, `a_waiter_refused_by_the_budget_keeps_its_place_and_the_permit_goes_on`, `a_refusal_does_not_move_the_count`, `a_release_between_choosing_a_waiter_and_its_reservation_offers_the_permit_again` |
| G3: an add doesn't move the budget's generation while watched | `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again` |
| G3: an add wakes no waiter | `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again` |
| G3: every add moves the generation, waiter or not | `with_no_waiter_an_add_moves_no_generation` |
| G3: a waiter never unwatches | `with_no_waiter_an_add_moves_no_generation` |
| G3/G6: a limit refusal the budget changed since still holds younger waiters back | none: **equivalent**. A younger waiter is asked only when every older waiter of its kind is ineligible, that is, holds a refusal at the current generation, so no stale refusal is ever read for "behind" (G4 picks the oldest eligible) |
| G4: contested grants always go live | `choose_takes_live_first_but_every_fourth_contested_grant_is_backfill`, `every_fourth_contested_grant_goes_to_the_oldest_backfill_waiter`, `an_uncontested_grant_does_not_move_the_count`, `a_refusal_does_not_move_the_count` |
| G4: the count moves on every grant | `an_uncontested_grant_does_not_move_the_count`, `a_refusal_does_not_move_the_count` |
| G4: a refusal moves the count | `a_refusal_does_not_move_the_count` |
| G5: a refusal never makes a waiter ineligible | 10 gate tests, each a hang (the refused waiter is asked again without yielding) |
| G5: no wake-up after a refusal | `a_refusal_wakes_the_waiter_behind`, `a_refusal_does_not_move_the_count`, `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again`, and two of the round-2 stale-refusal tests |
| G5: a reservation ignores a generation that moved since the choice | `a_release_between_choosing_a_waiter_and_its_reservation_offers_the_permit_again` |
| G6: never behind | `a_younger_waiter_behind_a_limit_refusal_takes_no_room_by_the_limit`, `a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves`, and the three round-2 stale-refusal tests |
| G6: a share refusal holds younger waiters back | `a_waiter_refused_by_its_share_holds_no_one_back` and the three round-2 stale-refusal tests |
| G6: a refusal made while behind stands with nothing ahead | `a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves` |
| G7: no wake-up after a grant | `a_grant_wakes_the_waiter_behind_for_the_permit_left` |
| G7: a dropped permit wakes no one | 8 gate tests |
| G8: a dropped waiter stays in its line | `a_waiter_dropped_before_its_grant_gives_its_place_to_the_next`, `a_selected_waiter_dropped_before_it_runs_wakes_the_next`, `a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves`, `many_waiters_never_hold_more_than_the_permits_and_every_one_is_granted` |
| G8: a dropped waiter wakes no one | `a_selected_waiter_dropped_before_it_runs_wakes_the_next`, `a_waiter_refused_for_being_behind_is_eligible_again_when_the_one_ahead_leaves` |
| G9: the builder never leaves the lines (slice 6a; until then "the one task ignores the lines") | 21 engine tests (R5 of §3.8); G6's "never behind" also fails the builder's `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused` |
| K1: no permit taken for a key read | `walks_read_one_key_page_at_a_time` |

Removed in round 3: the wake-up of every waiter that a refused waiter sent when it saw the budget change.
No test needed it, and G7 says why none can: a change of the budget wakes every waiter whose standing it
changes.

**What waits for 6a/6b.** The lines (G2–G8) have no waiter in production until index tasks exist; this
slice proves them with many waiters in `gate::tests` and keeps the one task on G9. "At most one waiter per
task, only while it can use a permit now" (a batch ready, not publishing a full lane's item) is the
caller's rule, built with the tasks. Two requirements recorded in review round 1 (no code in slice 5):

- **K1 with several walks (6b).** The key-read permit lives inside the walk's stored fetch future, which
  runs only while its walk is stepped. With several walks, a walk that stops being stepped mid-read (its
  batch full behind a parked lane) would keep the one permit and stop every other index's backfill. 6b
  must run each read with its permit so that it finishes without its walk being stepped (a spawned read
  handing its page to the walk), or give the permit back when stepping stops. Acceptance test:
  `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`.
- **The one task's direct path (6a, done).** `Gate::choose` yielded only to eligible waiters and never
  reserved behind, so beside real waiters it would have passed a limit-refused waiter and taken room by the
  limit, which G6 forbids. **6a removed it**: every build goes through the lines (G9, R5 of §3.8).
  Acceptance test: `builds::tests`' `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused`.

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
  deferred (`Builds::with_deferral(None)`): one index behaves exactly as before. 6a replaced the constant
  with the `max_waiting_per_index` setting (same default), which is also the reader's lead per index.

**The rules, as built after review round 3** (a build is granted when rules 1–3 allow it, or rule 4 or 5
does). The requirement: with two or more indexes, an index whose backfill or reindex runs keeps building its
steady live stream beside it no worse than `main` (where live had everything the backfill's half left:
`total + estimate ≤ limit`, backfill ≤ half); one index bursting **live** holds at most its share; one index
**stuck** (a parked lane holding its share and its backfill for hours) leaves the others building for as long
as this slice can (below: until its own backlog reaches the deferral cap); and the total
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
keeps the others building while one index is stuck**: a healthy index acknowledges its live batches, so it holds no
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

- **One stuck index leaves the others building, until its backlog reaches the cap.** A parked lane never
  gives its live bytes back, so the stuck index is never granted again (rule 5 never applies to it) and its
  waiting set grows by one per distinct document changed. At `2 × max_changes` waiting the live batch waits
  whole and the reading stops for every index (the slice-2 deferral cap). The full promise needs 6b's
  exemption of an index held by the budget from the lead (counted against `max_pending_documents`), which
  needs takes that are not all or nothing. Pinned by
  `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap` (`orders` parked and changing
  one of fifty documents a second: the reading goes on until second 61, when 32 = `2 × 16` are waiting, then
  stops for `users` too). Against `main`, with no backfill of the stuck index: `main` stops every index once
  the stuck index fills the whole budget; this slice once it fills its share and has the cap waiting, so
  sooner for a stuck index changing many distinct rows, later (never, by the budget) for one rewriting a few
  documents. Budget guards: `a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch`
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
  200 distinct `orders` documents changing every 2 ms, budget 1 MiB, batch 16, builds 4), on paused time.
  Slice 4 ran it twice, on the shared budget and with the share off as a stand-in for `main`'s rules, and
  asserted the shared arm no later. **Slice 6a dropped that comparison**: with the reader reading ahead, the
  test sink's fixed cost per item makes the arms' order a matter of how items are cut. Both arms land
  `orders`' live documents about three times sooner than slice 5's engine (21.2 s shared and 17.6 s share off,
  against 59.2 s and 66.8 s), and the bug the comparison was written for (the share counting backfill bytes)
  passes it (17.2 s). It now asserts `orders`' last live document is built before its backfill completes.
  The budget guards catch that bug (`an_index_backfilling_keeps_the_live_room_main_gave_it` and four more fail
  under it).
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

### 3.6 Changes committed

**Status: built in slice 3** (owner's option 1, 2026-11-20; restored here, the section was dropped by
mistake in `7ab45856`). A change is committed for a sink once **every lane of that sink has landed it,
or had nothing for it**, with every change read before it. A stalled index makes its sink's
`changes_in_flight` grow for all of that sink's indexes: the count follows the oldest unlanded change,
as confirmation does.

**Why it comes from `Progress`:** once several builders publish (6b), a live run's changes touch
several indexes whose batches are built and published separately, so no single item can carry the
count. Read by `/status` (`sinks.<s>.changes_committed`, `changes_in_flight`), `flusso.changes.*`, the
stop's deadline message and `dev/bench`'s `wait_drained`.

**Which moments moved against `main`** (each later, never earlier; totals end equal to the reads):

| Case | `main` credited | Since slice 3 | Test (`daemon` `tests::committed`) |
| --- | --- | --- | --- |
| a batch with items on two or more lanes of a sink | when the first lane's item lands | when every lane's item lands | `a_change_counts_once_every_lane_of_the_sink_landed_it` |
| a batch with no item for a sink | at publish | when that sink's lanes landed every earlier item | `a_change_with_nothing_for_a_sink_counts_after_its_earlier_changes` |
| an earlier item still out on another lane of the sink | later batches credited as their first item lands | nothing past it until it lands | `a_stalled_lane_holds_its_sinks_count_for_other_indexes` |
| a redelivered item that had landed | credited again | once | `a_redelivered_item_that_had_landed_counts_once` |
| an engine run ends with reads never published; the source replays them | never credited | both reads count once the replay lands | `every_change_read_counts_once_per_sink_across_a_replay` |

**As built (slices 3 and 6a).** `Progress` counts every live change read and closes a **`Step`** at
each live grant, through the candidate's read count (6a's `close_step_through`). Every lane of the
deployment is registered with every step, with an item (`published_build`) or without
(`skipped_build`). A lane keeps the steps that can still answer, at most `k + 2` for `k` unlanded
items (`progress::tests::parked`); a sink's count is the read total at the oldest step its lanes have
committed. Per read, not per position: a replayed change is a second read. Reads an ended run never
granted count with the next step routed. Nothing listens to the count; readers poll it.

**6b replaces the global step with a frontier per index** (§3.9, I5): with independent builders a
quiet index's lanes are registered with no other index's step, so the global order the steps rely on
is gone. The definition above does not change.

### 3.7 Run lifecycle

**Status:** rewritten for 6a as built and 6b's design (the plan's version spawned the reader and the
index tasks in a `JoinSet`; 6a measured why not, §3.8). Everything below is futures of the engine
run's one task, plus the builds and key reads, which run on tasks of their own and are aborted when
dropped.

**The stop barrier.** A builder may return from a stop or a live end only once the reader has
**ended** (handed over its last run and returned the stream). Otherwise a builder could see its set
empty and return while the reader, finishing its last run, still marks an id that nobody builds:
`drain_lanes` checks only the lanes, the close confirms short of it, the process exits 0 and the
restart rewrites landed documents.

| Event | Reader | Builders (one per index from 6b) | The engine run |
| --- | --- | --- | --- |
| start | — | nothing in flight: `forget_all` ran first (R10) | `prepare`, open live, poll reader, dispatcher and builders together |
| live stream ends | hands over its last run, returns the stream | after the reader's end: build what waits, serve queued requests, return | `Ok` once every builder returned and the request lane is empty (`main`'s rule) |
| stop (#371) | reads nothing more, hands over its cut run, returns the stream | abandon the walk (deliveries dropped, redelivered next start); after the reader's end: finish running builds, build and publish what waits (at most the lead; an exempt backlog is left, §3.9 I11), return | `drain_lanes`, `close_live` on the returned stream, poll it to its end (unchanged) |
| a builder fails (6a–7) | dropped in place | the others dropped in place | `Err`; the daemon restarts the engine with `[backoff]`, as `main` |
| a builder fails (slice 8) | goes on | that builder alone: give back, its own `Backoff`, restart; its dropped requests come back through the lane | — |
| a panic | unwinds the run's task | as the reader | the daemon restarts the engine, as `main` |

**What a stop costs**, against `main`: `main` builds one batch and finishes `2 × builds` slots. 8a
builds at most the lead (2,048 ids at defaults over every index, 512 per index) plus one run, and
finishes the slots in flight: a few batches more, well inside `--shutdown-timeout-secs` (30 s) at any
realistic build rate. An exempt index's backlog (6b) is never built at a stop.

### 3.8 Slice 6a: one reader, one builder

**Status:** rules written before the code (slice 6a). One reader reads and resolves; one builder does
everything else `main`'s one task did. Both are futures of the engine run's one task (review round 3,
below); no index task yet (6b).

**Who owns what.**

| Task | Owns | Never does |
| --- | --- | --- |
| **Reader** (a future of the engine run, polled first) | the live stream, the resolver, the run being read | build, publish, confirm, wait on a lane, touch a request |
| **Builder** (the engine run's other future) | `Builds` (slots, gate waiters, budget waits), the lanes' producers, the request consumer, snapshots, the queue of handed-over runs, confirmation and the 1 s tick | read the stream, resolve |
| `Progress` (shared) | pending sets (the reader marks, the builder takes), the read count, steps | — |

**What crosses between them, and how.**

- Reader → builder: one `Run` per run over an **unbounded channel** (each change's position and the
  instant the reader read it, the read count through its last). The channel closing is the reader's
  end; a one-shot then carries the live stream (or the reader's error) to the builder.
- Builder → reader: after every live grant, a wake of the reader's one registration (`AtomicWaker`;
  `Notify` until review round 3) when it waits (a flag the reader raises before it reads the lead
  again, with a `SeqCst` fence on each side: either the reader sees the grant's take, or the grant
  sees it waiting; its waker is registered before that check).
- Both: `Progress`. The reader writes marks and the read count; the builder takes and closes steps.

**One task, two futures (review round 3).** The measurement of round 2 found every live batch about
1.5 µs dearer than `main` locally (8 µs on CI) at `max_changes = 1`, and nothing else: with the lead
unbounded the same bench ran in a third of the time, so the cost was the round trip between two
tasks, a hand-over waking the builder on another worker and a grant waking the reader back. So the
reader is not spawned. The engine run polls the reader's future and the builder's in its own task
(`ingest/together.rs`), each with a waker of its own: woken while the task is being polled, a waker
raises its future's flag and the poll goes on with that future; woken from elsewhere, it also wakes
the task. So a hand-over or a grant costs a flag, not a trip through the scheduler. Which of the two a
pass polls first swaps on every poll (review round 4), so neither spends the whole of tokio's
per-poll budget every time; no rule depends on the order, since a run handed over is queued before
any batch is formed (R3). The flags and the
"being polled" mark are bits of **one atomic byte** (review round 4): a wake is one `fetch_or`
that also reads the mark, and a pass ends with one `compare_exchange` that drops the mark only while
no running future's flag is raised, so a wake is either taken by the pass or wakes the task, on any
architecture (one location's changes are seen in one order by every thread; round 3's two
locations needed a `SeqCst` store-load argument). A loom model checks every interleaving and loom's
acquire/release model (`every_wake_is_taken_by_a_pass_or_wakes_the_task_in_every_interleaving`).
Only running futures' flags count: an ended reader, woken by the stop or a stale waker, keeps no
poll going. A poll ends when no running future's flag is raised, or after 32 passes, yielding to the
worker and woken again at once. A spent cooperative budget ends a poll by itself: tokio wakes a
future it refused for want of budget only once the task's poll has returned, so no flag is raised
for it meanwhile (a check of `has_budget_remaining` before each further pass was tried and dropped:
its mutation broke nothing, for that reason). Concurrency between the two is
interleaving at their awaits, which is what `main`'s one task had: the reader's waits (the stream, a
resolve, the lead) overlap the builder's (a build, a lane, the budget, a page), and neither runs while
the other computes. The reader runs without yielding only while the stream has changes ready and the
lead has room, so the builder waits at most for a lead's worth of resolving (R2), and no longer than
32 passes before the worker's other tasks run. **The stop is watched once per run**: `together` polls
`Stop::requested` only when it fires and then wakes the reader, which reads the stop flag on every
poll and registers nothing for it (registering it at each wait at the lead took the stop's lock
twice per wait, every second batch at `max_changes = 1`). Nothing is spawned for the reader, so
nothing of it outlives its run, and dropping the run drops it in place (R9). 6b's design keeps every
index's builder a future of this task too (§3.9, I1).

**The rules.**

| Rule | |
| --- | --- |
| R1 | **A run** is what `main`'s `drain_ready` buffered: the changes the stream has ready, cut at `max_changes` changes, at `max_delay` from the run's first change, when the stream would block, or at the stop. The reader resolves each change, then **marks** every id of the run (`Progress::mark`, one lock per touched index; ids of an index with no lane are dropped there), then **counts** the run's changes read (`Progress::read_changes`), then **hands the run over**, each change with the instant it was read. Marks, count, hand-over, in that order. |
| R2 | **The lead.** Before reading a run the reader waits while one index has `max_waiting_per_index` ids waiting (marked, not taken), or every index together has `max_waiting`, or `max_waiting` changes are read and not yet in a granted batch. It waits on the builder's take and the stop, nothing else. Finding the lead reached, it registers its wake-up, says it waits, and reads the lead again, so a take after the first read is seen by the second and one after the second wakes it (review round 3: with room, the first read is all it costs). The stop is read on every poll; the engine run's watch wakes it (above). A run may overshoot by its own ids and changes, once. |
| R3 | **Batches as `main` cut them** (review round 1). `main` stopped reading once a batch closed and read the backlog the moment the next grant reopened it, so its batch's deadline ran from the later of its first change's read and the previous live grant. The builder's live candidate is, from the oldest change not yet granted, every change read before `max(that change's read, the last live grant) + max_delay`, up to `max_changes`, cut inside a run where needed (the reader's runs end where the stream would block, not where `main`'s batch closed), plus what an earlier grant left waiting for the budget. It is **full** at `max_changes` changes. **A batch is formed only after the hand-over is drained**: `Runs` owns the channel's receiving end, and both ways to form a candidate (`candidate`, `full_candidate`) queue every run already handed over first (review round 4; round 2 had each arm of the builder's loop call `take_handed`, and three arms were untested): `select!` picks among ready arms at random, and an arm taken before a run waiting in the channel would otherwise grant without changes read before it. Beside a running live build only a full candidate may start, so a hand-over there walks nothing while fewer than `max_changes` changes are queued, nor once the candidate is known to be cut by its deadline, which only a grant moves (`Runs::full_candidate`; review round 2). A trickle read while a live build runs is one batch after it, a backlog read while builds outlast `max_delay` is taken whole, up to `max_changes`, and a second live build starts only beside a full candidate. At the end (`settle`) the deadline is ignored: everything read is built in batches of `max_changes`. Checked against a model of `main`'s loop over the same arrivals (`main_batches` in `ingest::tests::reading`), valid for arrivals on a millisecond grid without two at one instant: tokio's paused timer rounds a sleep up to the next millisecond, so microsecond arrivals reach the engine at other instants than the model reads them (the only divergence the review of round 3 found). **Tokio's cooperative budget** (review round 4): the reader and the builder spend one budget per poll, as `main`'s one task did. Once it is spent, a tokio-backed stream answers the reader's `now_or_never` as pending, so the run is cut and, the stream looking blocked, the builder may grant the batch then, short of `max_changes`, as `main` committed at its own `now_or_never` cut. The reader's next read polls the stream with its own waker, which the spent budget wakes at once, so it is never parked with changes ready, and a spent budget ends `together`'s poll by itself, since tokio wakes what it refused only once the poll has returned (`a_burst_past_the_cooperative_budget_neither_stalls_nor_breaks_a_batch`: 5,000 changes through a tokio channel, 40 runs, 21 batches of about 253 after a first of 128, every change landed). What does differ from `main` is that two futures share the budget: polled first every time, a reader under a burst would spend it all on every poll and leave the builder only pending answers until the lead stopped it, so `together` swaps which one a poll takes first (before the swap the same burst gave 24 batches, every third about 125; `a_spender_polled_first_starves_nothing`, `a_spender_polled_second_starves_nothing`). |
| R4 | **A take never reaches past its candidate.** A live build takes, from each index the budget admits, only the ids waiting at or before the candidate's last position (`take_through`), closes the step through the candidate's read count (`close_step_through`), and carries the candidate's last position. A newer run's id waits for the next grant. The candidate is offered only while none of those ids is in a running build (`main`'s admission). |
| R5 | **Every build goes through the lines.** `Gate::choose` is gone. A grant pass (`Gate::pass`) puts each batch the builder has ready, with a slot to run it, in its kind's line, asks once (G4 between its two batches, G5–G6 under the gate's lock), and leaves before the pass ends. So it waits in a line only while it can use a permit now, and every reservation obeys G6. With no waiter in either line (one task, today) the pass is one hold of the gate's lock: its batches stand at the front of empty lines, nothing joins, watches or wakes (review round 1, for the hot path). |
| R6 | **Confirmation and fences** move as on `main`: a marked id holds its index's lanes (the pending minimum); a change read but not yet routed by a build (a queued run, or the reader's run in progress) is never confirmed or fenced, by the source's own `next_seq` test, as `main`'s buffered run was. The builder confirms after every grant and landing, and on the tick. |
| R7 | **The stop.** The reader checks the stop before every read and on every poll (it registers nothing for it: the engine run watches the stop once and wakes the reader, review round 3); once it is requested it reads nothing more, marks and hands over what it read (its run cut short), closes the channel and returns the stream. The builder, seeing the stop, abandons a snapshot as `main` does, then **waits for the reader's end** (the barrier), takes every run handed over, builds, publishes and drains them (`settle`, then `drain_lanes`), and closes the stream with `close_live`. A stop while the reader waits at its lead ends that wait at once. |
| R8 | **The stream's end** is R7 without `close_live`: the reader hands over its last run and returns; the builder serves queued requests, then settles. |
| R9 | **Errors and panics.** A reader error (the stream's or a resolve's) ends the reader, its run not handed over; the builder, at the channel's end, takes its result, abandons its builds and returns that error. A builder error ends the run's future, which **drops the reader in place** before the run returns (review round 3; round 1 aborted and awaited a spawned task): nothing it does (a mark, a count, its stream) outlives the run, so the next run's `forget_all` and `live()` never meet it. A reader panic unwinds the engine run's task, as a panic of `main`'s one task did. On every path a change read counts read: the run being read is a guard that counts what wasn't handed over when it is dropped (an error, the run dropped), so `changes_captured` and the read count move together as on `main` (review round 1). Dropping the run's future (the supervisor's abort, the stop's deadline) drops the reader with it, at once. |
| R10 | **An engine restart** forgets what the ended run marked (`forget_all`) **before** the new reader starts; confirmation never passed it, so the replay marks it again. |

**Bounds, from the rules.**

- **Ids waiting** (marked, not taken): per index ≤ `max_waiting_per_index` plus one run's ids; in all
  ≤ `max_waiting` plus one run's ids (R2).
- **Changes read ahead** of the builder's grants: ≤ `max_waiting` plus one run (R2). A change that
  resolves to nothing still counts, so a stream of such changes behind a blocked builder can't grow the
  queue without bound (the lead in ids alone wouldn't stop it).
- **What a stop builds:** what was read: the lead plus the run cut short (R2, R7), in batches of
  `max_changes` (R3's end: no deadline), so about `lead / max_changes` builds, `builds` at once.
- **A batch:** at most `max_changes` changes, plus what the budget left waiting (as in slice 4);
  never a later change's ids (R4); the same changes `main`'s batch would hold for the same reads (R3).
- **Defaults** (`[concurrency]`, flag, `FLUSSO_*`): `max_waiting_per_index` = `2 × max_changes` (512),
  `max_waiting` = `2 × builds × max_changes` (2,048). An unset `max_waiting` rises to a larger
  `max_waiting_per_index`, an unset `max_waiting_per_index` falls to a smaller `max_waiting` (as
  `[backoff]`'s `max_ms` follows `initial_ms`; review round 1). Refused at start (`check`, `build`,
  `run`) at 0, or with both set and `max_waiting` below `max_waiting_per_index`, each value named with
  where it came from.
- **The deferral cap** of slice 4 is `max_waiting_per_index`: an index the budget leaves out may wait
  while it has fewer waiting; past that the live batch waits whole, and the reader stops at the same
  number (R2). 6b exempts an index held by the budget.

**Against `main`, what changes.** Reading no longer stops at a closed batch: the reader reads ahead up to
the lead while builds run. Batches hold the changes `main`'s would have held for the same reads (R3),
so build sizes and the number of builds stay `main`'s, trickle or burst, whatever a build takes. What a stop builds grows from one batch plus the slots to the lead plus one
run (decision (b)1). A refused reservation at grant (a race past `admit`) now lets the other kind's
batch take the permit in the same pass (G5) where `main` granted nothing until the next wake.

**Each invariant with two tasks.**

| Invariant (CLAUDE.md, §4) | How it holds |
| --- | --- |
| At-least-once, flush-then-ack, the watermark | unchanged: a build's ids hold their lanes until registered; a queued run is unrouted, so the source confirms nothing of it (R6) |
| An emitted, unrouted change is never confirmed or fenced | the reader's run and queued runs are what `main`'s buffered run was (R6) |
| A fence observes per sink, two levels | unchanged: fences read `Progress`, which the builder routes as before |
| Read-after-visible (#343) | unchanged: the hold is in the capture, before the reader |
| `changes_committed` | a step closes through the candidate's read count and the take never reaches a newer run (R1, R4): no step claims a change whose ids wait |
| Built once; build order is delivery order | one take per batch, admission before the offer, grant-order slots, as before (R4); a mark of an id building waits again (`Pending::mark`) |
| Backfill per sink, requests at-least-once, coalesced | unchanged, in the builder |
| A live stream that ends drains queued requests | the builder ends only once the reader ended, no snapshot runs and no request waits (R8) |
| Graceful stop (#371) | R7: the barrier, then `main`'s settle and drain, then `close_live` with the stream the reader returned |
| #120 staging order | unchanged: `prepare` before the stream opens, before the reader exists |
| Reindex per sink, no restart | unchanged, in the builder |
| A lone change commits at once (#135) | the reader hands a run over as the stream would block; the builder grants it at once when no live build runs (R3): one task wake more than `main`, no window |
| `max_delay` caps a batch | a batch takes no change read at or after its deadline, `max_delay` past the later of its first change's read and the last live grant (R3), as `main`'s open batch stopped taking them |
| Item-level vs flush-wide, lane parking | sink side unchanged |
| Script failures on the ingest side (#198) | unchanged, in the builder |
| Memory budget, gate fairness | the budget is asked at every grant through the gate's lines (R5); the deferral cap is the setting |
| Observability | `Resolve` is the reader's, per run; `Idle` is the builder's wait, ended where the reader began the run that woke it, so the two never overlap; budget waits stay the builder's |
| Pacing | the lead (R2) replaces "a closed batch stops the reading" |
| Only indexes with lanes | `Progress::mark` drops ids of an index with no lane, so they never wait, pin or count toward the lead |

**Mutations of the reader's and the builder's rules** (re-run in review round 1, on the code as pushed;
the rows review round 3 touched, R1, R2, R5, R7–R9 and `together`, re-run on round 3's code, and
round 4's, R3's drain and `together`, on round 4's;
each compiles; run over the engine's unit tests with nextest's 10 s per-test limit; "hang" marks a test
the limit ended). `tests::concurrent::a_document_is_never_written_twice_at_once_and_its_versions_never_go_down`
takes about 25 s on the unchanged code (28 s before this slice) and is left out of these runs.

| Mutation | Tests that fail |
| --- | --- |
| R1: marks after the hand-over | `a_run_is_marked_and_counted_before_it_is_handed_over` |
| R1: a run handed over never counted read | 11 tests, `a_build_error_leaves_every_change_captured_counted_read_when_the_run_returns` and `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` among them |
| R2: no lead per index | 6 tests, `the_reader_waits_at_the_lead_per_index_and_resumes_after_a_take` and `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap` among them |
| R2: no lead over every index | `the_reader_waits_at_the_lead_over_every_index` |
| R2: no lead in changes | `changes_that_build_nothing_still_count_toward_the_lead` |
| R2: the wake-up registered after the lead is read (round 3: after the second read) | `a_take_between_the_check_and_the_wait_still_wakes_the_reader` |
| R2: the lead not read again once the wake-up is registered (round 3: the fast path's first read trusted) | `a_take_before_the_wake_up_is_registered_is_seen_by_the_check_after_it` |
| R2: the wake-up `enable`d before the lead is read (round 1's `Notify`, gone in round 3) | **equivalent**: a `Notified` counts `notify_waiters` from its creation, polled or not; review round 2 removed the call |
| R2: a grant skips the wake-up of a waiting reader | 36 tests (most hang) |
| R2: the reader never says it waits | 36 tests (most hang) |
| R2: a grant never lets the reader on | 37 tests (most hang) |
| R3: the deadline ignores the last grant | `a_paced_trickle_behind_slow_builds_is_batched_as_main_batched_it` |
| R3: no deadline | the paced trickle, `max_delay_caps_a_batch_while_changes_keep_arriving` |
| R3: the deadline in `settle` too | `a_stop_with_a_full_lead_behind_slow_builds_builds_batches_of_max_changes` |
| R3: batches past `max_changes` | 21 tests, `a_batch_ends_inside_a_run_where_main_closed_it` among them |
| R3: never full | 7 tests |
| R3: always full | the paced trickle, `a_batch_ends_inside_a_run_where_main_closed_it`, `a_trickle_read_while_a_live_build_runs_is_one_batch_after_it` |
| R3: a split run loses its rest | `a_batch_ends_inside_a_run_where_main_closed_it` (hang), the random schedules |
| R3: a batch ending inside a run closes its step at the run's last change (`read: run.read`, review round 2) | `a_batch_ending_inside_a_run_closes_its_step_at_its_own_last_change` |
| R3: a landing grants without taking the runs handed over (review round 2; the per-arm take it mutated is gone since round 4, so the two rows below pin the drain) | `a_grant_takes_the_runs_handed_over_beside_the_landing_that_wakes_it` keeps the scenario end to end |
| R3: a candidate formed without draining the hand-over (round 4: `Runs` owns the receiver; replaces round 2's per-arm `take_handed`, three of whose five calls were untested) | `a_batch_is_formed_only_after_every_run_handed_over_is_queued` |
| R3: a full candidate formed without draining the hand-over (round 4) | `a_batch_is_formed_only_after_every_run_handed_over_is_queued` |
| R3: the hand-over check keeps "cut by its deadline" past a grant | `a_full_batch_starts_beside_a_running_build_after_one_its_deadline_cut` |
| R3: the hand-over check counts a granted change as still queued | the same, 4 more (hang) |
| R3: the hand-over check walks with fewer than `max_changes` queued | the same, 4 more (hang): a candidate short of changes, not cut by its deadline, would be taken as cut |
| R3: the hand-over check offers a candidate that isn't full | **equivalent**: `Builds::choose` still starts a second live build only beside a full candidate, the rule's one home |
| R3: the hand-over check never remembers "cut by its deadline" | **equivalent**: a walk per hand-over, nothing else |
| R4: a take reaches past its batch | 21 tests |
| R4: the step closes at every change read | `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` |
| R5: a pass beside waiters skips the lines | `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused` |
| R5: a pass alone moves the count uncontested | `a_pass_alone_hands_the_permit_to_the_other_kind_when_its_turn_is_refused`, `every_fourth_contested_grant_goes_to_the_backfill` |
| R5: a pass alone keeps the permit from the other kind after a refusal | `a_pass_alone_hands_the_permit_to_the_other_kind_when_its_turn_is_refused` |
| R7: no barrier (runs still in the channel at the stop dropped) | `a_stop_while_the_builder_waits_on_a_full_lane_builds_every_run_handed_over`, `a_stop_raised_mid_burst_ends_the_batch_there_and_lands_what_was_read`, `a_stop_mid_run_builds_what_the_reader_marked_last` |
| R7: the barrier reports no resolve | `a_stop_while_the_builder_waits_on_a_full_lane_builds_every_run_handed_over` |
| R7: the reader drops the run the stop cut | 4 tests, `a_stop_mid_run_builds_what_the_reader_marked_last` among them |
| R7: the reader at its lead never reads the stop (round 3) | `a_stop_while_the_reader_waits_at_its_lead_ends_it`, `a_stop_with_a_full_lead_behind_slow_builds_builds_batches_of_max_changes` |
| R7: the stop not watched by the engine run (round 3) | 19 tests, `the_reader_waits_at_the_lead_and_resumes` and the stop tests among them (3 hang) |
| R7: the stop not read before the stream (round 3) | 20 tests, `nothing_is_read_from_the_live_stream_once_the_stop_is_requested` among them |
| R8: the stream's end drops the last run | 35 tests |
| R9: the reader spawned (detached), not polled in the run's task (round 3; replaces round 1's "not aborted" and "not awaited" rows) | 23 tests, `a_build_error_leaves_every_change_captured_counted_read_when_the_run_returns`, `a_build_error_ends_the_run_and_the_reader_with_it`, `a_reader_panic_unwinds_the_engine_run` among them |
| R9: the run being read counts nothing when dropped | `a_build_error_leaves_every_change_captured_counted_read_when_the_run_returns`, `a_run_cut_by_an_error_still_counts_its_changes_read` |
| R1: a full run reads on (round 3: the count checked before the clock) | 7 tests, the lead's among them |
| R1: no deadline for a run | `a_backlog_resolved_a_change_a_millisecond_is_batched_as_main_batched_it` |
| R5: nothing reserved whatever the parts (round 3) | 12 tests, the budget's |
| R5: the budget asked with nothing to reserve (round 2's form) | **equivalent**: an empty reservation is always admitted and adds nothing; a lock, nothing else |
| `together` (round 4, one word): the mark dropped by a store after the flags are read, round 3's two-step form | `a_wake_at_the_end_of_a_pass_is_not_lost`, `every_wake_is_taken_by_a_pass_or_wakes_the_task_in_every_interleaving` (loom) |
| `together`: relaxed orderings on the word | `every_wake_is_taken_by_a_pass_or_wakes_the_task_in_every_interleaving` (loom: a pass that took a flag misses what its waker wrote) |
| `together`: a wake from outside never wakes the task | 79 tests (most hang) |
| `together`: a wake while polled also wakes the task | `wakes_between_the_two_are_served_in_the_same_poll`, `a_wake_from_another_thread_during_a_pass_is_not_lost`, `a_wake_at_the_end_of_a_pass_is_not_lost`, `a_poll_yields_to_the_worker_after_its_passes` |
| `together`: no cap on the passes | `a_poll_yields_to_the_worker_after_its_passes` |
| `together`: an ended future's flag keeps the poll going | `an_ended_first_keeps_no_poll_going` |
| `together`: an ended `first` is polled again (the `running & FIRST` guard before its take removed; final re-check) | `an_ended_first_keeps_no_poll_going`, whose `first` panics when polled after it ended |
| `together`: the watch ending raises an ended first | **equivalent**: the flag of an ended future is masked at the end of a pass (the row above is the mask's) |
| `together`: the first always polled first | `a_spender_polled_first_starves_nothing` |
| `together`: the second always polled first | `a_spender_polled_second_starves_nothing` |
| `together`: the watch ending wakes no one | 22 tests, `the_watch_is_polled_when_it_fires_and_its_end_wakes_the_first` among them |
| `together`: the watch polled on every pass | `the_watch_is_polled_when_it_fires_and_its_end_wakes_the_first` |
| R10: nothing forgotten as a run starts | the random schedules, the tiny-budget and three-index schedules (some hang) |
| `Idle` ends where the builder woke | `the_builders_idle_ends_where_the_reader_began_the_run` |

A mutation that hangs a test hangs it on CI too, where it ran up to the job's limit: `.config/nextest.toml`
now ends every engine unit test after 3 minutes (60 s, three times; the gate's after 30 s), in both
profiles. Checked: "a grant never lets the reader on" ends `a_batch_takes_one_key_per_index_for_every_lane`
at 180 s.

R6 is slice 2's machinery, unchanged; its mutations are slice 2's (the model test). A reader error ending
the run is pinned by `a_resolve_error_ends_the_reader_with_it` and `a_reader_error_ends_the_run_with_its_error`.

**Existing tests changed, each for a stated reason:**

- `a_stop_while_a_closed_batch_waits_for_a_build_ends_the_run` → `a_stop_while_the_reader_waits_at_its_lead_ends_the_run`:
  grants `[[0], [1], [2]]` and three changes read, where `main` read two: the lead at `max_changes = 1` is
  two documents waiting, one more than `main`'s closed batch. Still: nothing read after the stop, and what
  was read is built and published.
- `tests::reports_lifecycle_and_progress_to_the_observer`: the ingest side's `stage idle` events are
  checked apart: at most two, the first before the first `stage resolve live`, each before it or after
  `stage publish live` (review round 2); every other event in `main`'s order. The confirm tick's first
  tick is immediate and races the reader's task to the builder, so its idle wait lands before the first
  run or after its batch (deduplicating only adjacent ones still failed about 1 run in 15; 40 runs of the
  checked form passed).
- `reader::tests` (review round 3) run the reader as the engine runs it: `reading` beside a builder
  that only waits for its end, the stop watched once (`together`), on a spawned task so the test drives
  it from outside. `dropping_the_reader_task_aborts_it` → `dropping_the_run_drops_the_reader_and_its_stream_at_once`
  (the reader is a future: dropping the run's future drops the stream with no wait);
  `a_reader_panic_is_resumed_where_it_is_joined` → `a_reader_panic_unwinds_the_task_that_polls_it`, and
  `a_reader_panic_unwinds_the_engine_run` checks it on the engine.
- `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap`: samples half a second off
  the bursts. On a burst's own instant the watcher raced the reader's task (now separate) for it; the
  values asserted are unchanged (960 and 31 at 60 s, 976 and 32 after).
- `an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building`: the comparison with the share
  off is dropped (§3.5's bounds say why); the backfill check stays.
- Gate tests that took a permit with `Gate::choose` take it through the lines (`take`, `take_either`);
  `choose_takes_live_first_…` is `both_kinds_ready_take_live_first_…`, `the_one_task_gets_no_permit_…` is
  `a_grant_pass_gets_no_permit_ahead_of_an_eligible_waiter`.

**Not in this slice.** `a_stop_with_one_index_stalled_lands_the_others` (§4) needs 6b: with one builder a
publish blocked on a full lane holds every index, as on `main`. `flusso.reader.waits{bound}` is slice 7's.

**Hot-path costs added** (for the measurement; review round 1 cut the grant pass down, review round 3
put the reader in the engine run's task):

- **Per change:** nothing `main` didn't do: the reader reads the clock once per change for the run's
  deadline, as `main`'s `is_open` did (none once the run is full), and keeps that instant beside the
  change's position.
- **Per run:** one `Vec` of `(position, instant)` handed over (one allocation), one channel send, the
  builder's flag raised (a wake inside the task, no scheduler), one atomic add for the read count
  (`main` did one per change), one read of the lead (an atomic load per index). **A trickle pays this
  per change**: each change is its own run (review round 2).
- **Per wait at the lead:** the waker registered (`AtomicWaker`, no lock), a flag, a `SeqCst` fence and
  the lead read again; nothing for the stop. At `max_changes = 1` with the default lead (two changes)
  the reader waits once every second change.
- **Per hand-over beside a running live build:** O(1) until `max_changes` changes are queued (a count
  kept as runs come and go), then one walk, and none after it while the deadline is known to cut the
  candidate (`Runs::full_candidate`, review round 2). Without a live build running, the candidate's walk
  over the changes it takes, as below.
- **Per candidate formed:** one `try_recv` on the hand-over channel (`Runs` drains it first).
- **Per batch (a live grant):** two budget locks and one gate lock, as `main` took: `affordable` admits
  (`Budget::admit`, the first budget lock), then the pass reserves under the gate's lock (the gate lock,
  and `reserve_behind`, the second budget lock, skipped when nothing is reserved: no budget counts the
  documents, as `main` skipped it; review round 3), an atomic `fetch_max` and a fence for the reader (a
  wake only while it waits), one clock read for the deadline of the next batch, and the candidate's
  walk over the queued changes it takes.
- **Per grant pass (every wake of the builder):** one lock and at most one walk per index
  (`Progress::waiting_through`: the count through the candidate and the held check in one), where
  `main` took two locks per index (the count, then the held check, which walked too while builds ran).
  The walk is skipped while nothing builds and the candidate covers everything waiting; it covers only
  the candidate's documents otherwise.
- **Per poll of the engine run's task:** the task's waker registered, one `fetch_or` for the "being
  polled" mark, one `fetch_and` per flag taken, and one load and one `compare_exchange` to end a pass
  (review round 4: one atomic byte; round 3 had two `SeqCst` stores and three flag reads per pass).
- **Beside real waiters (6b):** joining, asking and leaving the lines cost what slice 5's waiters cost.

**Review round 3's A/B** (`engine` bench, `taskset -c 0-3,12-15`, three rounds of `main` 637c2ab3 then
this, the method of round 2's measurement; raw in `target/ab-446-r3/`):

| bench | round 2 head | round 3, rounds 1 / 2 / 3 |
| --- | ---: | ---: |
| `live_drain_held/sinks1/1` | +21.6% | +2.5% / +3.6% / +4.5% |
| `live_drain_held/sinks2/1` | +13.1% | +12.1% / +11.0% / +12.5% |
| `live_drain_held/sinks1/64` | −15.5% | −0.9% / +0.4% / +0.9% |
| `live_drain_held/sinks2/64` | −15.6% | +0.6% / +0.3% / +0.9% |
| `live_drain_held/sinks1/256` | −30.3% | −24.0% / −23.3% / −21.8% |
| `live_drain_held/sinks2/256` | −26.1% | −22.0% / −22.0% / −21.6% |
| `snapshot/rows` | −0.3% | −1.7% / +0.9% / +0.3% |

At 64 the reader no longer resolves on another worker while builds run, so round 2's gain there is
gone (equal to `main`); at 256 most of it stays.

**Where `main`'s one-change batches still win (review round 3, measured).** `live_drain_held/sinks1/1`
on this machine, per 10,000 changes: `main` reads and resolves in 2.0 ms and marks in 1.6 ms (inside its
grant), 3.6 ms in all; the reader takes 6.65 ms for the same work plus the per-run and per-wait costs
above, about 0.3 µs a change. The grant path (`pre` the candidate and its waiting set, `choose`) costs
the same on both; building and publishing take about 0.35 µs a batch more, because with the reader
two changes ahead the builder grants two one-change batches back to back, so the bench's sink frees
batch *n* exactly while build *n + 1* runs (its stated contended upper bound) where `main` read a change
in between (a lead of one change was 2 ms faster). No other per-batch cost measured above noise
(±1.5 ms between runs): the run's `Vec` (≈ 0), the channel (≈ 1 ms in all, measured by adding a
second send), a self-wake (≈ 0).

### 3.9 Slice 6b: a builder per index (design, not built)

**Status:** design only, for review before any code. Against `main` at `7e10c43c` (6a merged). The
rules (I1–I12) are written to be built and tested as stated; the owner's open questions are at the
end, apart from the ones a reviewer can settle.

**What 6b delivers.** A builder per index (live and backfill of that index, its lanes, its grant
order), a dispatcher for backfill requests that never blocks, walks of several indexes at once, and
the exemption of an index held by the budget from the reader's lead, counted against
`max_pending_documents`. Not in 6b: per-index status, metrics and endpoints (slice 7), per-index
restart (slice 8).

#### What 6a as built changes in the plan above

| The plan said | 6b does | Why |
| --- | --- | --- |
| an index **task** per index, spawned in a `JoinSet` (§1, §3.4, §3.7) | an index **builder**, a future of the engine run's task, polled by `together` generalised to N (I1) | 6a measured the hop: a spawned reader cost +21.6% locally and +54% on CI at one change per batch; one task removed it (§3.8). A spawned builder would pay the same hop per hand-over and per grant |
| the reader marks a run, then notifies the index task (§3.1) | marks, counts, then hands each touched index its slice of the run (I2) | per-index batching needs each change's read instant and read number (R3) |
| one live build at a time, a second while `max_changes` ids wait (§3.4) | R3 and R4 per index: a second live build only beside a full candidate, `max_changes` changes of that index (I3) | 6a's rule, `main`'s batches, per index |
| global steps closed by the one builder (§3.6, 6a's `close_step_through`) | a **frontier** per index carried by each item (I5) | every lane is registered with every step today; with independent builders a quiet index's lanes would never be, and its sinks' counts would stop |
| a live build takes the indexes that fit and defers the rest (slice 4) | gone: a build is one index's. Refused, the index's batch waits whole and only its builder waits (I7) | each index reserves for itself; the deferral existed because one build spanned indexes |
| R2's bound on changes read and not granted | per index: changes queued on an index and not granted (I6) | a change touching no index is queued nowhere, so nothing grows; the bound was there for queued runs |
| the builder confirms after every grant and landing | the dispatcher confirms at most once per poll of the task in which something was registered, and on the tick (I9) | N builders registering would confirm N times a poll, each `landed()` walking every lane |
| coalescing waits inline (`collect_requests`'s `timeout_at`) | a deadline per builder (I10) | already planned in §3.4; an inline wait stalls the index's live changes |
| `together` of two futures and a watch | `together` of N futures, flags in groups of one word (I1) | the one-word handshake and its loom model carry over |

#### 1. The structure: one task, N + 2 futures

**Options, each with its cost per change, per run, per batch:**

| | Per change | Per run (per touched index) | Per batch | Independence |
| --- | --- | --- | --- | --- |
| **A: a spawned task per index** | as 6a | a channel send **and a wake of another worker** | a wake of the reader on another worker when it waits at its lead; a cross-task wake of the dispatcher to confirm | full, and parallel CPU for each index's bookkeeping |
| **B: one task, the reader, a dispatcher and N builders as futures (chosen)** | as 6a | a channel send and a flag (no scheduler) | a flag for the reader; confirmation coalesced in the task | at every await: a builder waiting on a lane, the budget, a permit or a page is pending, the others are polled |
| **C: hybrid** (builders spawned only for indexes that backfill, or past some load) | B's or A's | two paths, two sets of rules | — | — |

**What actually needs another worker thread**, and already has one:

- **A build's CPU** (decoding rows, `coerce_document`, scripts): a build is polled once where it is
  granted and, if not finished, runs on a task of its own (`Flights`), on `main` and 6a alike. The
  builder only grants, takes and publishes.
- **A key read** (I/O) runs on a task of its own from 6b (I10), for K1's sake.
- **The sink engines** are tasks of their own, as today.

What stays in the engine run's task is bookkeeping: marks, candidates, takes, gate and budget
decisions, an `Arc` send per lane, registrations, confirmation. 6a's builder spent about 6.7 µs a batch
on the in-process bench **including** the mock build polled inline; a real build leaves the task at
its first query. So a second worker would buy parallel bookkeeping that costs microseconds, at the
price A pays per run and per grant. C buys nothing B lacks and doubles the rules to prove.

**Choice: B.** The index builders are futures; "independence" (ADR §1) holds at every await, which is
where an index waits: its lane, its budget, its permit, its key page, its builds. One index's builds
already run beside another's (`Flights`). The ADR's wording ("one task per index") needs the owner's
nod (open question O1).

**`together` for N futures (I1).** The same protocol as 6a, generalised:

- **One word.** An `AtomicU64`: bit 0 the "being polled" mark, bits 1–63 **groups**. Future `i` (the
  reader 0, the dispatcher 1, index builder `k` at `2 + k`) raises group `1 + (i mod 63)`. A raised
  group polls every running future in it. With more than 61 indexes, futures share a group: one of
  them may be polled without being woken, which every future already tolerates (each re-checks its
  state when polled). One location, as in 6a, so the argument that a wake is either taken by a pass
  or wakes the task holds on every architecture; the loom model checks it with three waking threads
  and a shared group.
- **Fairness.** A poll's passes start at a rotating future, advanced every poll (6a swapped two). A
  future therefore gets tokio's fresh per-poll budget first once every `N + 2` polls, and a spender
  among N starves none.
- **The pass cap** stays 32 passes per poll; a pass polls only raised groups, so its cost is the raised
  ones, not N.
- **Ended futures** are masked, as in 6a. The dispatcher's end is the run's end (it returns the run's
  result).

**Expected hot-path cost**, in 6a's terms (§3.8). One index:

| Where | 6a | 6b, one index | 6b, K indexes touched by a run |
| --- | --- | --- | --- |
| per change | a clock read for the run's deadline | same, plus the change's read number kept beside it (8 B) | same |
| per run | one `Vec` of `(position, instant)`, one send, a flag, an atomic add, a lead read | same (the run's `Vec` moves to the only index touched) | K `Vec`s, K sends, K flags; an atomic load per index for the lead |
| per wait at the lead | `AtomicWaker` registration, a flag, a fence, the lead read again | same, plus the exempt flags read with the lead (an atomic load per index) | same |
| per batch | two budget locks and a gate lock, a `fetch_max` and a fence, a clock read, the candidate's walk; the registry lock to close the global step | same, **minus** the global step lock; plus the frontier: the first entry of the pending order (kept) or one acquire load of the read count | per index batch, the same; a run touching K indexes makes up to K batches where `main` made one, each smaller |
| per confirmation | after every grant and landing | once per poll in which something registered, and on the tick | same: fewer `landed()` walks than one per registration |
| per poll | the task's waker, a `fetch_or`, a `fetch_and` per flag, a load and a `compare_exchange` per pass | same, plus the rotation counter | the bit scan over raised groups |
| per key page (backfill) | none | a task spawned for the read (6b-2) | same |

So with one index 6b should measure as 6a, which measures as `main` on CI (gate green at `a8ca2f4e`,
`sinks1/1` +2.8%, `sinks2/1` −3.2%). With several indexes it pays K builds per run where `main` paid one
`build_many`, and gets back what `main` lost by building those indexes one after another inside that
call (`PgDocumentBuilder::build_many` groups by index and awaits each in turn): §9 measures it.

#### 2. Who owns what

| Future | Owns | Never does |
| --- | --- | --- |
| **Reader** (one) | the live stream, the resolver, the run being read, the read count | build, publish, confirm, wait on a lane, touch a request |
| **Dispatcher** (one; the run's result) | the request consumer, routing each request to its builder, confirmation and the 1 s tick, the stop's wind-down (`drain_lanes`, `close_live`), the end | build, walk, wait on a builder |
| **Builder** (one per index with lanes) | its queued run slices, its `Builds` (slots, admission, grant order), its gate waiters (at most one per kind), its budget parts, its lanes' producers, its walk and key reads, its request inbox and coalescing deadline, its frontier, its held and exempt state | resolve, confirm, read the stream |
| `Progress` (shared) | pending sets (reader marks, builder takes), the read count, item keys, registrations, the per-lane record of frontiers | — |
| shared | `Gate` (permits, lines), `Budget`, the key-read permit, the `Lead` | — |

**A run, a batch, a step, per index:**

- **A change's read number**: its place in read order over the deployment's life (`Progress`'s read
  count, R1). Each mark carries it; a pending document keeps the read number of its oldest change not
  yet built (beside that change's position).
- **An index's run slice**: the changes of a run whose resolve gave at least one document of the index,
  in read order, each with its position, read instant and read number. A change touching no index
  (with lanes) is in no slice: it is only counted.
- **An index's batch**: R3 over the index's queued slices, with the index's own last live grant (I3).
- **An item's frontier**: carried in place of 6a's step (I5).

**Why `changes_committed` stays exact when indexes publish independently** (I5). A lane `L` of index
`A` has committed the frontier `F` of the newest item `k` of `A` such that every item of `A` through `k`
has landed on `L` or had nothing for `L`. `F` is set at `k`'s take: the least read number still waiting
on `A`, minus one, or, with nothing waiting, the read count read before the take. So every document of
`A` touched by a change through `F` was taken into an item through `k` (a document of such a change
still waiting at `k`'s take would have made `F` smaller; one re-marked by such a change was marked
before the take, so it was waiting then). When `L` has landed every item and `A` has nothing waiting or
building, `L` has committed the read count, read **before** `A`'s pending state (the reader marks
before it counts, R1, with release and acquire on the count, so a counted change's marks are seen).
A sink's count is the least over its lanes, as today. Consequences: a quiet index pins no count, a
change touching two indexes counts once both have it landed, and a partial take (I7) counts only
through its oldest document left waiting. Per read, not per position, and reads of an ended run count
with the next item routed, as slice 3 built (§3.6). Model test: the per-index frontier against a model
that keeps every step, over seeded independent takes, publishes, landings, skips and ended runs on
three indexes and two sinks.

#### 3. Order and position

- **Lane order per index is its builder's grant order**: one `Builds` per index, publication in grant
  order, live and backfill of the index through the same slots (§3.4's reason: admission is released
  at landing).
- **A change touching documents of two indexes** is pending on both from its marks until each builder
  takes and lands it. Confirmation and fences take the least over every lane of each index's pending
  minimum and its lanes' unlanded items (§3.2), so both wait for both; the committed count waits by I5.
- **Positions on a lane stay monotonic**: an item's position is the newest position marked on its index
  when the batch was taken (decision (b)2), taken in grant order by one builder. Two items of an index
  may carry one position (a backlog taken in several parts, I7); never a smaller one.
- **stdout's `seq`**: non-decreasing per index, may repeat, no order across indexes (decision (b)2).
  `docs/src/reference/sink-stdout.md` says otherwise today: "across indexes the lines keep position
  order" (line 11) and "Monotonic within a run" (the `seq` row). The documentation agent rewrites both
  in the slice that makes builders independent (6b-1): a consumer resumes per index, from the last
  `seq` inclusive.

#### 4. The reader's lead with the exemption

- **Held.** An index is held while its builder's live batch is refused by the budget (its share or the
  limit) and waits for a release; it stops being held at its next live grant.
- **Exempt** (I7), with hysteresis so a flapping hold doesn't flap the lead: an index becomes exempt
  when it is held with `max_waiting_per_index` documents or more waiting (exactly when it would stop the
  reader), and stays exempt until fewer than `max_waiting_per_index` are waiting. So a lane that reopens
  drains its backlog without stopping the reader for every other index.
- **Which bound applies** (I6): a non-exempt index counts toward `max_waiting_per_index` (documents
  waiting, and changes queued on it), and the non-exempt indexes together toward `max_waiting`. The
  exempt indexes together count toward `max_pending_documents` only.
- **At `max_pending_documents`** the reader waits and every index stalls, as ADR §5 decided ("past it
  the reader waits, and every index stalls"): no document is lost, memory stays bounded, and reading
  resumes as the exempt backlog drains. An error instead would restart the engine onto the same
  backlog, a loop with no progress.
- **While exempt**, the builder keeps no queued slices (they would grow per change, not per document):
  its pending documents with their read numbers are all it needs. Its takes are oldest first, at most
  `max_changes` documents each (budget-sized, as one batch's estimate), one live build plus a second
  while `max_changes` more wait; each carries the frontier of what it left (I5). The released backlog is
  therefore built in `max_changes` steps, never one huge batch. Leaving the exemption, R3 resumes on the
  slices queued from then on; documents still waiting from before go with the next batch's take (they
  are older than its position).
- **Memory per pending document**: the row key behind an `Arc` (its `Vec` of `(ColumnName,
  GenericValue)`, the column name's heap string), a `waiting` map entry (key, 32-byte position, counter,
  and from 6b the read number), an `order` entry. For a one-column integer key about 250 B with
  allocator rounding, so about 250 MB at a million (ADR §5: 150–250 B; more for text or composite
  keys). It sits outside `max_built_mib`; the manual states both bounds. Measured by a test with a
  counting allocator in the slice that brings the exemption.
- **A stop with a million pending documents of an exempt index** (decision (b)1: a stop builds what is
  pending, capped by the lead): the exempt backlog is beyond the lead by definition, so its builder
  **builds none of it**. It finishes its running builds and returns. Its documents stay waiting, so
  confirmation stops before its oldest (the pending minimum, §3.2) and the restart replays them: never
  confirmed and lost. The stop does not wait for the backlog. It still waits for items already published
  on the index's lanes (`drain_lanes`, unchanged), so a parked lane runs the stop to its deadline and
  exits 1, as `main` does with a stalled sink (§3.1).

#### 5. Backfill

- **The dispatcher** receives each `Request::Backfill` and hands the delivery, ack handle included, to
  its index's builder through an unbounded inbox and the builder's flag. It never waits on a builder.
  An index with no lane, or one the builder doesn't build, is warned and acknowledged, as `main`. A
  dropped, unacknowledged delivery returns to the request lane (`ChannelAck`'s `Drop`).
- **Coalescing, per builder**: `main`'s rule per index, on a deadline arm: start once every sink with a
  lane of the index asked, else at `max_delay`. Sinks coalesce into one walk; a request mid-walk waits
  for the walk's end (#435's tickets and cuts as in §3.4). `SnapshotComplete` per lane under the
  greatest ticket it asked with; the request is acknowledged after its index's `SnapshotComplete` is
  published.
- **Several walks at once** (6b-2). Each builder walks its index. **K1 with several walks**: a key read
  runs on a task of its own that holds the key-read permit from before the read until the read returns,
  and hands the page to its walk over a one-shot. So a walk that stops being stepped mid-read (its batch
  full behind a parked lane) still finishes the read and gives the permit back
  (`one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`). The task is aborted with its
  walk. One permit stays (the builder pool `builds + 1` is unchanged).
- **Fairness between backfilling indexes**: each builder has at most one backfill waiter, and a waiter
  granted rejoins at the back of the line (G2), so backfilling indexes take backfill grants in turn.
  The every-fourth rule (G4) gives the fourth contested grant to the oldest eligible backfill waiter,
  whichever index. Backfill's half of the budget (rule 2) is shared; rule 4 (backfill holding nothing
  gets one batch) holds for all of them together, and G6 keeps small batches of one index from
  starving a large one of another.
- **The cost recorded in slice 5**: a refused backfill waiter is asked again on every live grant,
  because each grant's add moves the budget's generation and `released_since` wakes every refused
  waiter. With real waiters that is one budget decision per refused waiter per live grant. An add can
  never make a waiter fit (it only takes room), so it need not wake one. It can only change who is held
  back (a limit refusal of an older waiter turning into a share refusal). **Proposed (G7′)**: an add
  wakes only waiters refused while behind; a release wakes every refused waiter. Kept:
  `a_settle_that_adds_bytes_lets_the_waiter_behind_be_asked_again`. New:
  `a_refused_waiter_is_not_asked_again_on_an_add`. Measured in 6b-2.
- **Live and backfill of one index** go through one builder's slots, so they keep grant order on its
  lanes, as today.

#### 6. Failure and restart

A builder's error (`on_error = "stop"`, a query error) ends the engine run with it, as today; the
other futures are dropped in place. Cancel-safety is today's: ids in flight are forgotten when the
next run starts (R10), spawned builds and key reads are aborted with their futures. A panic unwinds
the run's task (6a). **Slice 8 stays easy**: each builder keeps a slot of its own in `together`, so a
per-index restart replaces one future in its slot after giving its builds back (slice 2 recorded that
give-back returns with slice 8), and catches that future's panic (`catch_unwind` around its polls)
instead of unwinding the task. Nothing in 6b ties one builder's state to another's.

#### 7. Slices inside 6b (proposed; the reviewer decides)

| # | What moves | Tests written first | Never worse than before it, because |
| --- | --- | --- | --- |
| 6b-1 | `together` for N (I1); the reader's slices (I2); one builder per index, live and backfill of its index, R3/R4 per index (I3–I4); the frontier (I5) replaces global steps; the dispatcher with confirmation (I9); coalescing per builder; **walks one at a time** (a walk token held by the walking builder: `main`'s order); the gate's lines with real users (I8); stdout wording | the N-future poller tests and loom model; `committed_matches_the_model_over_random_independent_publishes`; `a_full_lane_of_one_index_holds_no_other_indexs_live_changes`; `a_task_blocked_on_a_full_lane_holds_no_permit`; `a_slow_build_of_one_index_delays_no_other_index`; `each_index_cuts_its_own_batches_as_main_cut_them`; `a_change_touching_two_indexes_is_confirmed_once_both_landed` | one index is 6a's path; walks keep `main`'s order; the lead keeps 6a's bounds |
| 6b-2 | walks at once: the token goes; key reads on their own task (K1); G7′ | `indexes_walk_at_once`; `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`; `two_backfilling_indexes_take_backfill_grants_in_turn`; `a_refused_waiter_is_not_asked_again_on_an_add` | the budget's backfill half and K1 bound what more walks can take |
| 6b-3 | the exemption (I6–I7): held and exempt states, `max_pending_documents` (key, flag, env, refusal, `just schema-gen`), backlog takes of `max_changes`, the stop leaving it (I11) | the acceptance test turned around; `a_released_backlog_is_built_in_batches_of_max_changes`; `reading_stops_for_every_index_at_max_pending_documents`; `an_exemption_ends_only_below_max_waiting_per_index`; `a_stop_leaves_an_exempt_backlog_unconfirmed_and_builds_the_rest`; the memory test | without it, 6b-1's lead stops the reader at the cap as 6a does |

Each merges on its own, with its CLAUDE.md lines and its manual pages written by the documentation
agent; 6b-3 is the only one adding a user-facing setting.

#### 8. Rules, bounds, guarantees, mutation skeleton

| Rule | |
| --- | --- |
| I1 | **One task.** The engine run polls the reader, the dispatcher and one builder per index with lanes as futures of its task (`together` for N): one `AtomicU64`, bit 0 the mark, a group bit per future (`1 + (i mod 63)`), a raised group polling every running future in it, ended futures masked, passes from a rotating start, 32 passes per poll. Nothing is spawned for them; builds and key reads run on tasks of their own, aborted with their futures. |
| I2 | **Slices.** Per run the reader resolves, marks (each mark with its change's read number), counts, then hands each touched index the run's changes that touched it, in read order (R1's order per index). A change touching no index is counted only. An exempt index is handed nothing. |
| I3 | **Batches per index.** R3 over the index's slices and its own last live grant: every change read before `max(the oldest not granted's read, the last live grant) + max_delay`, up to `max_changes` changes, full at `max_changes`; the inbox drained before a candidate forms; a second live build only beside a full candidate; no deadline at the end. |
| I4 | **Takes per index.** R4: a live take reaches no document marked after its candidate's last change; offered only while none is in a running build of the index. |
| I5 | **Frontier.** Each take sets `F` (least read number still waiting − 1, or the read count read before the take); the index's items carry it on every lane of the index, published or skipped; a lane has committed the `F` of its newest item with every item of the index through it landed or skipped there, and the read count (read before the pending state) when it has landed everything and its index has nothing waiting or building; a sink the least over its lanes. The read count is written with release after the marks and read with acquire. |
| I6 | **Lead.** Before each run the reader waits while a non-exempt index has `max_waiting_per_index` documents waiting or changes queued, the non-exempt indexes together have `max_waiting` documents waiting, or the exempt ones together have `max_pending_documents`. Woken by a take, a grant and a change of exemption; 6a's registration-then-reread protocol; a run overshoots once. |
| I7 | **Exemption.** Exempt from being held with `max_waiting_per_index` or more waiting, until fewer wait. While exempt: no queued slices, takes of at most `max_changes` documents oldest first, each with its frontier. |
| I8 | **Gate.** A builder joins the live line only with a candidate ready, a free slot and no publish blocked on a full lane, and the backfill line only with a page batch ready and a free slot: at most one waiter per kind per builder, only while it can use a permit now. G1–G9 unchanged; G7′ (5) proposed. |
| I9 | **Dispatcher.** Routes requests without waiting on a builder. Confirms at most once per poll in which a builder registered something, and on the 1 s tick. The run's result: it ends the run once every builder returned (live end, stop) or with the first builder's error. |
| I10 | **Backfill.** Coalescing per builder on a deadline arm; tickets and cuts per §3.4; acknowledged after its `SnapshotComplete` is published. 6b-1: one walk at a time (a token). 6b-2: walks at once, each key read on its own task holding the key-read permit until it returns. |
| I11 | **Stop and end.** A builder returns only after the reader's end (the barrier). At a stop: abandon the walk, finish running builds; a non-exempt builder builds and publishes what waits (at most its lead), an exempt one builds none of its backlog. Then the dispatcher runs `drain_lanes` and `close_live`. At the stream's end: the same without the close, and queued requests served first. |
| I12 | **Failure.** A builder's error ends the run with it; the others are dropped in place; a panic unwinds the task. |

**Bounds, from the rules:**

- Documents waiting, non-exempt: per index ≤ `max_waiting_per_index` + one run's; together ≤
  `max_waiting` + one run's. Exempt, together: ≤ `max_pending_documents` + one run's (I6).
- Changes queued per index ≤ `max_waiting_per_index` + one run; none on an exempt index (I2, I7).
- Builds at once ≤ `builds` (G1); live builds of one index ≤ 2 (I3); slots per index `2 × builds`.
- Key reads at once ≤ 1 (K1); key-read tasks ≤ one per walk.
- Gate waiters ≤ 2 per builder (I8).
- What a stop builds ≤ every non-exempt index's lead + one run (I11); an exempt backlog: nothing.
- Memory: built documents as §3.5 (rules 1–5; rule 3's share needs two or more indexes); pending ids
  ≤ (`max_waiting` + `max_pending_documents`) × ~250 B.

**Each guarantee, one line:**

| Invariant (CLAUDE.md, §4) | How it holds in 6b |
| --- | --- |
| At-least-once, the watermark | unchanged: pending minimum per index plus registrations; confirmation coalesced, never early (I9) |
| An emitted, unrouted change is never confirmed or fenced | the reader routes after its marks, as 6a; a slice queued on a builder is pending |
| Fences per sink and index, two levels | unchanged machinery; per index from each builder's registrations |
| `changes_committed` | the frontier (I5), exact as argued in 2 |
| Built once; build order is delivery order, per index | one builder per index, admission and grant order (I3, I4) |
| Backfill per sink, requests at-least-once, coalesced, tickets | per builder (I10) |
| A live stream that ends drains queued requests | the dispatcher ends the run only once every builder returned and the request lane is empty (I9) |
| Graceful stop | the barrier, the lead built, an exempt backlog left unconfirmed (I11) |
| #120 staging order | unchanged |
| Reindex per sink, no restart | a request to the index's builder |
| A lone change commits at once (#135) | per index as 6a: a slice handed over as the stream would block, granted at once when no live build of the index runs |
| `max_delay` caps a batch | per index (I3) |
| Item-level vs flush-wide, parking | sink side unchanged |
| Script failures (#198) | in the builder; `stop` fails the run (I12) |
| Memory budget, gate fairness | many real reservers and waiters (I8); rules 1–5 |
| Pacing | the lead per index (I6), the exemption and `max_pending_documents` (I7) |
| Only indexes with lanes | a builder per index with lanes; marks of others dropped |
| Read-after-visible (#343) | unchanged: in the capture |
| Independence (ADR §1) | at every await of a builder (I1) |

**Mutation skeleton** (each row: the rule, the mutation, the test written first to fail under it):

| Rule | Mutation | Test |
| --- | --- | --- |
| I1 | a group's raise polls only its first future | `a_shared_group_polls_every_future_in_it` |
| I1 | the start never rotates | `a_spender_among_n_starves_none` |
| I1 | the mark dropped by a store after the flags are read | the loom model with three wakers; `a_wake_at_the_end_of_a_pass_is_not_lost` |
| I1 | builders spawned, not polled in the task | `a_build_error_ends_the_run_and_drops_every_builder_in_place` |
| I2 | a change's slice handed to an index it didn't touch | `an_untouched_index_is_handed_nothing` |
| I2 | slices handed before the marks | `a_slice_is_marked_and_counted_before_it_is_handed_over` |
| I3 | one deadline shared by every index | `each_index_cuts_its_own_batches_as_main_cut_them` |
| I3 | a second live build of an index beside a partial candidate | `a_trickle_on_one_index_while_its_build_runs_is_one_batch_after_it` |
| I4 | a take reaching past its candidate | `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` (per index) |
| I5 | the frontier at the read count despite documents left waiting | `a_partial_take_counts_committed_only_through_its_oldest_document_left` |
| I5 | a quiet index's lane never catching up | `a_quiet_index_holds_no_sinks_count` |
| I5 | the read count read after the pending state | `committed_matches_the_model_over_random_independent_publishes` |
| I6 | an exempt index counted in `max_waiting` | `an_index_held_by_the_budget_does_not_stop_the_reader` |
| I6 | no bound on exempt documents | `reading_stops_for_every_index_at_max_pending_documents` |
| I7 | exemption ends at the first grant (no hysteresis) | `an_exemption_ends_only_below_max_waiting_per_index` |
| I7 | the whole backlog in one take | `a_released_backlog_is_built_in_batches_of_max_changes` |
| I8 | a builder joins the live line while its publish is blocked | `a_task_blocked_on_a_full_lane_holds_no_permit` |
| I8 | two live waiters of one builder | `a_builder_never_has_two_waiters_of_a_kind` |
| I9 | the dispatcher waits on a builder's inbox | `coalescing_for_one_index_delays_no_live_change_of_it` |
| I9 | confirmation skipped in a poll where something registered | `a_change_touching_two_indexes_is_confirmed_once_both_landed` |
| I10 | the key read stepped by its walk (K1 inside the walk, as slice 5) | `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads` |
| I10 | the walk token kept in 6b-2 | `indexes_walk_at_once` |
| I11 | a builder returns before the reader's end | `a_stop_mid_run_builds_what_the_reader_marked_last` (several indexes) |
| I11 | an exempt backlog built at the stop | `a_stop_leaves_an_exempt_backlog_unconfirmed_and_builds_the_rest` |
| I12 | the other builders left running after an error | `a_build_error_ends_the_run_and_drops_every_builder_in_place` |
| G7′ | an add wakes every refused waiter | `a_refused_waiter_is_not_asked_again_on_an_add` |

Every existing row of §3.5's and §3.8's tables is re-run on 6b-1's code.

#### 9. What gets measured at the end, and what I expect

| Where | Expect | Why |
| --- | --- | --- |
| in-process `live_drain_held/*`, `snapshot/rows` (the PR gate; one index) | as 6a, so as `main` | one index is 6a's path, minus the global step lock |
| a new in-process `live_drain_held/indexes3/{1,64,256}` (three indexes, a mock build that yields once so builds of different indexes overlap), added in 6b-1 and laid over `main` by `review-ab` | better than `main` at 64 and 256; at 1, within the gate | builds overlap; at 1 each change is its own batch everywhere |
| scenario `reference` (three indexes), `builds = 4`: burst drain, visible latency p50/p99 | drain up, p50 down | `main`'s batch builds `users`, `products`, `orders` one after another in one `build_many`; 6b builds them at once |
| `reference`, a burst on one index of three | the other two's latency down | independence |
| `reference` backfill documents/s, seed wall (after 6b-2) | up, by as much as the pool allows | three walks at once, key reads one at a time |
| scenario `complex` (one index) | flat | pure overhead |
| peak RSS, CPU | flat to a few MiB up | `Builds` and queues per index |
| a stop under steady writes | flat | the same lead |
| `sync-standby`, by hand | flat | the reader is unchanged |

If one index loses to `main` on the gate, it is a regression of 6b-1 to fix before merging, as 6a's
was.

#### Open questions

**For the owner, with a recommendation:**

- **O1. "A task per index" becomes "a builder per index, each a future of the ingest engine's task".**
  ADR 0012 says "one task per index" (Decision, §1, the diagram). Builds and key reads still run on
  tasks of their own; independence holds at every await. Recommended, for 6a's measured reason (§1
  above); the ADR text changes with it.
- **O2. `[batch] max_changes` becomes per index.** Each index cuts its own batches, up to
  `max_changes` changes of that index, on its own deadline (I3). With several indexes under a burst a
  lane item can hold more documents than `main`'s share of a mixed batch, and up to one batch per index
  builds at once (each within the budget). The manual's "changes per batch" becomes "per index".
  Recommended: it is what lets one index's batch close without waiting for another's.

**For the reviewer:** the frontier (I5) and its model test; the exemption's hysteresis (I7); backlog
takes of `max_changes` documents; confirmation once per poll (I9); G7′; key reads on their own task;
the slice split and the walk token in 6b-1; the `Idle` stage with N builders (proposed: the dispatcher
reports `Idle` only while no builder has anything granted, building or publishing, so its sum keeps
its meaning; per-index stages are slice 7's).

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
| Pacing | the lead bound (6a); the held-index exemption and `max_pending_documents` (6b) | `a_stalled_sink_paces_builds_and_they_resume_when_it_does` | `the_reader_waits_at_the_lead_and_resumes`, `a_parked_lane_bounds_pending_memory`, `an_index_held_by_the_budget_does_not_stop_the_reader` (6b; its acceptance case is `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap` with the reading going on) |
| #120 staging order | unchanged | `fresh_source_*`, daemon `fresh_source_rebuilds_seeded_sinks_before_preparing`, `every_index_of_every_sink_is_staged_before_the_source_prepares` | — |
| A lone change commits at once (#135) | `main`'s run cut; a waiting id is taken once a slot is free | `a_lone_change_on_a_quiet_stream_commits_without_waiting`, `max_delay_caps_a_batch_while_changes_keep_arriving` | — (rerun on the reader) |
| Script failures on the ingest side (#198) | in the index task | `skip_policy_leaves_a_failed_document_out_and_keeps_going`, `stop_policy_fails_the_ingest_engine_on_a_failed_document` | slice 8: `a_stop_failure_on_one_index_lets_the_others_land` |
| Item-level vs flush-wide; lane parking | sink side unchanged | `tests::parking`, `tests::merge`, daemon `a_stop_rejection_halts_its_lane_writing_several_items_at_once` | `a_parked_lane_fills_only_its_index_half_and_other_backfills_go_one_batch_at_a_time` |
| Memory budget | many reservers; rules 1–5 per §3.5 (slice 4) | `random_schedules_under_a_tiny_budget_stay_within_it_and_finish`, `live_keeps_room_beside_a_backfill_holding_its_half`, `a_batch_bigger_than_the_budget_still_builds` | `budget::tests` (`concurrent_reservations_never_pass_the_limit`, `a_release_wakes_every_waiting_task`, `a_release_between_the_refusal_and_the_wait_is_not_lost`, `an_index_holding_no_live_bytes_gets_one_live_batch_past_the_limit`, `the_oldest_index_refused_by_the_limit_holds_younger_ones_back`, `every_index_gets_every_byte_back_once`), `one_bursting_index_of_three_holds_its_share_and_the_others_build`, `a_deferred_index_is_never_counted_committed_before_its_documents_land`, `an_index_past_its_waiting_bound_holds_the_live_batch_whole`, `random_schedules_over_three_indexes_with_one_bursting_stay_within_their_shares` (binds: fails with the share off), `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it`, `an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building`, `an_index_backfilling_keeps_the_live_room_main_gave_it`, `four_indexes_pass_the_limit_by_at_most_a_live_batch_each_and_a_backfill_batch`, `a_hold_across_an_engine_restart_is_one_wait`, `a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment`, `a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes`, `a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch`, `a_large_batch_and_a_small_one_both_build_beside_a_parked_index` |
| Gate fairness | lines per kind; budget asked at grant (G1–G9, slice 5) | `gate::tests`, the ingest gate tests | `gate::tests` (FIFO per kind, every fourth contested grant, a refused waiter keeps its place and the permit goes on, behind a limit refusal takes no limit room, a share refusal holds no one back, a waiter's standing read at the budget's current generation (after a release and after a settle larger than its estimate), cancel-safety, many waiters never past the permits and every one granted, the one task never ahead of an eligible waiter), `snapshot::tests`' walks reading one key page at a time; 6a: `a_task_blocked_on_a_full_lane_holds_no_permit` (two indexes, one lane full, `builds = 1`) |
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
| 5 | **Gate with lines** (budget asked at grant), the key-page permit (§3.5, G1–G9 and K1, as built). One task: the lines have no waiter in production until 6a; the task asks `Gate::choose`, which yields only to an eligible waiter, so grants are as before. The budget gains `reserve_behind` and the refusal's reason (G6). | gate tests of §4: each mutation of the rules (G1–G9, K1) in the table under §3.5's bounds fails at least one test, but one shown equivalent | `snapshot/rows` at `builds = 1` and 4 |
| 6a | **Reader task + one builder task** over every pending set, the **lead bound** (`max_waiting_per_index`, `max_waiting`), the stop barrier, the stream handed back. No independence yet; `main`'s pacing. | `the_reader_waits_at_the_lead_and_resumes`, `a_stop_mid_run_builds_what_the_reader_marked_last`, `a_stop_with_many_pending_ids_finishes_within_the_deadline`, `an_id_of_an_index_without_lanes_pins_nothing` | the hop and the locks: the full in-process suite, `shutdown` e2e, the end measurements |
| 6b | **Per-index builders, dispatcher, per-index walks and coalescing**, the held-index exemption and `max_pending_documents`. Designed in §3.9, proposed as three PRs: 6b-1 builders (walks one at a time), 6b-2 walks at once, 6b-3 the exemption. | §3.9's mutation skeleton; the independence tests; `indexes_walk_at_once`; `a_task_blocked_on_a_full_lane_holds_no_permit`; the seeded schedule over three indexes | everything; the end measurements (§3.9, 9) |
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
| `max_waiting` (`--max-waiting` / `FLUSSO_MAX_WAITING`) | `2 × builds × max_changes`, risen to a larger `max_waiting_per_index` (which, unset, falls to a smaller `max_waiting`) | 0, or below `max_waiting_per_index` with both set | 6a |
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
