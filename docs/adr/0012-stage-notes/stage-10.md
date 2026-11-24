# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status:** design only, for the owner's decisions in §11. Nothing is built.

- **Revision:** the fourth, rewritten after three independent design reviews of PR #478. The appendix maps every finding to the rule that answers it, or marks it moot.
- **Base:** `main` at `8a4053b6`. A claim about `main` names the code it was read in; §12 lists what was not verified.

**What #394 asks.** ADR 0012 §5 deferred "dropping and rebuilding one index" to its own design. The owner decided on 2026-11-19, and this note does not reopen those decisions:

1. Detach past a threshold. It is on by default, with an opt-out. flusso stops holding the slot for the stuck index, marks the index detached in `/status`, and alerts louder than the stalled-lane alert. Other indexes and sinks are unaffected.
2. The threshold measures time stuck, or WAL held. It never counts errors.
3. Rebuild automatically once the index accepts writes again ("the lane's probe lands"), into a new generation, while the old generation keeps serving reads.
4. A rebuild that fails returns the index to detached. The next attempt waits twice as long, up to about an hour.
5. A setting makes the rebuild wait for an operator. `reindex` forces a rebuild at any time.
6. A `backfill = false` sink cannot be rebuilt by flusso. Its index stays detached until an operator acts.

## 1. The shape: attach only while the index's builder is down

Every hole the three reviews found in the earlier revisions sat in one place: **attaching a lane while ingest runs.** The cause is a build of the index whose content was read before the attach, published to the lane after it. The patches for it were:

- an expected ticket;
- a key floor;
- a `Through(from)` clamp with an end rule;
- one rule at a process start and another inside a process.

**The recommended shape removes the cause.** A lane is attached only while no build of its index is in flight. The steps:

1. **The request.** The rebuild's backfill request carries `attach: Some(attempt)`.
2. **The builder goes down voluntarily.** The index's own builder receives the request. If it has builds in flight, it takes #456's per-index down path, minus the failure and the backoff:
   - it waits for its running builds;
   - every unpublished build goes back to the index's pending set (`Builds::abandon`, `down`);
   - its snapshot and coalescing set are kept, with each lane resumed where it got (`Snapshot::into_requests`).
3. **The attach.** With nothing of the index in flight, the builder:
   - attaches the lane in `Progress` (under the registry lock);
   - publishes a new `LaneItem::Attached { attempt }` on that lane;
   - comes back up and gathers the request as usual.
4. **The sink side.** The sink engine discards everything it receives on the lane before `Attached`, and treats everything after it as the open lane it is.

**Why it is sound.**

- **Only that builder publishes.** Only an index's builder publishes to that index's lanes: live items, pages and `SnapshotComplete` (`commit.rs`). So once it has no build in flight, no item built before the attach can still be sent.
- **Lane order separates before from after.** A build published before the down skipped the lane: it was detached when the build decided its lanes (`commit.rs:648-690`). Builds given back are built again after the attach, from the database as it is then. The lane is FIFO, so `Attached` marks the exact boundary between items from before the attach and items from after it.
- **Page order.** Every page of every request is read after the attach, because requests are gathered after it.
- **The start.** At a process start, staging runs before the ingest engine exists (`supervise.rs`), so no builder runs, and the lane is attached directly. It is the same rule, so one path serves a start and a process that is already running.

**What disappears:**

- the key floor;
- the expected-ticket gate and the rule about which request may attach;
- the `Through(from)` clamp, its end rule, and `from` itself;
- the split between a start and a running process;
- dropping old pages by ticket. Every page read after the attach is correct content, and the existing `answers()` still decides which `SnapshotComplete` seeds.

**What it costs:**

- **The paused index.** Index `I` stops building on every sink while its builder waits for its in-flight builds. That is one build, milliseconds to seconds, once per rebuild attempt.
- **What keeps running.** The reader keeps marking `I`'s documents, and the other indexes keep building and publishing.
- **What doesn't change.** Nothing is replayed, `/readyz` and the phase don't change, and another index's backfill doesn't move.
- **A lone builder.** With one index, the pause is the whole deployment's, for that same moment.

**Alternative, not recommended (O23): attach while the whole ingest engine is stopped.** The ingest engine's run would be stopped, the lane staged as a start does, and the run restarted. The cost:

- every index pauses;
- the unconfirmed tail is replayed: as much WAL as the slowest attached lane holds back, possibly gigabytes;
- `/status` falls back to `starting`, so `/readyz` fails;
- fences answer `503 starting`;
- every other index's running backfill is abandoned with its run, and its request comes back with the `after` it was asked with, not where its walk got (`builder.rs` module doc: dropped at the stop, "served again"). That is hours of another index's backfill lost, unless the stop is reworked to hand requests back resumed, as `down` does.

It gains nothing the per-index shape lacks.

## 2. What exists on `main` that this builds on

- **Confirmation** (`progress.rs`).
  - `landed()` is the minimum over every lane, `Idle` over none (524).
  - Postgres confirms nothing on `Idle` once something was emitted (`ack.rs:25-26`, `122-135`), and takes the maximum of what it is told (223).
  - `forget_all` (run start, `ingest.rs:502`) clears `routed`.
  - A lane answers from its unlanded registrations, `sent_to`, `backlog_for` and `newest`.
  - `register` runs after the lane's own send returns.
  - Item keys are per index.
