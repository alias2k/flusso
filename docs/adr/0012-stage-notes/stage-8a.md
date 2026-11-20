# Stage 8a design: one reader, one task per index

**Status:** design, rewritten against `main` at `a187cf91` (2026-11-20). It replaces the note written
before stages 3–7, stage 9's first half (#427), #434 (`engine::Backoff`) and #435 (backfill tickets,
cut backfills) landed. The earlier implementation branch is gone; 8a restarts from `main`.

**The owner's bar:** every slice correct, every guarantee kept, no slower than `main` on a same-runner
comparison. Speed-ups are welcome, not required. Concurrent by default; serial needs a reason.

## 1. The shape

Today one ingest task reads, resolves, batches, builds and publishes for every index, and walks every
backfill one index after another (`libs/2-engine/src/ingest.rs`'s `follow`). After 8a:

```text
LiveStream ─▶ reader ── resolve (own conn) ── mark run ──▶ Progress.pending[index] ──notify──┐
              │  routed, confirm (per run, 1 s tick)                                         ▼
requests ─▶ dispatcher ── Backfill{index,…} ─────────────────────────────▶ index task(index)
                                                         take · build · publish · walk · Complete
                                    shared: Gate (builds permits) · Budget · builder pool · Progress
```

| Task | Owns | Never does |
| --- | --- | --- |
| **Reader** (one) | the live stream, the resolver connection, `routed`, confirmation, the 1 s tick, the pending bound | build, publish, wait on a lane |
| **Dispatcher** (one, the engine's own task) | the request consumer; routes each `Backfill` to its index task; supervises the others | walk, build |
| **Index task** (one per enabled index) | its `Builds` (slots, admission, grant order), its lanes' producers, its walk, its request queue | resolve, confirm |
| `Progress` (shared, outlives runs) | pending sets, floors, item keys, lane registrations and marks | — |

What `main` already has and 8a reuses unchanged: the resolver on its own one-connection pool
(`IngestEngine::with_resolver`, `backends.rs`), the `Gate`'s rule (live first, every fourth contested
grant to backfill), `Builds` (grant-order slots, admission by id hashes), the keyset `Walk` with
8b's segments (`plan_walk`), tickets per lane (#435), one lane per `(index, sink)`, the sink engine as
it is (merging, parking, restaging, fences per lane), `engine::Backoff`.

## 2. The old note against `main`

| Old note | Now | Why |
| --- | --- | --- |
| §1 resolution builder, `with_resolver`, CLI wiring | **done on `main`** | groundwork merged with #422 |
| §1 reader marks per change, one lock per index per change | **changed:** per drained run (§3.1) | `main` cuts batches per drained run; one lock per touched index per run is cheaper and keeps `main`'s batch cut |
| §1 register an item on every lane **before** sending | **dropped:** register after each send, release the floor after all (§3.3) | registering first leaves a phantom registration when a task is aborted between the two: it pins its lane for good |
| §1 per-lane `newest` and `skipped` go | still right | the floor and a global `routed` replace them |
| §2 pending set in `Progress`, mark unconditional, atomic take, give back | still right | fix: order by `(seq, counter)`, never by an id hash (two ids sharing a seq and a hash would merge in a set) |
| §2 `Batch.key`; `position` = the item's oldest | key right; **position changed** | the oldest breaks stdout's "monotonic `seq`" promise; decision (b)2 |
| §3 `Builds` per index, `2 × builds` slots, shared gate | still right | add why grant order stays: admission is released when a build *lands*, which is safe only because publication is in grant order (§3.4) |
| §3 gate with FIFO waiters | still needed | the first attempt (`Gate::acquire`) was reverted with the pending sets (final review M2) |
| §3 a page drops ids waiting or in flight live | **deferred** | not needed for correctness (`main` has none: admission plus grant order already order a page against a live build); an optimisation to measure later |
| §3 pool `builds + 2` → `builds + 1` | **changed:** stays `builds + 1` + resolver | the builder's `+1` serves the walk's light queries (`last_key`, `resume_after`, `compare_keys`); key pages take a permit |
| §3 budget per-index half | still needed, **and more** | `Budget` assumes one reserver ("the check and the add need no lock") and one waiter (`notify_one`): wrong with N index tasks (§3.5) |
| §3 `changes_committed` from `Progress` | still right, now a recommendation | `Batch.changes` has no per-index meaning (§3.6) |
| §3 backfill per index via a dispatcher | still right | now with tickets, cuts, segments and restaging (§3.4) |
| §4 any failure fails the run | right for the main slice | per-index restart is cheap once pending sets outlive tasks: its own slice (§5, slice 9) |
| §5 a stop leaves unbuilt ids for replay | **conflicts with a guard** | `shutdown`'s `…a_restart_rewrites_none_of_it` fails: replay rebuilds landed documents. Decision (b)1 |
| §7 `/status` `indexes.<name>` gains counts | **wrong place** | `indexes` maps a name to a string (`pending`/`backfilling`/`seeded`); decision (b)4 |
| §9 migration in 7 commits | rewritten (§5) | stages 3–7 and 9 changed every file it named |
| §10 tracks A and C, freezes | **dropped** | the tracks are gone |
| §10a, §10b exit criteria vs stage 7 | **rebased on `main`** (§7) | the bar is `main`, same runner |
| §11 open points | answered in §6 | |

**Missing from the old note**, covered below: tickets and cut backfills (§3.4), restaging one lane
(§3.4), parked lanes and the budget (§3.5), the stop across tasks (§3.7), coalescing across sinks
(§3.4), the watermark and `reached` with floors (§3.2), failure policies on the ingest side (§3.4),
a failed index task against the others (§3.7), budget and gate with many callers (§3.5), the fence
listener (§3.2), cancel safety (§3.3), the item key outliving a run (§3.3), the ordered sink (§3.3).

## 3. Design

### 3.1 The reader

A **run** is what `main`'s `drain_ready` buffers: the changes the stream has ready, cut at
`max_changes` changes, at `max_delay` from the run's first change, when the stream would block, or at
the stop. Per run:

1. For each change, in position order: `resolve` on the resolver, then add each id to a local map
   per index, keeping the first (oldest) position per id. `on_change_captured` as today.
2. For each touched index: one lock, `Pending::mark` every id (an id already waiting keeps its older
   position), notify the index task.
3. `routed` = the run's last position (one lock), **after** every mark of the run.
4. Fire the fence listener once; confirm if `Progress::landed()` changed.

A change that resolves to nothing is only routed. On the 1 s tick the reader always confirms (as
`main`). The reader never waits on a lane, a build or the budget. It waits only on the pending bound
(slice 7), in a `select!` over the stop, the tick and the release notify.

A reader failure (source or resolve error) fails the whole engine: the stream must be reopened anyway.

### 3.2 Pending sets in `Progress`

One `Mutex<Pending>` per index inside `Progress` (it outlives every run):

```rust
struct Pending {
    waiting: HashMap<DocumentId, (Position, u64)>,  // oldest unbuilt position, order counter
    order: BTreeMap<(u64, u64), DocumentId>,        // (seq, counter): oldest first
    in_flight: HashMap<DocumentId, Token>,          // live builds not yet published
    builds: HashMap<Token, (Position, Vec<(DocumentId, Position)>)>, // oldest, ids with their take positions
    floor: BTreeMap<Position, usize>,               // multiset: each waiting id, each build's oldest
    next_key: u64,                                  // the item key counter (§3.3)
}
```

| Operation | Who | Under the index lock |
| --- | --- | --- |
| `mark(ids)` | reader | add each id at its position unless waiting at an older one; **also when in flight** (that build may have read before the change) |
| `take(max) -> Taken` | index task | oldest first, skip in-flight ids, move up to `max` to `in_flight` under a new token; the build's oldest replaces the ids' own floor entries |
| `published(token)` | index task, after every lane's send | drop the token's ids from `in_flight`, its entry from `floor`; wake the task if one waits again |
| `give_back(token)` | a failed build; every token at a run's start | each id back to `waiting` at `min(take position, newer mark)` |

**The lane answer** (what confirmation and fences read):

| Lane state | Answer |
| --- | --- |
| an unlanded item, or the index's floor non-empty | `Before(min(oldest unlanded, floor min))` |
| neither, something routed | `Through(routed)` |
| nothing routed | `Idle` |

**Read order, which keeps every answer conservative.** Writers: the reader marks, then raises
`routed`; an index task registers on each lane, then releases the floor. A reader of `Progress` takes
`routed` first, then each index's floor, then each lane's registrations, and never holds an index lock
while taking the registry lock. So a routed change it sees is in a floor, registered, or landed.

**`reached` and confirmation** are unchanged at the port: the source still gets one `Landed` case and
its `next_seq` test still covers an emitted, unrouted change. A quiet index has an empty floor and no
unlanded item, so it answers `Through(routed)` and pins nothing.

**The fence listener** (`Progress::on_registered`) fires after each publish's release, after each
reader run and on the tick: the three moments an answer can move without a sink mark.

### 3.3 Lane items: key and position

Live items can't be keyed by "the batch's last position" any more: takes skip in-flight ids, so an id
skipped once is taken later with an older position.

- `Batch.key: Option<u64>`: the item's number on its index's lanes, from `Pending::next_key`, taken at
  publish (grant order). In `Progress`, so a restarted task never reuses a key still unacknowledged on
  a lane. `None` for snapshot items.
- `Progress` registers `(key, oldest)` per lane; `applied(lane, key)` / `searchable(lane, key)`; the
  sink's `Fences` order by key. `Observer::on_position_applied` keeps a `Position` (the item's oldest).
