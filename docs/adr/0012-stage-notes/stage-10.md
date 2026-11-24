# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status:** decided by the owner on 2026-11-24. Every decision in §11 is taken, and the slice order in §10 is accepted. Nothing is built.

**Three consequences the owner accepted knowingly** (§11):

- **O14.** While any lane is detached, a `/wait` naming no index fails for every index, until the caller names what it reads.
- **O16.** On a `backfill = false` sink, the gap left by a detach is permanent. Deletes in it leave ghost documents, and updates in it stay stale until their row changes again.
- **O19.** A full disk past the threshold leaves that node's indexes detached until an operator acts, even after OpenSearch lifts the block itself.

- **Revision:** the fifth. Rewritten after three independent design reviews of PR #478, then edited in place after the fourth. The appendix maps every finding to the rule that answers it, or marks it moot.
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

**The decided shape removes the cause (O23).** A lane is attached only while no build of its index is in flight: the index's builder **quiesces**, and it never goes down. The steps:

1. **The attach request.** The sink engine publishes `Request::Attach { index, sink, attempt }`. The dispatcher routes it on the builder's own channel, as `Rebuild` requests are routed (`dispatcher.rs` `Route`), so it never waits behind a walk. The backfill inbox is read only `if snapshot.is_none()` (`builder.rs:421`).
2. **Quiesce.** The builder of `I` stops granting batches of `I`, live and backfill. Its running builds finish and are published, ready slots included, each to the lanes it decides at publish, which exclude the detached lane. Walks of `I` for other sinks keep their place: R7 removed `L` from every walk at the detach.
3. **Attach.** With nothing of `I` granted and unpublished, the builder:
   - calls `Progress::attach(L)` under the registry lock;
   - then publishes `LaneItem::Attached { attempt }` on `L`;
   - then acknowledges the request and resumes granting.