- **Builders.**
  - Each index with lanes has its own builder.
  - `down` (`builder.rs`) and `Builds::abandon` wait for running builds, drop what they built, give documents back to the index's pending set, and keep the requests resumed.
  - Requests reach their index's builder through `dispatcher.rs`'s `forward`.
- **Sink engine.**
  - A parked lane is not drained (`writes.rs` module doc).
  - `queue` enters a live item into `Fences` as `Writing` before anything else (`writes.rs:1107`), and `advance_applied` stops at the first `Writing` (`fences.rs:171-187`).
  - `answers` is `true` with nothing outstanding (`sink_engine.rs:990`).
  - `LaneState` already has `rebuild: Option<Ticket>`, the fresh-build ticket (`health.rs:226`), and `parked_item` (240).
  - `Parked.since` and `Restaging.since` are reset by `restage()` and passed through `reopen()` (`health.rs:345, 374`).
  - `ParkReason::Refused { ids }` keeps at most 5 rendered ids, cut (`writes.rs:3117`, `sink_engine.rs:1299-1307`).
  - `keys_of` gives an item's `RowKey`s, which are serializable (`writes.rs:3180`, `kernel/src/row_key.rs:22`).
  - `reindex`:
    - ignores `backfill = false` (1128);
    - skips an index only when its backfill is outstanding and its lane is open (1140);
    - `stage_reindex` asks for a backfill whatever `skip_backfill` says (1218).
  - The daemon runs `stage(Resumed)` before every restart of a sink engine's run (`supervise.rs:200`).
- **Staging.** `step` calls `ensure_index`, then `seeding`, for every staging path (906-937).
- **OpenSearch sink.**
  - The seed marker (the meta doc keyed by hash alias) gets a whole-document `PUT` in `ensure_index` (169), `reindex` (634) and `mark_seeded`.
  - Partial updates have no `retry_on_conflict`.
  - `ensure_index` gives a generation aliases only when nothing serves yet (`first`, 162).
  - `mark_seeded` swaps the aliases and drops every other generation in one call.
  - A write for an index with no target goes to the logical name (752).
  - `Meta::read` ignores unknown fields and a `walked` of another generation.
- **Daemon.**
  - The stall age resets whenever the lane's oldest item changes.
  - WAL behind = slot lag − (reached(lane) − reached(landed)).
  - The fence's `wait` returns "not following live changes yet" when any lane's `reached` is `None`, and answers `Unserved` only at the timeout.
  - `is_ready` reads phases only.
  - `max_slot_wal_keep_size` is read nowhere.

## 3. The rules