- `Batch.position`: what the envelopes carry and an ordered sink orders by. Recommended: the newest
  position marked on the index when the batch was taken (non-decreasing per lane). Decision (b)2.
- **Registration after each send succeeds** (as `main`), the floor released after every lane's send.
  Cancel-safe: a task aborted mid-publish leaves its ids in flight, given back at the next start, and
  no lane registered with an item it never got. A full lane then holds its index's floor, so it pins
  its sibling lanes of the same index too; it happens only at the channel's `LANE_BACKSTOP`, which the
  budget keeps out of reach.

### 3.4 An index task

One per index of `DocumentBuilder::backfill_indexes()`. Its loop (`select!`):

| Arm | Does |
| --- | --- |
| pending notified, a build landed, a budget release, a gate grant | grant (below), then publish ready slots in grant order |
| a key page arrived | buffer keys, grant |
| a request (from the dispatcher), no walk running | coalesce, plan, start the walk |
| the stop | §3.7 |

**Live grant.** One live build at a time; a second only while at least `max_changes` ids wait (ADR
§1). Order: a gate permit (await), then `try_reserve` for `min(waiting, max_changes)` ids at the
index's estimate; refused, drop the permit and wait for a release. Then `take` that many. A task never
holds a permit while it waits for the budget.

**Builds per index.** `Builds` moves into the task as it is: `2 × builds` slots per index, admission
by id hashes against running builds, publication in grant order, `SnapshotComplete` as its own slot.
**Why grant order stays:** admission is released when a build lands. A page landing before an earlier
slot waits in the queue; a live build of the same id admitted meanwhile gets a later slot and so
publishes after it. Publishing live builds as they land would need admission released at publish.