4. **The sink side.** The sink engine discards everything before `Attached` on `L` (H3's rules, R6). On `Attached` the lane becomes open and rebuilding, and only then does the sink engine publish the rebuild's ordinary `Request::Backfill`. So every page is read after the attach.

**Why the quiesce is sound.** A change is in one of three cases:

- **Emitted before the attach.** It is visible to every read after it (#343), and every page of the rebuild is read after the attach, because the backfill is asked only on `Attached`.
- **Already in a build of `I` granted before the attach.** That build was published before the attach. It decided its lanes while `L` was detached, so it skipped `L`. Its content never reaches `L`, and the page for its row, read later, carries the row as it is then. A row deleted meanwhile has no page, and the new generation starts empty, so a missing tombstone leaves nothing behind.
- **Marked, but not granted at the attach.** It waits in the pending set. It is granted after the attach, built from a read after it, and published with `L` among its lanes.

Nothing falls between: a grant is either before the quiesce began, and so published before the attach, or after it, and so built after it. Admission still keeps two builds of one document apart, and slots still publish in grant order (`builds.rs`), so per-document build order holds across the attach.

**The rest of the argument.**

- **Only the current run's builder of `I` publishes to `I`'s lanes.** That covers live items, pages and `SnapshotComplete`; substitutes go to the substitute lane (`commit.rs`, `snapshot.rs:180-197`). The next run's builder is a new object (`ingest.rs:597-600`).
- **The lane is first in, first out,** and redelivers a returned delivery before anything new (`stream/src/queue.rs:15-21`). So `Attached` is the exact boundary, given H3's rules on the sink side.
- **At a process start,** staging runs before the ingest engine exists, so there is no builder, and the lane is attached directly. Its channel is new and empty.

The quiesce gives nothing back, needs no fold and no hold, raises no failure, and is the same for a lone builder. #456's `down` is not reused: on a lone builder it leaves the builder folded for the rest of the run (`builder.rs:180-183, 264, 578-583`; `reader.rs:783`). (W2)

**What disappears:**

- the key floor;
- the expected-ticket gate and the rule about which request may attach;
- the `Through(from)` clamp, its end rule, and `from` itself;
- the split between a start and a running process;
- dropping old pages by ticket. Every page read after the attach is correct content, and the existing `answers()` still decides which `SnapshotComplete` seeds.

**What it costs:**

- **The paused index.** Index `I` grants nothing new while its running builds finish and publish: about one build, milliseconds to seconds, once per rebuild attempt. A build blocked on another sink's full lane lengthens it, bounded by that lane.
- **What keeps running.** The reader keeps marking. The other indexes keep building. A walk of `I` for another sink only pauses its grants.
- **What doesn't change.** Nothing is replayed or given back, and `/readyz` and the phase don't change.
- **A lone builder.** With one index, the pause is the whole deployment's, for that moment.
- **The rebuild's own walk waits behind a walk of `I` that is already running for another sink** (the inbox gate). Meanwhile `L` is attached, writing its live items into the new generation and holding the slot only for them (O27).

**The rejected alternative (O23): attach while the whole ingest engine is stopped.** The ingest engine's run would be stopped, the lane staged as a start does, and the run restarted. The cost:

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
  - Each index with lanes has its own builder, dropped with the run (`ingest.rs:597-600`).
  - Requests reach their index's builder through `dispatcher.rs`'s `forward`. `Backfill` goes to an inbox read only while no snapshot runs (`builder.rs:421`); `Rebuild` has its own channel.
  - `down` folds a lone builder for good, so it is not used here.
  - `publish_live` puts no item on a lane with nothing built: it registers a skip (`commit.rs:687-704`), whatever its doc says.
- **Sink engine.**
  - A parked lane is not drained (`writes.rs` module doc).
  - **Acknowledgements:**
    - `acknowledged` awaits `ack()` inline in the run loop (`writes.rs:2816`).
    - `acknowledge` may hand the handle to a refresh flight instead, when a caught-up write is owed (2713).
    - A held item's acknowledgement waits for the run's next step (`settle_held` → `unacknowledged_held` → `acknowledge_held`).
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
| R2 | **Eligible to detach:** `enabled`, the index not `detach = false`, the lane parked or restaging now, and either of two halves. **Time:** `now − stuck_since ≥ stuck_for`. **WAL:** the WAL the detach would free is at least `wal_held` (O25). The lane must be the one `Progress::pinned_by` names. What it frees runs from its own position, `reached(Before(own oldest))`, to the next answer once it is left out: the next-least lane's, or the source's horizon when there is none. Not the whole slot lag, which also counts WAL not yet read and WAL another sink holds (`stall.rs:198-201`). `stuck_since` is set at the lane's first park or restage. It is cleared only once the lane has stayed open with writes landing for `grace`, or at a detach (d3). So a lane flapping between a short open and a long park keeps its clock. An open lane is never eligible. |
| R3 | **A detach happens at a refusal.** The daemon asks with `SinkControl::Detach`. The sink engine detaches at the lane's next probe or restage that the destination answers with a refusal: `Refused`, `Blocked` or `Missing`, never a sink-wide failure. A probe that lands drops the ask. After an outage the lane is probed before anything else, so a lane whose cause went away opens instead (O18). |
| R4 | **Recorded before it takes effect, seed marker first.** `Sink::detach` runs three steps, in this order. **d2b:** an update of the seed marker, conditional on the `seq_no` read at d1, to the next generation, unseeded, with `walked` and `uuid` null, and `detaching_from: { generation, seeded, uuid }` (the serving generation as it was). A marker that already carries `detaching_from` is left exactly as it is, generation included, so re-running a detach never moves it again (H4). Skipped on a `backfill = false` sink. **d2a:** the detach record, created, or updated conditionally on the `seq_no` read at d1. **d2c:** delete the unserved generations other than `serving`, best effort. The lane leaves the minimum only once d2b and d2a both returned `Ok` in this process. A sink that can't record never detaches: stdout keeps nothing. |
| R5 | **The fence is frozen first.** The fence serves `Detached { through }`, frozen at both levels, and `reached()` no longer raises it. Then `Progress::detach(lane)` runs under the registry lock: the lane's registrations and steps are cleared, `register` on it is a no-op, and it is skipped by `least_of`, `pinned_by`, `own_oldest`, `committed` and every fence over all lanes. `forget_all` keeps the detach. **With no attached lane, `landed()` answers `Landed::Free`:** nothing is needed, so Postgres confirms through its horizon. That is a new `Landed` case (A5). |
| R6 | **Discard and release everything of the lane.** <br>• **Acknowledged unwritten and unapplied:** the window's items, the held items (with `holding` decremented), and everything received until `Attached`. The discard is decided before `queue` touches `Fences`, F9, the account, `walked` or the held check. A detached lane is always taken from, so a send blocked on it completes. <br>• **Acknowledgement order (H3):** <br>  1. A discard is acknowledged **inline**, through `acknowledged` (awaited in the run loop), before the next delivery of the lane is taken. Never through `acknowledge`'s refresh flight, and never through the held items' next-step deferral: held items discarded at the detach are acknowledged at the barrier. <br>  2. `Attached` takes effect, and is acknowledged, only when nothing of `L` is unacknowledged: not in the window, `held`, `settling`, `unacknowledged_held`, a flight or a refresh. Otherwise it waits at the lane's front. <br>  3. `Attached` is matched by `attempt`: an older attempt is discarded; the current attempt on a lane already open and rebuilding is a no-op (H2). <br>• **Cleared:** `parked_item`, the fresh-build ticket `rebuild`, F9 and its account, `walked` (reset), `held_for_seed`, the owed reindex with its retry, the index in `pending_reindex`, and the lane's `Fences` entries with `covered_below`. `deferred` is fixed, and `Fences.applied` is kept, since later keys are larger. <br>• **Kept, marked cut:** the outstanding backfill entry. <br>• **The barrier:** the detach waits until nothing of the lane is in flight: writes, probes, refreshes, seeds, records, restages. |
| R7 | **Ingest publishes nothing to a detached lane.** For a detached lane, `publish_live` does what it does for a lane with nothing built (a skip, no lane item); the skip's `register` does nothing, and `settle_build` still runs. At the detach, `L` is removed from every running walk and coalescing set of `I` (`Snapshot::requested`), and its request is acknowledged with the snapshot's. Another sink's walk goes on. A `Backfill` request for a detached lane is acknowledged, not served for that lane. |
| R8 | **Attach by quiesce (§1).** `Request::Attach { attempt }`, on its own route, whose `attempt` is the lane's current one in `Progress`. The builder of `I`: <br>1. quiesces; <br>2. calls `Progress::attach(L)`; <br>3. **then** publishes `LaneItem::Attached { attempt }`; <br>4. then acknowledges the request. <br>**A request finding `L` already attached for its attempt** publishes `Attached` again and is acknowledged (H2). That is the case of a run that died between steps 2 and 3, whose request comes back. **The reverse order** (publish, then attach) **loses data:** the sink would open `L` and ask for its backfill while `Progress` still skips `L`. A run dying in between lets the slot pass changes that never reach `L` and that pages read before them miss. At a process's first staging, no builder runs, and staging attaches directly. **Inside a process, `Progress`'s own state decides the lane.** The record decides only the sink side, and `Progress` only at the first staging (H1). **At the attach, `L.newest` is set to the newest position routed in this process** (`routed_ever`, kept by `forget_all`). Every change up to it was emitted before the attach, so the rebuild's pages, read after it, cover it. A lane that registered nothing then answers `Through(routed_ever)`, not `Idle`, and a quiet database keeps confirming (H7). |
| R9 | **At most one partial generation.** A rebuild starts only after deleting every unserved generation except `serving`. So does the attach at a start, which also keeps the generation the marker names. |
| R10 | **The rebuild starts when its probe lands (§6)**, on the doubling schedule, or at `reindex`. **It fails** when its lane is parked or restaging at that moment and its `stuck_since` has passed `rebuild_grace`. The clock was cleared at d3 (H1), so it starts at the first park or restage after `Attached`, and flapping keeps it (R2). The lane then detaches again with attempts + 1: the partial is deleted, and the marker names the next generation. |
| R11 | **Records are conditional.** The detach record and the stuck clock are separate documents (§4). Every write is a create, or an update on the `seq_no` last read. A conflict re-reads and decides again, so no late write overwrites a newer state (H2). |
| R12 | **No envelope of a detached index reaches `Sink::write`.** A test guards it. |
| R13 | **`reindex`** on a detached lane forces the rebuild now. On a rebuilding lane, see O20. `backfill = false` refuses `reindex`, as today. `--skip-backfill` blocks automatic rebuilds only. See O26 for what a restart under it does to a running rebuild. |
| R14 | **A `backfill = false` sink** records `live_only: true` and skips d2b, so its marker stays on the serving generation. A sink switched to `backfill = false` while an index is detached or rebuilding gets the same state (H5). Its marker is restored to the record's `serving` with its recorded seeded state and uuid, the record is rewritten with `live_only: true`, and the partial is deleted. The lane is reattached only by the explicit verb (O16), which needs the marker on `serving`. |

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

**Staging reads both documents and the marker first, in one `_mget`,** on every staging path, before `ensure_index`. Only a `404` or a missing meta index means "none". Any other failure fails the staging step, which is retried as today, so a timed-out read is never taken for "no record" (H6). A late d2a from a dead process, landing just after a restore, leaves "`detached` + names `serving`" for the next staging, which detaches again. That is self-healing.

Rows are classified by whether the marker names `serving`:

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

**Demotion:**

- **A rebuilding record under `--skip-backfill`:** the record goes back to `detached`. The marker names a generation past every existing one, conditionally, as an explicit move rather than R4's d2b, which leaves a marker carrying `detaching_from` alone. The walk is forgotten and the partial deleted.
- **A sink switched to `backfill = false`:** R14 applies.
- **Restoring** takes the generation from the record's `serving`, never from `detaching_from` (H4).

`rebuild_stale_seeds` never sees a detached index. The `Seeding` and `Staged` matches name every variant.

## 5. Order and kill walk

### Detaching lane `L` (index `I`, sink `S`)

| Step | What happens |
| --- | --- |
| d1 | `L`'s probe or restage was refused; the barrier (R6); `refused_keys` taken |
| d2b | the marker moves to the next generation, keeping `detaching_from` (durable) |
| d2a | the detach record is written (durable) |
| d2c | the partials are deleted (best effort) |
| d3 | the fence is frozen; `LaneHealth::Detached`; `stuck_since` cleared and the `stuck` document deleted, conditionally |
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
| r2 | `ensure_index` creates the generation; the fence serves `Rebuilding`, keeping `through`; `Request::Attach { attempt }` is published |
| r3 | the builder of `I` quiesces, attaches `L`, then publishes `Attached`, then acknowledges; the sink engine, with nothing of `L` unacknowledged, takes `Attached`: `L` is open and rebuilding, and it publishes `Backfill` under a new outstanding ticket |
| r4 | `SnapshotComplete`: `mark_seeded` swaps the aliases, drops `serving`, `PUT`s the marker seeded (`detaching_from` goes with the whole-doc `PUT`), then deletes the record |

**An end at each step, then a restart:**

| Ended after | The restart finds | Why it is correct |
| --- | --- | --- |
| r0–r1b | `detached` | nothing started; the schedule resumes from `last_attempt` |
| r1c, r2 (`L` not attached) | `rebuilding`, no walk mark | attached at the first staging; the walk starts from the first key, every page after this attach |
| r3: pages up to `k` landed (mark at `k`); a change `c` to a row up to `k`, published to `L`, not landed; killed | `rebuilding`, mark `k`; `c` replayed, since `L` held the slot from its attach | `L` is attached at staging, so `c` is written. Rows up to `k` were read after the first attach; the resumed walk reads the rows after `k` |
| r3, a graceful stop | the same | `L` is open, so the stop writes its live items (stage 9 §15.3), and items not landed are replayed. The walk is abandoned (`skip_backfill`), and its mark names only the landed prefix (`walked.rs`) |
| r3, a sink engine run restart inside the process (H1) | `Progress`: `L` attached | the record decides only the sink side (`ensure_index` of the rebuilding generation); `Progress` is untouched. A lane still waiting for `Attached` is attached only by its request in the builder (R8) |
| r3, an ingest run dying between `Progress::attach` and `Attached` (H2) | `L` attached, no boundary on the lane | the request was not acknowledged and comes back to the next run's builder, which finds `L` attached for this attempt and publishes `Attached` again |
| r3, a sink run dying after acting on `Attached` | earlier deliveries all acknowledged (R6); `Attached` redelivered | a no-op on the open lane; nothing earlier can come back (H3) |
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
- A rebuild starts at the first landed probe at or after `last_attempt + retry_initial × 2^(attempts − 1)`, capped at `retry_max`. With no attempt yet, the earliest start is `retry_initial` after the detach (O11).
- `attempts` resets only at a seed.
- With `rebuild = "operator"`, probes still run and are shown, but only `reindex` rebuilds.

## 7. Interactions

- **Budgets and #452.**
  - Discarded items release their budget bytes.
  - A detached lane acknowledges at once, so `Outstanding` drops its guards and the index reads as free (`outstanding.rs`, `reader.rs`'s `someone_free`).
  - `index_states`' lane hold ends when the blocked send goes through.
  - The quiesce only pauses grants of `I`: no fold, no hold, no exemption change.
- **Substitutes.** A stale substitute is dropped when read (F8). The refused-row probe reads its own ticket.
- **Several sinks.** The slot follows the healthy sinks. For fences see O14. The builder of `I` pauses on every sink for its down.
- **A whole sink.**
  - A sink that is down has no diagnosed lane, so nothing detaches.
  - A sink that refuses data writes only: each lane detaches on its own, and the slot is freed (R5).
  - Flood stage covers the meta index only when its shard sits on a full node (spike S4).
- **A schema change while detached.** The new hash alias has no record. The old alias's documents and generations are left behind, as old aliases are today (O15).
- **#477.** d2b always names a generation number past every existing one, so a late checkpoint of an abandoned walk is never read.
- **The stream.** The channel stream lives in memory, so requests don't cross processes, and `Request::Attach` carries `attempt`, which gates a stale one. **A constraint on any future durable stream adapter:** a redelivered old `Attach` whose attempt number matches would attach a lane with no rebuild under way, so attempts must stay unique per record (they are durable, so they are), and the adapter must keep the in-order redelivery contract (`queue.rs`) that R6's boundary relies on.
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
- `Request::Attach { index, sink, attempt }` on its own route, and `LaneItem::Attached { attempt }`.
- The builder's quiesce: stop granting, let running builds publish, then attach.
- `Progress::routed_ever`, kept by `forget_all`.
- `Fences::forget_lane`.
- `Staged::{Detached, Rebuilding}`.
- `SinkControl::Detach`, not public (O12).

**Fence:** `Serving::Detached { through }`, answered at once; `slowest` skips detached lanes.

**Source:** `ChangeCapture::wal_limit()` reads `current_setting('max_slot_wal_keep_size')`, with `-1` as unlimited.

**Configuration.** `[detach]` is the machine's, as `[backoff]` is: never written into the lock. Each key also has a flag and an env var (`--detach-*`, `FLUSSO_DETACH_*`).

| Key | Default |
| --- | --- |
| `enabled` | `false` until slice 4, then `true` |
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
- `an_attach_quiesces_the_builder_and_attaches_with_nothing_of_the_index_in_flight`, with one builder and with several, during another sink's walk of the index.
- `items_before_attached_are_discarded_and_after_it_written`.
- `a_change_marked_before_the_attach_and_granted_after_reaches_the_lane`.
- `a_run_dying_between_attach_and_attached_republishes_it` and `a_second_attached_of_the_same_attempt_discards_nothing` (H2).
- `a_run_restart_after_attached_never_writes_a_redelivered_earlier_item` (H3).
- `a_rebuild_does_not_inherit_the_detach_clock` (H1).
- `a_rerun_detach_never_moves_the_marker_again` (H4); `a_switch_to_backfill_false_while_detached_restores_serving` (H5).
- `a_failed_read_of_the_record_is_never_none` (H6).
- `an_attach_on_a_quiet_database_keeps_confirming` (H7).
- `a_lone_builder_quiesces_and_keeps_its_batching` (W2).
- `the_wal_half_detaches_only_the_lane_whose_detach_frees_wal` (D1).
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
| S1 | the write check writes no document and is refused under: a write block, `read_only`, `read_only_allow_delete`, closed, a cluster block, a `fail` or `drop` pipeline, the alias form, a deleted generation, the NUL `_id` | slice 4 (run in slice 3) |
| S2 | `mark_seeded` drops an old generation that is `read_only`, closed or write-blocked. If the call fails, the seed fails and full backfills loop. **Reachable on `main` today** through an operator `reindex` of a seeded index whose generation is blocked | issue #479; runs before slice 2 |
| S5 | `Landed::Free` against Postgres: every lane detached, a transaction held for visibility (the `visibility` e2e's standby), and only keepalives arriving. The slot follows the horizon and never passes the held commit | slice 2 (its first test) |
| S6 | engine level, injected failures: an ingest run dying between `Progress::attach` and `Attached` (H2); a sink run killed while a discard's acknowledgement is pending (H3) | slice 2 (its first test) |
| S3 | `write_serving` through the alias against a deleted, closed or blocked serving generation | slice 4 (run in slice 3) |
| S4 | `max_slot_wal_keep_size` read as the deployment's role, and where the meta index's shard sits under flood stage | slice 4 (run in slice 3) |

### Slices

The owner accepted this order on 2026-11-24. Each slice is safe if nothing after it is built. Spike S2 is issue #479 and runs before slice 2.

1. **Read the documents; never half-stage.**
   - `stuck` documents are ignored.
   - A `detaching_from` with no record is restored.
   - A `detached` or `rebuilding` record logs loudly and stages as today. That is a correct rebuild, since the marker moved first.
   - A `live_only` record logs loudly and stages as today, and `/status` says a newer flusso detached the index, so a gap exists. Nothing restages forever (H3).
   - Cost: one `_mget` per index per staging; only a `404` reads as none (H6).
2. **Detach and rebuild, reachable only from tests,** with `enabled = false`: R1, R4–R9, R11–R14, the quiesce, `Request::Attach` and `Attached`, `Landed::Free`, `routed_ever`, the fence. It starts from S5 and S6. It follows #479 (S2).
3. **Spikes S1, S3, S4.**
4. **Thresholds and the probe-gated rebuild,** with `enabled = true`: R2, R3, R10, the stuck clock, the probes, the schedule, the alert. Gated on slice 3.
5. **Runbook split and manual pages;** a docs agent writes the user pages.
6. **O10,** later, if measured worth it.

## 11. Decisions

Decided by the owner on 2026-11-24. Each row gives the decision and one line on what the rejected alternatives would have cost. Rows marked **accepted knowingly** carry a cost the owner took on deliberately.

| # | Question | Decision | Rejected, and what it would have cost |
| --- | --- | --- | --- |
| O1 | Default thresholds | 4 h parked or restaging (`stuck_for`), or the WAL half of O25 with `wal_held` at half of `max_slot_wal_keep_size` (8 GiB if unlimited) | 1 h: more rebuilds for short outages. WAL only: never detaches on a quiet database |
| O2 | `/wait` on a detached lane | vouch up to the position frozen at the detach, then `503 detached` at once; `rebuilding` from r2 until the seed | `rebuilding` throughout: "nothing kept" and "being rebuilt" can't be told apart |
| O3 | `/readyz` | unchanged | not ready while a lane is detached: restarts or drops a process that a restart doesn't cure |
| O4 | Alert | `FlussoIndexDetached`, `max by (index, sink) (flusso_lane_detached) > 0`, `for: 0m`, `severity: critical` | — |
| O5 | Every lane of a reachable sink refusing data writes | each lane detaches on its own | none detach: the slot fills, and past the WAL limit every index of every sink is rebuilt |
| O6 | Where `[detach]` lives | on the machine (`#[serde(skip)]`, flags and env vars); per index, `detach = false` in the lock, written only when set | all in the lock: thresholds can't follow each environment's disk |
| O7 | stdout | never detaches: it has nothing to record | detaching in memory: a consumer silently misses changes across a restart |
| O8 | Rebuild trigger | the probe (§6), on the doubling schedule | a timer: routes around an operator's block, and its seed deletes the blocked generation |
| O9 | Flap state | in the detach record | in memory: every restart rebuilds at once |
| O10 | An index detached on every sink | stop reading its changes, as a later slice if it measures worth it. It must stop marking them too, or the dispatcher's end check (`dispatcher.rs:207-217`) raises `DocumentsLeft` at a stop | build it for no lane: wasted builds and reads |
| O11 | First rebuild | no earlier than `retry_initial` (5 min) after the detach, at a landed probe | the first landed probe: usually meets the cause again |
| O12 | Operator `detach` command | none | `POST /detach` / `flusso detach`: a new way to drop changes on purpose, undone only by a rebuild |
| O13 | Stuck clock across restarts | recorded in its own document with `seen_at` (§4) | memory only: a process restarting faster than `stuck_for` never detaches, and the WAL limit then loses the slot for every index |
| O14 | A fence naming no sink or no index while a lane is detached | answers `detached` at once. **Accepted knowingly:** while any lane is detached, a `/wait` naming no index fails for every index, until the caller names the sink or index it reads (`flusso-query` names its indexes) | skipping detached lanes: passes readers of frozen data |
| O15 | What is left and reported | `flusso check` unchanged; `/status` and `/indexes` report; a schema change leaves the old alias's documents and generations, as old aliases are left today | sweeping them: deletes data, which nothing does today for any old alias |
| O16 | Reattaching a detached `backfill = false` lane | a distinct verb, `flusso reattach --accept-gap`. **Accepted knowingly:** the gap is permanent. It holds every build published between the detach and the reattach, and the items discarded at the detach. Deletes in it leave ghost documents for good; updates in it stay stale until the row changes again | `reindex` or `reindex --accept-gap`: makes "rebuild" mean "accept a gap" |
| O17 | A park during a rebuild | the rebuild fails when its lane is parked or restaging at that moment past `rebuild_grace` (10 min), on a clock started at the first park after `Attached` | at once: loses hours of backfill to a 30 s block. After `stuck_for`: holds the slot for hours per attempt |
| O18 | What gates a detach after an outage | the lane's next refusal | time since the destination answered: one more clock. The run's age: never detaches in a crash loop |
| O19 | Full disk (`read_only_allow_delete` met by the probe) | the rebuild waits for an operator. **Accepted knowingly:** a flood stage longer than `stuck_for` leaves every index on that node detached until an operator runs `reindex`, even after OpenSearch lifts the block itself. Reads stay frozen and the alert stays critical | automatic: writes a second full copy onto a full cluster and can block other tenants' indexes. A headroom check: heuristic, since the disk is shared |
| O20 | `reindex` on a rebuilding lane | restarts a rebuild that is parked or restaging; ignored, and logged, while it is open | always ignored: no way out but waiting for `rebuild_grace`. Always restart: throws away a healthy backfill |
| O21 | `committed` in `/status` | counts attached lanes only, and lists the detached ones it leaves out | frozen at the detach: the sink looks stuck while detached |
| O22 | Stuck clock while `enabled = false` | not written | written: meta writes for an unused feature |
| O23 | Where the attach happens | the index's builder quiesces (§1) | stopping the whole ingest engine: pauses every index, replays the unconfirmed tail, fails `/readyz` and fences during the restart, and restarts other indexes' backfills from where they were asked |
| O24 | Slice 1 on a record left by a newer flusso after a rollback | log loudly and stage as today: a rebuild, or for `live_only` resuming over the gap | restaging until upgraded: pins the slot until the WAL limit drops it for every index |
| O25 | The WAL half | only the lane `pinned_by` names, measuring the WAL its detach would free (its position to the next-least lane's, or to the horizon); no grace | the lane's WAL behind as measured today: counts WAL not yet read and WAL another sink holds, so an outage causes a storm of rebuilds and detaches that free nothing. After a grace: the slot can be lost within it. A 90% hard bound: one more rule for the same case |
| O26 | `--skip-backfill` at a start that finds a running rebuild | demote it, with a loud log: the rebuild and its partial are dropped, the lane is detached | keep it attached and paused: pins the slot for the whole run |
| O27 | The rebuild's walk while another sink's walk of the index runs | it waits for that walk, with the lane attached and live meanwhile, so reads stay on the old generation for up to that walk's length | cut the other walk: costs the other sink its progress since its last mark |

## 12. Not verified

- **The spikes:** S1–S4.
- **The quiesce:** the builder has no "stop granting" state today. Slice 3 adds it beside the budget's refusal. That a slot blocked on another sink's full lane bounds the pause by that lane is read from `publish_to`, not tested.
- **`routed_ever`:** that seeding `newest` with it never claims a change not yet emitted. The argument is that a position is routed only after its change was read; the existing `is_last_emitted` check is relied on, not re-read.
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
| 4 | W1 an attach request waits behind a walk (inbox gate) | §1 step 1: `Request::Attach` on its own route; the rebuild's walk may still wait (O27) |
| 4 | W2 `down` folds a lone builder for good | §1: the quiesce replaces `down`; R8 |
| 4 | minor: one builder per run; empty batches are skips | §1, §2, R7 |
| 4 | H1 `stuck_since` survives the detach | R2, R10, d3 clears it; O17 |
| 4 | H2 a run dying between attach and `Attached` | R8: attach, then publish; republished for the same attempt; the reverse loses data |
| 4 | H3 the sink-side boundary and acknowledgements | R6: inline acknowledgement, `Attached` only with nothing of `L` unacknowledged, matched by attempt |
| 4 | H4 re-running a detach ratchets the marker | R4: a marker with `detaching_from` is left as it is; demotion restores from `serving` |
| 4 | H5 `backfill = false` while detached | R14, one rule |
| 4 | H6 a failed read taken for "no record" | §4: one `_mget`, only `404` is none |
| 4 | H7 an attach in a run that routed nothing | R8: `newest` from `routed_ever` |
| 4 | D1 the WAL half frees nothing | R2, O25(d) |
| 4 | D2 a rebuild waits behind another walk | O27 |
| 4 | D3 O10 and `DocumentsLeft` | O10 |
| 4 | live tests | §10: S2 now, S5, S6 |
