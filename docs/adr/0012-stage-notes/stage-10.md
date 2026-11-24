# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status: design only, for the owner's decisions (§13). Nothing built.**

**ADR 0012 §5:** "Capping those costs by dropping and rebuilding one index is deferred to its own issue and design."
Issue #394 holds the holes three review rounds found and, in its comment, the owner's six decisions of 2026-11-19.
This note designs against `main` at `8a4053b6` (after stage 9's second half and #471). Where it says how `main`
behaves, it was read in the code named; §14 lists what was not verified.

## 0. The owner's decisions this starts from (not reopened)

1. **Detach past a threshold, on by default, with an opt-out.** Flusso stops holding the slot for the stuck
   index, marks it detached and in need of a rebuild in `/status`, and raises an alert louder than the stalled-lane
   one. Other indexes and sinks are unaffected.
2. **The threshold measures what hurts:** time stuck, or WAL held. Not a count of errors.
3. **Rebuild automatically once the index accepts writes again,** by backfill into a new generation while the old
   one keeps serving reads.
4. **Flap protection:** a failed rebuild returns the index to detached; the next attempt waits twice as long, up to
   about an hour.
5. **A setting makes the rebuild wait for an operator;** `reindex` forces one at any time.
6. **A `backfill = false` sink cannot be rebuilt by flusso:** its index stays detached and reported until an
   operator deals with it.

Left open, answered here: default thresholds (§10, O1); what `/wait` answers (§9, O2); `/readyz` (§9, O3); the
alert and its runbook (§9, O4).

## 1. What exists on `main` that this builds on

- **Confirmation is the minimum over every lane** (`progress.rs`): `Progress::landed()` takes `over(self.marks.keys())`,
  every `(index, sink)` lane, each answering `Before` its oldest unlanded item, a batch being published it hasn't
  taken (`sent_to`), or its index's **backlog** for that sink (`pending.rs`'s `backlog_for`: documents waiting or in a
  build not yet registered on that sink's lane). `pinned_by()` is the same computation. A lane the batch has
  nothing for is told so (`skipped`), and answers `Through`.
- **The pending set is per index, shared by every sink of the index:** a document is built once and published to
  every lane of its index (stage 8a). A lane that is full holds its index's builder (`held_by: lane`), so today a
  parked lane on sink A eventually stops the index for sink B too, bounded by the lane's capacity and the budget's
  exemption rules (#452).
- **Lane health** (`sink_engine/health.rs`, stage 9): `open`; `parked` (`refused`, `blocked`); `restaging` (target
  gone; owed reindex `LaneState.reindex`; left out by staging `LaneState.unensured`, "could not be ensured" or "not
  tried"). A lane may hold items for a seed (`held_for_seed`, X2), have a fresh build asked (`rebuild`), an F9 map
  (`superseded`), and an outstanding backfill request (`outstanding`, with `cut`) whose walk checkpoint belongs to
  that one backfill (R5, §18 of the stage 9 note; #477 open for the destination-side hole).
- **Stall age and WAL behind** (`daemon/stall.rs`, stage 8c): sampled once a second per lane through
  `Progress::own_oldest`; age resets whenever the lane's oldest item changes, so **a lane that acknowledges never
  ages**; WAL behind = slot lag − (reached(lane) − reached(landed)); `pinned_by` in `/status`. Backfill items carry
  no position and age nothing.
- **Reindex machinery** (stage 9 §15, X1–X22): `SinkControl::Reindex`, `stage_reindex`/`settle_reindex`, owed
  retries on the restage backoff, the stop table of §15.3, and the OpenSearch sink's `reindex` (meta names the next
  generation unseeded; the aliases stay on the serving generation until `mark_seeded` swaps them and drops the old).
  **It is a no-op while the index is unseeded** (`sink_impl.rs`'s `reindex`: "nothing to stage").
- **Seeding is read from the sink** (`Sink::seeding` → `Seeded` / `Unseeded { walked }`), so a restart learns from
  the destination, not from memory, whether an index needs a backfill.
- **The fence** (`daemon/fence.rs`, #379): per lane `Serving::{Pending, Writes, Backfilling, Rebuilding}`;
  `rebuilding` freezes `through` at what the serving generation had and answers `503 rebuilding` until `seeded`.
- **Readiness** (`status.rs` `is_ready`): the phases only; a parked or restaging lane leaves its sink `live`.
- **Alerts** (`dev/prometheus/rules/flusso.rules.yml`): `FlussoLaneStalled` (age > 10 min, or > 1 GiB behind and
  aged > 1 min, `for: 5m`), `FlussoLaneParked` (`for: 5m`), `FlussoIndexBacklog`; all `severity: page`.
- **`max_slot_wal_keep_size` is read nowhere** today.

## 2. The rules

| # | Rule |
| --- | --- |
| D1 | **Detached is a lane state.** A new `LaneHealth::Detached`, beside `open`, `parked`, `restaging`: per `(index, sink)`, run-outliving like the others. The same index on another sink is untouched. |
| D2 | **Only a stuck lane can detach.** Eligible only while its stall age (stage 8c: its own oldest item unchanged) is past `[detach] stuck_for`, or past a short floor (10 min, the stalled alert's) and its WAL behind is past `[detach] wal_held`. A lane that acknowledges anything resets its age, so a busy index under a bulk update is paced, never detached, whatever WAL it holds. |
| D3 | **Recorded at the sink before it takes effect.** The detach is written durably by the sink (`Sink::detach`, §7) before the lane leaves the confirmation minimum. A sink that can't record it (unreachable, refusing the meta write, or a sink with nowhere to record) is **never detached**: the lane stays as it was and the attempt is retried on the lane's backoff, reported in `/status`. |
| D4 | **Then it leaves the minimum.** `Progress::detach(lane)`: the lane no longer counts in `landed()`, `pinned_by()` or a fence over every lane, and its index's backlog no longer counts for that sink. The slot moves on. |
| D5 | **Its items are discarded and their memory released.** Every item of the lane (window, in flight once returned, held for a seed, on the channel, arriving later) is acknowledged unwritten and unapplied; budget bytes go back with the last lane's acknowledgement, as today. Its fresh build ticket, F9 map, held identities, owed reindex and outstanding backfill request are dropped; the request is cut, so no page of it records a checkpoint (R5) or seeds. |
| D6 | **The ingest engine publishes nothing to a detached lane** (it registers the lane as skipped, as for a batch with nothing for it), so a detached lane never fills and never holds its index's builder. An index all of whose lanes are detached is not built at all (O10). |
| D7 | **A rebuild is always a new generation**, staged by the existing reindex path (X1–X22), never a second one. The durable detach record already names the new generation; the rebuild only clears the record and asks for its backfill. |
| D8 | **The lane attaches at the backfill barrier.** The ingest engine attaches the lane (`Progress::attach`) when it serves the rebuild's backfill request, before the first page is read: live publishing resumes from that point in the reader's order, and the pages read the database after it. A change before the barrier is in the pages; one after is published live; one around it is both, per document in build order. |
| D9 | **The rebuild is the probe.** An automatic rebuild starts on the flap schedule (§6); "the index accepts writes again" means the rebuild's own writes land, into a generation that serves no reader. A lane that parks or restages during a rebuild returns to detached at once (recorded again), and the next attempt waits twice as long, up to `retry_max`. No document is written that shouldn't exist: the new generation is either seeded whole or replaced. |
| D10 | **The flap state is durable.** Attempts and the time of the last one are kept in the detach record (§7), so a crash loop doesn't reset the doubling. |
| D11 | **An operator's `reindex` forces a rebuild now**, whatever the schedule or `rebuild = "operator"`. |
| D12 | **A `backfill = false` sink, or `--skip-backfill`, never starts a rebuild.** The lane stays detached and reported (decision 6). On a `backfill = false` OpenSearch sink an operator's `reindex` reattaches the lane live only, accepting the gap, the only meaning a reindex has there (O7). |
| D13 | **Detached is not stuck.** A detached lane has no stall age, is never `pinned_by`, holds no budget bytes and is not `held_by` anything; an index all of whose lanes are detached is neither working nor stuck for #452's exemption, nor for #456's per-index restart. |

## 3. States and transitions

A lane's states after this stage (new ones in bold):

| State | Holds the slot | Written | Recorded at the sink |
| --- | --- | --- | --- |
| open | its unlanded items | yes | seeded or unseeded |
| parked (refused, blocked) | its parked item and everything behind | probes | as before |
| restaging (gone, owed, left out) | its items | no | as before |
| **detaching** | its items (until D4) | no | the record being written |
| **detached** | nothing | nothing (items discarded) | `detached` |
| **rebuilding** | its new items from the barrier | yes, into the new generation | unseeded, the new generation |

`rebuilding` is `open` plus a mark (`LaneState.rebuild_attempt`) that turns a park during it into D9's return; it
needs no new `LaneHealth` variant. `detaching` is a flight like a restage, shown as the state it leaves.

**Into `detaching`,** from any state where D2 holds, at a lane barrier (nothing of the lane in flight, as a restage
waits):

| From | Notes |
| --- | --- |
| parked, refused | its fresh build ticket is dropped; a substitute arriving later is acknowledged and dropped (F8) |
| parked, blocked | the usual case (a write block, a closed index that isn't opened) |
| restaging, target gone | its restage stops being retried |
| restaging, owed reindex | the owed reindex is dropped: the rebuild replaces it |
| restaging, left out (#471) | the `unensured` mark is dropped |
| open, with items held for a seed (X2) | the held items are discarded with the rest; the seed they wait for is cut |
| open, mid-backfill with live items behind a stuck page | the backfill is cut; its pages, arriving later, are discarded |
| open, a write hung | only once it returns (the barrier); a transport hang fails the run first, as today |
| rebuilding | a park returns to `detached` at once (D9), without waiting for D2 |

**Out of `detaching`:** the record written → `detached` (D4, D5); the record refused or the sink unreachable → back to
the state it left, retried on the lane's restage backoff, `/status` naming why (D3).

**Out of `detached`:** a rebuild starts (schedule, or `reindex`) → `rebuilding` once its backfill is asked;
`--skip-backfill`, `backfill = false`, or `rebuild = "operator"` → stays.

**Out of `rebuilding`:** its `SnapshotComplete` seeds → `open` (fence `Writes`); a park or restage → `detaching`
again (D9, attempts + 1).

**Never:** detached → open without a rebuild, except D12's live-only reattach on a `backfill = false` sink.

## 4. The order of recording, and a kill at each step

Detach, on one lane `L` of index `I` on sink `S`:

| Step | What happens | Where |
| --- | --- | --- |
| d0 | the daemon finds D2 true, sends `SinkControl::Detach { index }` (or an operator does) | daemon, memory |
| d1 | the sink engine stops granting `L` and waits for its writes in flight (lane barrier) | sink engine, memory |
| d2 | `Sink::detach(I)`: the sink's meta names the next generation unseeded, `detached: {since, attempts, last}`, no walk checkpoint; the serving generation and its aliases stay | **sink, durable** |
| d3 | `LaneHealth::Detached` stored; observer told; fence `Detached` | engine and daemon, memory |
| d4 | `Progress::detach(L)`: `L` leaves `landed()`, `pinned_by()`, fences over all lanes | memory |
| d5 | `L`'s items acknowledged unapplied as they come; requests cut | memory, channel |

A kill (process gone) at each point, then a restart:

| Killed after | What the restart finds | Correct because |
| --- | --- | --- |
| d0 or d1 | nothing recorded; the lane as it was (still stuck) | the slot never passed `L`'s changes; D2 fires again |
| d2 | `seeding(I)` answers `Detached`: staging starts `L` detached (d3, d4 at once), asks no backfill | the slot never passed `L`'s changes either; replaying them to a detached lane discards them; the rebuild will backfill |
| d3, d4 or d5 | the same | the slot may have passed `L`'s changes; the record says the index needs a new generation, so no change is lost: the backfill reads the database |

A rebuild of `L`:

| Step | What happens | Where |
| --- | --- | --- |
| r0 | the schedule is due (or `reindex`); the engine stages a reindex of `I` on `S` (X1–X5 path) | memory |
| r1 | `Sink::reindex(I)` on a detached record: clears `detached`, keeps the named unseeded generation, counts the attempt | **sink, durable** |
| r2 | `ensure_index` creates the new generation; `Backfill { I, S }` published; fence `Rebuilding` | sink, stream |
| r3 | the ingest engine serves it: `Progress::attach(L)` at that point in the reader's order, then pages | memory |
| r4 | the backfill completes; `mark_seeded` swaps the aliases and drops the old generation | **sink, durable** |

| Killed after | What the restart finds | Correct because |
| --- | --- | --- |
| r0 | still `Detached` | nothing changed |
| r1 or r2 | an unseeded generation, not detached: ordinary staging asks its backfill and the lane starts attached | the backfill covers every change since; the old generation still serves reads |
| r3 | the same | the same |
| r4 | seeded | done |

**A rebuild that fails** (D9): the lane parks during r2–r4 → d1–d5 again: `Sink::detach` records detached with
`attempts + 1`; the generation built so far is abandoned unseeded (the next rebuild stages yet another, and the seed
sweeps orphans, #121).

## 5. Interactions

- **Ingest side.** A detached lane is skipped at publish (D6), so `held_by: lane` can't come from it. An index all
  of whose lanes are detached: the reader marks no ids for it (as for an index no live change touches), so it holds
  no pending ids, builds nothing, and isn't exempt or stuck (#452) or restarted (#456). On attach (r3) the reader
  marks its ids again from that point. The budget: discarded items release their bytes with the last lane's
  acknowledgement, as today; the per-index share of a detached index on one sink is unaffected for its other sinks.
- **Several sinks.** `I` detached on `S` while healthy on `T`: `I` is built and published to `T` as before; the slot
  follows `T`. A fence naming `I` on `T` passes; on `S`, or on every sink, answers `detached`.
- **A whole sink stuck** (every lane of `S`). Unreachable: no record can be written, nothing detaches (D3), the slot
  stays pinned as today. Reachable but refusing writes to its data indexes (a block on them, not on the meta index):
  each lane meets D2 alone and detaches; `S` is rebuilt index by index once it accepts writes (O5).
- **A reindex control while detached** forces the rebuild (D11), bypassing the schedule and `rebuild = "operator"`.
- **An owed reindex** on a lane that detaches is dropped (§3). One on a rebuilding lane is the rebuild's own.
- **R5 and #477.** The cut at d5 stops every page of the old backfill from recording a checkpoint. The durable record
  at d2 has no checkpoint, so #477's late-applied write (a timed-out checkpoint applied after the cut) lands in a
  meta doc that now names another generation: still #477's to close, and harmless here only if #477 ties the
  checkpoint to the generation's identity.

**The stop** (extends the §15.3 table):

| Item \ lane | detaching | detached | rebuilding |
| --- | --- | --- | --- |
| live item | the record write in flight finishes; recorded → discarded (as detached); not → the lane's previous row of §15.3 | discarded (acknowledged unapplied) | written as usual (the lane is open) |
| backfill page | `skip_backfill` | discarded | `skip_backfill`, index left unseeded |
| `SnapshotComplete` | cut, seeds nothing | discarded | as open |
| a rebuild due | not started once stopping | not started | — |

A process restart in each state: §4's tables.

## 6. The rebuild schedule

- **First attempt:** `retry_initial` after the detach (O11), then doubling to `retry_max` (about an hour, decision 4)
  after each failed attempt. Attempts and the last attempt's time are in the record (D10): a restart resumes the
  schedule; it never restarts it at the first wait.
- **`rebuild = "operator"`:** no automatic attempt; `reindex` (or a future `flusso rebuild`) starts one.
- **A failed attempt** is D9's park; a rebuild whose own staging fails is an owed reindex (X15) and retries on the
  restage backoff while the lane stays detached.

## 7. Port changes

- **`Sink::detach(&IndexMapping) -> Result<()>`** (new, default `Err(Unsupported)`): durably record that the index's
  current target can no longer be trusted: the next staging must create a new target and backfill it; reads keep
  being served from the current one until that seed; any walk checkpoint is forgotten. Unlike `reindex`, it stages a
  new generation **also while the index is unseeded** (a mid-backfill generation that missed changes while detached
  holds rows since deleted: the issue's second hole). OpenSearch: the meta doc gains `detached: {since, attempts,
  last_attempt}` and names `generation + 1` unseeded.
- **`Seeding::Detached { attempts, last_attempt }`** (new variant): what `seeding` answers for such a record, so
  staging restores the lane detached after a restart.
- **`Sink::reindex`** on a detached record: clears `detached` and keeps the named generation (r1).
- **`Progress::detach(lane)` / `attach(lane)`**, and `Progress::is_detached(lane)` for the ingest engine's publish.
- **`SinkControl::Detach { index }`** and `DaemonControl::detach(index, sink)`.
- **`Observer::on_lane_detached(sink, index, why)`, `on_lane_attached`;** the fence's `Serving::Detached` with
  `Unserved::Detached`.
- **`ChangeCapture::wal_limit() -> Option<u64>`** (new, default `None`): Postgres reads `max_slot_wal_keep_size`
  (`-1` = unlimited → `None`), so `wal_held` can default to a fraction of it.

## 8. Configuration

A `[detach]` table in `flusso.toml` (additions only; the format stays frozen for the major), each key with a flag
and env var by the existing rule (`--detach-stuck-for-secs` / `FLUSSO_DETACH_STUCK_FOR_SECS`), flag > env > file:

| Key | Default (recommended) | Meaning |
| --- | --- | --- |
| `enabled` | `true` | decision 1's opt-out |
| `stuck_for_secs` | 14400 (4 h) | stall age past which a lane detaches |
| `wal_held_mib` | half of `max_slot_wal_keep_size`, else 8192 | WAL behind past which a lane stalled ≥ 10 min detaches |
| `rebuild` | `"auto"` | `"operator"` makes the rebuild wait (decision 5) |
| `retry_initial_secs` | 300 | the first rebuild's wait after a detach |
| `retry_max_secs` | 3600 | the cap of the doubling (decision 4) |

Plus `detach = false` on an `[[index]]` entry (beside `on_error`): an index too expensive to rebuild never detaches.
Where each lives (the lock or the machine) is O6.

## 9. Observability

- **`/status`:** lane `health: "detached"`, `reason` (what it was stuck on, and since when), `detached_since`,
  `rebuild: {attempts, next_attempt_at, waiting_for: "schedule" | "operator" | "backfill = false"}`; a `detach`
  sub-object on a lane whose detach couldn't be recorded (`failing: <reason>`); top-level `detached: [{index,
  sink}]`. A detached lane reports no `stall_seconds` or `wal_behind_bytes` and is never `pinned_by`.
- **`/wait` (O2):** a lane in `detached` answers `503` with `reason: "detached"`, both levels; from r2 on it is the
  existing `rebuilding` until seeded. A fence naming every sink, or every index, includes detached lanes and so
  answers `detached`: reads would see a generation that misses changes.
- **`/readyz` (O3):** unchanged. A detached lane leaves its sink `live`, as a parked one does (#476): the process is
  serving and a restart would not help.
- **Metrics:** `flusso.lane.detached{index,sink}` (gauge, 1 while detached); `flusso.lane.detaches{index,sink}`
  (counter); `flusso.lane.rebuild_attempts{index,sink,outcome}` (counter: `seeded`, `parked`).
- **Alert (O4):** `FlussoIndexDetached`, `expr: flusso_lane_detached > 0`, `for: 0m`, `severity: critical` (above
  the `page` of `FlussoLaneStalled`/`FlussoLaneParked`: the slot was let go, changes are not being kept for that
  index); annotations name the index, sink and runbook. `FlussoLaneStalled` stops firing for it (no stall age).
- **Runbook:** `docs/src/operate/stalled-lane.md` is at its size limit. Proposed split (pages not written here): keep
  `stalled-lane.md` for finding the lane and reading its health; move the per-health cures to
  `operate/parked-lane.md` (refused, blocked, closed) and `operate/restaging-lane.md`; add
  `operate/detached-index.md` (what detached means, what was lost and what the rebuild restores, how to force or
  hold a rebuild, a `backfill = false` sink).

## 10. Thresholds: how measured, where evaluated

- **Measured** by `stall.rs`, which already samples every lane's age and WAL behind once a second: no new
  hot-path cost. WAL behind is the slot lag (`ChangeCapture::lag`) less the lane's distance ahead of `landed`.
- **Evaluated** in the daemon, in the same sampling task, which sends `SinkControl::Detach`; the engine never sees WAL
  bytes. One detach per lane at a time; a lane already detaching or detached is skipped.
- **Why it can't fire on a healthy lane:** both thresholds require a stall age (D2), and the age resets whenever the
  lane's oldest item changes. A lane acknowledging slowly under a bulk update holds WAL but ages nothing. A lane whose
  index's documents are pending but not yet published ages nothing either (`own_oldest` excludes the backlog).
- **Why it can't fire on an unreachable sink:** D3; and while the sink is unreachable its run fails and restarts, its
  lanes' items stay unacknowledged, but the record can't be written.

## 11. Tests (red first)

Engine (`sink_engine`, `progress`, `ingest`):

- `progress::a_detached_lane_leaves_the_minimum_and_its_index_backlog_too`.
- `a_detach_is_recorded_before_the_lane_leaves_the_minimum` (a sink fake logging the call order; a refusing
  `detach` leaves the lane as it was and nothing excluded).
- `a_detached_lanes_items_are_discarded_and_their_bytes_released` (window, held for a seed, on the channel, arriving
  later; budget back to zero).
- `a_detached_lane_is_skipped_at_publish_and_never_holds_its_builder` (two sinks; the index keeps going on the other).
- `a_rebuild_attaches_the_lane_at_its_backfill_barrier` (a change before, around and after the barrier).
- `a_park_during_a_rebuild_returns_to_detached_with_the_wait_doubled`; `the_flap_schedule_survives_a_restart`.
- `rebuild_operator_waits_and_reindex_forces`; `backfill_false_stays_detached`; `skip_backfill_stays_detached`.
- the kill walk of §4 at each step, over a sink fake that fails or dies after each call (as the commit-journal and
  stage 9 kill matrices do).
- the stop table of §5.

Daemon: `stall::a_lane_acknowledging_under_a_bulk_update_never_detaches`; `a_stuck_lane_detaches_past_stuck_for`;
`past_wal_held_only_once_stalled_ten_minutes`; `an_unreachable_sink_detaches_nothing`; `fence::a_detached_lane_answers_detached_then_rebuilding`.

Live (OpenSearch, `lane_parking`): `a_detached_index_is_rebuilt_into_a_new_generation_without_rows_deleted_meanwhile`
(rows deleted while detached are gone after the rebuild); `a_restart_while_detached_comes_back_detached`;
`a_closed_index_detaches_and_rebuilds_once_reindexed`.

## 12. Slice plan

Each leaves `main` correct and releasable.

1. **The state, the record, detach by an operator.** `Sink::detach` + `Seeding::Detached` (OpenSearch), D1 and
   D3–D8 and D12–D13, the stop table, fences and `/status`, `POST /detach?index&sink` on the private surface and
   `flusso detach`, rebuild only by `reindex` (D11). Tests: §11's engine set but the schedule, the kill walk, the
   first live test. Nothing detaches by itself yet, so no default changes behaviour.
2. **Thresholds.** `[detach]` with `enabled = true`, `stuck_for_secs`, `wal_held_mib`, `ChangeCapture::wal_limit`,
   the daemon's evaluation (D2), the per-index opt-out, the alert and metrics. Tests: §11's daemon set.
3. **Automatic rebuild and flap protection.** D9, D10, `rebuild`, `retry_*`, the durable schedule. Tests: the
   schedule set and the live restart test.
4. **Not building an index all of whose lanes are detached** (D6's second sentence), if slice 1 measures it worth it:
   until then such an index is built and published to no lane.

## 13. Decisions for the owner

| # | Question | Options | For an operator | Recommended |
| --- | --- | --- | --- | --- |
| O1 | Default thresholds | (a) stuck 4 h, or WAL past half of `max_slot_wal_keep_size` (8 GiB when unlimited) once stalled 10 min; (b) shorter, e.g. 1 h; (c) WAL only | (a) leaves four hours after the stalled page to act before an index is let go and rebuilt; (b) frees WAL sooner at the price of more rebuilds; (c) never detaches a lane stuck on a quiet database | **(a)** |
| O2 | What `/wait` answers for a detached lane | (a) `503 detached`, then `rebuilding` from the rebuild's start until seeded; (b) `rebuilding` throughout | (a) tells "nothing is being kept" from "being rebuilt"; (b) one word for both | **(a)** |
| O3 | `/readyz` | (a) unchanged; (b) not ready while any lane is detached | (a) a pod isn't restarted for a state a restart doesn't cure; (b) a load balancer drops a process that still serves every other index | **(a)** |
| O4 | The alert | `FlussoIndexDetached`, `flusso_lane_detached > 0`, `for: 0m`, `severity: critical` | pages at once, louder than the stalled and parked alerts, once per lane | **as proposed** |
| O5 | Every lane of a reachable sink stuck (writes to its data indexes refused) | (a) each detaches on its own; (b) none, as a sink outage | (a) frees the slot, the sink is rebuilt index by index after (plan maintenance with `enabled = false` or `rebuild = "operator"`); (b) the slot fills and, past `max_slot_wal_keep_size`, every index of every sink is rebuilt | **(a)** |
| O6 | Where `[detach]` lives | (a) the whole table is the machine's, never in the lock (as `[backoff]`), the per-index `detach = false` in the lock (as `on_error`); (b) all in the lock | (a) thresholds follow the database's disk and are tuned per environment with flags; (b) one file decides everything | **(a)** |
| O7 | A sink with nowhere to record (stdout) and a `backfill = false` OpenSearch sink | (a) stdout never detaches (D3: nothing to record); a `backfill = false` OpenSearch lane detaches and is reattached live only by `reindex`, the gap accepted and logged; (b) detach stdout in memory only, forgotten at restart | (a) keeps "recorded before it takes effect" without exception; (b) a stdout consumer silently misses changes across a restart | **(a)** |
| O8 | What a failed rebuild is | (a) its lane parks or restages during the rebuild: back to detached at once; (b) it stalls past the thresholds again | (a) a refused row costs one backfill up to it per attempt; (b) holds the slot hours per attempt | **(a)** |
| O9 | Flap state | (a) in the sink's record (durable); (b) in memory | (a) a crash loop keeps the doubling; (b) every restart rebuilds at once | **(a)** |
| O10 | An index all of whose lanes are detached | (a) not read for live changes until a lane attaches; (b) built and published to no lane | (a) no wasted builds and database reads; (b) simpler, wastes the index's live work | **(a), in slice 4 once measured** |
| O11 | First rebuild attempt | (a) 5 min after the detach; (b) at once | (a) a block lifted soon after costs one backfill; (b) the cause that stuck the lane is usually still there | **(a)** |

## 14. Not verified

- That a cluster-wide write block (`read_only_allow_delete`) also refuses the meta index write, so D3 holds for it
  (expected from OpenSearch's block semantics; not tried).
- `SHOW max_slot_wal_keep_size` on every supported Postgres (13+ has it; 12 and older do not, so `wal_limit` answers
  `None` there).
- The cost of D6's second sentence (not building) against building for no lane; slice 4 measures before building.
- How `FlussoLaneStalled`'s `for: 5m` and the detach at `stuck_for` interact in practice; the alert is expected to
  page hours before a detach with the recommended defaults.

## 15. Not in this stage

- Closing #477 (a checkpoint tied to the generation's identity): separate issue, referenced in §5.
- A config reload: `enabled` and thresholds change with a restart.
- Detaching on any signal but time and WAL (error counts: rejected by decision 2).
- Rebuilding a `backfill = false` sink (decision 6).