**Publish** as `commit.rs` does, to the index's lanes only: live to every lane of the index; pages to
the lanes that asked. `on_changes_settled` and `Progress::skipped` go.

**Script failures (#198)** stay in the task: `skip` reports and publishes nothing for the id; `stop`
halts the task's grants, publishes nothing after the failed slot, and fails the task (§3.7).

**Backfill.**

- **Dispatcher.** Receives each `Request::Backfill` and forwards the delivery, ack handle included, to
  its index's task over an in-process channel. An index the builder doesn't build: warn and ack (as
  `main`). A dropped, unacknowledged delivery returns to the request lane (`ChannelAck`'s `Drop`), so
  a task that dies or abandons its walk loses no request.
- **Coalescing, per index.** Take what is queued, then wait up to `max_delay` while some sink with a
  lane of this index hasn't asked (`main`'s rule, per index). Sinks coalesce into one walk; indexes no
  longer coalesce: each walks in its own task, at once.
- **The walk** is `main`'s `Walk` over one index, with 8b's `plan_walk` segments and `lanes_now`.
  A key page takes a **backfill permit** for its read (ADR §5), so N walks never queue on the pool.
- **Tickets and cuts (#435).** Each lane is answered under the greatest ticket it asked with before the
  walk started. A second request of the same lane arriving mid-walk (a cut asks again from the first
  key) waits for that walk's end, then gets its own walk; the old walk's pages still land and its
  `SnapshotComplete`, naming the older ticket, seeds nothing. No preemption: `main`'s behaviour.
- **Restaging one lane (stage 9 §4)** is a request like any other: only that index's task serves it,
  with no wait on another index's walk.
- **Acknowledgement** after the index's own `SnapshotComplete` is published (stage 7 review 12, now
  per index).

### 3.5 Shared: the gate, the budget, the pool

**Gate.** `builds` permits shared by every index task. `acquire(work) -> Permit` waits in a FIFO per
kind; a freed permit goes to the oldest live waiter, except every fourth contested grant to the oldest
backfill waiter. Cancel-safe: a waiter dropped after its grant hands the permit on.

**Budget**, from one reserver to many:

| Today | 8a |
| --- | --- |
| check then add, "only the ingest engine reserves" | one compare-and-swap loop (or a lock): N tasks never pass the limit together |
| `want_release` + `notify_one`: one waiter | `Notified::enable()` before the check, `notify_waiters()` on a release: every waiting task rechecks |
| an estimate per kind | an estimate per index and kind (stage 5's "per index is where it gets better") |
| rule 4 per kind | per index: an index holding nothing is granted one batch |
| — | rule 2: one index holds at most half. Applied only with two or more indexes, decision (b)5 |

**Parked lanes and the budget.** A parked lane's items keep their charges (stage 9 §3). Its index then
reaches its half and stops building, on every sink; other indexes go on until the other half or the
pending bound fills. That is ADR §5's accepted cost. Stage 9 §6's `Charge::forget` (fresh builds)
comes after 8a and needs `Request::Rebuild` on the index task; 8a leaves the dispatcher arm for it.

**Pool.** Builder `builds + 1`, resolver 1, as `main`. Builds and key pages hold permits; the `+1`
serves the walk's light queries.

### 3.6 Changes committed

`Batch.changes` counted a change on the first item of each sink. Per index it has no meaning (one
change marks several indexes). Recommended: `Progress` keeps routed seqs as ranges (one range while
seqs are dense, as Postgres's are); a sink's `changes_committed` is the routed changes below its
lanes' landed bound, computed when `/status` or the metric is read. Same name, same meaning;
`flusso.changes.committed` becomes an observable counter.

### 3.7 Run lifecycle

| Event | Reader | Index tasks | Engine |
| --- | --- | --- | --- |
| start | — | `give_back` every in-flight token (idempotent) | `prepare`, open live, spawn reader, dispatcher and tasks in a `JoinSet` (aborted with the engine) |
| live stream ends | ends | build every pending id, serve queued requests | `Ok` once the reader ended and every task is idle with the request lane empty (`main`'s rule) |
| stop (#371) | reads nothing more; dispatcher takes no request | abandon the walk (deliveries dropped, so redelivered next start), finish running builds, build and publish what is pending (decision (b)1), return | join, `drain_lanes`, `close_live`, poll the stream to its end (unchanged) |
| index task fails (slices 6–8) | — | the others are aborted | `Err`; the daemon restarts the engine with `[backoff]`, as `main` |
| index task fails (slice 9) | goes on | that task alone: give back, wait its own `Backoff`, restart; its dropped requests come back through the lane | — |
| task panics | — | `JoinSet` error = a failed task | as above (`a_panicking_build_…` keeps holding) |

A stalled index under slice 9 pins confirmation through its floor, as a stalled sink does on `main`.
`advances()` counts confirmations past the furthest and completed walks, as now; slice 9 adds one per
index for its own backoff reset.

### 3.8 Observability

- `on_stage`: the reader reports `Resolve` and `Idle` with no index; an index task `Build`,
  `Publish`, `LaneWait`, `Page`, `Snapshot`, `Idle` with its index (`StageTiming` has the field).
- Per index: ids waiting, ids building, builds running, held bytes; deployment: ids pending in all,
  reader waits on the bound. Names: decision (b)4.

## 4. Guarantees

| Invariant (CLAUDE.md) | After 8a | Existing guards | New test |
| --- | --- | --- | --- |
| At-least-once: flush-then-ack, landed is the minimum | lane answer adds the index's floor; registration after send; floor released after every send | `confirms_no_position_before_its_flush`, `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, `progress::tests` (`every_interleaving_answers_as_the_model_does`, `concurrent_registration_and_landing_never_answer_past_a_lane`), daemon `failed_*_restarts_*`, `wal_release` e2e | `a_marked_id_pins_confirmation_until_its_build_lands`; model test extended with mark/take/publish/give_back; `an_aborted_publish_leaves_no_phantom_registration` |
| An emitted, unrouted change is never confirmed or fenced | `routed` raised after a run's marks; the source's `next_seq` test unchanged | ADR §8's `a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`, `the_tick_releases_idle_wal_while_routed_stays_put` | `a_run_half_marked_confirms_nothing_past_its_first_change` |
| Fences per sink and index, applied and searchable | answers include the floor; sink `Fences` keyed by item key | daemon `fence::tests`, `fence` e2e, `a_position_becomes_searchable_when_its_refresh_interval_passes`, `a_deferred_position_waits_for_a_caught_up_immediate_write` | `a_fence_waits_for_an_id_still_pending_on_its_index`, `a_fence_on_a_quiet_index_passes_while_another_index_builds` |
| Read-after-visible (#343) | unchanged: the hold is in the capture; the reader only consumes | `visibility` e2e, `daemon` `a_fence_never_covers_a_commit_read_before_it_was_visible` | — |
| Built once | one resolve per change, one build per take for every lane; dedup now spans waiting ids too | `builds_a_repeatedly_touched_document_once_per_batch`, `a_backfill_of_related_indexes_builds_each_document_once` | `a_document_marked_while_waiting_is_built_once` |
| Build order is delivery order, per index | take skips in-flight ids (released at publish); admission + grant order per index; keys per index | `a_document_is_never_built_twice_at_once`, `random_schedules_keep_every_document_in_order`, `a_backfill_page_and_a_live_batch_of_one_document_build_one_after_the_other`, sink `a_document_is_never_written_twice_at_once_…` | `a_change_marked_while_its_document_builds_is_built_again_after`; the seeded schedule over several indexes and tasks |
| Backfill requests at-least-once, coalesced | per index: ack after its `SnapshotComplete`; dropped deliveries redelivered | `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`, `a_backfill_with_every_lane_asking_starts_without_waiting`, `a_straggling_reindex_request_…`, `snapshots_go_only_to_the_requesting_lane`, `tests::resume`, `tests::gone` | `each_index_acknowledges_its_own_request_when_its_walk_completes`, `indexes_walk_at_once` (replaces `indexes_are_walked_one_after_another_…`), `a_request_for_one_index_waits_for_no_other_index_walk` |
| Tickets and cut backfills (#435) | per-lane greatest ticket per walk; a mid-walk request waits for the walk | `one_walk_answers_a_lanes_two_requests_under_the_later_ticket`, `a_completion_answering_an_older_request_seeds_nothing`, `a_resumed_request_and_a_fresh_one_…` | `a_cut_request_mid_walk_gets_its_own_walk_after_it` |
| A live stream that ends drains queued requests | engine ends when reader ended, tasks idle, lane empty | `an_unseeded_sink_requests_a_snapshot_then_marks_seeded` (staged before a live stream that ends at once) | `a_request_queued_for_one_index_after_live_ends_is_served` |
| Graceful stop (#371) | §3.7 | `tests::stop` (all eight), daemon `tests::stop`, `shutdown` e2e incl. `…a_restart_rewrites_none_of_it` and `random_stops_under_writes_converge_after_a_restart` | `a_stop_builds_what_every_index_had_pending` (or its opposite, per (b)1); `a_stop_with_one_index_stalled_lands_the_others` |
| #120 staging order | unchanged: the daemon stages, then the engine prepares before spawning tasks | `fresh_source_*`, daemon `fresh_source_rebuilds_seeded_sinks_before_preparing`, `every_index_of_every_sink_is_staged_before_the_source_prepares` | — |
| A lone change commits at once (#135) | the run cut is `main`'s; a waiting id is taken the moment a slot is free | `a_lone_change_on_a_quiet_stream_commits_without_waiting`, `max_delay_caps_a_batch_while_changes_keep_arriving` | — (rerun on the reader) |
| Script failures on the ingest side (#198) | in the index task; `stop` fails the task | `skip_policy_leaves_a_failed_document_out_and_keeps_going`, `stop_policy_fails_the_ingest_engine_on_a_failed_document` | slice 9: `a_stop_failure_on_one_index_lets_the_others_land` |
| Item-level vs flush-wide; lane parking | sink side unchanged; budget per index | `tests::parking`, `tests::merge`, daemon `a_stop_rejection_halts_its_lane_…` | `a_parked_lane_fills_only_its_index_half_and_others_keep_building` |
| Memory budget | many reservers, per-index rules | `random_schedules_under_a_tiny_budget_stay_within_it_and_finish`, `live_keeps_room_beside_a_backfill_holding_its_half`, `a_batch_bigger_than_the_budget_still_builds` | `concurrent_reservations_never_pass_the_limit`, `a_release_wakes_every_waiting_task`, `an_index_holding_nothing_gets_one_batch` |
| Resume checkpoints (8b) | the walk and its marks move as they are | `tests::resume`, `a_killed_backfill_resumes_…` e2e | — |
| Reindex per sink, no restart | sink side unchanged; served by the index's task | `reindex_control_stages_and_requests_a_snapshot_without_restarting`, daemon `reindex_operation_targets_one_sink` | — |
| Independence (ADR §1) | new | — | `a_slow_build_of_one_index_delays_no_other_index`, `a_backfill_of_one_index_delays_no_live_change_of_another`, `positions_stay_bounded_while_one_index_is_stalled` |

## 5. Slices

Each compiles, passes the CI sequence in CLAUDE.md, and merges alone. Each carries its CLAUDE.md
lines; the manual is written by the documentation agent before its PR is ready.

| # | What moves | Tests written first | Could regress; checked by |
| --- | --- | --- | --- |
| 1 | **Item keys.** `Batch.key`, a key counter per index in `Progress`; `Progress` and the sink's `Fences` keyed by it. One task still; `position` unchanged. | `progress::tests` on keys; `a_key_survives_an_ingest_restart` | nothing in behaviour; `live_drain_held` (one more field) |
| 2 | **Pending sets in `Progress`, one task.** `mark`/`take`/`published`/`give_back`, the floor, global `routed`; `newest`/`skipped` go. The one task marks a run then takes at once. | pending unit tests (dedup, mark in flight, give back, floor never above an outstanding change, `(seq, counter)` order); model test extended | confirmation or fences moving early: model test, `fence` e2e, `wal_release`; cost of the locks: `live_drain_held` |
| 3 | **Changes committed from `Progress`.** Routed seq ranges; `Batch.changes` and `on_changes_settled` go. | `changes_committed_counts_what_every_lane_of_the_sink_landed`; status tests | `/status` and metric values: daemon status tests, CLI metric tests |
| 4 | **Budget for many reservers, per index.** CAS reserve, wake-all, per-index estimate, rules 2 and 4 per index. One task still. | `concurrent_reservations_never_pass_the_limit`, `a_release_wakes_every_waiting_task`, the per-index rules | the tiny-budget schedule; `reference` RSS at the end |
| 5 | **Gate with a waiting line.** `acquire` per kind, every fourth contested across callers; key pages take a backfill permit. One task still. | gate tests: two callers, fairness, a dropped waiter hands on its permit | backfill throughput: `snapshot/rows` in-process |
| 6 | **Reader, dispatcher, index tasks.** §3.1, §3.4, §3.7 with "any failure fails the engine". | the independence and stop tests of §4; the seeded schedule over three indexes; `indexes_walk_at_once` | everything: the full in-process suite, the e2e list, the end measurements (§7) |
| 7 | **Pending bound.** `max_pending_documents` (decision (b)3), the reader's wait. | `the_reader_waits_at_the_pending_bound_and_resumes`, `a_stop_while_the_reader_waits_ends_the_run` | none on a healthy run |
| 8 | **Per-index ingest status and metrics** (decision (b)4), the Grafana panel. | status and metric tests | — |
| 9 | **Per-index restart** (8b's other half). Backoff per index; the reader and the others go on. | `a_stop_failure_on_one_index_lets_the_others_land`, `a_restarted_index_task_gives_back_its_builds_and_serves_its_request_again`, daemon backoff tests per index | a poison index looping: backoff tests |

**Independent review.** Slices 1, 4 and 5 touch disjoint files and can be reviewed (and built) in any
order; 2 and 3 follow 1. Slice 6 needs 1–5. Slices 7, 8 and 9 follow 6 and are independent of each
other. Slices 1–5 change nothing a user sees; slice 6 is the one to measure end to end.

## 6. Decisions

### (a) Technical, with a recommendation

1. **Pending sets live in `Progress`**, one lock per index: confirmation and fences read them, and they
   must outlive a task.
2. **Mark per drained run**, not per change: one lock per touched index per run, and `main`'s batch cut.
3. **Register after each send, release the floor after all sends:** cancel-safe (§3.3).
4. **Keep grant-order publication per index:** it is what makes releasing admission at landing safe.
   Publishing live builds as they land is a later optimisation with admission released at publish.
5. **No page dropping in 8a:** not needed for correctness; measure first.
6. **Key pages take a backfill permit; the pool stays `builds + 1` plus the resolver.**
7. **The gate waits in FIFO lines; no permit is held while waiting for the budget.**
8. **The budget takes many reservers** (CAS, wake-all, per-index estimate).
9. **`changes_committed` is computed from `Progress`.**
10. **A failure fails the engine until slice 9;** the reader's failure always does.
11. **A request arriving mid-walk waits for the walk** (no preemption), as `main`.
12. **The fence listener fires on publish, per reader run and on the tick.**

### (b) For the owner

1. **What a stop does with pending ids.** ADR question 7 was answered "leave them for replay". That
   breaks `shutdown`'s `…a_restart_rewrites_none_of_it` (replay rebuilds landed documents) and
   CLAUDE.md's "the buffered batch is published".
   - (A) Build and publish everything pending when the reader stops, as `main` does with its batch;
     a stalled index then runs into the deadline, as a stalled sink does now. **Recommended.**
   - (B) Leave them for replay, change the guard: restarts rewrite documents.
2. **What stdout's `seq` and order promise after 8a.** Indexes publish independently, so "across
   indexes the lines keep position order" no longer holds.
   - (A) Order per index only; `seq` = the newest change the index had marked when the batch was
     taken, non-decreasing per index. **Recommended.**
   - (B) `seq` per document: its oldest unbuilt change. Precise, not monotonic.
3. **`max_pending_documents`.** ADR: `[concurrency] max_pending_documents`, `--max-pending-documents`,
   `FLUSSO_MAX_PENDING_DOCUMENTS`, default 1,000,000 (~150–250 MiB at worst).
   - (A) As the ADR says. **Recommended.**
   - (B) A fixed constant, no key.
4. **Names for per-index ingest state.** `/status` `indexes` already maps a name to a state string.
   - (A) A new `/status` `ingest.indexes.<name>`: `{pending, building, builds, held_bytes}`, plus
     `ingest.pending` in all; metrics `flusso.index.pending{index}`, `flusso.index.building{index}`,
     `flusso.index.held_bytes{index}`, `flusso.reader.waits`. **Recommended.**
   - (B) Turn `indexes.<name>` into an object: breaking for `/status` readers.
5. **The per-index half of the budget with one index.**
   - (A) Only with two or more enabled indexes: a one-index deployment keeps `main`'s whole budget.
     **Recommended.**
   - (B) Always, as the ADR reads: a one-index deployment holds half of `max_built_mib`.
6. **Per-index restart in this series (slice 9) or in a separate 8b PR.** Recommended: in this series;
   it is small once pending sets outlive the task, and it is what makes one index's failure its own.

## 7. What to measure at the end

Nothing here was measured. After slice 6 (and again after the series), on the same runner, `main`
against the branch, interleaved:

| Where | What | Why it could move |
| --- | --- | --- |
| in-process `engine` `live_drain_held/sinks{1,2}/{1,64,256}` (the PR gate) | changes/s | one more hop (reader → index task), the pending locks, per-run notifies; `/1` shows the per-batch cost most |
| in-process `engine` `snapshot/rows` | rows/s | a walk in its own task, a permit per key page |
| Docker `engine` `pipeline` (`change/item_update`, `change_burst`), via `just review-ab` | latency, burst | real resolve and build under the new hop |
| scenario `reference`, `default` scale, `builds = 4`, `writes = 4` | visible latency p50/p99 | the hop and the wake per run |
| same | burst drain, writes and documents per write | batches are now cut per index; stage 7's criterion: documents per write within ~15% of `main` |
| same | backfill documents/s, seed wall | the three walks run at once; expected up, must not go down |
| same | peak RSS, CPU | pending sets, `2 × builds` slots per index, N walks |
| scenario `complex` (one index) | latency, backfill, RSS | no gain possible: shows the pure overhead, and rule 2 with one index ((b)5) |
| scenario `sync-standby`, by hand | latency during a hold | the reader behind the visibility hold |
| `/metrics` stage attribution | `Resolve` busy share on the reader; `Build`/`Publish`/`Idle` per index | where a regression comes from |

Sanity only, expected flat: `pgoutput`, `resolve_no_io`, `documents`, `render`, `scripted`.