| # | Rule |
| --- | --- |
| R1 | **Detached** is a lane health, `LaneHealth::Detached`. **Rebuilding** is an ordinary health (open, parked or restaging) plus `LaneState.rebuilding: Some(Rebuilding { attempts })`. The new field is named apart from the existing fresh-build ticket `rebuild`. Every `is_open()` gate then treats a rebuilding lane as the lane it is. The same index on another sink is untouched. |
| R2 | **Eligible to detach:** `enabled`, the index not `detach = false`, the lane parked or restaging now, and either of two halves. **Time:** `now − stuck_since ≥ stuck_for`. **WAL:** the WAL this lane holds back (stage 8c's WAL behind, measured from its own oldest unlanded item) is at least `wal_held` (O25). `stuck_since` is set at the lane's first park or restage, and cleared only once the lane has stayed open with writes landing for `grace`. So a lane flapping between a short open and a long park keeps its clock (H7). An open lane is never eligible. |
| R3 | **A detach happens at a refusal.** The daemon asks with `SinkControl::Detach`. The sink engine detaches at the lane's next probe or restage that the destination answers with a refusal: `Refused`, `Blocked` or `Missing`, never a sink-wide failure. A probe that lands drops the ask. After an outage the lane is probed before anything else, so a lane whose cause went away opens instead (O18). |
| R4 | **Recorded before it takes effect, seed marker first.** `Sink::detach` runs three steps, in this order. **d2b:** an `_update` of the seed marker to the next generation, unseeded, with `walked` and `uuid` null, and `detaching_from: { generation, seeded, uuid }` (the serving generation as it was), with `retry_on_conflict`. Skipped on a `backfill = false` sink. **d2a:** the detach record, created, or updated conditionally on the `seq_no` read at d1. **d2c:** delete the unserved generations other than `serving`, best effort. The lane leaves the minimum only once d2b and d2a both returned `Ok` in this process. A sink that can't record never detaches: stdout keeps nothing. |
| R5 | **The fence is frozen first.** The fence serves `Detached { through }`, frozen at both levels, and `reached()` no longer raises it. Then `Progress::detach(lane)` runs under the registry lock: the lane's registrations and steps are cleared, `register` on it is a no-op, and it is skipped by `least_of`, `pinned_by`, `own_oldest`, `committed` and every fence over all lanes. `forget_all` keeps the detach. **With no attached lane, `landed()` answers `Landed::Free`:** nothing is needed, so Postgres confirms through its horizon. That is a new `Landed` case (A5). |
| R6 | **Discard and release everything of the lane.** <br>• **Acknowledged unwritten and unapplied:** the window's items, the held items (with `holding` decremented), and everything received until `Attached`. The discard is decided before `queue` touches `Fences`, F9, the account, `walked` or the held check. A detached lane is always taken from, so a send blocked on it completes. <br>• **Cleared:** `parked_item`, the fresh-build ticket `rebuild`, F9 and its account, `walked` (reset), `held_for_seed`, the owed reindex with its retry, the index in `pending_reindex`, and the lane's `Fences` entries with `covered_below`. `deferred` is fixed, and `Fences.applied` is kept, since later keys are larger. <br>• **Kept, marked cut:** the outstanding backfill entry. <br>• **The barrier:** the detach waits until nothing of the lane is in flight: writes, probes, refreshes, seeds, records, restages. |
| R7 | **Ingest publishes nothing to a detached lane.** `publish_live` treats it as a lane with nothing built: `register` does nothing, and `settle_build` still runs. A backfill request for a detached lane without the current `attach` is acknowledged, not served for that lane. |
| R8 | **Attach only with the index's builder down (§1).** A request `Backfill { attach: Some(attempt) }` whose `attempt` is the lane's current one in `Progress`: <br>1. the builder of `I`, with nothing of `I` in flight (going down voluntarily if needed), calls `Progress::attach(lane)`; <br>2. it publishes `LaneItem::Attached { attempt }` to the lane; <br>3. it gathers the request. <br>The attach is idempotent: an attached lane is left as it is. At a process's first staging, no builder runs, and staging attaches directly. **Inside a process, `Progress`'s own state decides the lane.** The record decides only the sink side, and `Progress` only at the first staging (H1). |
| R9 | **At most one partial generation.** A rebuild starts only after deleting every unserved generation except `serving`. So does the attach at a start, which also keeps the generation the marker names. |
| R10 | **The rebuild starts when its probe lands (§6)**, on the doubling schedule, or at `reindex`. **It fails** when its lane's `stuck_since` (R2, so flapping counts) passes `rebuild_grace` during it (O17). The lane then detaches again with attempts + 1, the partial is deleted, and the marker names the next generation. |
| R11 | **Records are conditional.** The detach record and the stuck clock are separate documents (§4). Every write is a create, or an update on the `seq_no` last read. A conflict re-reads and decides again, so no late write overwrites a newer state (H2). |
| R12 | **No envelope of a detached index reaches `Sink::write`.** A test guards it. |
| R13 | **`reindex`** on a detached lane forces the rebuild now. On a rebuilding lane, see O20. `backfill = false` refuses `reindex`, as today. `--skip-backfill` blocks automatic rebuilds only. See O26 for what a restart under it does to a running rebuild. |
| R14 | **A `backfill = false` sink** records `live_only: true` and skips d2b. Its marker stays on the serving generation. It is reattached only by the explicit verb (O16). |

## 4. What is recorded

Two documents in the meta index, besides the seed marker:

- **`detach:{hash_alias}`, the detach record:**

  ```json
  { "state": "detached" | "rebuilding", "live_only": false,
    "serving": 3, "why": "blocked: index.blocks.write",
    "refused_keys": [<RowKey>…], "attempts": 2, "last_attempt": "…", "since": "…" }
  ```

- **`stuck:{hash_alias}`, the stuck clock:** `{ "since": "…", "seen_at": "…" }`.
  - Written once the lane has been parked or restaging for `grace`, with `seen_at` refreshed every `grace` while it stays so.
  - Deleted, conditionally, when R2 clears the clock.
  - A park resumes the recorded `since` only if `seen_at` is within `grace` of now. Otherwise the clock starts fresh and the document is rewritten.
  - Inside a process the clock is an `Instant`, plus the recorded offset taken once, so a jump of the wall clock can't shorten it.
  - Nothing is written while `enabled = false` (O22).

**`refused_keys`** are the parked item's `RowKey`s, taken by `keys_of` at d1 while the item is still in the window. They are empty when two keys render alike, and then the rebuild is its own probe.

**Staging reads both documents first,** on every staging path, before `ensure_index`. Rows are classified by whether the marker names `serving`:

| Record | Marker | Staging |
| --- | --- | --- |
| none, and the marker has `detaching_from` | the next generation | d2b's write survived a kill before d2a: restore the marker to `detaching_from`, then stage as today. Never a rebuild nobody asked for, never around an operator's block (H5) |
| none | — | today's staging |
| `live_only` | names `serving` | detached, live only: no `ensure_index`, no backfill |
| `detached` | another generation, unseeded | detached: no `ensure_index`, no backfill |
| `rebuilding` | another generation, unseeded | `ensure_index`. At the first staging of a process: attach directly (R8), run R9's delete, ask for the backfill (its walk resumes from its mark). Under `--skip-backfill` or `backfill = false`: demote (O26) |
| `detached` or `rebuilding` | another generation, seeded | finished, by the seed or by an older flusso: delete the record, then stage as today |
| `detached` or `rebuilding`, not `live_only` | names `serving` | the detach was restored by hand, or died oddly: run `Sink::detach` again, then detached |
| anything else | — | run `Sink::detach` again |

**Demotion** (a rebuilding record under `--skip-backfill`, or a sink switched to `backfill = false`):

- **Not `live_only`:** `Sink::detach` runs again. d2b names a fresh generation, and the walk is forgotten.
- **`live_only`:** the marker is restored to `serving` and the generation's seeded state as recorded.

In both cases the partial is deleted.

`rebuild_stale_seeds` never sees a detached index. The `Seeding` and `Staged` matches name every variant.

## 5. Order and kill walk

### Detaching lane `L` (index `I`, sink `S`)

| Step | What happens |
| --- | --- |
| d1 | `L`'s probe or restage was refused; the barrier (R6); `refused_keys` taken |
| d2b | the marker moves to the next generation, keeping `detaching_from` (durable) |
| d2a | the detach record is written (durable) |
| d2c | the partials are deleted (best effort) |
| d3 | the fence is frozen; `LaneHealth::Detached` |
| d4 | `Progress::detach(L)` |
| d5 | the drain and discard (R6) |

**A kill after each step:**

| Killed after | The restart finds | Why it is correct |
| --- | --- | --- |
| d1 | no record | the slot never passed `L`; it parks again, and the stuck clock resumes from its document |
| d2b | `detaching_from`, no record | the marker is restored; today's staging; nothing was confirmed past `L` |
| d2a, d2c | `detached` | starts detached; the slot never passed `L` in the dead process |
| d3–d5 | `detached` | the slot may have passed `L`; the rebuild reads every row from the database |

- **A d2a that times out** is retried. If the lane opens meanwhile, the record is deleted, conditionally on the `seq_no` it was written with. If that delete fails, the next start finds `detached` and rebuilds once. Under `rebuild = "operator"` that means waiting for an operator: stated in the runbook.
- **A downgrade** ignores both documents. The older flusso finds the marker on the next generation, unseeded, and rebuilds into it. That is correct.

### The rebuild

| Step | What happens |
| --- | --- |
| r0 | the probe landed on schedule, or `reindex` |
| r1a | delete every unserved generation but `serving`; a refused delete fails the attempt, and nothing is read |
| r1b | the marker names a generation that doesn't exist yet, unseeded (already so after d2b) |
| r1c | the record says `rebuilding`, attempts + 1, `last_attempt` (durable) |
| r2 | `ensure_index` creates the generation; a new outstanding ticket; the fence serves `Rebuilding`, keeping `through`; `Backfill { attach: Some(attempts) }` is published |
| r3 | the builder of `I` goes down if busy, attaches `L`, publishes `Attached`, comes up and gathers the request; the sink engine sees `Attached`: `L` is open and rebuilding |
| r4 | `SnapshotComplete`: `mark_seeded` swaps the aliases, drops `serving`, `PUT`s the marker seeded (`detaching_from` goes with the whole-doc `PUT`), then deletes the record |

**An end at each step, then a restart:**

| Ended after | The restart finds | Why it is correct |
| --- | --- | --- |
| r0–r1b | `detached` | nothing started; the schedule resumes from `last_attempt` |
| r1c, r2 (`L` not attached) | `rebuilding`, no walk mark | attached at the first staging; the walk starts from the first key, every page after this attach |
| r3: pages up to `k` landed (mark at `k`); a change `c` to a row up to `k`, published to `L`, not landed; killed | `rebuilding`, mark `k`; `c` replayed, since `L` held the slot from its attach | `L` is attached at staging, so `c` is written. Rows up to `k` were read after the first attach; the resumed walk reads the rows after `k` |
| r3, a graceful stop | the same | `L` is open, so the stop writes its live items (stage 9 §15.3), and items not landed are replayed. The walk is abandoned (`skip_backfill`), and its mark names only the landed prefix (`walked.rs`) |
| r3, a sink engine run restart inside the process (H1) | `Progress`: `L` attached | the record decides only the sink side (`ensure_index` of the rebuilding generation); `Progress` is untouched. A lane still waiting for `Attached` is attached only by its request in the builder (R8) |
| r4, before the record's delete | `rebuilding`, marker seeded on another generation | finished; the record is deleted |

A failed rebuild (R10) runs d1–d5 again.

## 6. The probe

A probe is classified by the cause it meets now, not by the recorded `why`.

- **The write check, `accepts_writes`.** It sends a one-item `_bulk` index op through the hash alias, with `require_alias: true`, `if_seq_no: 0` and `if_primary_term: 1`.
  - The `_id` contains U+0000, which no Postgres `text` value holds.
  - The answer is read by `read_bulk_response`.
  - The check **lands** on `version_conflict_engine_exception` (nothing written). Any other answer is a refusal.
  - OpenSearch checks the global block, the closed index and the index blocks before the shard's version check. A missing target answers `index_not_found`.
  - It is never a delete, which `read_only_allow_delete` allows.
- **For a refused row:** the write check, and then the fresh build of `refused_keys`. That build goes out as a `Request::Rebuild` under a probe ticket and is written alone through the hash alias with `require_alias` (`write_serving`). Both must land: a tombstone lands even under flood stage, so it proves nothing alone (A4).
- **With no keys recorded, or while restaging:** the rebuild itself, which fails at r1 or r2 before reading any page.
- **Fallback, if spike S1 proves the order of checks wrong:** a read-only check of the blocks, the cluster blocks and the open or closed state. Pipelines and permission refusals are then left to the rebuild.
- **A full disk** (`read_only_allow_delete` met now): see O19.

**The schedule.**

- Probes run on the lane's backoff.
- A rebuild starts at the first landed probe at or after `last_attempt + retry_initial × 2^(attempts − 1)`, capped at `retry_max`.
- `attempts` resets only at a seed.
- With `rebuild = "operator"`, probes still run and are shown, but only `reindex` rebuilds.

## 7. Interactions

- **Budgets and #452.**
  - Discarded items release their budget bytes.
  - A detached lane acknowledges at once, so `Outstanding` drops its guards and the index reads as free (`outstanding.rs`, `reader.rs`'s `someone_free`).
  - `index_states`' lane hold ends when the blocked send goes through.
  - The voluntary down holds and folds the index for its moment, as #456's does.
- **Substitutes.** A stale substitute is dropped when read (F8). The refused-row probe reads its own ticket.
- **Several sinks.** The slot follows the healthy sinks. For fences see O14. The builder of `I` pauses on every sink for its down.
- **A whole sink.**
  - A sink that is down has no diagnosed lane, so nothing detaches.
  - A sink that refuses data writes only: each lane detaches on its own, and the slot is freed (R5).
  - Flood stage covers the meta index only when its shard sits on a full node (spike S4).
- **A schema change while detached.** The new hash alias has no record. The old alias's documents and generations are left behind, as old aliases are today (O15).
- **#477.** d2b always names a generation number past every existing one, so a late checkpoint of an abandoned walk is never read.
- **The stream.** The channel stream lives in memory: requests don't cross processes, and `attempt` gates a stale attach request. A durable stream adapter must keep `attempt` checks, since a redelivered old request with a matching attempt number would attach a lane with no rebuild under way. That is a constraint on any future stream adapter.
- **The stop.** Extends stage 9 §15.3:

  | Item \ lane | detaching (d2 in flight) | detached | rebuilding |
  | --- | --- | --- | --- |
  | d2 or r1 in flight | finishes; recorded: detached; otherwise: its previous row | — | — |
  | live item | discarded once recorded | discarded | as open: written; replayed if not landed |
  | backfill page | discarded | discarded | `skip_backfill`; the record stays `rebuilding` |
  | `SnapshotComplete` | cut | discarded | as open |
  | probe landing, rebuild due | not started | not started | — |

## 8. Ports and configuration

**`Sink`** (the defaults change nothing for an existing sink):

- `detachment(&IndexMapping) -> Result<Detachment>`: reads the record, the stuck clock and `detaching_from`; a missing meta index or document reads as none.
- `detach(…) -> Result<Recorded>`: `NotKept` by default.
- `forget_detachment`, `record_stuck` / `clear_stuck`, `begin_rebuild`, `accepts_writes -> Probe` (`Unknown` by default), `write_serving`, `restore_marker`.
- `mark_seeded` also deletes the record.

**Engine:**

- `Progress`: `detach`, `attach`, `is_detached`, `Landed::Free`.
- `Request::Backfill { attach: Option<u64> }` and `LaneItem::Attached { attempt }`.
- The builder's voluntary down, which a lone builder may take too.
- `Fences::forget_lane`.
- `Staged::{Detached, Rebuilding}`.
- `SinkControl::Detach`, not public (O12).

**Fence:** `Serving::Detached { through }`, answered at once; `slowest` skips detached lanes.

**Source:** `ChangeCapture::wal_limit()` reads `current_setting('max_slot_wal_keep_size')`, with `-1` as unlimited.

**Configuration.** `[detach]` is the machine's, as `[backoff]` is: never written into the lock. Each key also has a flag and an env var (`--detach-*`, `FLUSSO_DETACH_*`).

| Key | Default |
| --- | --- |
| `enabled` | `false` until slice 5, then `true` |
| `stuck_for_secs` | 14400 |
| `wal_held_mib` | half of `max_slot_wal_keep_size`, else 8192 |
| `grace_secs` | 600 |
| `rebuild_grace_secs` | 600 |
| `rebuild` | `"auto"` |
| `retry_initial_secs` | 300 |
| `retry_max_secs` | 3600 |

Per index, `detach = false` beside `on_error`. It goes in the lock, **written only when set**, so every existing lock stays byte-identical.

## 9. Observability

- **`/status`.**
  - Per lane: `detached` or `rebuilding { attempts }`, `why`, `since`, `serving: frozen`, `rebuild { earliest, probe, waiting_for }`, `detach { asked, refused }`.
  - Top level: `detached: [{index, sink}]`.
  - A detached lane has no stall age, is never `pinned_by`, and counts in `committed` per O21.
- **`/indexes`** shows the same. **`flusso check`** is unchanged.
- **`/wait` (O2).**
  - A detached lane vouches up to its frozen `through`. Past it, the wait answers `503 detached` at once; from r2 on, `rebuilding`.
  - After a restart `through` is unknown: `detached` at once.
  - A detached lane never turns a wait into "not following live changes".
- **`/readyz`:** unchanged (O3).
- **Metrics:**
  - `flusso.lane.detached`;
  - `flusso.lane.detaches`;
  - `flusso.lane.rebuild_attempts{outcome}`;
  - `flusso.lane.detach_refused`;
  - `flusso.ingest.attach_pauses{index}`, with their length.
- **Alert (O4):**
  - `FlussoIndexDetached`, `max by (index, sink) (flusso_lane_detached) > 0`, `for: 0m`, `critical`;
  - its text: "reads of {index} on {sink} are frozen at the detach; changes and deletes since are not applied".
- **Runbook:** the proposed split of `stalled-lane.md` (at its size limit) into:
  - `parked-lane.md`;
  - `restaging-lane.md`;
  - a new `detached-index.md`, covering what reads see, the rebuild, forcing or holding it, `backfill = false`, a full disk, and a record left behind.

## 10. Tests, spikes and slices

### Tests to write first

**Engine:**

- `progress`:
  - `a_detached_lane_leaves_the_minimum_and_its_backlog`;
  - `with_every_lane_detached_the_source_is_free_even_after_forget_all` (A5);
  - `a_registration_on_a_detached_lane_is_ignored`;
  - `forget_all_keeps_the_detach`.
- `an_attach_waits_for_the_builder_to_have_nothing_in_flight`, with one builder and with several.
- `items_before_attached_are_discarded_and_after_it_written`.
- `a_build_given_back_at_the_down_is_built_again_for_the_lane`.
- `a_sink_run_restart_mid_rebuild_leaves_progress_alone` (H1).
- `a_detach_drains_the_lane`: the blocked send, budget, `holding`, F9, the account, `walked`, `parked_item`, `Fences`. The first item after `Attached` is reported applied.
- `the_outstanding_backfill_is_kept_cut`.
- `a_restart_mid_rebuild_loses_no_change`; `a_graceful_stop_mid_rebuild_loses_no_change`.
- `skip_backfill_demotes`.
- `a_flapping_rebuild_fails_past_rebuild_grace` (H8).
- `nothing_of_a_detached_index_reaches_write`.
- `the_kill_walk` over d1–d5 and r0–r4, for `Fresh` and `Resumed`.
- `staging_classifies_every_record_and_marker_pair`.
- `a_marker_with_detaching_from_and_no_record_is_restored`.
- `record_writes_never_overwrite_a_newer_state`.

**Daemon:**

- `an_open_lane_never_detaches`;
- `the_stuck_clock_survives_reopen_and_flapping`;
- `a_stale_stuck_clock_is_not_resumed`;
- `a_crash_loop_detaches_at_the_first_refusal_past_the_threshold`;
- `the_wal_half_detaches_without_the_time_half`;
- `an_unreachable_sink_detaches_nothing`;
- `fence::a_detached_lane_answers_at_once`.

**Live:**

- `a_detached_index_is_rebuilt_without_rows_deleted_meanwhile`;
- `a_restart_while_detached_comes_back_detached`;
- `a_deploy_mid_rebuild_loses_no_change`;
- `an_operator_block_holds_the_rebuild`.

### Spikes (live OpenSearch or Postgres, no feature behind them)

| Spike | Proves | Gates |
| --- | --- | --- |
| S1 | the write check writes no document and is refused under: a write block, `read_only`, `read_only_allow_delete`, closed, a cluster block, a `fail` or `drop` pipeline, the alias form, a deleted generation, the NUL `_id` | slice 5 |
| S2 | `mark_seeded` drops an old generation that is `read_only`, closed or write-blocked. If the call fails, the seed fails and full backfills loop | slice 3 (an operator `reindex` on a detached index can reach it) |
| S3 | `write_serving` through the alias against a deleted, closed or blocked serving generation | slice 5 |
| S4 | `max_slot_wal_keep_size` read as the deployment's role, and where the meta index's shard sits under flood stage | slice 5 |

### Slices

Each slice is safe if nothing after it is built.

1. **Read the documents; never half-stage.**
   - `stuck` documents are ignored.
   - A `detaching_from` with no record is restored.
   - A `detached` or `rebuilding` record logs loudly and stages as today. That is a correct rebuild, since the marker moved first.
   - A `live_only` record logs loudly and stages as today, and `/status` says a newer flusso detached the index, so a gap exists. Nothing restages forever (H3).
   - Cost: one meta `GET` per index per staging.
2. **Spike S2.**
3. **Detach and rebuild, reachable only from tests,** with `enabled = false`: R1, R4–R9, R11–R14, the voluntary down and `Attached`, `Landed::Free`, the fence.
4. **Spikes S1, S3, S4.**
5. **Thresholds and the probe-gated rebuild,** with `enabled = true`: R2, R3, R10, the stuck clock, the probes, the schedule, the alert.
6. **Runbook and user pages,** written by a docs agent.
7. **O10,** if measured worth it.

## 11. Decisions for the owner

New and changed rows are marked.

| # | Question | Options | For an operator | Recommended |
| --- | --- | --- | --- | --- |
| O1 | Default thresholds | (a) 4 h diagnosed, or WAL held past half of `max_slot_wal_keep_size` (8 GiB if unlimited); (b) 1 h; (c) WAL only | (a) leaves 4 h after the parked page | **(a)** |
| O2 | `/wait` on a detached lane | (a) vouch up to the frozen position, then `503 detached` at once, `rebuilding` from r2; (b) `rebuilding` throughout | (a) tells "nothing kept" from "being rebuilt" | **(a)** |
| O3 | `/readyz` | (a) unchanged; (b) not ready while a lane is detached | (b) restarts a process a restart doesn't cure | **(a)** |
| O4 | Alert | `FlussoIndexDetached`, `for: 0m`, `critical` | pages at once, louder than parked or stalled | **as proposed** |
| O5 | Every lane of a reachable sink refusing data writes | (a) each detaches; (b) none | (a) frees the slot; (b) fills it | **(a)** |
| O6 | Where `[detach]` lives | (a) on the machine, with `detach = false` in the lock written only when set; (b) all in the lock | (a) tuned per environment | **(a)** |
| O7 | stdout | (a) never detaches; (b) detaches in memory | (b) misses changes silently across a restart | **(a)** |
| O8 | Rebuild trigger | (a) the probe (§6); (b) a timer | (b) routes around an operator's block and its seed deletes that generation | **(a)** |
| O9 | Flap state | (a) in the record; (b) in memory | (b) rebuilds at once at every restart | **(a)** |
| O10 | An index detached everywhere | (a) stop reading its changes; (b) build it for no lane | (a) saves builds | **(a)**, later, once measured |
| O11 | First rebuild | (a) at least 5 min after the detach, at a landed probe; (b) the first landed probe | (b) meets the cause again | **(a)** |
| O12 | Operator `detach` command | (a) none; (b) `POST /detach` and `flusso detach` | (b) drops changes on purpose, undone only by a rebuild | **(a)** |
| O13 | Stuck clock across restarts | (a) recorded, with `seen_at`; (b) memory only | (b): restarts faster than the threshold never detach | **(a)** *(changed: `seen_at`, its own document)* |
| O14 | A fence naming no sink or no index | (a) `detached` at once; (b) skip detached lanes | (a) a raw `/wait` with no index fails for every index until the caller names what it reads; (b) passes reads of frozen data | **(a)**, documented |
| O15 | What is left behind | (a) reported, old aliases left as today; (b) swept | (b) deletes data | **(a)** |
| O16 | Reattaching a `backfill = false` lane | (a) `reindex`; (b) `flusso reattach --accept-gap`; (c) `reindex --accept-gap` | **the gap:** deletes in it leave ghost documents for good; updates in it stay stale until the row changes again | **(b)** |
| O17 | A park during a rebuild | (a) fails after `rebuild_grace` on `stuck_since`; (b) at once; (c) after `stuck_for` | (b) loses hours of backfill to a 30 s block | **(a)** *(changed: flapping counts, H8)* |
| O18 | What gates a detach after an outage | (a) the next refusal; (b) time since the destination answered; (c) the run's age | (c) never detaches in a crash loop | **(a)** |
| O19 | Full disk | (a) the rebuild waits for an operator while the probe meets `read_only_allow_delete`; (b) automatic, with a headroom check; (c) automatic | (a): a flood stage longer than `stuck_for` detaches every index on that node, and each waits for an operator **even after OpenSearch lifts the block itself**. Reads stay frozen and the alert stays critical until `reindex`. (c) writes a second full copy onto a full cluster and can block other tenants' indexes | **(a)**, now classified by the live probe *(changed: cost stated)* |
| O20 | `reindex` on a rebuilding lane (decision 5: "at any time") | (a) always ignored; (b) restarts the rebuild; **(c) restarts a rebuild that is parked or restaging, ignored while it is open** | (c) is what an operator types it for: the rebuild is stuck; (b) throws away a healthy backfill; (a) leaves no way out but waiting for `rebuild_grace` | **(c)** *(new option)* |
| O21 | `committed` in `/status` | (a) attached lanes only, with the detached ones listed; (b) frozen at the detach | (b) looks stuck | **(a)** |
| O22 | Stuck clock while `enabled = false` | (a) not written; (b) written | (a) no writes for an unused feature | **(a)** |
| O23 *(new)* | Where the attach happens | (a) with the index's builder down (§1); (b) with the whole ingest engine stopped | (a) pauses one index for one build; (b) pauses every index, replays the unconfirmed tail, fails `/readyz` and fences for the restart, and restarts other indexes' backfills from where they were asked | **(a)** |
| O24 *(new)* | Slice 1 on a record left by a newer flusso after a rollback | (a) log loudly and stage as today (a rebuild; for `live_only`, resume over the gap); (b) restage the index until upgraded | (b) pins the slot until the WAL limit drops it for every index | **(a)** |
| O25 *(new)* | The WAL half | (a) stands alone: the WAL this lane holds back past `wal_held` while it is parked or restaging, no grace; (b) only after `grace` diagnosed; (c) also a hard bound at 90% of `max_slot_wal_keep_size` that ignores everything but R3 | (b) can let the slot be invalidated within the grace on a high-WAL system; (a) detaches a lane briefly blocked on a high-WAL database | **(a)**, with `wal_held` defaulting to half the limit |
| O26 *(new)* | `--skip-backfill` at a start that finds a running rebuild, including one an operator forced | (a) demote: the rebuild and its partial are dropped, the lane is detached; (b) keep it attached and paused, holding the slot | (b) pins the slot for the whole run; (a) loses the rebuild's progress | **(a)**, with a loud log naming it |

## 12. Not verified

- **The spikes:** S1–S4.
- **`Builds::abandon` and a slot mid-publish.** Whether `abandon` waits for a slot that is mid-publish or drops it. Either is safe: its lanes were decided before the attach. The voluntary down must not drop a publish that already registered on some lanes without giving its documents back. To be read in slice 3.
- **A voluntary down of a lone builder.** It is new: a lone builder's failure ends the run today.
- **`flusso check`:** that it reads no sink state.
- **O10's cost.**

## 12a. Not in this stage

- #477, which is harmless here.
- A config reload.
- Detaching on errors.
- Rebuilding a `backfill = false` sink.
- A public detach command.
- Sweeping old aliases.

## Appendix: every review finding

"Moot" means the chosen shape (§1) removes the cause.

| Round | Finding | Answered by |
| --- | --- | --- |
| 1 | A1 a restart erases the record | §4: a record of its own, read before `ensure_index` |
| 1 | A2 a dropped outstanding entry answers everything | R6: kept, cut |
| 1 | A3 pre-detach items in the new generation | §1: `Attached` and the down; the floor is gone |
| 1 | A4 the fence vouches past the old generation | R5 |
| 1 | A5 discarded items pin the slot | R5, R6 |
| 1 | A6 an old request attaches early | R7, R8: `attempt` |
| 1 | A7 the stall age isn't "stuck" | R2 |
| 1 | B1 partials pile up | R9 |
| 1 | B2 timer or probe | §6, O8 |
| 1 | B3 detach on return from an outage | R3, O18 |
| 1 | B4 `backfill = false` writes an unread generation | R14, O16 |
| 1 | B5 where the attach goes | R8 |
| 1 | B6 leftovers | R6 |
| 2 | A1 a restart or stop in `rebuilding` loses changes | §5: attach at the first staging; the stop row |
| 2 | A2 everything detached gives `Idle` | R5: `Landed::Free` |
| 2 | A3 `Fences` keep discarded items | R6 |
| 2 | A4 the fence says "not following" | R5, §9 |
| 2 | A5 the probe recreates the partial | §6: through the alias, `require_alias` |
| 2 | A6 no target, writes to the logical name | R12; slice 1 never half-stages |
| 2 | A7 `since` isn't continuous | R2, §4 |
| 2 | A8 re-asks strand a rebuilding lane | moot (no expected ticket); R1; O20 |
| 2 | A9 flags, uuid, 409s | R13, R4, R11 |
| 2 | B1 a failed d2b leaves a record | R4: marker first; §4 `detaching_from` |
| 2 | B2 unlisted pairs | §4 table |
| 2 | B3 attach idempotent per ticket | moot: an attach with the builder down is idempotent by itself (R8) |
| 2 | B4 the run-start grace | R3, O18 |
| 2 | B5 any park aborts a rebuild | R10, O17 |
| 2 | B6 flood stage | O19 |
| 2 | B7 refused ids after a restart | §4 `refused_keys` (round 3's A4) |
| 2 | B8 the clamp's end | moot: no clamp |
| 2 | C1–C6 silent decisions | O16, O20, O17, O18, O22, O21 |
| 2 | E the probe | §6, S1 |
| 3 | A1 the stop writes, not releases | §5 stop row, §7 |
| 3 | A2 `LaneState.rebuild` exists | R1: `rebuilding` |
| 3 | A3 O20 needs code | O20(c), R13 |
| 3 | A4 ids can't be parsed back | §4 `refused_keys`; §6 both checks must land |
| 3 | A5 `Through(routed)` is `Idle` after `forget_all`; `from` undefined | R5 `Landed::Free`; `from` is gone |
| 3 | H1 in-process restaging re-attaches | moot (no floor), and R8: `Progress` decides inside a process |
| 3 | H2 one record, unordered writers | R11, §4: two documents, conditional writes, `seen_at` |
| 3 | H3 slice 1 pins the slot | §10 slice 1, O24 |
| 3 | H4 `backfill = false` contradicts itself | R14, §4 rows and demotion |
| 3 | H5 the partial and an operator's block after d2b | R9, §4 `detaching_from` restore |
| 3 | H6 the drop list | R6 |
| 3 | H7 the WAL half needs continuity | R2, O25 |
| 3 | H8 a flapping rebuild never fails | R10, O17 |
| 3 | C1–C5 silent decisions | O19, O20, O25, O26, §7 (stream constraint) |
| 3 | D spikes | §10 S1–S4 |
| 3 | E coherence | this rewrite |
| 3 | F the simpler shape | §1, O23 (narrowed to one index) |
