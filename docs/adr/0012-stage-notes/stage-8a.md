# Stage 8a design: one reader, one task per index

**Status:** design, against `main` at `a187cf91` (2026-11-20), revised after an independent review of
`61323c20`; the owner's decisions are in §6 (b). Slices 1–6a merged (6a: #446, `7e10c43c`); 6b's
design is §3.9 (revised after design review round 1), for review before code. Slice 1 merged (#436). It replaces the note written before stages 3–7, stage 9's first half (#427), #434
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
  go on, as ADR §5 decided. A stop with such an index builds its backlog under the deadline, or
  fails (exit 1) naming it, as `main` does with a stalled sink (§3.9, 8; O3, decided).
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

- **Total:** the limit, plus one live batch per index (rule 5), plus one backfill batch per index (rule 4,
  per index since 6b-3; one batch before), plus what each build in flight settles past its estimate.
  Guards: `four_indexes_pass_the_limit_by_at_most_a_live_and_a_backfill_batch_each`
  (the reviewer's sequence: 224 KiB before 6b-3, 288 KiB with rule 4 per index), `random_reservations_stay_within_the_stated_bounds` (seeded
  reservations and releases over four indexes; a guard, it doesn't tell rule sets apart), and the parked-lane
  engine test below, which checks the bound at every write for ten simulated minutes.
- **Backfill:** half, plus one batch per index that held no backfill bytes (6b-3; before: one batch when
  backfill held nothing), plus the same settle slack.
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

**As built (slices 3 and 6a; 6b-1a below).** `Progress` counts every live change read and closes a **`Step`** at
each live grant, through the candidate's read count (6a's `close_step_through`, gone in 6b-1a). Every lane of the
deployment is registered with every step, with an item (`published_build`) or without
(`skipped_build`). A lane keeps the steps that can still answer, at most `k + 2` for `k` unlanded
items (`progress::tests::parked`); a sink's count is the read total at the oldest step its lanes have
committed. Per read, not per position: a replayed change is a second read. Reads an ended run never
granted count with the next step routed. Nothing listens to the count; readers poll it.

**6b-1a replaces the global step with a frontier per index** (§3.9, I5): with independent builders a
quiet index's lanes are registered with no other index's step, so the global order the steps rely on
is gone. The definition above does not change; these moments do:

- **a change that builds nothing** counts as soon as it is read and everything before it landed (the
  catch-up), before its empty build publishes: earlier than 6a and `main`, correct by the definition;
- **a sink's count never goes down** (a per-lane high-water), and across an engine restart an ended
  run's unbuilt reads count no sooner than the new run's first live grant (slice 3's moment), while reads
  that built nothing or all landed count at once (the restart cap, §3.9 §3);
- **a sink with no lane** has committed every read (slice 3: every closed step);
- **a lane that never had an item** answers `Through(routed)` where it answered `Idle`: the source
  confirms through a run that built nothing even before any grant;
- **reads cut before their marks** (a reader error) stop every frontier and catch-up from the moment
  they are counted, through the restart's backoff, not only from the next run's `forget_all`.

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
| stop (#371) | reads nothing more, hands over its cut run, returns the stream | abandon the walk (deliveries dropped, redelivered next start); after the reader's end: finish running builds, build and publish what waits (at most the lead; from 6b-2 an exempt backlog too, under the deadline, §3.9 I12), return | `drain_lanes`, `close_live` on the returned stream, poll it to its end (unchanged); never `Ok` with a document waiting |
| a builder fails (6a–7) | dropped in place | the others dropped in place | `Err`; the daemon restarts the engine with `[backoff]`, as `main` |
| a builder fails (slice 8) | goes on | that builder alone: give back, its own `Backoff`, restart; its dropped requests come back through the lane | — |
| a panic | unwinds the run's task | as the reader | the daemon restarts the engine, as `main` |

**What a stop costs**, against `main`: `main` builds one batch and finishes `2 × builds` slots. 8a
builds at most the lead (2,048 ids at defaults over every index, 512 per index) plus one run, and
finishes the slots in flight: a few batches more, well inside `--shutdown-timeout-secs` (30 s) at any
realistic build rate. From 6b-2 an exempt index's backlog is built too, under the deadline, or the
stop fails (exit 1) naming what was left (§3.9, 8; O3, decided).

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
acquire/release model (`every_wake_is_taken_by_a_pass_or_wakes_the_task_within_three_preemptions`).
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
| `together` (round 4, one word): the mark dropped by a store after the flags are read, round 3's two-step form | `a_wake_at_the_end_of_a_pass_is_not_lost`, `every_wake_is_taken_by_a_pass_or_wakes_the_task_within_three_preemptions` (loom) |
| `together`: relaxed orderings on the word | `every_wake_is_taken_by_a_pass_or_wakes_the_task_within_three_preemptions` (loom: a pass that took a flag misses what its waker wrote) |
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

**Status:** design, revised after design review rounds 1 (of `1ce6f24d`) and 2 (of `828fc333`).
6b-1a is built from this design (§3.6; as built in §11); the rest is not. Against
`main` at `7e10c43c` (6a merged). The structure (one task, N futures) is endorsed. The rules (I1–I13)
are written to be built and tested as stated. The owner's open questions are at the end.

**What 6b delivers.** A builder per index (live and backfill of that index, its lanes, its grant
order), a dispatcher for backfill requests that never blocks, walks of several indexes at once, and
the exemption of an index held by the budget from the reader's lead, counted against
`max_pending_documents`. Not in 6b: per-index status, metrics and endpoints (slice 7), per-index
restart (slice 8).

#### Design review round 1: what changed

Each point was checked against `origin/main` before it was taken.

| Point | Verified against | Change |
| --- | --- | --- |
| F1: the reader stopped before an index could become exempt (the per-index **changes** bound I added) | 6a's `Lead::reached` (`reader.rs`): its changes bound is the global `max_waiting`, read count minus the last grant's | the changes bound stays global, as 6a's; a **held** builder folds its slices (no per-change entries) and counts by documents only, as 6a's deferred index did (§5, I6, I7). The slice-4 scenario stops no earlier than second 61 in 6b-1c and never in 6b-2 |
| F2: leaving the exemption stranded up to 511 documents with no candidate | my I7 | a held or exempt builder works through its **backlog** (everything waiting through its fold) in takes of `max_changes` ids, granted as soon as a permit and the budget allow; the backlog and the exemption end only when nothing from before waits (§5, I7, I8) |
| F3: a stop or the stream's end left an exempt backlog unbuilt | my I11; §2's reversal | everything read is built at the stream's end; a stop builds and lands everything read, the exempt backlog included, under the shutdown deadline, else fails (exit 1, naming the index and what was left). No run ends `Ok` with a document waiting (§8, I12) |
| F4: G9 as is breaks the lines with N builders | `Gate::pass` (`gate.rs` 411–460: no waiter in a line takes the fast path, `reserve_behind(…, false)`); `Permit::drop` (`gate.rs` 127–141: wakes only while a line holds someone) | a builder's waiter **persists** across polls, from a batch ready until granted, the batch gone, or a publish blocked; wake-ups are targeted (G9′, G7″, §6) |
| F5: G7′ left a permit idle | G3's standing at the current generation (`gate.rs`) | **dropped**; the cost stays recorded for a later, measured change (§6) |
| F6: the frontier credited an ended run's reads before their replay landed | `forget_all` (`progress.rs`: a step routed to no lane) | a **replay window** caps every lane at what was committed when the run ended until the new run has read as many changes as were left uncommitted; then an index credits them with its first item taken after the replay, or the catch-up (§3, I5) |
| F7: "live builds of one index ≤ 2" contradicted 6a | `Builds::choose` (`builds.rs` 497: `live_running.is_empty() \|\| candidate.full`) | 6a's rule kept: full live batches up to `builds`, the exempt drain too (I3, I7) |
| Gaps: late request at the end; results and errors; confirm after the registering builder; the stop to N builders; "building"; the record's bound; metrics; memory; cross-index visibility; the stop bound; tests owed | the code each names | §2, §3, §4, §8, §9, §10, §12 |
| R2: every waiting builder woken per change | `Gate`'s `changed.notify_waiters` (G7) | targeted wakes designed (G7″), checked by a 24-index throwaway measurement in 6b-1c before it merges (§6) |
| R3: independence at awaits, not for CPU | the build's first poll (`builds.rs`'s spawn after one poll) | stated, with what bounds the synchronous work per poll (§1) and in O1's wording |
| R4: a builder waiting for the walk token | — | it keeps coalescing until it gets the token (§7) |
| Slice plan | — | the reviewer's: 6b-1a, 6b-1b, 6b-1c, 6b-2 (the exemption), 6b-3 (walks at once) (§11) |
| O2 | ADR 0012 §1 ("up to `max_changes` per build") | the id cap evaluated beside the change cap; decided: documents (O2) |

#### Design review round 2: what changed

| Point | Verified against | Change |
| --- | --- | --- |
| D1: a quiet index pins confirmation and every per-sink fence; `routed` was never built | `least_of` (`progress.rs`: a lane with nothing outstanding answers `Through(newest)`, `Idle` without one); `publish_live` (`commit.rs`: every grant registers every lane but a deferred index's) | **`routed` built in 6b-1a**: raised by the reader after a run's marks and count; a lane with nothing outstanding answers `Through(max(newest, routed))`; unset at `forget_all` (§3) |
| D2: the replay window lowered counts, ended early, or never | `flusso.changes.committed` (`apps/cli/src/telemetry/metrics.rs`: an observable counter); the stream resumes at the confirmed position | the window is gone: a per-lane **high-water** (never down), and a **restart cap** one below the least read the ended run left unbuilt, lifted by the first live step after the restart (slice 3's moment). A source-terms end is described and not taken (§3) |
| D3: the fold's and the exemption's ends were each stated two ways | my §5 | the fold ends when nothing waits; entering it drops the queued slices; the exemption is latched at the cap and cleared below it, with the consequence for a busy index (§5) |
| D4: the exemption could never be decided in the slice-4 scenario | my §5 (exemption as builder state, a fold raising no flag) | **one writer per state, in its poll**: held and fold by the builder, exempt by the reader, which latches it at the moment the index would stop it (§5) |
| a lost turn while a builder can't ask | `start_backfill`'s `walk_marks` await, `finish_snapshot`'s lane send, `builds.start` (`commit.rs`) | a waiter is in a line only while every poll of its builder reaches its ask: it leaves before an await outside the loop; a publish is polled once before it is awaited (§6) |
| who registers on the budget; what the tick does | `Gate::acquire`'s loop (`gate.rs`) | with wake-all each waiter registers itself, as today; the dispatcher holds nothing; the tick is each builder's backstop grant (§6) |
| a shrinking waiter keeps a stale limit refusal | G3/G6 (`gate.rs`) | a shrink clears the refusal, a growth keeps it (§6) |
| G7″ before measuring | — | **not built first**: G9′ with slice 5's wake-all is the design; a throwaway 24-index measurement with one index parked comes first in 6b-1c; G7″ only if wake-all costs (§6) |
| "held" missed a lane full at its backstop and a deployment with no budget | `LANE_BACKSTOP`; `Builds`' optional budget | held includes a publish blocked on a full lane (§5) |
| O2 answered with "changes" | — | moot: the owner chose documents (O2, decided) |
| 6b-1a is not invisible | the catch-up | stated in §3 and §3.6; on 6b-1a's user-visible list |
| a partial take can only be a `progress` test until 6b-1c; the skeleton's holes | — | §12's skeleton |
| §4's "unrouted" row, §5's row 2, I1's running mask, the exit-1 message's source, a sink with no lane | the code each names | fixed in place |

#### What 6a as built changes in the plan above

| The plan said | 6b does | Why |
| --- | --- | --- |
| an index **task** per index, spawned in a `JoinSet` (§1, §3.4, §3.7) | an index **builder**, a future of the engine run's task, polled by `together` generalised to N (I1) | 6a measured the hop: a spawned reader cost +21.6% locally and +54% on CI at one change per batch; one task removed it (§3.8) |
| the reader marks a run, then notifies the index task (§3.1) | marks, counts, then hands each touched index its slice of the run, or folds it into a held builder's backlog (I2) | per-index batching needs each change's read instant and read number (R3) |
| one live build at a time, a second while `max_changes` ids wait (§3.4) | 6a's rule per index: full live batches up to `builds` | 6a's `Builds::choose` |
| global steps closed by the one builder (§3.6, `close_step_through`) | a **frontier** per index carried by each item (I5) | every lane is registered with every step today; with independent builders a quiet index's lanes would never be |
| a live build takes the indexes that fit and defers the rest (slice 4) | gone: a build is one index's; refused, the builder is **held** and folds (I7) | each index reserves for itself |
| the builder confirms after every grant and landing | the dispatcher confirms once per poll in which something was registered, and on the tick (I10) | N builders would each walk every lane |
| a grant pass joins the lines, asks once and leaves (G9) | a waiter persists across polls (G9′) | with N builders in one task nobody would ever be in a line (F4) |
| coalescing waits inline | a deadline per builder (I11) | an inline wait stalls the index's live changes |

#### 1. The structure: one task, N + 2 futures

| | Per change | Per run (per touched index) | Per batch | Independence |
| --- | --- | --- | --- | --- |
| **A: a spawned task per index** | as 6a | a send **and a wake of another worker** | a cross-worker wake of the reader at its lead, and of the dispatcher to confirm | full, and parallel bookkeeping |
| **B: one task, the reader, a dispatcher and N builders as futures (endorsed)** | as 6a | a send and a flag | a flag | at every await |
| **C: hybrid** | B's or A's | two paths, two sets of rules | — | — |

**What needs another worker thread already has one**: a build's CPU after its first poll (decoding
rows, scripts, `coerce_document`) runs on a task of its own (`Flights`), as on `main` and 6a; key reads
get a task of their own in 6b-3; sink engines are tasks.

**Independence holds at awaits, not for CPU (R3).** What runs on the shared task, synchronously, per
poll of one future:

| Work | Bounded by |
| --- | --- |
| a build's first poll (query text, binding the keys) | `max_changes` documents per build (O2, decided) |
| a take, under one index's pending lock | `max_changes` ids, oldest first: O(`max_changes` · log n) |
| a candidate's walk | `max_changes` changes |
| a publish | one `Arc` send per lane |
| a grant pass | one gate lock, two budget locks |
| confirmation | one `landed()` walk over every lane, once per poll (I10) |

**`together` for N (I1).** One `AtomicU64`: bit 0 the "being polled" mark, bit 1 the stop's watch
(6a's third future, polled only when it fires; its end raises the reader's and the dispatcher's groups),
bits 2–63 groups; future `i` (reader 0, dispatcher 1, builder `k` at `2 + k`) raises group
`2 + (i mod 62)`; a raised group polls every running future in it (past 60 indexes futures share a group
and may be polled unwoken, which each tolerates: every one is an `async` block or a `poll_fn` that
re-checks its state). A pass starts by **taking every raised bit of a running future or the watch with
one `fetch_and`** (6a took one flag per future), then polls the raised running futures from a rotating
start, advanced every poll; a poll makes **at most 64 future polls** (6a's 32 passes of two), so a
poll holds the worker no longer than 6a's whatever N is. A poll cut by the cap mid-pass raises again,
with one `fetch_or`, the groups it took and didn't poll, then ends with the task woken (6a's yield), so
the next poll serves them. Ended futures are masked by a running mask per future, apart from the group
bits: a group stays live while any future in it runs, and polling a group skips its ended futures. One
location, so a wake is either taken by a pass or wakes the task on every architecture; the loom model is
extended to three waking threads, two of them sharing a group, and the cap's re-raise.

**Expected hot-path cost**, in 6a's terms (§3.8):

| Where | 6a | 6b, one index | 6b, K indexes touched by a run |
| --- | --- | --- | --- |
| per change | a clock read for the run's deadline | same, plus its read number kept (8 B) | same |
| per run | one `Vec`, one send, a flag, an atomic add (release from 6b), a lead read | same | K `Vec`s, K sends, K flags; a fold is one atomic `fetch_max` instead of a send |
| per wait at the lead | `AtomicWaker` registration, a flag, a fence, the lead read again | same, plus the oldest queued read number of each builder (an atomic load each) | same |
| per batch | two budget locks, a gate lock, a `fetch_max` and a fence, a clock read, the candidate's walk, the registry lock to close the global step | same, minus the global step lock, plus the frontier (the pending order's first entry); the waiter joins and leaves its line (two gate locks) only when a pass doesn't grant at once | per index batch, the same |
| per confirmation | after every grant and landing | once per poll with a registration, and the tick | same |
| per poll | waker, a `fetch_or`, a `fetch_and` per flag, a load and a `compare_exchange` per pass | same, plus the rotation counter | the bit scan over raised groups |
| per gate change | `notify_waiters` (no waiter in production) | one wake, of the waiter the permit goes to (G7″) | same |

So one index should measure as 6a, which measures as `main` on CI (gate green at `a8ca2f4e`: `sinks1/1`
+2.8%, `sinks2/1` −3.2%).

#### 2. Who owns what, and what crosses

| Future | Owns | Never does |
| --- | --- | --- |
| **Reader** (one) | the live stream, the resolver, the run being read, the read count | build, publish, confirm, wait on a lane, touch a request |
| **Dispatcher** (one; the run's result) | the request consumer, routing requests, confirmation and the 1 s tick, the end signal, the builders' results, the wind-down (`drain_lanes`, `close_live`) | build, walk, wait on a builder |
| **Builder** (one per index with lanes) | its queued slices or its fold, its `Builds` (slots, admission, grant order), its gate waiters (one per kind, persistent), its budget parts, its lanes' producers, its walk and key reads, its request inbox and coalescing deadline, its held, backlog and exempt state | resolve, confirm, read the stream |
| `Progress` (shared) | pending sets, read numbers, the read count, item keys, registrations, the per-lane frontier record, the replay window | — |
| shared | `Gate`, `Budget`, the key-read permit, the `Lead` | — |

**What crosses, and how** (all inside the task: a send and a flag, or an atomic):

- **Reader → builder**: a slice over the builder's inbox, or, while the builder folds, an atomic
  `fetch_max` of its fold's newest read number (no entry).
- **Builder → reader**: the lead's wake after a take or a grant, and on a change of held or exempt
  state (6a's protocol).
- **Builder → dispatcher**: a builder's result (it returned, or its error) is written to its own cell
  and raises the dispatcher's flag; the dispatcher reads every cell when polled. A registration raises
  the dispatcher's flag too, so its "confirm once" runs after the builder that registered, in the same
  poll or the next. Not through the `Progress` listener: it has one slot, the fence's (the daemon's
  `on_registered`). `Progress` keeps a registration signal of its own for the engine run (a flag and an
  `AtomicWaker`; the flag's swap wakes only on the first registration since the dispatcher last looked),
  raised wherever the listener fires: a run routed, a build's lanes registered, a build released.
- **Dispatcher → builder**: a request (the inbox and the flag); the **end signal** (§8); the **stop**:
  `together`'s watch fires once and wakes the reader and the dispatcher; the dispatcher sets each
  builder's stopping flag and raises it, once per builder per run. No builder registers on the stop.

**"Building"** in I5 means a build taken and not yet released (`Pending.builds`, through
`Progress::release`), not the admission set `Pending::land` clears.

#### 3. Batches, takes and the frontier, per index

- **A change's read number**: its place in read order over the deployment's life (R1). Each mark
  carries it; a pending document keeps its oldest unbuilt change's read number beside its position.
- **An index's slice**: the run's changes whose resolve gave a document of the index, in read order,
  each with its position, read instant and read number. A change touching no index is only counted.
- **An index's batch** (I3): R3 over its slices with its own last live grant: every change read before
  `max(the oldest not granted's read, the last live grant) + max_delay`, up to `max_changes` changes,
  full at `max_changes`, the inbox drained first. Full candidates start up to `builds` (6a); a partial
  one only while no live build of the index runs.
- **A take** (I4): the documents waiting at or before the candidate's last change, oldest first, **at
  most `max_changes`** (O2), skipping documents in flight or page-held. Documents left by the cap are a
  **standing candidate**: full while `max_changes` or more are left (it may start beside a running
  build), else granted as soon as no live build of the index runs. Never more than `max_changes` ids;
  #135 holds (a lone change's documents go at once).

**The frontier (I5) and why `changes_committed` stays exact.** Each take sets `F`: the least read number
still waiting on the index, minus one, or, with nothing waiting, the read count read (acquire) before the
take; its items carry `F` on every lane of the index, published or skipped. A lane `L` of index `A` has
committed the `F` of its newest item `k` with every item of `A` through `k` landed or skipped on `L`;
once `L` has landed every item and `A` has nothing waiting or building, the read count read before `A`'s
pending state. A sink's count is the least over its lanes. Every document of `A` touched by a change
through `F` went into an item through `k`: a document of such a change still waiting at `k`'s take
would have lowered `F`, and one re-marked by such a change was marked before the take. The reader
marks before it counts (R1) and counts with release, so a counted change's marks are seen. A partial
take (the cap, the backlog) counts only through its oldest document left.

**Routed (D1; built in 6b-1a).** `Progress` keeps the newest position the reader has routed, raised
after a run's marks and its count (R1), under the registry lock, then the fence listener fires. A
reader of `Progress` takes the registry lock first and each index's pending set under it, so one that
sees a routed position sees every mark made before it. The two writers that change what such a reader
combines across both kinds of lock keep it whole: `forget_all` holds the registry lock across every
pending set's forgetting, the unset of `routed` and the caps (review round 1: it took the registry lock
only after the pending sets, and a reader between saw the documents gone with `routed` still set and no
cap); `read_unmarked` records the cut before the read count moves (release), and `committed` and every
frontier read the count (acquire) before the cut, so whoever counts the cut reads sees the cut. A lane with nothing outstanding (no unlanded
item, no build of its index not yet registered on it, nothing waiting on its index) answers
`Through(max(newest, routed))`, and `Idle` only while both are unset. So a quiet index no longer pins
confirmation or a per-sink fence at the last grant that registered its lanes, and a lane that never had a
live item answers through what was routed, not `Idle`. **Across an engine restart** `forget_all` unsets
it: what the ended run routed and never built is forgotten, and no lane may answer through it; until the
new run routes its first run a lane answers through `newest`, which only registered items moved. **The
position contract** holds: `routed` is a position the current stream emitted and the reader marked; a
source that replays a change under the position it had routes it again after the restart, and the answer
is the larger of two positions each naming what it named. 6b-1a keeps the one builder's skips (they
still move `newest` and carry each lane's frontier); from 6b-1c a builder registers only its own index's
lanes.

**Monotone, per lane (D2a).** A lane's committed value is a high-water mark: it never goes down, so a
sink's, the least over its lanes, never does (`flusso.changes.committed` is an observable counter: a
decrease reads as a reset). It is needed because the catch-up credits the read count while an index is
idle, and an older entry answers again once the index marks a new document.

**Catch-up, exactly.** A lane catches up to the read count, read (acquire) before anything else, when it
has no unlanded item and its index has nothing waiting and no build taken and not yet released
(`Pending.builds`, which `Progress::release` clears, not the admission set `Pending::land` clears). A
change that builds nothing therefore counts as soon as it is read and everything before it landed, before
its empty build publishes: earlier than 6a and `main`, correct by the definition (it needed nothing), and
on 6b-1a's user-visible list.

**Ended runs (D2): the restart cap.** At `forget_all` `Progress` records the least read number among
what the ended run leaves unbuilt: documents waiting, documents in builds taken and never released, and
reads the reader counted without marking them (R9's guard counts a run cut before its marks through
`Progress::read_unmarked`). Until a live step is registered on the lane after the restart, the lane's
catch-up is capped one below it (per lane: a lane the first grant passes by, its index deferred by the
budget, keeps its cap; review round 1 found the global lift through the seeded model). A cut is honoured
from the moment it is counted: until the next run's `forget_all`, no frontier and no catch-up passes the
first cut read, and no step of the dying run lifts that; entries taken before the restart are below it already (a frontier is below every
document waiting at its take or marked after it). Then:

- **never earlier than slice 3's moment**: an ended run's unbuilt read counts no sooner than the first
  live grant after the restart that registers the lane, when slice 3 counted it ("with the next step
  routed"; slice 4 already left a deferred index's lanes out of a step);
- **reads before the cap still count**: a change that built nothing (confirmed through `routed`, never
  replayed) or whose documents all landed; a cap over the whole ended run would hold such a count short
  for ever;
- **never short for ever once everything read has landed**: after the first grant an index credits the
  ended run's reads with its first entry after the restart once it lands, or by the catch-up once idle;
  a quiet index catches up (6b-1a: the one builder's skips give every lane but a deferred index's an entry
  at every grant, and a deferred index is taken later, which registers its lanes; 6b-1c, where a builder
  registers only its own lanes: the first grant after the restart must also lift the cap of every lane
  whose index has nothing waiting or building, or a quiet index stays capped (6b-1c's test, written first:
  `after_a_restart_a_quiet_indexs_lanes_catch_up_with_no_step_of_their_own`); a waiting or deferred
  index's lanes keep it until their own step); a short replay or a stream end lands what was read and
  catches up; a second restart before a lane's step keeps the lower cap. A replay that never comes (the stream ends at once) leaves the count short of reads never
  built, which is right;
- **never down**: the cap only stops a lane from rising; the high-water keeps what it had.

**What this does not promise**: that an ended run's reads count only once their own replays land.
Postgres resumes at its confirmed position, which is coalesced, at commit boundaries and may be well
before the last landed change, so "the replay has read again through the ended run's last read" is
known only in source terms. The port would have to provide it: `ChangeCapture` stating that a position's
stamps are ordered across streams, or a `reread_through(old, new)` answer. Not worth it for a counter
whose floor slice 3 set; recorded here. Tests: `an_ended_runs_reads_count_only_with_the_first_grant_after_the_restart`
(the intermediate count), `a_sinks_count_never_goes_down_across_a_restart` (two sinks),
`a_replay_from_before_the_committed_point_credits_nothing_before_the_first_grant`,
`a_short_replay_and_a_quiet_index_count_everything_once_landed`,
`reads_that_built_nothing_count_across_a_restart_with_no_replay`.

**A sink with no lane** has committed every read: it has nothing to land (slice 3: every closed step).

**The record's bound behind a parked lane.** A lane keeps, in order, its unlanded items with their `F`
and one newest entry; a skip (an empty build, or one whose documents all failed under `skip`) replaces
the newest entry, as slice 3's compaction does. So at most `k + 2` entries for `k` unlanded items,
however many empty builds pass (an empty build holds no budget, so a parked index could make one per
change). `progress::tests::parked` is ported per index.

**Model test** (written first, the model independent of the code, as slice 3's): seeded reads marking
documents, takes (whole, capped, around a held document), deferrals, publishes, empty builds under a
token, landings and redelivered landings, skips, routed raises, cuts and ended runs (one with a build in
flight) on three indexes and two sinks. The model keeps, per read, the documents it marked on each index and the take
that covered each; a read is done for a lane once every such take has landed there, and an ended run's
undone reads once the lane has landed an entry of a step closed after the restart (slice 3's floor). At
every step each sink's count is at most the model's and never lower than before; once everything has
landed it equals the model's.

#### 4. Order, position and visibility

- **Lane order per index is its builder's grant order**: one `Builds` per index, live and backfill
  through the same slots.
- **A change touching documents of two indexes** is pending on both until each builder lands it;
  confirmation, fences and the committed count take the least over every lane of each index's pending
  minimum and its lanes' unlanded items, so they wait for both.
- **What users can now see across indexes**: one change touching `A` and `B` can be searchable in `A`
  well before `B`. A per-sink fence (`/wait` without an index) still waits for both. The manual says so
  (the documentation agent, in 6b-1c).
- **Positions on a lane stay monotonic within an engine run**: an item's position is its batch's newest
  change (the live candidate's last change; for the fold's backlog take, the newest position waiting on
  the index when it was taken), taken in grant order. Two items of an index may share one (a take cut
  by the cap), never a smaller one. *(Corrected in fix round 1: "the newest position marked on its
  index" was not what the code does.)*
- **stdout's `seq`**: non-decreasing per index, may repeat, no order across indexes (decision (b)2).
  `docs/src/reference/sink-stdout.md` today says "across indexes the lines keep position order" (line
  11) and "Monotonic within a run" (the `seq` row); rewritten in 6b-1c: a consumer resumes per index,
  from the last `seq` inclusive.

#### 5. The lead, the hold, the backlog and the exemption

**One writer per state, each in its own poll (D4).** **Held** and **fold** are written by the index's
builder; **exempt** by the reader. Everyone else only reads them.

**Held.** A builder is held while its live batch is refused by the budget (its share or the limit) **or
a publish of its is blocked on a full lane**. The second is decided: without it an index parked by its
lane at the item backstop, or in a deployment with no budget, would never fold or become exempt, and the
reader would stop at `max_waiting` changes (no worse than 6a, but 6b-2's promise would not hold there;
with the 1 KiB floor the lane fills before the share once `max_built_mib` passes about 2 GiB). It stops
being held at its next live grant, or once the blocked publish went through.

**Fold (F1, D3).** Entering the hold, the builder enters the fold and **drops its queued slices**: left in
place they would surface later as candidates with stale read instants; their documents stay waiting and
the backlog takes them. While it folds the reader hands it nothing (it reads the builder's fold flag): no
per-change entry, so a held index's changes cost no memory and count toward no changes bound, as 6a's
deferred index counted by documents only. **The fold ends when nothing waits on the index** (a moving
point; the high-water read number of round 1 decided nothing and is gone). Then the builder queues slices
again and R3 cuts its batches.

**Backlog.** While it folds, the builder takes oldest first, at most `max_changes` ids a take, as soon as
a permit and the budget allow (a standing candidate, full while `max_changes` or more wait, so several
build up to `builds`), each take with its frontier. A busy index that never drains to zero stays in the
fold and is built by availability, in `max_changes`-id takes, instead of R3's cut; it returns to R3 the
first time it drains. No document waits without a candidate (F2): a folding builder always has one while
anything waits.

**Exempt (6b-2, D3, D4).** The reader latches an index exempt in its poll when it finds it **held with
`max_waiting_per_index` documents or more waiting**, and clears the latch when fewer than
`max_waiting_per_index` wait. It reads those counts at every lead check, so it decides at the moment the
index would stop it. In the slice-4 scenario at second 61 the reader marks `orders`' 32nd document, finds
`orders` held at the cap, latches it exempt and reads on; exemption as builder state could not be decided
there, since the held builder is polled only after a budget change and none comes. **For a busy index**:
below the cap it counts toward the lead again even while it still folds; held again at the cap it is
latched again. An index whose builds keep up drains below the cap and paces the reader as before.

**Exempt grows only while held, and while another index isn't (review of #452).** The exemption is
there so that other indexes keep going. An exempt index's documents grow only while its builder is held
**and** some other index's builder is not held; that is also the only state in which an index becomes
exempt. Latched but not growing (its hold ended, or every index is held), it is **frozen** at the count it
had when it stopped growing: it may drain, and the reader stops whenever it has that many waiting (at
least its cap), so it never grows past it; below its cap the latch clears and it paces the reading like
any index. So an index held once and then **merely slow** (its builds refuse nothing, so it is never held
again) reads ahead no further than it was when its hold ended, and paces the reading once it drains: the
6b-2 as first built kept it exempt and read ahead to `max_pending_documents` (the review's
`a_slow_index_once_held_paces_the_reading_again`: 17 waiting at the hold, 65 later). Either edge of a
hold wakes the reader: becoming held may turn a cap into an exemption, a hold ending may let a frozen
index grow again (some index is free again) or may be the moment nobody is held. One predicate decides
it (`Lead::someone_free`), so the owner can change the rule in one place.

**"Some other index" means a free index (review round 2 of #452).** Round 1's rule took "not held" for
"making progress", and an index with no traffic is never held: a whole sink down with one quiet index
still read every exempt index to `max_pending_documents`, and at the defaults a light index is held only
once its 256 MiB share fills, a backfilling-only one never (only a live refusal held). So: **an index is
free while it is not held and none of its items has been waiting on a lane for an acknowledgement for
`STUCK_AFTER` (5 s) or more without any of them being acknowledged.** An exempt index grows only while
it is held itself and some other index is free; otherwise it is frozen, as above.

| An index | Free? |
| --- | --- |
| quiet, nothing outstanding on any lane | yes: healthy as far as anyone knows (a stuck index beside it must still be exempt, or a later change of it would sit unread behind the stuck one's backlog) |
| publishing and acknowledged (an acknowledgement within `STUCK_AFTER` of the last, or of the first publish) | yes |
| items outstanding, none acknowledged for `STUCK_AFTER`: live or backfill, on any lane | no: evidence it is stuck too, long before the budget holds it |
| held (its live batch refused, or a publish blocked on a full lane) | no |

What counts as outstanding: an item from its publish until every lane is done with it, i.e. its `Hold`
is dropped (acknowledged and out of every ledger). A `backfill = false` lane is never sent a backfill
item; a lane that acknowledges without applying drops it at once; a lane at rest holds nothing. So none
of them makes an index look stuck. **Why not the stall age** (`daemon::Stalls`): it lives in the daemon,
a layer above the engine, and it counts live items only (backfill items carry no position), so a
backfilling-only index on a dead sink would look free. The engine keeps one small record per index
instead (`Outstanding`: how many items are outstanding, and when one was last acknowledged or the
first published), written where the items are published (a guard inside the item's `Hold`) and when
the last lane lets go of one (the guard's drop). It is read only while some index is held.

**`STUCK_AFTER` = 5 s.** A healthy sink acknowledges within its flush, hundreds of milliseconds (a
`Within(refresh_interval)` report delays the searchable fence, not the acknowledgement), and a busy index
is acknowledged many times a second. A sink engine restarting after a failure waits 1 s, then 2 s, 4 s,
… up to 60 s: one restart stays under 5 s **for a write that fails fast** (a refused connection); a
write that fails only at its timeout adds that timeout, so one restart can pass 5 s. A third failure in a
row (1 + 2 + 4 = 7 s) passes it whatever the write, which is when a sink is failing rather than
blinking. Either way the effect is bounded: a neighbour exempt meanwhile is frozen, not lost, and grows
again at the tick after the next acknowledgement (below). The cost of the wait: while the first-held index waits for
the others to turn not free, it may add `STUCK_AFTER × its change rate` documents (5,000 at 1,000 a
second), always within `max_pending_documents`. A constant, not a setting.

**Self-correction.** A healthy index whose acknowledgements take longer than 5 s counts as not free:
**a healthy sink taking more than 5 s per acknowledgement** (a large backfill bulk, a reindex's bulk)
makes its index not free, and an exempt neighbour then freezes and the reader may stop, until that
acknowledgement plus at most a tick. Never worse than `main`, which stopped the reading at the
neighbour's cap for as long as it was stuck. When the slow item is acknowledged the index is free again;
a reader parked at its lead is woken by the dispatcher's 1 s tick, which wakes a waiting reader
whenever there are two or more builders (one wake a second at most, a flag load otherwise), so it
re-checks the bounds. No faster polling; the acknowledgement itself wakes nobody.

**A whole sink down** (two or more indexes, every lane of the one sink stalled: every builder is held,
by its full lane or by the budget its unacknowledged items keep). No index benefits from reading on, so
none becomes exempt, and an index already exempt is frozen. The reader stops at the non-exempt bounds,
as before 6b-2: `max_waiting_per_index` per index and `max_waiting` together, at most one run past them,
plus whatever an exempt index had when the last other index was held (that backlog stays as it is). So
what is read during the outage is what `2e5e598e` read, the memory is the lead's, and the reader runs no
reverse-resolution queries against Postgres while it waits. **In truth** (round 2): that holds within
about `STUCK_AFTER` of each index with traffic having an item unacknowledged; in those first seconds a
held index may grow exempt by `STUCK_AFTER × its rate` documents. **An index that publishes nothing**
(nothing outstanding: no change at all, or only changes that build nothing, such as updates of columns
no document reads, #169) stays free, so the held ones read on, exempt, until `max_pending_documents` (at the
defaults about 266 MB and 1,000,000 documents, resolving each change they read) or until the silent index
publishes an item and that item too goes unacknowledged for 5 s. That case cannot be told apart from "one
index stuck beside quiet healthy ones", the slice's main case, and is bounded by the cap. As first built, 6b-2 latched every index
exempt and read on to `max_pending_documents` (1,000,000 by default, about 300 MB), resolving every
change it read: the review's `with_a_whole_sink_down_*` read 106 changes of 600 with 100 waiting where
the exemption off read 13 with 7. When any index's hold ends the reader is woken, and the others' backlogs
grow again, exempt, while that index flows. One index is unchanged (it is never exempt).

**The log** (review of #452, corrected in rounds 2 and 3): one line per index names its state, **not
exempt**, **exempt and growing** (reading goes on for the other indexes), or **exempt and frozen** (it
may drain but not grow, and the reading stops while it has that many waiting); one more names the
pending cap (the reading stopped at `max_pending_documents`, or reading on). Each is throttled on its
own: at most one line a minute per index (whichever state it names) and one for the cap, the first
always; once the minute is over a state that differs from the last line is logged at the next check.
**The lines are checked on the dispatcher's 1 s tick, and only there** (`Lead::tick`), for every index
and the cap, whether the reader is parked, the stream idle, or an earlier index over its bound: the
check reads the state the reader decides at its next check (`Lead::deciding`, the latch's rule without
its writes) and writes nothing the reader reads. **The line state is the engine's** (`ExemptionLog`,
kept across engine runs as `Outstanding` is), so a run that restarts goes on from the last line logged.
So **within a minute and a second of any change, the last line logged for an index names its current
state**, with one condition: while no engine run is live (a failed run's restart backoff, up to
`max_restart_backoff`) nothing is read and nothing is logged; the next run's first tick logs the state.
Round 1 throttled each kind on its own; round 2 logged from the reader's checks, so an idle stream (no
check), a restart (a new lead's lines) and an index after one over its bound (the check returns there)
could each leave a stale line for as long as they lasted.

**The lead (I6).** Before each run the reader waits while:

| Bound | Counts | Since |
| --- | --- | --- |
| a non-exempt index has `max_waiting_per_index` documents waiting | documents of that index | 6a |
| the non-exempt indexes together have `max_waiting` documents waiting | documents | 6a |
| `max_waiting` changes are read ahead: the read count minus the oldest read number still queued on a builder that doesn't fold | changes, as 6a's (global) | 6a, per builder's queue |
| the exempt indexes together have `max_pending_documents` documents waiting | documents | 6b-2 |

**The slice-4 scenario** (`orders` held at its share, changing one of fifty documents a second, cap 32):
`orders` folds, so its changes count toward no bound; its documents reach 32 at second 61. In 6b-1c the
reader stops there, as in 6a. In 6b-2 the reader latches `orders` exempt at that moment and the reading
goes on (the acceptance test turned around). Tests: `a_parked_index_changing_many_documents_does_not_stop_the_reading` (6b-2),
`an_index_reaches_its_exemption_with_nothing_else_in_flight` (6b-2, D4: no other index writes, so no
budget change ever wakes the held builder), and the repeat-heavy case
`a_held_index_rewriting_one_document_holds_no_bound_and_no_memory_per_change`.

**At `max_pending_documents`** the reader waits and every index stalls, as ADR §5 decided; no document
is lost and reading resumes as the backlog drains. While an index is exempt the slot stays pinned and
the WAL grows (`flusso.replication.slot_lag`, `flusso.slot.pinned_by` naming the index); the manual
says so.

#### 6. The gate with N builders (F4, F5, R2; round 2)

**G9′ (replaces G9).** A builder's waiter **persists** across polls: it joins its kind's line when the
builder has a batch ready, a free slot and no publish blocked on a full lane, and leaves when it is
granted, the batch is gone (taken by a stop, given back) or a publish blocks. While it waits its batch
may grow (more changes join the candidate, up to the cap); it keeps its place and is asked with its
current parts (G5 reserves at grant). One waiter per kind per builder. So the lines hold every builder
that can use a permit, G6 holds across indexes (a younger builder reserves behind an older one the limit
refused), a freed permit has someone to wake, and G4's every-fourth is contested across builders.

**In a line only while every poll reaches its ask (round 2).** A builder's waiters are arms of its
`select!`. Before any await outside that loop that can pend (`walk_marks`' database call in
`start_backfill`, `finish_snapshot`'s lane send, `builds.start`), the builder leaves its lines; a publish
is an arm, polled once before it is awaited, so a blocked publish is seen and the waiter leaves. A waiter
that cannot ask holds no place: G4 or G6 would select it, the others would be told `NotNow`, and the
permit would idle until it came back.

**Refusals and the budget.** With slice 5's wake-all (below) each waiter registers itself on the budget
at its refusal's generation, and again after each wake (`Gate::acquire`'s loop, as today); the dispatcher
holds no registration. The 1 s tick recomputes nothing in the gate: it is each builder's backstop grant,
as 6a's.

**Parts while waiting (round 2).** A waiter whose batch **grows** keeps its place and its refusal: a
refusal is monotone in size (a larger batch is refused too). One whose batch **shrinks** at the same
generation (a page hold, a stop) has its refusal cleared: a stale limit refusal would otherwise hold
younger waiters behind it (G6). Test: `a_waiter_that_grows_keeps_its_refusal_and_one_that_shrinks_drops_it`
(both halves).

**Wake-ups: slice 5's wake-all, measured first (the reviewer's decision).** G9′ with G7 as built (every
change wakes every waiter) is the design. The first work of 6b-1c is a throwaway in-process measurement
with 24 indexes, **one of them parked**: a held-at-share waiter is asked again after every grant
elsewhere, so each healthy grant costs the held builder a poll, a budget decision, a refusal and a
re-registration. G7″ (one targeted wake per change) is written only if wake-all costs; G9′ with wake-all
is the named fallback. If G7″ is built, two rules come with it: the "woken, not yet asked" mark clears at
every poll of its builder and the gate re-selects (a builder awaiting outside its loop would otherwise
keep the turn), and the dispatcher registers the gate on the budget after a refusal made in a builder's
poll at generation `g`, and again after each fire.

**G7′ is dropped (F5).** Not waking a refused waiter on an add left a permit idle: an add made its
refusal stale, `standing` counted it eligible and oldest, so it was selected while asleep. The cost it
aimed at stays recorded: while a backfill waiter is refused, every live grant's add costs it one budget
decision. A later, measured change could bring it back with two generations (release-only for refusals
not made behind; every change for "behind" and the reason).

**Pinned by** `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused` across two builders
(fix round 1: it failed, since a builder asked the budget before joining a line; see "Fix round 1"),
`a_freed_permit_wakes_a_waiting_builder` (no idle permit until the tick; a live waiter: no test grants
a backfill-only builder its freed permit), `every_fourth_contested_grant_goes_to_another_builders_backfill`,
`a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line`, the parts test above, and every
`gate::tests` case.

#### 7. Backfill

- **The dispatcher** hands each `Request::Backfill` delivery, ack handle included, to its builder's
  inbox and raises its flag; it never waits on a builder. An index with no lane, or one the builder
  doesn't build, is warned and acknowledged. A dropped delivery returns to the request lane.
- **Coalescing per builder**, on a deadline arm: start once every sink with a lane of the index asked,
  else at `max_delay`. **The walk token (6b-1c, R4)**: one walk at a time, `main`'s order; a builder
  waiting for the token keeps coalescing, so requests from other sinks of its index that arrive
  meanwhile join its walk. Tickets and cuts per §3.4. Acknowledged after its `SnapshotComplete` is
  published.
- **Walks at once (6b-3)**: the token goes. **K1 with several walks**: a key read runs on a task of its
  own, holding the key-read permit from before the read until it returns, and hands the page over a
  one-shot, so a walk not stepped mid-read still gives the permit back; the task is aborted with its
  walk. One permit stays (the pool is `builds + 1`).
- **Fairness between backfilling indexes**: one persistent backfill waiter per builder, FIFO (G2), so
  they take backfill grants in turn; G4 gives every fourth contested grant to the oldest eligible
  backfill waiter, whichever index; backfill's half (rule 2) is shared, and rule 4 is per index
  (corrected in 6b-3: "for them together" let a stuck walk keep the half and stop every other walk).

#### 8. End, stop and failure (F3)

- **The end signal.** A builder returns only once the dispatcher signals the end: the reader has ended,
  the request lane is empty, and every builder is idle (nothing waiting, no walk, inbox empty). A
  builder that went idle earlier stays, so a request arriving late for its index is served
  (`a_request_queued_for_one_index_after_live_ends_is_served`).
- **The stream's end**: everything read is built and landed, the exempt backlog included; no exception.
- **A stop** (I12; O3, decided): the reader reads nothing more and hands over its
  cut run (R7); every builder abandons its walk (deliveries redelivered next start), then builds and
  lands **everything read**, the exempt backlog included, under the shutdown deadline; the dispatcher
  runs `drain_lanes` and `close_live`. If that can't finish (a parked lane, or not in time), the stop
  fails as a stalled sink's does today: exit 1, the message naming each index with documents left and
  how many (`Progress::waiting_by_index`, which the CLI's stop path reads through the daemon's `Status`:
  it holds the `Progress` already, for `changes_committed`), beside the sinks with changes in flight. **No run ends `Ok` with
  a document waiting**: the dispatcher checks `Progress::waiting() == 0` after the builders return, else
  `Err`. The restart converges by replay.
- **What a stop builds**: at most `max_waiting` documents plus one run for the non-exempt indexes, plus
  every exempt backlog (at most `max_pending_documents`), under the deadline.
- **Failure**: a builder's error ends the run with it (the dispatcher reads its cell and returns it);
  the others are dropped in place; ids in flight are forgotten at the next start (R10); builds and key
  reads are aborted with their futures; a panic unwinds the task (6a). **Slice 8**: each builder keeps
  its slot in `together`, so a restart replaces one future after giving its builds back, and catches
  that future's panic around its polls.

#### 9. Metrics with N builders

- **Budget waits.** `builds::Waits` defined one live wait through the deferral, which goes. With N
  `Builds`, per-builder waits would count one hold several times. So the engine keeps one `Waits`
  shared by its builders: a live wait starts at the first refusal while no builder is held, and ends
  when no builder is held; overlapping holds of several builders are one wait, counted once
  (`flusso.memory.refused{live}`), as slice 4 defined (`overlapping_holds_of_two_builders_are_one_wait`).
  Backfill likewise.
- **Stages.** `Build`, `Publish`, `Page`, `Snapshot` stay per batch durations: they overlap across builds
  already on `main` with `builds > 1`, so their sums are work, not wall. `Idle` is the engine's wall
  time with no builder busy (nothing granted, building or publishing), measured by the dispatcher,
  so its sum keeps its meaning. Per-index stages are slice 7's.

#### 10. Memory per pending document

The review's arithmetic for a one-column integer key: the key behind its `Arc` about 144 B (the `Arc`,
the `Vec` of `(ColumnName, GenericValue)`, the column name's string), the `waiting` map about 120 B per
entry at a million (a 32-byte position, the counter, from 6b the read number, the table's slack), the
`order` map about 30 B: **about 300 B, and about 360 MB at a million while the map resizes** (both
tables held for a moment). Outside `max_built_mib`. 6b-2 measures it with a counting allocator (a test
that marks a million documents and asserts the bytes per document under a stated ceiling) and the
manual quotes the measured number with the slot-pinning note of §5.

#### 11. Slices (decided by the reviewer)

| # | What moves | Tests written first | Never worse than before it |
| --- | --- | --- | --- |
| 6b-1a | the frontier (I5), `routed` (D1), the per-lane high-water and the restart cap (D2) replace the global steps, with today's one builder. Visible: a change that builds nothing is confirmed and counted once read with everything before it landed; a quiet index answers through what was routed; a sink with no lane counts every read | the seeded model test; D1's and D2's tests (§3); `progress::tests::parked` per index; `a_partial_take_counts_committed_only_through_its_oldest_document_left` (a `progress` test until 6b-1c) | daemon `tests::committed`; `fence` and `wal_release` e2e |
| 6b-1b | the dispatcher (requests, confirmation, tick, coalescing on a deadline, the end signal) and `together` for N; still one builder | the N-future poller tests and loom model; `coalescing_for_one_index_delays_no_live_change_of_it`; `the_dispatcher_confirms_after_the_builder_that_registered` | one builder: 6a's path |
| 6b-1c | builders per index; persistent waiters and G7″ (F4, R2); the lead with folding (F1); the backlog (F2); the walk token; stdout and visibility wording | §12's skeleton for I2–I4, I6–I7, I9; `a_held_index_rewriting_few_documents_stops_the_reading_no_earlier_than_6a`; `a_stop_with_one_index_stalled_lands_the_others`; `a_backfill_of_one_index_delays_no_live_change_of_another`; `each_index_acknowledges_its_own_request_when_its_walk_completes`; `a_cut_request_mid_walk_gets_its_own_walk_after_it`; `a_request_queued_for_one_index_after_live_ends_is_served`; `one_index_starts_full_live_builds_up_to_builds` | the slice-4 scenario stops no earlier than second 61; one index is 6a's path |
| 6b-2 | the exemption and `max_pending_documents` (key, flag, env, refusal, `just schema-gen`); the stop and the end with an exempt backlog (F3) | the acceptance test turned around; `reading_stops_for_every_index_at_max_pending_documents`; `an_exemption_ends_only_when_nothing_from_before_it_waits`; `the_stream_end_builds_an_exempt_backlog`; `a_stop_builds_and_lands_an_exempt_backlog_under_the_deadline`; `no_run_ends_ok_with_documents_left_waiting`; the `shutdown` e2e for exit 1 naming the index; the memory test | without it, 6b-1c's lead stops the reader as 6a's does |
| 6b-3 | walks at once; key reads on their own task | `indexes_walk_at_once`; `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`; `two_backfilling_indexes_take_backfill_grants_in_turn` | the budget's backfill half and K1 bound it; measured |

**Order constraints checked.** 6b-1a needs only read numbers on marks, which the 6a reader has (each
change's place in its run); with one builder a deferred index gets no item, so its frontier stays, as
its steps do today. 6b-1b needs nothing of 6b-1c. 6b-1c's lead needs the fold (F1) but not the
exemption. 6b-2's stop rule (F3) needs only the backlog of 6b-1c. 6b-3's K1 task is needed only once
walks overlap. No order constraint makes the cut wrong.

#### 6b-1a as built

Built as §3 states, with today's one builder. Where the code sits:

- **Marks carry read numbers.** `Progress::mark_reads(id, position, read)`. `Pending.waiting` keeps
  `(position, counter, read)`, and a build keeps the least read it took (`Build.least_read`). The reader
  numbers each change `counted + its place in the run`, marks, counts (`read_changes`, release), then
  raises `routed` (`Progress::routed`, under the registry lock, then the listener).
  - `Progress::mark` (embedders, tests) marks as the next read.
  - The run's guard counts a run cut before its marks through `read_unmarked`, which records the least
    such read for the restart cap.
- **Frontiers.** A take records its index's frontier under the pending lock it already holds
  (`Taken::frontier`, the read count loaded before the lock). `start_live` adds `Progress::frontier`
  for every other index with lanes that isn't deferred, then `close_steps`: one step number, a
  `Step { number, read: frontier }` per index. `publish_live` registers each lane with its index's step.
  `close_step_through` is gone. `close_step` (every read, every index) stays for embedders and tests
  where nothing waits.
- **`committed`.** It loads the read count first, then takes the registry lock, reads the cut, and each
  lane's pending lock for `is_idle`. Per lane: the max of the landed step's frontier, the catch-up (the
  read count short of the cut, capped while the lane's `restart` is set), and the lane's high-water. A
  sink with no lane answers the read count.
- **The cut.** `read_unmarked` records its first read (`unmarked`, a `fetch_min`) before it raises the
  count. `readable()` (the count short of the cut) bounds every frontier (`take`, `frontier`,
  `close_step`) and `committed` bounds the catch-up with it.
- **`forget_all`.** Under the registry lock throughout: it takes the cut, forgets every pending set,
  unsets `routed`, and lowers every lane's `restart` to one below the least read among the waiting
  documents, the builds never released and the cut. A step registered on a lane clears that lane's.
- **Deferred indexes** (slice 4) get no frontier and no registration at a grant, as they got no step
  before: their lanes keep their last entry, and their waiting documents keep their backlog answer.

**Mutations** (`progress.rs`, `pending.rs`, `reader.rs`, `commit.rs`; every `flusso-engine` and
`flusso-daemon` unit test under a 5 s slow timeout, terminate after 2; no test hung):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| D1 | a lane with nothing outstanding answers through `newest` only | 6: `a_fence_on_a_quiet_index_passes_while_another_index_builds`, `a_quiet_index_answers_through_what_the_reader_routed`, `a_quiet_index_pins_no_confirmation_while_another_index_builds`, `changes_that_build_nothing_still_count_toward_the_lead`, `routed_moves_only_after_the_runs_marks`, `routed_never_moves_back` |
| D1 | `routed` raised before the run's marks and count | `routed_moves_only_after_the_runs_marks` |
| D1 | `routed` kept across `forget_all` | `a_forgotten_run_answers_through_nothing_it_routed` |
| D1 | `routed` may move back | `routed_never_moves_back` |
| I5 | a take's frontier at the read count with documents left | `a_partial_take_counts_committed_only_through_its_oldest_document_left`, `a_batch_ending_inside_a_run_closes_its_step_at_its_own_last_change`, the model test |
| I5 | an index not taken registered at the read count (`Progress::frontier`) | `a_skipped_index_counts_through_its_frontier_not_the_read_count`, `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` |
| I5 | the same, in `start_live` (`changes_read` instead of `frontier`) | `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` |
| I5 | the catch-up while a build is unreleased (`is_idle` ignores builds) | `a_build_taken_and_not_released_holds_its_lanes_count`, `a_change_read_ahead_is_not_counted_committed_before_its_build_lands`, `a_batch_ending_inside_a_run_closes_its_step_at_its_own_last_change` |
| D2a | no high-water | `a_lanes_count_never_goes_down_when_its_index_marks_again`, `compaction_changes_no_count`, the model test |
| D2 | no restart cap | 6: `an_ended_runs_reads_count_only_with_the_first_grant_after_the_restart`, `a_replay_from_before_the_committed_point_credits_nothing_before_the_first_grant`, `a_build_in_flight_at_the_restart_caps_the_count`, `reads_cut_before_their_marks_count_no_sooner_than_the_first_grant`, `a_run_cut_before_its_marks_counts_no_sooner_than_the_first_grant`, the model test |
| D2 | the cap never lifted | `after_the_first_grant_a_change_that_builds_nothing_counts_once_read` (written after the first run: nothing failed, because with one builder every grant registers every lane, so only a catch-up between grants shows it) |
| D2 | the cap over the whole ended run | `reads_that_built_nothing_count_across_a_restart_with_no_replay`, `compaction_changes_no_count` |
| D2 | builds in flight left out of the cap | `a_build_in_flight_at_the_restart_caps_the_count`, `forgetting_leaves_nothing_waiting_or_building` |
| D2 | reads counted without marks left out of the cap | `reads_cut_before_their_marks_count_no_sooner_than_the_first_grant`, `a_run_cut_before_its_marks_counts_no_sooner_than_the_first_grant`, the model test |
| D2 | the reader's guard counts its cut run as marked | `a_run_cut_before_its_marks_counts_no_sooner_than_the_first_grant` |
| I5 | a sink with no lane answers 0 | `a_sink_with_no_lane_has_every_read` |
| I5 | a skip appended instead of replacing the newest entry | 7: `progress::tests::parked` (3), the daemon's three `*_holds_its_sinks_count_and_a_sibling_counts_on`, `compaction_changes_no_count` |
| I5 | the read count loaded after the pending state | not testable single-threaded; argued in §3 (release after the marks, acquire first) |

The `a_sinks_count_never_goes_down_across_a_restart` test passes under "no high-water": its two sinks
never see an older entry answer again. The high-water is pinned by the three tests in its row.

**Hot-path costs against `main`** (measured nothing):

- **Per change:** the read number (one add), and 8 more bytes per waiting document (`Pending.waiting`'s
  value).
- **Per run:**
  - `routed`: one registry lock and one listener call (the fence's wake check);
  - the read count's `fetch_add` is release (it was relaxed; no instruction changes on x86-64).
- **Per take:** the least read over the ids taken (they are walked already), and the frontier: one
  `BTreeMap::first_key_value` and one `HashMap` lookup under the lock held.
- **Per grant:**
  - one pending lock and one atomic load per index the grant didn't take (`Progress::frontier`);
  - a `Vec` and a `BTreeMap<IndexName, Step>` with one `IndexName` clone per index;
  - one map lookup per lane at publish.
- **Per `/status` or `/metrics` read of `changes_committed`:** one pending lock per lane of the sink
  (`is_idle`), under the registry lock. Confirmation and fences (`least_of`) add one `Option<Position>`
  read.
- **Per engine restart:** a walk of every waiting document and build for the least read.

#### 6b-1a review round 1: what changed

Each point was reproduced before it was fixed; the tests were written first and failed on the code as
pushed (`8646caef`).

| Point | Change | Tests |
| --- | --- | --- |
| 1. `forget_all` cleared the pending sets before it unset `routed` and set the cap | holds the registry lock throughout (registry before pending, the documented order; no writer takes the registry lock holding a pending lock) | `forgetting::a_reader_started_mid_forget_sees_none_of_it_half_done` (a test hook starts a reader inside `forget_all` after the pending sets are forgotten; it must not get in before the end); `forgetting::the_count_read_during_forget_all_never_counts_the_forgotten_run`, `forgetting::a_lane_read_during_forget_all_never_answers_through_the_forgotten_run` (stress, ≤ 2,000 rounds or 2 s; both failed within 0.13 s on `8646caef`) |
| 2. Reads cut before their marks counted until the next `forget_all` | the cut is recorded before the count and bounds every frontier and catch-up until `forget_all` turns it into the caps | `cut::reads_cut_before_their_marks_count_no_sooner_than_their_replay`, `cut::a_step_closed_after_a_cut_in_the_same_run_counts_no_cut_read`, the reader's `a_run_cut_before_its_marks_counts_no_sooner_than_the_first_grant` (asserts before `forget_all` too) |
| 3. The model's blind spots | the model gains deferral, a held document, empty builds under a token (per lane), redelivered landings, a cut checked until the run ends, a build in flight across `forget_all`; it found a third defect: the restart cap was lifted for every lane by the first grant, so a lane whose index that grant deferred lost its cap, and a second restart let it catch up through the first ended run's unbuilt reads. The cap is now per lane, lifted by a step registered on that lane | the model (200 seeds; 2,000 once by hand), `restart::a_deferred_index_keeps_its_cap_until_its_own_lanes_get_a_step` |
| 4. Minor | the module doc's "Why no answer claims too much" rewritten for `Through(max(newest, routed))` and the backlog; the lock argument above extended to `forget_all` and `read_unmarked`; `a_frontier_never_passes_the_read_count`; the contributor page's "released"; the moment of a lane that never had an item (§3.6) | |

**What the model still does not cover**, and what does: threads (the `forgetting` tests; the reader's
own order, `reader::tests`' `routed_moves_only_after_the_runs_marks`); publish order against grant order
(the model grants synchronously; one builder publishes in grant order, `ingest::tests`); the searchable
level (fence tests); the stop and its drain (`tests::stop`).

**Writers checked** (point 1): every multi-step update a reader holding the registry lock may combine is
atomic for it, or each intermediate state answers no further and counts no more.

- `mark_reads`, then `read_changes`, then `routed`: marks without the count only add backlog; the count
  (release) follows the marks, and `routed` (registry lock) follows both.
- `read_unmarked`: the cut before the count; a concurrent count can only lower the recorded cut.
- `take`: one pending lock moves documents from waiting to a build, both backlog; the frontier uses the
  readable count loaded before the lock (older is lower).
- `close_steps`: numbers only, under the registry lock.
- `published_build`/`skipped_build`: the lane's entry (registry lock; the lane's cap cleared) before the
  build is settled for the sink (pending lock). Between, both the item and the build hold the lane back;
  a skip's entry counts its frontier, which the build needed nothing of on that lane.
- `build_sending`, `build_landed`, `release`: one pending lock each; `release` after every lane's
  registration.
- `applied`/`searchable`: one atomic per lane, its sink engine the only writer; read early, a lane only
  answers older.
- The high-water: written by `committed` under the registry lock.
- `forget_all`: one critical section, as above.
- Not combined with answers: the item key counters, the waiting counters (the lead).

No `loom` model: `Progress` and `Pending` use `std` locks and atomics throughout, so a model would mean
loom types through both; the hook test pins the gap deterministically and the stress loops cover the
interleavings around it.

**Mutations of this round** (same harness; no test hung):

| Mutation | Tests that fail |
| --- | --- |
| `forget_all` takes the registry lock after forgetting the pending sets | the three `forgetting` tests above |
| the catch-up ignores the cut | 4: both `cut` tests, the reader's cut test, the model |
| `read_unmarked` records no cut | 5: both `cut` tests, `reads_cut_before_their_marks_count_no_sooner_than_the_first_grant`, the reader's cut test, the model |
| a take ignores the cut | `cut::a_step_closed_after_a_cut_in_the_same_run_counts_no_cut_read`, the model |
| `frontier()` ignores the cut | the same two |
| a frontier past the read count (no `.min(read)`) | `a_frontier_never_passes_the_read_count` |
| the cap lifted for every lane at the next step (as pushed) | `a_deferred_index_keeps_its_cap_until_its_own_lanes_get_a_step`, the model |
| the cap never lifted by a registration | `after_the_first_grant_a_change_that_builds_nothing_counts_once_read` |
| `forget_all` replaces a lane's lower cap instead of keeping the least | `a_deferred_index_keeps_its_cap_until_its_own_lanes_get_a_step`, the model |
| the cut recorded after the count moves | not testable single-threaded (one writer); argued above |

**Costs this round adds** (measured nothing): one more atomic load per take, per `frontier`, per
`committed` read and per `read_unmarked`; `forget_all` (once per engine restart) holds the registry lock
while it forgets every pending set and writes one cap per lane, so a `/status`, `/metrics`, `/wait` or
confirmation read waits that long once per restart.

#### 6b-1b against the code (checked before building)

What 6b-1b builds, against the code after 6b-1a (`dafa4990`), with one builder over every index:

- **Nothing depends on O1–O3** (decided since, as recommended). No ADR wording is used (O1); no take
  is capped per index, the one builder takes as 6a (O2, which 6b-1c builds); there is no exemption, so
  no exempt backlog at a stop (O3, 6b-2's).
- **The futures**: the reader (6a's), the dispatcher, one builder; the stop's watch beside them.
- **The builder** is 6a's builder loop without the request lane, confirmation and the wind-down's
  `drain_lanes`/`close_live`: it owns `Runs`, `Builds`, the snapshot, the lanes' producers, its request
  inbox and coalescing deadline, and 6a's idle and page timing (`waited`; §9's "measured by the
  dispatcher" is 6b-1c's: with one builder "no builder busy" is this builder's own measure). It sends its
  result (the reader's end with the live stream, or its error) over a one-shot and returns.
- **Coalescing on a deadline, the one builder's rule**: `main`'s over every sink with a lane (6b-1c makes
  it per index). The first request opens a coalescing set with a deadline at `max_delay`; the set starts
  its snapshot once every sink asked and the inbox is empty, else at the deadline. `main`'s "what is
  already queued still joins" is kept by the dispatcher forwarding every request ready on each poll: a
  builder that finds its inbox empty has every request the dispatcher had. Live batches are granted
  meanwhile; on 6a and `main` they waited out the coalescing (up to `max_delay`): the one moment that
  moves, earlier.
- **The tick**: the dispatcher's, confirming always, and raising the builder's tick flag; the builder
  grants on it only while the budget refused it (6a's backstop).
- **The end**: the builder reports idle (the reader ended with its stream, no snapshot, no coalescing
  set, inbox empty) and waits; the dispatcher forwards what is ready and signals the end once its request
  lane is empty, then forwards nothing more; the builder settles only while idle itself, so a request
  forwarded after it said idle is served first; then it returns, and the dispatcher confirms and returns
  `LiveEnded` (6a's order). (As built: the first version also checked, on the dispatcher's side, that the
  builder was still idle and had received everything forwarded; mutations showed both redundant with the
  builder's own check, so they went.)
- **The stop**: the watch wakes the reader and the dispatcher; the dispatcher raises the builder, which
  drops its snapshot and coalescing set (their deliveries unacknowledged, redelivered next start, as 6a
  dropped the snapshot), waits for the reader's end (R7), settles and returns; the dispatcher runs
  `drain_lanes` and `close_live` (6a's `wind_down`) and returns `Stopped`.
- **Errors and panics**: the builder's error reaches the dispatcher through its one-shot, a reader's
  through the builder (it joins the reader, R7), and the dispatcher's own (the request lane, `drain_lanes`,
  `close_live`) is the run's; the run's builds are abandoned after `together` returns, as 6a's. Every
  future is polled inline, so a panic in any of them unwinds the engine run's task, as 6a's.
- **Writers of shared state** (the list of 6b-1a's round 1): none added; the registration signal is
  written beside the listener and read only by the dispatcher, in the task.

#### 6b-1b as built

Built as "6b-1b against the code" states; where the code sits:

- **`together` for N** (`ingest/together.rs`): `together(reader, dispatcher, builders, watch)`, the
  dispatcher's output the run's. The flags are an `AtomicU64` (`Flags<W: Word>`, `Word` now over `u64`
  for `std`'s and loom's atomics); one waker per group; `running` and `live` per future and per group,
  allocated once per engine run with the boxed builders. A pass is one `take` of every raised running
  bit (and the watch's), the watch first, then the futures from `start` (advanced every poll);
  `MAX_POLLS` (64) counts every poll, the watch's included; a poll cut mid-pass `raise`s the groups of the
  futures it took and didn't reach, then ends with the task woken.
- **The cap is checked twice**: before every pass and before every future. The first matters only for a
  pass that polls nothing but the watch (a watch that kept waking itself would loop without it,
  `a_watch_that_keeps_waking_itself_still_yields`); both stay.
- **The dispatcher** (`ingest/dispatcher.rs`, `IngestEngine::dispatch`) and **`Signals`** (in-task
  flags and two `AtomicWaker`s: the builder's look, tick and end; its idle report). `Look` is raised once per stop or end, so a builder that can't act on it yet doesn't
  spin (the level-triggered first try spun on the random schedules, which caught it as a hang).
  `forward` stops at a send the builder's dropped inbox refuses: the delivery is released and would
  be received again at once, forever (a random schedule caught that too, seed 157:
  `a_request_for_a_builder_gone_goes_back_to_its_lane_once`).
- **The builder** (`ingest/builder.rs`, `IngestEngine::build`): 6a's loop with the inbox, the
  coalescing set (`Coalescing`: the deliveries, the deadline at `max_delay` from the first, and the
  first one's arrival, which `Stage::Snapshot` is timed from as `main` timed its gathering; while it
  gathers, the loop's waits are the snapshot's, not `Idle`, as on `main`), the signals arm, and its end
  over a one-shot. `collect_requests` is gone; `wind_down` became the builder's settle and the
  dispatcher's `finish` + `close`.
- **`Progress::poll_registered`**: `registered()` raises a flag (a `swap`) and wakes its waiter only on
  the first raise since the waiter looked.

**Writers checked** (6b-1a round 1's list): the registration flag and `Signals` are written and read
inside the engine run's task only; nothing read from another thread (fences, `/status`, the sink
engines) changed.

**Hot-path costs against `main` (`dafa4990`)**, measured nothing:

| Where | `main` (6a + 6b-1a) | 6b-1b |
| --- | --- | --- |
| per change | — | — |
| per run | the builder confirmed after the run's grant | the run's `routed` raises the registration flag: a `swap`, an `AtomicWaker` wake (a waker clone when the dispatcher registers again) and a `fetch_or` on the flags word; the dispatcher is polled once or twice more in the same task poll, each one pass through its `select!` with no allocation and no lock (review round 1, below), and confirms once per poll |
| per grant | a confirmation after it | a `swap` per lane registration and release (the flag is already up: no wake); the confirmation moves to the dispatcher, once per poll however many registrations |
| per builder poll | the request lane's `recv` (a `tokio::sync::Mutex` try and the channel) in the builder's `select!` | the inbox's `recv` (an unbounded channel) and the signals (an `AtomicWaker` register and two `swap`s); the request lane's `recv` moves to the dispatcher's polls |
| per task poll | a `fetch_and` per running future per pass, a load and a `compare_exchange` | one `fetch_and` per pass for every future, the load and `compare_exchange`, a rotation counter, bounds-checked lookups in two small `Vec`s per polled future |
| per second | the builder's tick | the dispatcher's tick raises the builder's flag: the builder is polled once more a second |
| per engine run | — | four small `Vec`s, one box per builder, a one-shot, an unbounded channel |

**Mutations** (every `flusso-engine` and `flusso-daemon` unit test under a 5 s slow timeout, terminate
after 2; "hang" marks a test the nextest limit ended, every one of them a paused-time engine test whose
run never ends without the mutated rule):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I1 | a raised group polls only its first future | `a_shared_group_polls_every_future_in_it`, `an_ended_future_in_a_shared_group_is_never_polled_again`, `a_pass_cut_by_the_cap_raises_its_unpolled_groups_again` |
| I1 | the start never rotates | `a_spender_as_the_reader_beside_a_builder_starves_nothing`, `a_spender_polled_first_starves_nothing`, `a_poll_yields_to_the_worker_after_its_passes` |
| I1 | no cap before a pass | `a_watch_that_keeps_waking_itself_still_yields` (written after the first run: nothing failed, the cap before a future catching every pass but one polling only the watch) |
| I1 | no cap before a future | `a_poll_of_n_futures_yields_after_64_polls`, `a_pass_cut_by_the_cap_raises_its_unpolled_groups_again` |
| I1 | a cut pass raises nothing again | `a_pass_cut_by_the_cap_raises_its_unpolled_groups_again`, `a_shared_group_polls_every_future_in_it`, `an_ended_future_in_a_shared_group_is_never_polled_again` |
| I1 | the mark dropped by a `fetch_and` after the flags are read | both loom models, `a_wake_at_the_end_of_a_pass_is_not_lost` |
| I1 | an ended future masked with its group | `an_ended_future_in_a_shared_group_is_never_polled_again` |
| I1 | an ended future not masked (its group's count still drops) | `an_ended_future_in_a_shared_group_is_never_polled_again` |
| I1 | the watch's end wakes only the reader | `the_watch_wakes_the_reader_and_the_dispatcher_not_the_builders`, `an_ended_first_keeps_no_poll_going`, `a_stop_reaches_a_builder_whose_stream_ended_while_its_snapshot_waits_on_a_page` |
| I1 | builders polled unwoken | `an_idle_builder_is_polled_only_when_woken`, `the_watch_wakes_the_reader_and_the_dispatcher_not_the_builders`, two more |
| I10 | a registration confirms nothing (the tick only) | `the_dispatcher_confirms_after_the_builder_that_registered` |
| I10 | a registration raises the flag without waking | `the_dispatcher_confirms_after_the_builder_that_registered` |
| I10 | no stop fan-out | `a_stop_reaches_a_builder_whose_stream_ended_while_its_snapshot_waits_on_a_page` |
| I10 | a builder gone is forwarded to again | `a_request_for_a_builder_gone_goes_back_to_its_lane_once` |
| I10 | the end with the request lane not empty | none: `forward_ready` drains the lane just before; the check holds the end only while a sink's request is half published (the lane says non-empty, `recv` can't take it yet), which no single-threaded test reaches. Kept |
| I10/I12 | the builder settles at the end while busy | 15, among them `snapshots_go_only_to_the_requesting_lane`, `a_straggling_reindex_request_within_max_delay_joins_the_snapshot`, `random_schedules_keep_every_document_in_order` |
| I12 | the builder never reports idle | 74 (hang): every run that ends with its stream |
| I11 | coalescing inline | `coalescing_for_one_index_delays_no_live_change_of_it`, `a_straggling_reindex_request_within_max_delay_joins_the_snapshot` |
| I11 | every lane asking still waits the deadline | `a_backfill_with_every_lane_asking_starts_without_waiting`, three more |
| I11 | the snapshot starts at the first request | `a_straggling_reindex_request_within_max_delay_joins_the_snapshot`, `coalescing_for_one_index_delays_no_live_change_of_it` |
| tick | the builder's tick grants nothing | none: the backstop for a budget release that goes unheard, and every release is heard (as in 6a) |

The first version also had two defects the random schedules found as hangs before any mutation: a
level-triggered `Look` (fixed: raised once per stop or end) and the forwarding loop to a builder gone
(fixed, `a_request_for_a_builder_gone_goes_back_to_its_lane_once`).

**Review round 1 of #450** (no blocker; each point taken):

| Point | Change | Test, and the mutation it fails under |
| --- | --- | --- |
| 1. The builder's "inbox empty" idle check had no test | none in the code | `a_request_forwarded_after_the_builder_went_idle_is_served`: a request lane that publishes a request the first time it is asked whether it is empty, so the request reaches the lane as the dispatcher looks, after the builder reported idle; 40 runs, since which of the builder's arms wins is tokio's pick. Fails with the check removed; also with the dispatcher's own lane check removed, which last round's table called untestable: this lane is exactly the half-published case |
| 2. `inbox.recv(), if snapshot.is_none()` had no test | none | `a_request_arriving_mid_snapshot_waits_for_it` (`HangingKeys`; a second request mid-walk; 5 s of paused time; the walk is not asked for again) |
| 3. The watch's bit at the end of a pass had only an incidental test | none | `a_watch_woken_at_the_end_of_a_pass_is_not_lost` (a wake of the watch after it was polled, before the mark drops) |
| 4. The loom model's doc said "every interleaving" | the doc says what it is (bounded at three preemptions, four threads), what it drives and what the unit tests cover instead; the two tests renamed `…_within_three_preemptions` | — |
| 5. The cost table undercounted a lone change | the dispatcher keeps the request lane's `recv` in one long-lived future (`feed`) across its polls, dropped only when it yields or the builder reports idle; a registration confirms inside its arm's poll, which stays pending, so a wake for a registration is one pass through the `select!`; the tick stays as it was (`Interval::tick` allocates nothing) | the request rows below, re-run on the new code |
| 6. The note said the cap before a pass was gone | corrected above: it stays, and is required | — |

Mutations re-run on the code after the round: a registration confirming nothing and a registration
raising no wake fail `the_dispatcher_confirms_after_the_builder_that_registered`; no stop fan-out fails
`a_stop_reaches_a_builder_whose_stream_ended_while_its_snapshot_waits_on_a_page`; forwarding to a builder
gone fails `a_request_for_a_builder_gone_goes_back_to_its_lane_once`; the end with the request lane not
empty and the builder settling with a request in its inbox fail
`a_request_forwarded_after_the_builder_went_idle_is_served`; a request taken while a snapshot runs fails
`a_request_arriving_mid_snapshot_waits_for_it`; the watch dropped from the end of a pass fails
`a_watch_woken_at_the_end_of_a_pass_is_not_lost` and `a_watch_that_keeps_waking_itself_still_yields`.

**A lone change's cost, corrected** (the reviewer's count: one task poll; the reader and the builder
polled once each, the dispatcher once or twice). Before the round, each dispatcher wake entered its
`select!` at least twice (the ready arm, then to park), each entry creating the `async_trait` box of
the request lane's `recv`, taking and releasing its `tokio::sync::Mutex`, and polling the tick: about
2–4 boxes and mutex round trips per lone change beyond `main`. After it, a wake for a registration is
one entry: the result one-shot, the stop's flag, the registration flag (an `AtomicWaker` register, a
`swap`, the confirmation), `feed` (a poll of the `recv` it holds: the channel's atomics, no box, no lock;
the idle report's `AtomicWaker` register and `swap`) and the tick's timer. The `recv` box and the lock
are paid once per request received or idle report, not per wake.

**User-visible**: live batches are built and published while backfill requests are gathered (up to
`max_delay`), where `main` and 6a waited the gathering out. Nothing else.

#### 6b-1c against the code (checked before building)

**The measurement first** (the throwaway of §6; in-process, `taskset -c 0-3,12-15`, nothing else
running; builders as futures of one task waiting in `Gate::acquire`, the persistent waiter with slice
5's wake-all; an empty build per grant, so the figures are the gate's and the scheduling's alone):

| `builds` | indexes | ns per grant (median of 5) |
| --- | --- | --- |
| 1 | 1 | 668 |
| 1 | 24 healthy | 3,099 |
| 1 | 23 healthy + 1 held at its share | 6,390 |
| 4 | 1 | 675 |
| 4 | 24 healthy | 1,183 |
| 4 | 23 healthy + 1 held at its share | 7,043 |

Wake-all costs up to about 6 µs per grant with 24 indexes and one parked: the held builder's poll, a
budget decision, a refusal and its re-registration after every grant elsewhere. Against a build's
database round trip that is a few percent of a lone change, and nothing of a burst's (per batch, not
per change); one index is unchanged. **G7″ is not built**: G9′ with wake-all stays, and G7″ is the
follow-up if the several-index scenarios show this cost.

**Corrections and the rules made concrete** (where §3–§8 are not implementable as written, or leave a
choice open):

- **G9′ only with two builders or more (changes an approved gate point, G9′).** With one builder nobody
  else can stand in a line, so 6b-1b's grant pass (G9: join, ask once, leave, decided in one hold of
  the lock while the lines are empty) gives exactly what a persistent waiter would, without a join, a
  budget watch and a wake per wait. One index therefore stays on 6b-1b's grant pass and gate path; its
  batches differ from 6b-1b's only where the document cap splits a change (pinned by
  `one_index_never_waits_in_a_gate_line`). With two or more, a builder that a pass didn't grant
  keeps a persistent waiter per kind it has ready.
- **"Left before any await outside the loop"** is implemented as: an await that can pend (a publish's
  lane sends, `walk_marks`, a build's first poll) is polled once; if it pends, the builder leaves its
  lines before awaiting it. Equivalent to §6's rule for every await that pends, and free for the ones
  that don't.
- **`Idle`** is the engine's wall time with no builder busy, kept by a clock the builders share (each
  marks itself busy when it wakes to work and idle when it goes back to waiting; the span is reported
  when the first builder wakes). §9's "measured by the dispatcher" can't be: the dispatcher isn't woken
  by a builder's transitions.
- **The reader's end goes to the dispatcher** (it closes the live stream); each builder's barrier
  (R7) is its slice channel closing, which happens when the reader ends, after its last hand-over.
- **A change touching no index** goes to no builder: it counts toward no changes bound (6a counted it
  until its empty batch was granted) and its `Resolve` is reported by the reader; a run touching
  indexes reports its `Resolve` once, with its first slice.
- **Slices carry each change's read number**, for the changes bound (the oldest read number queued on a
  builder that doesn't fold; each builder keeps it in an atomic the reader reads at its lead check).
- **An item's position** is its batch's newest change: the live candidate's last change, or, for the
  fold's backlog take, the newest position waiting on the index when it was taken
  (`Progress::newest_waiting`).
- **The walk token** is a FIFO semaphore of one permit; a builder keeps its acquire across its polls
  (dropping it would lose its place) and coalesces meanwhile. The permit is held until its walk is
  done.
- **The quiet index's cap after a restart** (the rule in `forget_all`'s doc): every step closed after
  the restart lifts the cap of each lane whose index has nothing waiting or building; a lane whose
  index waits keeps it until its own step (`after_a_restart_a_quiet_indexs_lanes_catch_up_with_no_step_of_their_own`).
- **Slice 4's deferral goes**: a build takes one index, and an index the budget refuses holds only its
  own builder.

#### 6b-1c as built

Built as "6b-1c against the code" states, with these additions found while building:

- **The changes bound counts from each queue's oldest queued change** (`Lead`'s `Queue`: the oldest read
  number queued and not granted, set by the reader when it routes a slice and by the builder after each
  grant). The first version counted from each queue's last grant, so a queue whose first change was read
  after another index's counted that other index's changes too, and the reader stopped a change early
  (`a_change_read_ahead_is_not_counted_committed_before_its_build_lands` caught it).
- **A lane's catch-up is every read below its index's oldest unbuilt one** (`Pending::least_unbuilt`),
  not only while the index is idle. With a lane registered only with its own index's steps, 6b-1a's
  idle-only catch-up held `users`' lane at its last step while `orders` landed a read `users` had
  nothing for, a moment `main` had (the same test). By the frontier's own argument every read below the
  oldest unbuilt one is landed on the lane or had nothing for it.
- **A settling builder waits its turn** when it has a batch and no permit (another builder holds them):
  it is in a line, so a freed permit wakes it (`Gate::changed_since`).
- **Budget waits keep the held builders** (`Waits.held`): a wait starts at the first refusal while none is
  held and ends when none is; per-builder waits counted one hold six times.
- **The reader records whether it ended waiting** (`Lead::ended_waiting`): a builder's span up to the
  reader's end is idle only then, as 6a's join told it.
- **The random schedules' positions are strictly increasing within an engine run**, not across a
  restart: the replay starts at the least confirmed position over the lanes, so an index's lane that was
  ahead of another's is sent some of its positions again (as `main`'s lanes of two sinks, one lagging,
  always were). Recorded with a marker at each restart.
- **Tests that assert one batch across indexes now assert per-index batches**
  (`a_batch_is_split_by_index_and_a_quiet_index_pins_nothing`: `users` 1 and 3 one batch;
  `a_change_that_builds_nothing_settles_after_the_batch_before_it`: a change touching no index with lanes
  goes to no builder).

**Mutations** (every `flusso-engine` and `flusso-daemon` unit test under a 5 s slow timeout, terminate
after 2; "hang" marks a paused-time engine test the nextest limit ended):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I9 (G9′) | several builders ask with G9's pass, keeping no waiter | `a_freed_permit_wakes_a_waiting_builder`, `a_change_read_ahead_is_not_counted_committed_before_its_build_lands`, the three multi-index random schedules (hang) |
| I9 | one builder joins lines too | `one_index_never_waits_in_a_gate_line` |
| I9 | a waiter's grown batch drops its refusal / a shrunk one keeps it | `a_waiter_that_grows_keeps_its_refusal_and_one_that_shrinks_drops_it` (written against `set_parts`; both halves) |
| I7 | the fold never entered | `a_held_index_rewriting_one_document_holds_no_bound_and_no_memory_per_change`, `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap`, `a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes` |
| I7 | entering the fold keeps the queued slices | none: the slices queued before the hold resurface after it as candidates whose documents the backlog took, an empty build at most. Kept, for the memory a held index's queue would keep |
| I6 | a folding queue counted toward the changes bound | none, and the check is gone: entering the fold clears the queue's record and the reader routes it nothing |
| I4 (O2) | no document cap | `a_build_never_takes_more_than_max_changes_documents`, `a_change_split_by_the_document_cap_counts_only_once_its_last_part_lands` |
| I4 (F2) | a capped take keeps no candidate | the same two |
| I5 | the catch-up only while the index is idle | `a_change_read_ahead_is_not_counted_committed_before_its_build_lands` |
| I5 | no lift of a quiet index's cap at a step | `after_a_restart_a_quiet_indexs_lanes_catch_up_with_no_step_of_their_own` |
| §9 | one budget wait per builder, not per hold | `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it`, `a_hold_across_an_engine_restart_is_one_wait`, `a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment` |
| I11 | no walk token | `indexes_walk_one_at_a_time` (turned around in 6b-3 as `indexes_walk_at_once`) |
| I12 | a settling builder returns with a batch waiting for a permit | `random_schedules_keep_every_document_in_order`, `random_schedules_over_three_indexes_with_one_bursting_stay_within_their_shares` |
| I10/I12 | the end with one builder idle | `a_request_queued_for_one_index_after_live_ends_is_served` (written after the first run: nothing failed) |

Not mutated before fix round 1: the routing of a change to the builders of the indexes it touched (I2;
now `an_untouched_index_is_handed_nothing`, below); the idle clock (no test runs several builders and
checks `Idle`'s sum; one builder reports as 6b-1b did). Still not mutated: the idle clock.

**Hot-path costs against `main`** (6b-1b; measured nothing but §6's throwaway):

| Where | One index | N indexes |
| --- | --- | --- |
| per change | the read number kept with it in the run (8 B) | the same, plus one push per touched index into its slice |
| per run | the lead's routed record (an atomic `fetch_min` and `fetch_max`) | per touched index: a `Vec`, a send, a flag, the routed record; a lead check reads one atomic per builder |
| per grant | the take capped at `max_changes` (same lock); the lead's queue store; `close_steps` walks every lane's cap (a load each, a pending lock only while a cap is set after a restart) | the same per builder, plus G9′: a waiter joins (gate lock, budget watch), asks (gate lock, budget lock), and is asked again on every gate change (wake-all: about 6 µs per grant with 24 indexes and one held, §6) |
| per builder poll | as 6b-1b (no turn future, no clock, no fold check) | while in a line: an `Arc` clone and a `Notify` registration; the idle clock's lock; the fold check (one atomic load) |
| per `committed` read | the least unbuilt read (the pending order's first entry) instead of an idle check | the same per lane |
| per walk | the walk token's `try_acquire` (an atomic) | a builder waiting keeps its acquire across polls |
| per engine run | one gate, one semaphore, one lead queue | one `Builds`, `Runs`, channel pair, signals and one-shot per index |

**User-visible** (for the documentation agent, with the 0.19 upgrade page):

- **Batches are per index.** Each index cuts its own batches from the changes that touched it; a burst on
  one index no longer shares a batch, a deadline or a build with another's changes.
- **One index's build takes at most `[batch] max_changes` documents** (O2). A change touching more
  documents is built in several builds; it counts as committed, is confirmed and passes a fence once its
  last part landed. The setting's description: "a batch groups at most this many changes, and one
  index's build takes at most this many documents".
- **A change can be searchable in one index before another.** A per-index fence passes per index; a
  per-sink `/wait` (no index) still waits for both.
- **The stdout sink's `seq`**: non-decreasing per index, may repeat, no order across indexes (decision
  (b)2). `docs/src/reference/sink-stdout.md` says "across indexes the lines keep position order" and
  "Monotonic within a run": a consumer resumes per index from the last `seq` inclusive.
- **After an ingest engine restart**, an index whose lane was ahead of another's may receive some of its
  changes again (at-least-once, as with two sinks one of which lags).
- **An index held by the budget** stops the reading for every index once it has `max_waiting_per_index`
  documents waiting, as before (6b-2 lifts it); an index held while rewriting few documents no longer
  counts its changes toward the read-ahead bound.
- **Backfills of several indexes walk one at a time**, as `main`'s one snapshot walked them; a request
  for an index arriving during another's walk is gathered meanwhile.

#### 6b-1c, fix round 1 (the review of `897bb09e`)

Two blockers and five should-fix findings, each pinned by a test written first (failing, never
hanging, before its fix), and three more faults the new tests found.

**The invariant (blocker 1). A lane's `newest` names only positions whose every document of its index
has been taken.** A take cut by the document cap registered its item at the change's position while
part of the change waited; so did the fold's backlog take, at the newest position waiting while it took
the oldest `max_changes`. Inside a run the backlog held the lane at `Before`, but `forget_all` forgot
the waiting documents and kept `newest`, so the next run's first tick confirmed the change, a fence
passed it at both levels, and a second failure before the replay lost the documents. Now
`Progress::register` raises `newest` only while the index has nothing marked at or before the item's
position waiting or in another build not yet released (`Pending::all_taken_through`, read under the
registry lock, pending lock inside it, the existing order). Every writer checked against it:

| Writer | How it keeps the invariant |
| --- | --- |
| `published_build`, `skipped_build` (the engine's) | the check above, the item's own build excluded |
| `published`, `published_step`, `skipped`, `skipped_step` (tests, embedders) | the same check, no build excluded |
| `routed` | raised by the reader after a run's marks: every routed change's documents are pending or on a lane, and `forget_all` unsets it |
| `forget_all` | unsets `routed`, keeps `newest`: sound now that `newest` names only fully taken positions |

It rests on two facts of the engine, now written down: takes are oldest first, and no take passes a
document a running build holds (`waiting_of` answers `None` then, so nothing is granted). Then an item's
oldest change is no later than anything its take left, and its `Before` answer covers them. The seeded
model found that `Progress` alone, fed takes around a held document, answers past what they left, both
across a restart and within a run (a later item's oldest before the front item's); the engine never
does that, so the model now mirrors it rather than `Progress` growing a guard for it.

| Finding | Fix | Test (fails before the fix) |
| --- | --- | --- |
| 1: restart confirms a capped change's unbuilt part | the invariant above | `a_capped_take_lets_no_restart_answer_through_what_it_left_waiting`, `a_backlog_take_lets_no_restart_answer_through_what_it_left_waiting` (`progress`), `after_a_failed_part_no_run_confirms_the_change_before_its_replay` (two engine runs on one progress); the seeded model (below); the random schedules (below) |
| 2: a stop hangs behind a backfill waiter | a builder's builds leave every line when it ends, on any path (`LeavesLines`, a drop guard); the stop leaves them as it drops the snapshot and the requests; `settle`'s wait for its turn has the tick as a backstop | `a_stop_beside_a_backfill_waiting_for_a_permit_ends` (forty stop times, `builds = 1`, each bounded) |
| 3: a publish blocked on a full lane stops other indexes' builds | a build drops its permit as its task ends; each publish (and `start_backfill`'s `walk_marks`) is polled once and, pending, the builder leaves its lines and, with several builders, folds (held, §5) | `a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line`, `an_index_parked_at_its_lane_backstop_folds` |
| 4: a backlog take wakes no reader | the take wakes the lead (`Lead::wake`) | `a_backlog_take_wakes_the_reader` (failed: the reader waited at the cap until the backlog drained) |
| 5: a capped take's leftover never starts beside a running build | it is a full candidate while `max_changes` or more of its documents wait (`full_leftover`) | `a_capped_takes_leftovers_build_at_once_up_to_builds` |
| found: a grant reserved for every document waiting, not the `max_changes` it takes | the estimate counts at most `max_changes` per index | `random_schedules_under_a_tiny_budget_stay_within_it_and_finish` with fanned changes |
| found: G6 did not hold across builders | beside other builders the gate decides (G5): a refused batch keeps its waiter, and a waiter still refused at the budget's generation arms the release wait | `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused`, now across two builders |
| reviewer's cost note | the reader looks a change's builder up once per index, not per document | (no behaviour change) |

**Item 3 is built as "polled once, then awaited in place"**, not as an arm of the builder's loop (§6's
words). While it waits the builder holds no permit (its builds dropped them) and no place in a line, and
with several builders it folds, so its index stops no other's reading below its waiting cap; what it
doesn't do meanwhile is land finished builds' bookkeeping, take requests or start a coalesced
snapshot, none of which another index waits on. `builds.start` never pends (one inline poll), so it
needs no rule.

**Guards widened.** The seeded model (`progress::tests::model`) registers one index's lanes per grant (a
builder of one index, no deferral), models the quiet-index lift, replays each position's changes after
a restart from what the lanes landed, and checks at every step that `landed()` covers no position a
lane hasn't landed, across every `forget_all`. The random schedules' source resumes after what it was
confirmed, as a slot does, and a third of their seeds fan each change out to two to four documents, so
the cap splits changes and a failure can land between the parts (`newest` raised regardless fails
`random_schedules_under_a_tiny_budget_stay_within_it_and_finish`, seed 102); within a run the parts of
a split change share a position.

**Mutations** (each fails the test named; the rest of the table above is unchanged):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I5 (`newest`) | registration raises `newest` whatever waits | the three blocker-1 tests, the seeded model, `random_schedules_under_a_tiny_budget_stay_within_it_and_finish` |
| I9 | the builder leaves no line when it ends, and the stop leaves none either | `a_stop_beside_a_backfill_waiting_for_a_permit_ends` (each alone fixes it: the guard is the structural one, the stop's call says why there) |
| I9 | a build's permit held until the builder lands it | `a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line` |
| I9 | a blocked publish leaves no line | `a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line` |
| I7 | a blocked publish doesn't fold | `an_index_parked_at_its_lane_backstop_folds` |
| I6 | a backlog take wakes no reader | `a_backlog_take_wakes_the_reader` |
| I4 (F2) | a leftover never full | `a_capped_takes_leftovers_build_at_once_up_to_builds` |
| I9 (G6) | a builder asks the budget before joining | `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused` |
| I9 (G6) | a standing refusal arms no release wait | six budget tests hang (`an_index_past_its_waiting_bound_holds_the_live_batch_whole` among them) |
| I9 (G4) | every fourth contested grant turned off | `every_fourth_contested_grant_goes_to_another_builders_backfill` |
| I2 | a slice handed to every builder, or an empty one handed | `an_untouched_index_is_handed_nothing` |
| I13 | a builder's error taken as its end | `a_build_error_ends_the_run_and_drops_every_builder_in_place` |

Not tested: the tick backstop on `settle`'s turn wait (no test loses a gate change); `walk_marks`
pending (a slow walk mark beside another builder waiting: argued, not run).

**User-visible, added by the round** (for the documentation agent; the list above stands):

- **An index parked at its lane's backstop no longer stops other indexes' builds**, nor their reading
  below its waiting cap: it folds as a budget hold does. At its cap (`max_waiting_per_index`) the reading
  still stops for every index until 6b-2.
- **A change touching many documents builds its parts at once**, up to `[concurrency] builds`.

#### 6b-1c, fix round 2 (the review of `68987671`)

| Finding | Fix | Test |
| --- | --- | --- |
| a change of N documents cost time quadratic in N: with its parts building side by side, every grant check walked every waiting document up to the candidate (a hash lookup each), and `full_leftover` walked them again | "is a waiting document through the candidate building or held" is answered from the builds' side: each id building (`Pending::building`) and each id a running backfill build holds (`Builds::backfilling`, the build's own `Arc` of its ids) is looked up in the waiting set; counts stop at `max_changes` for a builder of one index (`waiting_of`'s `most`); whether a capped take left anything is one look at the oldest waiting (`Progress::waits_through`). `Holds` is now a visitor over the held ids | `a_grant_check_beside_running_builds_looks_at_no_more_than_builds_and_one_batch` (20,000 documents, 16 building: failed at 19,984 entries looked at; now at most 16 + 4, and 4 more with a backfill build holding four; one held and waiting stops the grant), through a test-only counter of entries looked at |
| in `settle` a builder kept its place in a line while it waited for its own build or a release, holding a freed permit back from the others until then (or the tick) | both waits also end on the builder's turn at the gate, which asks again; a `settle` pass that asks for nothing leaves the lines, as a grant pass does | no test pins the delay: it needs one builder's only running build held while its waiter is ahead of another builder's in the line during `settle`, an interleaving two attempts (log orders, `builds` 2 and 3) didn't produce, and a harness around `settle` alone would be new scaffolding. The second half is pinned: without the leave, `random_schedules_keep_every_document_in_order` spins (a turn wakes the pass to ask for nothing) |

**Mutations** (round 2):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I4 (cost) | the grant check walks the waiting set (round 1's `scan`) | `a_grant_check_beside_running_builds_looks_at_no_more_than_builds_and_one_batch` |
| I4 (cost) | counts not stopped at `max_changes` | the same |
| I9 | a `settle` pass that asks for nothing keeps its waiter | `random_schedules_keep_every_document_in_order` (spins; the nextest limit ends it) |

Still unpinned: M10 (no leave while `walk_marks` pends) and M11 (no tick backstop on `settle`'s turn
wait), as round 1 said: neither has a deterministic trigger short of a test harness of its own.

**Costs added by fix rounds 1 and 2, against 6b-1c as first built** (nothing measured):

| Where | What it costs |
| --- | --- |
| per lane registration | one more pending lock, taken inside the registry lock (it contends with the reader's `mark_reads` on that index), the oldest-waiting lookup and a walk of the index's builds (at most `2 × builds`) |
| per grant check | one lookup per document building and per document a running backfill build holds (at most `builds × max_changes`), plus at most `max_changes` waiting entries counted, instead of a walk of every waiting document while anything built; while a leftover is kept and a live build runs, one more such check (`full_leftover`) |
| per take that leaves something | one look at the oldest waiting (`waits_through`) instead of a full `waiting_of` |
| per ask, several builders | a refusal still standing at the budget's generation takes the `Waits` lock (`refuse`) and arms the release wait; the batch joins the line instead of being refused before it |
| per build | the permit is dropped on the build's own task, a gate wake when someone waits; ids are held in an `Arc`, cloned for a spawned backfill build (`backfilling`) and unwrapped at landing for reuse |
| per published slot | one `poll!` before the await; when it pends, leaving the lines and, with several builders, entering the fold |
| per backlog take | one `Lead::wake` (a fence and an atomic load; a waker only while the reader waits) |
| per `settle` wait | a turn future selected beside the landing or the release (an `Arc` clone and a `Notify` registration while in a line) |
| per change in the reader | fewer lookups: one builder search per run of documents of one index, not one per document |

#### 6b-2 as designed against the code (checked before building)

§5's and §8's 6b-2 rules, read against the code after 6b-1c's two fix rounds (the `newest` rule,
permits dropped on the build's task, the gate deciding the budget beside other builders, `Holds` as a
visitor). Where the paper design needed a detail it didn't give, or the code changed under it:

- **"Held" is its own flag, not the fold.** The fold lasts until nothing waits on the index; held
  (§5) ends at the next live grant or once a blocked publish went through. The exemption reads held, so
  a builder that was released and is draining its backlog still at the cap stays exempt (the latch
  clears only below the cap, §5) but is not latched anew. `Lead`'s queue gets a `held` flag beside
  `folding`, written by the builder: set when its live batch is refused by the budget (the fold check
  sees `refused() == Live`, the gate's refusal included since fix round 1 made the gate decide) or a
  publish pends on a full lane (`publish_ready`); cleared by a live grant (`start_live`, a backlog take
  included) and when the blocked publish completes. Setting it wakes the reader: the reader may be
  waiting at that index's cap (D4).
- **The latch is the reader's, in `Lead::reached`**, as §5 says: per builder, set when the builder is
  held and its index has `max_waiting_per_index` documents or more waiting, cleared when fewer wait.
  The per-index and total bounds sum the non-exempt indexes' `waiting_on` (one atomic load per index per
  lead check, where 6b-1c read two global atomics); the exempt indexes' documents count only toward
  `max_pending_documents`. The changes bound is unchanged: an exempt index folds, so its queue holds
  nothing.
- **One builder: no exemption.** With one index there is nothing to read on for, and 6b-1b's path stays
  as it is (a lead with no indexes keeps the two global loads). The fold needs two builders already.
- **`max_pending_documents` is its own setting**, not a field of `MaxWaiting` (which bounds what the
  reader reads ahead for builds that keep up): `[concurrency] max_pending_documents`,
  `--max-pending-documents`, `FLUSSO_MAX_PENDING_DOCUMENTS`, default 1,000,000, never in the lock,
  refused at 0 by `check`, `build` and `run` naming where it came from; `IngestEngine::with_max_pending_documents`.
  Below `max_waiting_per_index` it still works (an index is exempt at the cap and its first documents
  past it stop the reading), so nothing more is refused.
- **What the operator sees** (per-index status and metrics are slice 7's): the reader logs a warning
  when it latches an index exempt (the index, its documents waiting, that the slot stays pinned and the
  WAL grows) and an info line when the latch clears; a warning when reading stops at
  `max_pending_documents` (the exempt indexes and their counts), and an info line when it reads on.
  The slot's pin is named by `flusso.slot.pinned_by` and `/status` as today.
- **The stop (O3)** needs no new path: an exempt index folds, and `settle` already takes a folding
  builder's backlog in `max_changes` takes. Added: the stop's deadline log names each index with
  documents read and not built and how many (`daemon::Status::waiting_by_index`, from
  `Progress::waiting_by_index`; a log line, not a status field); and **no run ends `Ok` with a document
  waiting**: after every builder returned, the dispatcher fails the run naming them
  (`EngineError::DocumentsLeft`). §8 put the same check there.
- **The restart** needs nothing new: an exempt backlog is pending documents, forgotten by
  `forget_all`, and its takes are backlog takes, whose registrations the `newest` rule covers (fix
  round 1). The seeded model gains backlog takes; the random schedules gain one index stuck for a
  random stretch.
- **Release.** A released exempt builder drains its backlog in `max_changes` takes through the same
  lines as everyone (G9′: a FIFO per kind across builders, G4, G6), so a healthy index's batches keep
  their turns. Nothing new; pinned by a test.

#### 6b-2 as built

Built as "6b-2 as designed against the code" states. What building added:

- **Three slice-4 and 6b-1c tests pinned the old cap and were turned around or moved**:
  `a_parked_index_changing_many_documents_stops_the_reading_at_its_waiting_cap` is now
  `a_parked_index_changing_many_documents_does_not_stop_the_reading` (the acceptance test: the reading
  goes on for ten minutes, `orders` ends with its fifty documents waiting, `users`' lane fence passes
  within two batches of what was read at every sample, confirmation stays pinned before `orders`' first
  unbuilt change from the 70th second to the end, and the budget stays within its rules at every write);
  `an_index_past_its_waiting_bound_holds_the_live_batch_whole` is now
  `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on`; `a_backlog_take_wakes_the_reader`
  now waits at `max_pending_documents` (two), where a held index waits since its cap stops nothing.
- **The memory per pending document is measured**: 266 B held and 314 B at the peak for a one-column
  integer key (`tests/pending_memory.rs`, a counting global allocator, a million documents; Linux x86_64),
  under §10's 300 B and 360 B, which are the test's ceilings. At the default cap: about 266 MB, 314 MB
  while the map grows. The key's column name is a string of its own per document (a decoder makes one
  per key); a wider key costs more.
- **The stop's exit names the indexes**: the CLI's deadline log adds `orders: 1200 documents read and not
  built` beside the sinks (`shutdown::unbuilt_indexes`). The binary's `shutdown` e2e is not extended:
  it would need one index's lane filled behind a paused OpenSearch (a new scenario in the serialized
  OpenSearch group); the engine test that the stop doesn't end and the unit test of the line pin the two
  halves.

**Tests** (each fails under its mutation; "hang" never: every wait is bounded):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I8 latch | never exempt | the four lead unit tests, the acceptance test, `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on`, `the_stream_end_builds_an_exempt_backlog`, `a_stop_builds_and_lands_an_exempt_backlog_under_the_deadline`, `reading_stops_for_every_index_at_max_pending_documents`, `an_index_reaches_its_exemption_with_nothing_else_in_flight`, `a_released_backlog_drains_in_parts_beside_another_indexs_changes` |
| I8 | the latch cleared only when nothing waits | `an_exemption_ends_below_max_waiting_per_index` |
| I6 | exempt documents counted toward `max_waiting` | `exempt_documents_count_toward_no_other_bound` and five engine tests |
| I6 | no `max_pending_documents` | `exempt_documents_stop_the_reading_only_at_max_pending_documents`, `reading_stops_for_every_index_at_max_pending_documents`, `a_backlog_take_wakes_the_reader` |
| I7 (D4) | becoming held wakes no reader | `holding_a_builder_wakes_a_reader_waiting_at_its_cap` |
| I7 | a blocked publish holds nothing | five engine tests, `an_index_reaches_its_exemption_with_nothing_else_in_flight` among them |
| I7 | a budget refusal holds nothing | the acceptance test, `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on` |
| I12 | `settle` skips a folding backlog | `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on` (a builder's own loop drains its backlog before it settles in the other end tests) |
| I5 (`newest`, exempt takes) | registration raises `newest` whatever waits | the seeded model (now with backlog takes), `random_schedules_under_a_tiny_budget_stay_within_it_and_finish` |

**Not pinned**: a live grant ending the hold (M7: a hold left set after a grant only shows when a budget
hold is followed by slow builds reaching the cap unheld, a scenario of its own); the dispatcher's
`DocumentsLeft` check (M9: no path leaves a document waiting at the end, so nothing reaches it; it is the
guard against one); the line order on release (a waiter-newest-first gate: with two builders a freed
permit wakes both and the one polled first asks first, so the order in the line rarely decides;
`a_released_backlog_drains_in_parts_beside_another_indexs_changes` pins the latency, not the order); the
`shutdown` e2e through the binary (above). The random schedules hold `orders` stuck for a random stretch
in every third two-index seed, with failures restarting the run; they check order and the final state,
and resume after what was confirmed, so a confirmation past an exempt index's unbuilt document would lose
it.

**Writers checked** (6b-2's, beside fix round 1's table):

| State | Writer | Readers |
| --- | --- | --- |
| `Queue.held` | the builder: set on a live refusal (fold check) and a pending publish; cleared at a live grant and when the publish goes through | the reader's latch |
| `Queue.exempt` | the reader, in `Lead::reached` | the reader |
| `ExemptionLog` (each index's line and the cap's: the state last logged, and when) | the dispatcher's tick (`Lead::tick`); kept by the engine across runs since round 3 | the tick |
| a lane's `newest` from an exempt take | `Progress::register`, under the `newest` rule (a backlog take registers at the newest position waiting, raised only once nothing at or before it waits) | confirmation, fences |

**Costs against 6b-1c** (nothing measured):

| Where | What it costs |
| --- | --- |
| per lead check, two or more builders | one `waiting_on` atomic load per index and two atomic flags per builder, where 6b-1c read two global atomics; a log line on each latch edge |
| per live grant | one atomic swap (`hold(false)`) |
| per budget refusal seen by the fold check, per blocked publish | an atomic swap, and a reader wake when it turns held |
| per run end | one `waiting_by_index` (a lock per index) |
| per exempt document | about 266 B, up to `max_pending_documents` (default 1,000,000: about 266 MB, 314 MB while the map grows), outside `max_built_mib` |
| one builder | its lead keeps 6b-1b's check; `hold` returns before its swap (one branch per live grant and per blocked publish), so one index costs a branch where 6b-1c had nothing (corrected in the review of #452: as first built it swapped at every live grant, and a blocked publish swapped twice and ran a fence) |

**User-visible** (for the documentation agent; the 0.19 upgrade page):

- **`[concurrency] max_pending_documents`** (`--max-pending-documents`, `FLUSSO_MAX_PENDING_DOCUMENTS`),
  default 1,000,000, at least 1 (`check`, `build` and `run` refuse 0, naming where it was set), never in
  the lock, settable in the designer's config form.
- **A stuck index no longer stops the others**: with two or more indexes, an index held by the memory
  budget (its lane not acknowledging, so its share full) or by a full lane, with `max_waiting_per_index`
  documents waiting, is exempt. The other indexes keep being read, built and written, and their fences
  keep passing. The stuck index's changes are kept as documents to build (each document once, about
  266 B), up to `max_pending_documents` over every exempt index; there the reading stops for every index,
  as it did at `max_waiting_per_index` before. A warning names the index when it becomes exempt and when
  the reading stops at the cap; an info line when it ends.
- **What it does not do**: the slot stays pinned by the stuck index (confirmation never passes its
  oldest unbuilt change), so WAL grows for as long as it is stuck (`flusso.replication.slot_lag`,
  `flusso.slot.pinned_by`). An index that is merely slow, not held, still paces the reading at its cap
  (made true after a hold too in fix round 1, below; and see the whole-sink rule there).
  With one index nothing changes. Per-index status and metrics come later (slice 7).
- **The stop**: a stop builds and lands everything read, a stuck index's documents included, within
  `--shutdown-timeout-secs`; if it can't (the index still stuck), it exits 1 and the log names each
  index with documents read and not built and how many; it never exits 0 with read changes unbuilt. The
  next start reads them again from the slot.
- **Memory**: plan for up to about 266 B per document a stuck index keeps (314 B at the peak), on top of
  `max_built_mib`: about 270–320 MB at the default cap.

#### 6b-2, fix round 1 (the review of `a27affb7`)

| Finding | Fix | Tests (fail before the fix) |
| --- | --- | --- |
| 1: an index held once and then merely slow stayed exempt and read ahead to `max_pending_documents` (the latch never looked at "held" again) | latched but not growing, an index is frozen at the count it stopped growing at: it may drain, not grow (§5) | `a_slow_index_once_held_paces_the_reading_again` (the reviewer's: 17 waiting at the hold, 65 later on the first build; now at most one more), `an_exempt_index_whose_hold_ends_may_drain_but_not_grow` (lead unit) |
| 2: a whole sink down exempted every index and read on to `max_pending_documents` | an index grows exempt only while some other index is not held; with every index held none becomes exempt and exempt ones freeze (§5, "A whole sink down") | the five `with_a_whole_sink_down_*` (two and three indexes: at most 20 read and 12 waiting, where the first build read 106 with 100 waiting; one index unchanged), `no_index_grows_exempt_while_every_index_is_held` (lead unit), `the_exemption_lasts_while_some_index_flows_and_stops_when_none_does` (three indexes: two stuck exempt while `users` flows, then `users` held too and the reading stops, then everything lands) |
| 3: M7 had no test | — | `a_live_grant_ends_the_hold` (the reviewer's), still failing with `start_live`'s clear removed after the fix |
| 5: one builder paid for `hold` | `hold` returns at once without an exemption | (a cost: the table above) |
| 6: the total bound line | corrected in §12's bounds | — |
| 7: the log could flap | each kind of line at most once a minute per index, the first always (`LogEdge`) | `an_index_hovering_at_its_cap_logs_each_kind_of_line_at_most_once_a_minute` (paused time, a capturing subscriber) |
| 8: release, the other direction | — | `a_released_backlog_drains_in_parts_beside_another_indexs_changes` also asserts the backlog drains within a second beside `users`' steady load |

A hold ending now wakes the reader too (`a_hold_ending_wakes_a_reader_waiting_at_a_cap`): with every
index held and the reader stopped, the first index let go is what lets the exemption apply again, so no
wake is lost. The latch keeps its hysteresis: it sets at the cap only while growing, clears only below
the cap, and a frozen index's ceiling is taken once, when it stops growing, and kept until the latch
clears or it grows again (a ceiling following the count down would stop the reading for as long as the
index has its cap, which the mutation shows).

**Mutations** (round 1):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| I8 | a frozen index treated as growing | `an_exempt_index_whose_hold_ends_may_drain_but_not_grow`, `an_exemption_ends_below_max_waiting_per_index`, `no_index_grows_exempt_while_every_index_is_held`, `a_slow_index_once_held_paces_the_reading_again` |
| I8 | exempt with every index held (`someone_free` always true) | `no_index_grows_exempt_while_every_index_is_held`, `the_exemption_lasts_while_some_index_flows_and_stops_when_none_does`, the three `with_a_whole_sink_down_*` of two and three indexes |
| I7 | a hold ending wakes nobody | `a_hold_ending_wakes_a_reader_waiting_at_a_cap` |
| I8 | the frozen ceiling follows the count down | `an_exempt_index_whose_hold_ends_may_drain_but_not_grow`, `a_released_backlog_drains_in_parts_beside_another_indexs_changes` |
| log | no throttle | `an_index_hovering_at_its_cap_logs_each_kind_of_line_at_most_once_a_minute` |
| I7 (M7) | a live grant ends no hold | `a_live_grant_ends_the_hold` |

**User-visible, changed by the round** (for the documentation agent; replaces the matching lines above):

- **"An index that is merely slow still paces the reading at its cap" is now true after a hold too**: once
  a stuck index's hold ends (its lane or budget recovers), it may drain its backlog but no longer grow
  it; once it drains below `max_waiting_per_index` it paces the reading like any index.
- **The exemption is there so other indexes keep going**: an index is exempt only while some other index
  is not stuck. **With a whole sink down** (every index of two or more stuck at once), no index is
  exempt: the reading stops at `max_waiting_per_index` / `max_waiting`, as before 0.19 (no reading on
  to `max_pending_documents`, no extra memory, no resolution queries against Postgres while it waits),
  and goes on as soon as any index recovers. With one index nothing changes.
- **The log lines are throttled**: an index becoming exempt, no longer exempt, and the reading stopping
  at or going on below `max_pending_documents` each log at most once a minute per index (the first
  always).

#### 6b-2, fix round 2 (the review of `b33a2965`)

The rule is in §5 ("Some other index" means a free index), written before the code.

| Finding | Fix | Tests (fail before the fix) |
| --- | --- | --- |
| 1: a whole sink down beside a quiet, light or backfilling index still read to `max_pending_documents` ("not held" taken for "making progress") | an index is free while not held and nothing of it waits on a lane unacknowledged for `STUCK_AFTER` (5 s); the record per index (`ingest/outstanding.rs`) counts every published item, live or backfill, until every lane let go of it | (a) `with_a_whole_sink_down_a_silent_index_lets_the_others_read_on_to_the_pending_cap` (documents the stated behaviour), (b) `with_a_whole_sink_down_one_unacknowledged_change_stops_the_reading_after_five_seconds` (the reviewer's idle index with one change never acknowledged: stops, at most five seconds of changes past the bounds), (c) `with_a_whole_sink_down_a_light_index_is_not_free_once_its_item_waits_five_seconds` (stops by 8 s, its share would take ~28 s), (d) `with_a_whole_sink_down_a_backfilling_index_is_not_free_once_its_items_wait_five_seconds`, (e) `one_stuck_index_beside_a_quiet_and_a_flowing_one_is_exempt` (and the acceptance test, unchanged), (f) `a_slow_healthy_index_lets_the_reading_go_on_after_each_acknowledgement`; `an_index_is_stuck_while_its_items_wait_five_seconds_unacknowledged` (unit) |
| 2: the throttle could leave the opposite of the current state as the last line | the pair throttled together; the current state logged at the next check once the minute is over; the tick re-checks a waiting reader each second | `the_last_line_logged_names_the_current_state_within_a_minute_of_a_change` (the reviewer's), `an_index_hovering_at_its_cap_logs_each_kind_of_line_at_most_once_a_minute` (rewritten for the pair) |
| 3: §12's bound for `k` indexes rejoining | corrected | — |
| 4: loose whole-sink bounds | each tightened to its measured value plus one (15/8, 15/9, 17/11 for two and three indexes; 9/5 and 11/5 for one) | — |
| 5: no engine test of a hold ending's wake | — | `the_reading_goes_on_within_half_a_second_of_a_hold_ending` (measured from `users`' first write after its release, so the tick couldn't carry it) |

**Not as suggested.** The record is not the daemon's stall age (`daemon::Stalls`): that lives a layer
above the engine and counts live items only, so a backfilling-only index would look free (§5). Its
cost: per published batch, one allocation per index (the guard beside the budget's charge in the
item's `Hold`) and an atomic add; per item let go by every lane, a clock read, a store and an atomic
subtract; per lead check while some index is held, one load per index (a clock read and two loads for
an index with items outstanding); one wake of a waiting reader per second with two or more builders
(corrected in round 3: not "while the lead exempts"). With
one index there is no record and an item's hold is the budget's charge alone, as before.

**Mutations** (round 2):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| free | `STUCK_AFTER` ignored (stuck as soon as anything is outstanding) | `an_index_is_stuck_while_its_items_wait_five_seconds_unacknowledged`, `a_slow_healthy_index_lets_the_reading_go_on_after_each_acknowledgement` |
| free | outstanding ignored (never stuck) | the unit test, (b), (c), (d), (f) |
| free | a quiet index with nothing outstanding counted stuck | the unit test, (a), (e), and twelve more exemption tests (15 in all; corrected in round 3) |
| free | backfill items not counted | (d) |
| self-correction | the tick wakes nobody | (f) |
| log | a suppressed line marked logged (round 1's per-kind bug) | `an_index_hovering_at_its_cap_logs_each_kind_of_line_at_most_once_a_minute` |

**User-visible, changed by round 2** (for the documentation agent; replaces round 1's whole-sink and log
lines; the manual's "a whole sink down stops the reading" is not true as written):

- **An index counts as working ("free") while it is not stuck and nothing of it has waited on the sink
  for an acknowledgement for 5 seconds without any being acknowledged.** An index with no changes and
  nothing sent is working as far as flusso can tell. The 5 seconds are fixed, not a setting.
- **A stuck index is exempt only while another index is working.** Once its own hold ends, or no other
  index is working, it may build its backlog down but no longer add to it; an index merely slow (never
  refused) paces the reading at its cap.
- **A whole sink down** (every index of two or more stuck at once): within about 5 seconds of each
  index's first unacknowledged change, no index with traffic is working, and the reading stops at
  `max_waiting_per_index` / `max_waiting` as before 0.19. In those first seconds an index already stuck
  may keep about 5 seconds of its changes. An index that gets **no** change at all during the outage
  counts as working, so the stuck ones read on until `max_pending_documents` (about 266 MB at the
  default 1,000,000), as they would beside a healthy quiet index, which flusso cannot tell apart; the
  first change that index gets and the sink doesn't acknowledge within 5 seconds stops the reading.
- **Logs**: within a minute of any change (plus up to a second), the last exemption line logged for an
  index names its current state (exempt or not; the reading stopped at `max_pending_documents` or
  reading on); at most one line of each pair a minute per index.

#### 6b-2, fix round 3 (the review of `e639fc9e`)

The rules are in §5 (the log, `STUCK_AFTER`, self-correction, a whole sink down), written before the
code.

| Finding | Fix | Tests (fail before the fix) |
| --- | --- | --- |
| 1: the last line could name the wrong state for as long as (a) the stream was idle (no lead check runs; the tick woke only a parked reader), (b) after an engine restart (the lead's lines started fresh), or (c) for an index after one over its bound (`documents_reached` returns there) | the lines' state is the engine's (`ExemptionLog`, kept across runs like `Outstanding`); they are checked only on the dispatcher's 1 s tick, for every index and the cap, with no early return, reading the state the reader decides at its next check (`Lead::deciding`) and writing nothing it reads; the reader's checks decide and log nothing. Three states per index, the frozen one with its own line | (a) `on_an_idle_stream_the_last_lines_name_the_state_within_a_minute`, (b) `across_an_ingest_restart_the_last_line_names_the_state_within_a_minute` (both the reviewer's, renamed), (c) `an_index_after_one_over_its_bound_still_logs_its_state` (lead unit); `an_exempt_index_whose_hold_ends_logs_that_it_is_frozen` (lead unit) |
| 2: no test of a healthy sink acknowledging an index another sink holds | — (one guard per item, shared by its lanes, already) | `an_index_one_dead_sink_holds_is_not_free_although_a_healthy_sink_acknowledges_it` (two sinks, `audit` dead: `users` acknowledged by the healthy sink every second is not free after 5 s, the reading stops) |
| 3: one index paid for the record | `IngestEngine.exemption: Option<_>`, `None` with one index: an item's hold is built from the batch's charges moved, as 6b-1c built it (no lookup, no clone); with several, each charge is moved out of the batch's map (no `Arc` clone) | (a cost: the table below) |
| 4: wording | §5: one restart under 5 s only for a write that fails fast, the effect bounded; "publishes nothing", not "no change" (#169's unread-column updates publish nothing); the tick wakes a parked reader whenever there are two or more builders; the frozen state says what is true; a healthy sink slower than 5 s per acknowledgement freezes an exempt neighbour until the acknowledgement plus a tick, never worse than `main`; round 2's mutation count is 15 | — |

**The promise, and its one condition.** Within a minute and a second of any change the last line logged
for an index (and the cap's) names its current state: the minute is the throttle, the second the tick.
The condition: the tick runs only while an engine run is live, so during a failed run's restart backoff
(up to `max_restart_backoff`, 60 s by default) nothing is read and nothing is logged; the next run's
first tick logs the state. A restarted run's lead starts unlatched, so a line may name "not exempt" for
an index still held at its cap until the reader's next check latches it again: true for that moment,
since the new lead paces the reading at the cap until then.

**Not as suggested.** The slot instead of the by-name lookup (finding 3, two or more indexes) is not
taken: only a live batch knows its builder (through its `Runs`); a backfill batch is published with no
builder slot in hand, and the batch's charges are keyed by name too. What remains is in the table.

**Costs, changed by round 3** (nothing measured; replaces round 2's paragraph where they differ):

| Where | What it costs |
| --- | --- |
| one index, per batch | an `Option` check; the hold is the moved charge, as 6b-1c (round 2: an empty map lookup and an `Arc` clone and drop) |
| two or more indexes, per batch, per index it built for | one lookup by name in the record (a `BTreeMap` of the indexes: a few name comparisons), the charge moved out of the batch's charge map, one allocation (the guard beside the charge) and an atomic add |
| per lead check, two or more builders | the decision only: no clock read and no log check (round 2 checked two lines per index here) |
| per tick (1 s), two or more builders | one wake (a fence and a flag load, a wake if the reader waits); per index a `waiting_on` load, two flags and a `stuck` check (a clock read and two loads with items outstanding); per line, two loads; a log line only when one is due |

**Mutations** (round 3):

All twelve were rerun after the round, round 2's six rewritten where the code moved; each fails the
tests listed, none hangs, and every mutant compiles.

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| free | `STUCK_AFTER` ignored | `an_index_is_stuck_while_its_items_wait_five_seconds_unacknowledged`, (f), `an_index_one_dead_sink_holds_is_not_free_although_a_healthy_sink_acknowledges_it` |
| free | outstanding ignored (never stuck) | the unit test, (b), (c), (d), (f), the two-sink test |
| free | a quiet index with nothing outstanding counted stuck | 19 tests: the unit test, (a), (e), the lead units, both adopted log tests and the round's two lead log units among them |
| free | backfill items not counted | (d) |
| free (finding 2) | a guard per lane (a healthy sink's acknowledgement drops its own guard) | the two-sink test |
| self-correction | the tick wakes nobody | (f) |
| log | a suppressed line marked logged | `an_index_hovering_at_its_cap_logs_each_kind_of_line_at_most_once_a_minute`, both adopted log tests |
| log (a) | lines checked only while the reader is parked | the idle and restart tests, `an_index_after_one_over_its_bound_still_logs_its_state`, `an_exempt_index_whose_hold_ends_logs_that_it_is_frozen`, the hovering and last-line units |
| log (b) | a log per engine run | `across_an_ingest_restart_the_last_line_names_the_state_within_a_minute` |
| log (c) | the tick stops at an index over its bound | `an_index_after_one_over_its_bound_still_logs_its_state` |
| log | the frozen state logged as growing | `an_exempt_index_whose_hold_ends_logs_that_it_is_frozen` |
| log | the cap line never names the cap reached | `on_an_idle_stream_the_last_lines_name_the_state_within_a_minute` |

**User-visible, the whole slice as it now stands** (for the documentation agent; replaces every
user-visible list above, the first one's and rounds 1 and 2's):

- **`[concurrency] max_pending_documents`** (`--max-pending-documents`, `FLUSSO_MAX_PENDING_DOCUMENTS`),
  default 1,000,000, at least 1 (`check`, `build` and `run` refuse 0, naming where it was set), never in
  the lock, settable in the designer's config form.
- **A stuck index no longer stops the others** (two or more indexes): an index held by the memory budget
  (its share full because its lane isn't acknowledging) or by a full lane, with `max_waiting_per_index`
  documents waiting, is exempt. The other indexes keep being read, built and written, and their fences
  keep passing. The stuck index's changes are kept as documents to build (each document once, about
  266 B), up to `max_pending_documents` over every exempt index; there the reading stops for every
  index, as it did at `max_waiting_per_index` before.
- **Only while another index is working.** An index counts as working while it isn't stuck and nothing
  it sent has waited for an acknowledgement for 5 seconds with none acknowledged (fixed, not a setting).
  An item waits until **every** sink acknowledged it: a healthy sink acknowledging an index doesn't make
  it working while another sink holds its items. An index that sends nothing (no change, or only changes
  that build nothing, such as updates of columns no document reads) counts as working.
- **Once its hold ends, or no other index is working**, a stuck index may build its backlog down but no
  longer add to it; an index merely slow (never refused) paces the reading at its cap, after a hold too.
- **A whole sink down** (every index of two or more stuck at once): within about 5 seconds of each
  index's first unacknowledged item, no index with traffic is working and the reading stops at
  `max_waiting_per_index` / `max_waiting`, as before 0.19 (in those first seconds an index already stuck
  may keep about 5 seconds of its changes). An index that sends nothing during the outage counts as
  working, so the stuck ones read on to `max_pending_documents`, as beside a healthy quiet index, which
  flusso cannot tell apart; the first item that index sends and the sink doesn't acknowledge within
  5 seconds stops the reading.
- **A healthy sink slower than 5 seconds per acknowledgement** (a large backfill or reindex bulk), or a
  sink engine restart whose failing write takes its timeout, makes its index count as not working for
  that long: an exempt neighbour stops growing (the reading may stop) until the acknowledgement, plus up
  to a second. Never worse than before 0.19, which stopped the reading at the neighbour's cap for as
  long as it was stuck.
- **What it does not do**: the slot stays pinned by the stuck index (confirmation never passes its
  oldest unbuilt change), so WAL grows for as long as it is stuck (`flusso.replication.slot_lag`,
  `flusso.slot.pinned_by`). With one index nothing changes. Per-index status and metrics come later
  (slice 7).
- **The stop**: a stop builds and lands everything read, a stuck index's documents included, within
  `--shutdown-timeout-secs`; if it can't (the index still stuck), it exits 1 and the log names each
  index with documents read and not built and how many; it never exits 0 with read changes unbuilt. The
  next start reads them again from the slot.
- **Memory**: plan for up to about 266 B per document a stuck index keeps (314 B at the peak), on top of
  `max_built_mib`: about 270–320 MB at the default cap.
- **Logs**: one line per index naming its state: **exempt** (a warning: reading goes on for the other
  indexes, WAL grows), **frozen** (a warning: its hold ended or no other index is working, so it may
  drain but not grow, and the reading stops for every index while it has that many waiting), **no
  longer exempt** (info); and one for the cap (a warning when the reading stops at
  `max_pending_documents`, info when it reads on). At most one line a minute per index and one for the
  cap, the first always; within a minute and a second of any change the last line names the current
  state, on an idle stream and across an ingest engine restart too. Nothing is logged while the ingest
  engine waits to restart after a failure; its first second back logs the state.

#### 6b-3 as designed against the code (checked before building)

Checked against `c3b6233b` (6b-2 after its three fix rounds). What the paper (§3.5 K1, §7, §11, §12's
6b-3 rows) says and what the code needs:

- **The walk token goes.** Each index has its own builder (6b-1c), whose `Snapshot` walks only its own
  index, so walks at once means: drop the token (`Building.walks`, `start_walkable`'s wait, the
  `walk_wait` arm). A builder starts its gathered requests' snapshot as soon as they are ready (every
  sink with a lane asked, or `max_delay` passed), as `main` did and as one builder always found the
  token free. Coalescing, tickets, cuts, "acked only after its `SnapshotComplete` is published", "rows
  only to the requesting lanes" and the per-index order of snapshot and live builds are the builder's
  and its `Builds`' already, per index, and don't move. One walk per index at a time, as before.
- **K1 stays one permit, now held only by a read in progress.** Where the paper said "a key read runs
  on a task of its own": with two or more builders a key read (the bound, a page) is polled once where
  the walk is stepped and, if that doesn't finish it, spawned on its own task, as a build is (6a's
  `Builds::start`). The permit is taken inside the read, from before the query until it returns, so a
  walk whose builder stops stepping it (its builder awaiting a publish on a full lane, its batch waiting
  on the budget) still finishes its read and gives the permit back; the page waits in the task's output
  until the walk is stepped again. The handle aborts the task when the walk is dropped. **One builder
  reads inline, as today**: no spawn, no handle, so one index costs what it did (the hazard needs a
  second walk). The pool stays `builds + 1` plus the resolver; reads at once ≤ 1 over every walk.
- **Keys read ahead: no prefetch.** A walk starts its next read only once its keys in hand are taken
  (as today), so per walk at most one page, `KEY_PAGE` = 1,024 keys, is held: in hand, or in a finished
  read not yet taken; plus its batch's ids (`max_changes`). With N walks: N × 1,024 keys, about 110 B
  each for a one-column integer key (the `RowKey`, its `Vec` and the column name), so about 115 KB a
  walk, 1.1 MB for ten; a wider key costs more. The paper didn't state it.
- **Correction: rule 4 is per index.** §7 said "rule 4 holds for them together" (backfill holding
  nothing gets one batch). With walks at once that lets one stuck walk stop every other: its items stay
  unacknowledged, their charges stay, and its backfill fills the shared half (rule 2) and keeps it, so
  backfill never holds nothing again and every other index's backfill batch is refused for as long as it
  is stuck. Lanes don't stop it first: the channel's backstop is 1,048,576 items, so the budget is what
  bounds a stuck lane. The token hid it (the stuck walk held the token anyway). So, as round 3 of the
  budget did for live (rule 5): **an index holding no backfill bytes gets one backfill batch, whatever
  rules 1 and 2 say, and is never held back by an older backfill waiter**. A healthy index acknowledges
  its batches, holds none again and again, and walks on, one batch in flight at a time beside a stuck
  one; with nothing stuck rule 2 governs as before. With one index it is the old rule exactly (its
  backfill bytes are all the backfill). **Bounds change**: backfill ≤ half plus one batch per index
  walking (was one batch); total ≤ the limit plus one live and one backfill batch per index, plus the
  settle slack (was one backfill batch).
- **The stop, a failure, a drop.** A stop drops every builder's snapshot, unacknowledged (its requests
  redelivered on the next start), as today, now for every walk at once; a failure ends the run and drops
  every builder in place; an aborted run drops them too. Each drop aborts the walk's read task, so none
  outlives the run (an aborted task is dropped at its next poll; a read awaiting the database is
  cancelled there).
- **The exemption with walks at once.** An index only backfilling on a dead sink is not free after 5 s
  (built in 6b-2: backfill items carry the outstanding guard). A walk adds nothing to
  `max_pending_documents` or the lead: its documents are the snapshot's batch, never marked waiting in
  `Progress`. The budget's rules hold with N walks by the correction above (rule 2 for the half, rule 4
  per index).
- **Fairness.** Each builder keeps one persistent backfill waiter (6b-1c), FIFO (G2), so walking
  indexes take backfill grants in turn; G4 gives every fourth contested grant to the oldest backfill
  waiter, whichever index.

**Expected gain** (not measured here): the first backfill of several indexes overlaps. A small index no
longer waits behind a large one's whole walk, and one index's key read runs beside another's builds and
publishes. Builds at once stay ≤ `builds` and key reads ≤ 1, so the gain is wall time where one walk
alone left permits idle (a key read, a slow lane, a small table), not more than `builds` at once. One
index: nothing changes, and it costs what it did (no spawn, the same rule 4).

**Tests, written first** (a rule each; each fails on `c3b6233b` or under its mutation):

| Rule | Test |
| --- | --- |
| walks at once | `indexes_walk_at_once` (6b-1c's `indexes_walk_one_at_a_time` turned around: two walks with no page both start) |
| K1 read on its own task | `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads` (two walks, one stepped once and then never: the other reads every key) ; `a_walk_blocked_on_a_full_lane_holds_no_other_walks_key_reads` (engine, added while building) |
| no leak: drop | `a_dropped_walk_ends_its_key_read` (unit), `an_aborted_run_leaves_no_key_read_running` |
| no leak: stop | `a_stop_mid_walks_ends_every_key_read_and_serves_every_request_next_start` |
| no leak: failure | `a_failed_walk_ends_the_run_and_every_other_key_read` |
| rule 4 per index | `an_index_holding_no_backfill_bytes_gets_one_backfill_batch` (budget unit), `a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk` (engine: a dead sink for `orders`, both backfilling, `users`' walk lands) |
| fairness | `two_backfilling_indexes_take_backfill_grants_in_turn` |
| one index | `one_index_reads_its_keys_inline` (no task spawned for its reads) |

#### 6b-3 as built

Built as "6b-3 as designed against the code" states, in two commits that each build and pass: rule 4
per index (`Budget::decide`, `State::backfill_by`), then walks at once with the key read's task
(`ingest/snapshot.rs`'s `Read`/`KeyRead`/`Walk::reading_apart`, `start_snapshot`'s `apart`, the
builder without the token). What building added:

- **Every key read is spawned when the walk reads apart**, not polled once first as a build is: a read
  almost always waits on the database, and a first poll inline would take the permit on the builder's
  task, which is what the task is there to avoid. Cost: one spawn per bound or page read (per
  `KEY_PAGE` = 1,024 keys) with two or more builders.
- **A read that panics on its task panics the run** (`resume_unwind` of the task's panic), as a read run
  inline did; a cancelled one (only its own walk's drop aborts it, so never awaited) would be a query
  error.
- **The engine-level K1 test needs a full lane.** With lanes at the channel's backstop (1,048,576
  items) a builder stops stepping its walk mid-read only while a publish blocks, which the budget makes
  rare (a lane fills before the budget only past about 2 GiB of `max_built_mib`); the walk is otherwise
  stepped whenever its batch has room, and its batch can't fill while its read is pending. So the
  hazard is real but narrow in production; the test (`a_walk_blocked_on_a_full_lane_holds_no_other_walks_key_reads`)
  builds it with a two-item lane. The budget-bound stuck walk was the case with the wider reach, and
  it needed rule 4 per index, not the task.
- **One existing e2e expectation encoded the token**: `keyset_backfill`'s
  `rows_added_ahead_of_the_walk_end_it_where_it_began_and_are_built_once` counted 12 full pages, `live_items`
  walking after `items` and finding the pages added meanwhile. Both walk at once now and normally both
  bounds are read before either page (8); the test asserts the range 8–12 (each walk ends at the bound
  it found at its start) and still that no document is built twice.
- **Tests written first and how they failed on `c3b6233b`**: `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`
  (`users`' walk timed out), `indexes_walk_at_once` (one snapshot started), `two_backfilling_indexes_take_backfill_grants_in_turn`
  (orders' 8 grants, then users' 8), `a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk` (0 of
  200 `users` rows), `an_index_holding_no_backfill_bytes_gets_one_backfill_batch`,
  `a_stop_mid_walks_ends_every_key_read_and_serves_every_request_next_start` and
  `an_aborted_run_leaves_no_key_read_running` (one walk started). The leak and inline tests
  (`a_dropped_walk_ends_its_key_read`, `a_failed_walk_ends_the_run_and_every_other_key_read`,
  `one_index_reads_its_keys_inline`, `one_index_spawns_no_task_for_its_key_reads`,
  `a_key_read_panicking_on_its_own_task_panics_the_run`) pass on `c3b6233b`, which spawned nothing: they
  guard the task, and fail under its mutations. The leak tests count the runtime's alive tasks
  (`num_alive_tasks`) and each read's future, back to where they were, within 5 s.
- **Changed by rule 4 per index**: `four_indexes_pass_the_limit_by_at_most_a_live_and_a_backfill_batch_each`
  (renamed; 288 KiB, each index's first backfill batch admitted), `random_reservations_stay_within_the_stated_bounds`
  (bounds `limit + 4B + 4B`, backfill `half + 4B`), `gate::tests`' `a_refusal_does_not_move_the_count`
  (its refused backfill waiter's own index now holds the half, so it is still refused).

**Mutations** (each compiles; run over the engine's snapshot, builders, walks, budget, gate, stop, gone,
snapshot, exemption and random-schedule tests; "hang" never):

| Rule | Mutation | Tests that fail |
| --- | --- | --- |
| walks at once | 6b-1c's walk token kept (its builder and the token restored) | `indexes_walk_at_once`, `two_backfilling_indexes_take_backfill_grants_in_turn`, `a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk`, `a_stop_mid_walks_ends_every_key_read_and_serves_every_request_next_start`, `an_aborted_run_leaves_no_key_read_running`, `one_index_spawns_no_task_for_its_key_reads` |
| K1 task | several builders read inline | `a_walk_blocked_on_a_full_lane_holds_no_other_walks_key_reads` (the first run of the set had no engine test for it: the mutation survived, and the test was written then) |
| no leak | a dropped walk leaves its read running (no abort) | `a_dropped_walk_ends_its_key_read`, `a_failed_walk_ends_the_run_and_every_other_key_read`, `a_stop_mid_walks_ends_every_key_read_and_serves_every_request_next_start`, `an_aborted_run_leaves_no_key_read_running`, `indexes_walk_at_once` |
| one index | one builder reads on a task too | `one_index_spawns_no_task_for_its_key_reads` |
| panic | a read panicking on its task becomes an error | `a_key_read_panicking_on_its_own_task_panics_the_run` |
| rule 4 | over every index together (backfill holding nothing) | `an_index_holding_no_backfill_bytes_gets_one_backfill_batch`, `four_indexes_pass_the_limit_by_at_most_a_live_and_a_backfill_batch_each`, `a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk` |
| rule 4 | per index, but held back behind an older waiter | `an_index_holding_no_backfill_bytes_gets_one_backfill_batch`, `a_stuck_index_walking_does_not_hold_a_healthy_indexs_walk` |

**Not pinned**: no read ahead (a walk starting its next read before its keys in hand are taken would
only hold one more page per walk; no test counts the keys held); the abort of a read still waiting for
the permit, apart from the aborted-run test (which counts every task, the waiting one included); the
expected gain (nothing measured, as asked).

**Writers checked** (6b-3's):

| State | Writer | Readers |
| --- | --- | --- |
| the K1 permit | the read, on its task, from before its query to its return | every walk's reads (FIFO) |
| a walk's read in flight (`Walk.fetch`) | its walk, stepped by its builder; the task writes only its own output | its walk |
| an index's backfill bytes (`Held.backfill`) | the budget, under its lock (a backfill grant adds, a charge's drop subtracts) | `Budget::decide` (rule 4 per index) |

**Costs against 6b-2** (nothing measured):

| Where | One index | Two or more |
| --- | --- | --- |
| per key read | nothing | one spawn and a `JoinHandle` per bound or page (1,024 keys) |
| per walk | the token's `try_acquire` gone | the same |
| per backfill reservation | one map lookup for rule 4 (`backfill_by`) where a counter was read | one lookup per index of the batch (one) |
| memory | as before | ≤ 1,024 keys per walk (in hand or on its task), about 110 B each for a one-column integer key |

**User-visible** (for the documentation agent; the 0.19 upgrade page and the backfill, memory and
reference pages):

- **Backfills of several indexes run at once.** Each index's backfill (the first one, a reindex, a
  re-seed after its sink lost the index) starts as soon as its sinks asked, beside any other index's;
  before, one index's backfill waited for the other's whole walk. Builds at once stay within
  `[concurrency] builds` and key reads stay one at a time over every index, so the database sees no more
  connections than before. With one index nothing changes.
- **One index stuck stops no other index's backfill.** An index whose sink stops acknowledging keeps
  its backfill's batches in memory (up to half of `max_built_mib`, shared by every backfill) for as long
  as it is stuck; every other index walking still gets a batch each time its previous one was
  acknowledged, so its backfill goes on (one batch in flight at a time while the half is full). Before,
  the stuck index's walk held every other walk anyway.
- **Memory: backfill may pass half of `max_built_mib` by one batch per index walking** (it was one batch
  in all), so the whole stays within `max_built_mib` plus one live batch and one backfill batch per
  index. A walk holds at most 1,024 root keys at a time (about 110 B each for a one-column integer key).
- **The stop**: every backfill in progress is dropped, not acknowledged, and served again on the next
  start, as before for the one walk; nothing it read is left running.
- **Order**: which index's backfill finishes first is no longer the order they were asked in; a sink
  marks each index seeded when its own backfill completes, as before.

**Expected gain**, restated: initial backfills of several indexes overlap (key reads of one beside builds
and publishes of another, a small index not waiting for a large one's whole walk); builds at once stay
≤ `builds` and key reads ≤ 1. One index: unchanged, at its old cost.

#### 12. Rules, bounds, guarantees, mutation skeleton

| Rule | |
| --- | --- |
| I1 | **One task.** The reader, the dispatcher and a builder per index with lanes are futures of the engine run's task (`together` for N): one `AtomicU64`, the watch's bit, a group bit per future, one `fetch_and` per pass, ended futures masked, a rotating start, at most 64 future polls per task poll, a cut pass's groups raised again. Builds and key reads run on tasks of their own, aborted with their futures. |
| I2 | **Slices.** Per run the reader resolves, marks (with read numbers), counts (release), then hands each touched builder its slice in read order, or, to a folding builder, a `fetch_max` of its fold. A change touching no index is counted only. |
| I3 | **Batches per index.** R3 over the builder's slices with its own last live grant; full at `max_changes` changes; the inbox drained first; full candidates start up to `builds` (6a), a partial one only while no live build of the index runs; no deadline at the end. |
| I4 | **Takes.** Through the candidate's last change, oldest first, at most `max_changes` ids, skipping in-flight and page-held documents; the rest a standing candidate (full at `max_changes`). |
| I5 | **Frontier and routed.** As §3: `F` at each take, and for every other index with lanes at each live grant, at that grant; a lane's committed is the high-water of its newest landed entry's `F` and the catch-up read count (nothing unlanded, nothing waiting, no build unreleased), capped one below the least read the last ended run left unbuilt until a live step closes after it; a sink with no lane has every read; `routed` raised after a run's marks and count, unset at `forget_all`; the `k + 2` record. |
| I6 | **Lead.** The four bounds of §5; the changes bound over builders that don't fold; woken by a take, a grant and a change of held or fold state; 6a's registration-then-reread protocol; a run overshoots once. |
| I7 | **Hold and fold.** Written by the builder: held from a live refusal or a blocked publish to the next live grant or the publish's end; the fold entered with the hold drops the queued slices and ends when nothing waits; folding takes of at most `max_changes` ids as soon as possible, full ones up to `builds`. |
| I8 | **Exempt** (6b-2). Written by the reader: latched when it finds the index held with `max_waiting_per_index` or more waiting, cleared when fewer wait; counted only toward `max_pending_documents`. |
| I9 | **Gate.** G9′: persistent waiters, arms of the builder's loop, one per kind, joined only while a permit could be used, left before any await outside the loop and on a blocked publish; a shrink clears a refusal, a growth keeps it; slice 5's wake-all (G7) until the 24-index measurement says otherwise; G1–G8 otherwise unchanged; G7′ dropped. |
| I10 | **Dispatcher.** Routes requests without waiting (every request ready, each poll; none to a builder gone); confirms once per poll after a registration (raised by `Progress`'s registration signal, beside the fence's listener) and on the tick; reads each builder's result cell; sends the stop once per builder; signals the end (every builder reported idle: its reader ended with its stream, nothing to serve; the request lane empty) and forwards nothing after; returns the first error. A builder acts on the end only while idle itself. |
| I11 | **Backfill.** Coalescing per builder on a deadline; walks at once (6b-3: the walk token of 6b-1c is gone), each key read holding the one K1 permit inside it and, with two or more builders, running on a task of its own, aborted with its walk; rule 4 of the budget per index. |
| I12 | **End and stop.** Builders return only after the end signal; the stream's end builds everything; a stop builds and lands everything read under the deadline or fails with exit 1 naming what is left; no run ends `Ok` with a document waiting. |
| I13 | **Failure.** A builder's error ends the run; the others dropped in place; a panic unwinds the task. |

**Bounds, from the rules:**

- Documents waiting, non-exempt: per index ≤ `max_waiting_per_index` + one run's; together ≤
  `max_waiting` + one run's, except when exempt indexes' latches clear and they rejoin the total with up
  to `max_waiting_per_index − 1` documents each: with `k` of them rejoining while the reader is stopped,
  ≤ `max_waiting − 1 + k · (max_waiting_per_index − 1)` + one run's, for as long as they take to drain
  below it, not just briefly (review rounds 1 and 2 of #452). Exempt, together: ≤ `max_pending_documents` + one run's.
  A frozen exempt index: ≤ the count it stopped growing at + one run's.
- Changes queued on builders: read ahead ≤ `max_waiting` + one run (the global bound); none on a
  folding builder.
- A build: ≤ `max_changes` ids of one index (O2). Builds at once ≤ `builds` (G1); live builds of one
  index ≤ `builds` (6a).
- Key reads at once ≤ 1 (K1); key-read tasks ≤ one per walk (6b-3), none with one builder; keys held per
  walk ≤ `KEY_PAGE` (no read ahead), plus its batch's `max_changes` ids.
- Backfill bytes ≤ half the budget plus one batch per index walking (rule 4 per index, 6b-3).
- Gate waiters ≤ 2 per builder; wakes per gate change: one (G7″).
- Future polls per task poll ≤ 64 (I1).
- What a stop builds: ≤ `max_waiting` + one run, plus the exempt backlogs (≤ `max_pending_documents`),
  under the deadline.
- The frontier record per lane ≤ `k + 2` for `k` unlanded items.
- Memory: built documents per §3.5, with one live batch per index ≤ `max_changes` × its average
  document; pending ids ≤ (`max_waiting` + `max_pending_documents`) × ~300 B (§10).

**Each guarantee, one line:**

| Invariant (CLAUDE.md, §4) | How it holds in 6b |
| --- | --- |
| At-least-once, the watermark | pending minimum per index plus registrations; confirmation coalesced, never early (I10) |
| An emitted, unrouted change is never confirmed or fenced | the reader routes after its marks; a queued slice or a fold is pending |
| Fences per sink and index, two levels | unchanged machinery, from each builder's registrations |
| `changes_committed` | the frontier and the replay window (I5) |
| Built once; build order is delivery order, per index | one builder per index, admission and grant order (I3, I4) |
| Backfill per sink, requests at-least-once, coalesced, tickets | per builder (I11) |
| A live stream that ends drains queued requests | the end signal (I10, I12) |
| Graceful stop | the barrier, everything read built under the deadline or exit 1 (I12) |
| #120 staging order | unchanged |
| Reindex per sink, no restart | a request to the index's builder |
| A lone change commits at once (#135) | per index as 6a; a standing candidate or backlog is granted as soon as possible (I4, I7) |
| `max_delay` caps a batch | per index (I3) |
| Item-level vs flush-wide, parking | sink side unchanged |
| Script failures (#198) | in the builder; `stop` fails the run (I13) |
| Memory budget, gate fairness | persistent waiters, G6 across builders (I9); rules 1–5 |
| Pacing | the lead (I6), the fold (I7), the exemption and `max_pending_documents` (I8) |
| Only indexes with lanes | a builder per index with lanes; marks of others dropped |
| Read-after-visible (#343) | unchanged: in the capture |
| Independence (ADR §1) | at every await of a builder (I1); CPU bounded per poll (§1) |

**Mutation skeleton** (the rule, the mutation, the test written first to fail under it; a row marked
6b-2 or 6b-3 is that slice's; corrected in 6b-1c's fix round 1 to name the tests that exist):

| Rule | Mutation | Test |
| --- | --- | --- |
| I1 | a group's raise polls only its first future | `a_shared_group_polls_every_future_in_it` |
| I1 | the start never rotates | `a_spender_as_the_reader_beside_a_builder_starves_nothing`, `a_spender_as_a_builder_starves_nothing` |
| I1 | no cap on future polls per task poll | `a_poll_of_n_futures_yields_after_64_polls` |
| I1 | the mark dropped by a store after the flags are read | the loom models with three wakers (one shared group, the cut); `a_wake_at_the_end_of_a_pass_is_not_lost` |
| I1 | builders spawned, not polled in the task | `a_build_error_ends_the_run_and_drops_every_builder_in_place` |
| I2 | a slice handed to an index the change didn't touch | `an_untouched_index_is_handed_nothing` |
| I2 | a folding builder handed per-change entries | `a_held_index_rewriting_one_document_holds_no_bound_and_no_memory_per_change` |
| I3 | one deadline shared by every index | no test: structural (each builder owns its `Runs`, so no deadline can be shared) |
| I3 | at most two live builds of an index (F7) | no test of its own; `builds_run_at_once_up_to_the_setting` bounds builds at once (one index) |
| I4 | a take past `max_changes` ids | `a_build_never_takes_more_than_max_changes_documents` |
| I4 | leftovers of a capped take wait for the next change, or never start beside a running build (F2) | `a_capped_takes_leftovers_build_at_once_up_to_builds` (fix round 1) |
| I5 | the frontier at the read count despite documents left waiting | `a_partial_take_counts_committed_only_through_its_oldest_document_left` (a `progress` test until 6b-1c) |
| I5 | a skipped index's entry at the read count, not its frontier | `a_skipped_index_counts_through_its_frontier_not_the_read_count` |
| I5 | the catch-up without the idle check (a build unreleased) | `a_build_taken_and_not_released_holds_its_lanes_count` |
| I5 | no high-water | `a_lanes_count_never_goes_down_when_its_index_marks_again`, `a_sinks_count_never_goes_down_across_a_restart` |
| I5 | no restart cap | `an_ended_runs_reads_count_only_with_the_first_grant_after_the_restart`, `a_replay_from_before_the_committed_point_credits_nothing_before_the_first_grant` |
| I5 | the cap never lifted | `after_the_first_grant_a_change_that_builds_nothing_counts_once_read` |
| I5 | the cap over the whole ended run | `reads_that_built_nothing_count_across_a_restart_with_no_replay` |
| I5 | builds in flight left out of the cap | `a_build_in_flight_at_the_restart_caps_the_count` |
| I5 | reads counted without marks left out of the cap | `reads_cut_before_their_marks_count_no_sooner_than_the_first_grant` |
| I5 | the read count read after the pending state | not testable single-threaded; argued in §3 |
| I5 | a skip appended instead of replacing | `progress::tests::parked` |
| I5 | a sink with no lane at its last step | `a_sink_with_no_lane_has_every_read` |
| I5 (routed) | a lane with nothing outstanding answers through `newest` only | `a_quiet_index_answers_through_what_the_reader_routed`, `a_fence_on_a_quiet_index_passes_while_another_index_builds` |
| I5 (routed) | an index that never had a live item answers `Idle` | `a_quiet_index_pins_no_confirmation_while_another_index_builds` |
| I5 (routed) | routed raised before the run's marks | `routed_moves_only_after_the_runs_marks` (a listener that checks the marks when it fires) |
| I5 (routed) | routed kept across `forget_all` | `a_forgotten_run_answers_through_nothing_it_routed` |
| I6 | a per-index changes bound (F1) | `a_parked_index_changing_many_documents_does_not_stop_the_reading`, `a_held_index_rewriting_one_document_holds_no_bound_and_no_memory_per_change` |
| I6 | an exempt index counted in `max_waiting` (6b-2) | `exempt_documents_count_toward_no_other_bound`, `a_parked_index_changing_many_documents_does_not_stop_the_reading` |
| I6 | no bound on exempt documents (6b-2) | `reading_stops_for_every_index_at_max_pending_documents`, `exempt_documents_stop_the_reading_only_at_max_pending_documents` |
| I7 | the queued slices kept on entering the fold | none: the mutation survives (at most one empty build later; see "6b-1c as built") |
| I7 | the fold ends at the next grant | no test |
| I7 | the backlog in one take | no test of its own (the backlog take shares the capped take's bound) |
| I7 | a blocked publish not held | `an_index_parked_at_its_lane_backstop_folds` |
| I8 | exemption decided in the builder's poll (D4) (6b-2) | `an_index_reaches_its_exemption_with_nothing_else_in_flight`, `holding_a_builder_wakes_a_reader_waiting_at_its_cap` |
| I8 | the latch cleared only when nothing waits (6b-2) | `an_exemption_ends_below_max_waiting_per_index` |
| I9 | waiters leave the line after each pass (old G9, F4) | `no_build_takes_room_by_the_limit_ahead_of_a_waiter_the_limit_refused` across two builders; `a_freed_permit_wakes_a_waiting_builder` |
| I9 | a waiter left in its line across an await outside the loop | `a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line` |
| I9 | a builder joins while its publish is blocked, or keeps a finished build's permit | `a_builder_awaiting_outside_its_loop_holds_no_place_in_a_line` (fix round 1) |
| I9 | a shrink keeps the refusal | `a_waiter_that_grows_keeps_its_refusal_and_one_that_shrinks_drops_it` |
| I9 | a growth drops the refusal | `a_waiter_that_grows_keeps_its_refusal_and_one_that_shrinks_drops_it` |
| I10 | the dispatcher waits on a builder's inbox | `coalescing_for_one_index_delays_no_live_change_of_it` |
| I10 | confirmation before the registering builder ran | `the_dispatcher_confirms_after_the_builder_that_registered` |
| I10 | builders return before the end signal | `a_request_queued_for_one_index_after_live_ends_is_served` |
| I11 | the key read stepped by its walk (6b-3) | `one_index_stuck_mid_walk_does_not_stop_another_indexs_key_reads`, `a_walk_blocked_on_a_full_lane_holds_no_other_walks_key_reads` (6b-3) |
| I11 | the walk token kept (6b-3) | `indexes_walk_at_once` and five more (6b-3) |
| I11 | a builder waiting for the token stops coalescing | gone with the token (6b-3) |
| I12 | an exempt backlog left at the stream's end (6b-2) | `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on`; `the_stream_end_builds_an_exempt_backlog` and `a_stop_builds_and_lands_an_exempt_backlog_under_the_deadline` pin the outcome |
| I12 | a run ending `Ok` with documents waiting | no test of its own; the random schedules check every document landed |
| I12 | a builder returns before the reader's end | `a_stop_mid_run_builds_what_the_reader_marked_last` (several indexes) |
| I13 | the other builders left running after an error | `a_build_error_ends_the_run_and_drops_every_builder_in_place` |
| metrics | per-builder waits | `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it` |

Every existing row of §3.5's and §3.8's tables is re-run on 6b-1c's code.

#### 13. What gets measured at the end, and what I expect

| Where | Expect | Why |
| --- | --- | --- |
| in-process `live_drain_held/*`, `snapshot/rows` (the PR gate; one index) | as 6a, so as `main` | one index is 6a's path, minus the global step lock |
| a new in-process `live_drain_held/indexes3/{1,64,256}` (three indexes, a mock build that yields once), added in 6b-1c, laid over `main` by `review-ab` | better than `main` at 64 and 256; at 1 within the gate | builds of different indexes overlap |
| a throwaway 24-index in-process run (6b-1c, not committed) | G7″ cheaper than wake-all; nothing worse than `main` | R2 |
| scenario `reference` (three indexes), `builds = 4`: burst drain, visible latency p50/p99 | drain up, p50 down | `main`'s batch builds the three indexes one after another inside one `build_many` |
| `reference`, a burst on one index of three | the other two's latency down | independence |
| `reference` backfill documents/s, seed wall (after 6b-3) | up, as far as the pool allows | walks at once |
| scenario `complex` (one index) | flat | overhead only |
| peak RSS, CPU | flat to a few MiB up | per-index `Builds` and queues |
| a stop under steady writes | flat | the same lead |
| `sync-standby`, by hand | flat | the reader is unchanged |

If one index loses to `main` on the gate, it is a regression of that slice to fix before it merges.

#### The owner's questions, decided

All three as recommended (issue #386, the owner's answer of 2026-11-21).

- **O1. "A task per index" becomes "a builder per index, each a future of the ingest engine's task".**
  Asked why not its own task, the owner accepted the answer: 6a measured +54% for a cross-thread
  hand-over per change, and builds, sinks and key reads already run on tasks of their own. What comes
  with it:
  - independence holds wherever a builder waits (a lane, the budget, a permit, a key page, a build),
    not for CPU work on the shared task, which is bounded per poll (at most 64 future polls, each build
    at most `max_changes` documents);
  - until slice 8 a failure or a panic in one builder ends the run for every index;
  - builders keep separate state, so they can move to tasks of their own later, if a measurement with
    many indexes shows the coordination is the bottleneck.

  The ADR 0012 text (Decision and §1) changed in 6b-1c's PR, where it became true: *"One builder per
  index, each a future of the ingest engine's task; its builds and key reads run on tasks of their own.
  Independence holds wherever a builder waits, not for CPU work on the shared task, which is bounded per
  poll. Until slice 8, a failure or a panic in one builder still ends the run for every index."*
- **O2. What `[batch] max_changes` bounds per index: one index's build takes at most `max_changes`
  documents.** A batch still closes after `max_changes` changes or `max_delay`; one setting, no separate
  `max_documents`. A take takes at most `max_changes` documents, oldest first; the rest are a standing
  candidate. Every take (normal, backlog, exempt) follows the one rule. Against `main` with one index, a
  change touching more than `max_changes` documents is built in several builds where `main` built one.
  The manual's `[batch] max_changes`: "a batch groups at most this many changes, and one index's build
  takes at most this many documents".
- **O3. A stop with an exempt backlog.** A stop tries to build and land everything read, an exempt
  backlog included, within `--shutdown-timeout-secs`; if it cannot, it exits 1 naming the index and how
  much was left; it never exits 0 with read changes unbuilt. Decision (b)1's cap at the lead is lifted
  for that backlog only, since leaving it unbuilt makes the restart rewrite every other index's
  documents. Designed for in §8.

## 4. Guarantees

| Invariant (CLAUDE.md) | After 8a | Existing guards | New test |
| --- | --- | --- | --- |
| At-least-once: flush-then-ack, landed is the minimum | lane answer adds the pending minimum; registration after send; token published after every send | `confirms_no_position_before_its_flush`, `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, `progress::tests` (`every_interleaving_answers_as_the_model_does`, `concurrent_registration_and_landing_never_answer_past_a_lane`), daemon `failed_*_restarts_*`, `wal_release` e2e | `a_marked_id_pins_confirmation_until_its_build_lands`; the model test with mark/take/publish/give_back; `an_aborted_publish_leaves_no_phantom_registration`; `a_take_with_no_envelope_still_releases_its_ids` |
| An emitted, unrouted change is never confirmed or fenced | `routed` after a run's marks (built in 6b-1a; until then each lane's `newest`, moved by every grant); the source's `next_seq` test | `ack::tests`' `a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`, `the_tick_releases_idle_wal_while_routed_stays_put` | `a_run_half_marked_confirms_nothing_past_its_first_change` |
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
| Pacing | the lead bound (6a); the held-index exemption and `max_pending_documents` (6b) | `a_stalled_sink_paces_builds_and_they_resume_when_it_does` | `the_reader_waits_at_the_lead_and_resumes`, `a_parked_lane_bounds_pending_memory`, `an_index_held_by_the_budget_does_not_stop_the_reader` (6b; its acceptance case is `a_parked_index_changing_many_documents_does_not_stop_the_reading`) |
| #120 staging order | unchanged | `fresh_source_*`, daemon `fresh_source_rebuilds_seeded_sinks_before_preparing`, `every_index_of_every_sink_is_staged_before_the_source_prepares` | — |
| A lone change commits at once (#135) | `main`'s run cut; a waiting id is taken once a slot is free | `a_lone_change_on_a_quiet_stream_commits_without_waiting`, `max_delay_caps_a_batch_while_changes_keep_arriving` | — (rerun on the reader) |
| Script failures on the ingest side (#198) | in the index task | `skip_policy_leaves_a_failed_document_out_and_keeps_going`, `stop_policy_fails_the_ingest_engine_on_a_failed_document` | slice 8: `a_stop_failure_on_one_index_lets_the_others_land` |
| Item-level vs flush-wide; lane parking | sink side unchanged | `tests::parking`, `tests::merge`, daemon `a_stop_rejection_halts_its_lane_writing_several_items_at_once` | `a_parked_lane_fills_only_its_index_half_and_other_backfills_go_one_batch_at_a_time` |
| Memory budget | many reservers; rules 1–5 per §3.5 (slice 4) | `random_schedules_under_a_tiny_budget_stay_within_it_and_finish`, `live_keeps_room_beside_a_backfill_holding_its_half`, `a_batch_bigger_than_the_budget_still_builds` | `budget::tests` (`concurrent_reservations_never_pass_the_limit`, `a_release_wakes_every_waiting_task`, `a_release_between_the_refusal_and_the_wait_is_not_lost`, `an_index_holding_no_live_bytes_gets_one_live_batch_past_the_limit`, `the_oldest_index_refused_by_the_limit_holds_younger_ones_back`, `every_index_gets_every_byte_back_once`), `one_bursting_index_of_three_holds_its_share_and_the_others_build`, `a_deferred_index_is_never_counted_committed_before_its_documents_land`, `an_index_held_past_its_waiting_bound_is_exempt_and_the_reading_goes_on`, `random_schedules_over_three_indexes_with_one_bursting_stay_within_their_shares` (binds: fails with the share off), `an_index_held_at_its_share_is_one_wait_however_many_builds_pass_it`, `an_index_backfilling_behind_a_slow_sink_keeps_both_live_streams_building`, `an_index_backfilling_keeps_the_live_room_main_gave_it`, `four_indexes_pass_the_limit_by_at_most_a_live_batch_each_and_a_backfill_batch`, `a_hold_across_an_engine_restart_is_one_wait`, `a_wait_open_when_an_engine_run_ends_is_reported_up_to_that_moment`, `a_parked_index_at_its_share_leaves_a_healthy_index_writing_for_ten_minutes`, `a_parked_index_at_its_share_beside_its_backfill_leaves_a_healthy_index_its_batch`, `a_large_batch_and_a_small_one_both_build_beside_a_parked_index` |
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
| 2 | **Pending sets in `Progress`, one task**, and the stall/pin attribution. The one task marks a run, then takes **across every index's set into one build**, as `main`'s batch spans indexes, published per lane. `newest` and `skipped` stay (slice 2 as built); `routed` lands in 6b-1a; `skipped` of other indexes' lanes goes in 6b-1c. | pending unit tests (dedup, mark in flight, give back, the minimum, `(seq, counter)` order); the model test; `a_healthy_lane_is_not_stalled_by_its_index_backlog` | confirmation or fences early: model test, `fence` e2e, `wal_release`; lock cost: `live_drain_held` |
| 3 | **Changes committed from `Progress`** (§3.6, owner's option 1): every lane of a sink landed it, or had nothing for it. Moves when a change counts. One task. | daemon `tests::committed` (red on `main`), `progress::tests::committed` | `/status` and metric values; `drain_changes_per_s` steps once (window restarted) |
| 4 | **Budget for many reservers, per index** (§3.5, as built). Rule 3 (`index_budget_share`, one index's live bytes); a live build takes the indexes that fit and defers the rest, bounded at `2 × max_changes` waiting. Changes what a multi-index deployment holds: one index of three bursting live holds at most its share, the others keep building; an index's backfill or reindex leaves its live stream `main`'s room. One task. | budget tests of §4 | a burst on one index of three (§7); the tiny-budget schedule, and its three-index variant |
| 5 | **Gate with lines** (budget asked at grant), the key-page permit (§3.5, G1–G9 and K1, as built). One task: the lines have no waiter in production until 6a; the task asks `Gate::choose`, which yields only to an eligible waiter, so grants are as before. The budget gains `reserve_behind` and the refusal's reason (G6). | gate tests of §4: each mutation of the rules (G1–G9, K1) in the table under §3.5's bounds fails at least one test, but one shown equivalent | `snapshot/rows` at `builds = 1` and 4 |
| 6a | **Reader task + one builder task** over every pending set, the **lead bound** (`max_waiting_per_index`, `max_waiting`), the stop barrier, the stream handed back. No independence yet; `main`'s pacing. | `the_reader_waits_at_the_lead_and_resumes`, `a_stop_mid_run_builds_what_the_reader_marked_last`, `a_stop_with_many_pending_ids_finishes_within_the_deadline`, `an_id_of_an_index_without_lanes_pins_nothing` | the hop and the locks: the full in-process suite, `shutdown` e2e, the end measurements |
| 6b | **Per-index builders, dispatcher, per-index walks and coalescing**, the held-index exemption and `max_pending_documents`. Designed in §3.9, as five PRs (the reviewer's cut): 6b-1a the frontier, 6b-1b the dispatcher and `together` for N, 6b-1c builders per index (walks one at a time), 6b-2 the exemption, 6b-3 walks at once. | §3.9's mutation skeleton; the independence tests; `indexes_walk_at_once`; `a_task_blocked_on_a_full_lane_holds_no_permit`; the seeded schedule over three indexes | everything; the end measurements (§3.9, 13) |
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
