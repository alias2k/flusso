# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status:** decided by the owner on 2026-11-24. Every decision in §11 is taken, and the slice order in §10 is accepted. Slice 1 (reading the records at staging) is built; slice 2 is being built in four steps (§10a): steps 1 to 3 are built; slices 3 to 6 are not.

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
| R4 | **Recorded before it takes effect, seed marker first.** `Sink::detach` runs three steps, in this order. **d2b:** an update of the seed marker, conditional on the `seq_no` read at d1, to the next generation, unseeded, with `walked` and `uuid` null, and `detaching_from: { generation, seeded, uuid }`: the marker as it was (§10a, correction 1). A marker that already carries `detaching_from` is left exactly as it is, generation included, so re-running a detach never moves it again (H4). Skipped on a `backfill = false` sink. **d2a:** the detach record, created, or updated conditionally on the `seq_no` read at d1. **d2c:** delete the unserved generations other than `serving`, best effort. The lane leaves the minimum only once d2b and d2a both returned `Ok` in this process. A sink that can't record never detaches: stdout keeps nothing. |
| R5 | **The fence is frozen first.** The fence serves `Detached { through }`, frozen at both levels, and `reached()` no longer raises it. Then `Progress::detach(lane)` runs under the registry lock: the lane's registrations and steps are cleared, `register` on it is a no-op, and it is skipped by `least_of`, `pinned_by`, `own_oldest`, `committed` and every fence over all lanes. `forget_all` keeps the detach. **With no attached lane, `landed()` answers `Landed::Free`:** nothing is needed, so Postgres confirms through its horizon. That is a new `Landed` case (A5). |
| R6 | **Discard and release everything of the lane.** <br>• **Acknowledged unwritten and unapplied:** the window's items, the held items (with `holding` decremented), and everything received until `Attached`. The discard is decided before `queue` touches `Fences`, F9, the account, `walked` or the held check. A detached lane is always taken from, so a send blocked on it completes. <br>• **Acknowledgement order (H3):** <br>  1. A discard is acknowledged **inline**, through `acknowledged` (awaited in the run loop), before the next delivery of the lane is taken. Never through `acknowledge`'s refresh flight, and never through the held items' next-step deferral: held items discarded at the detach are acknowledged at the barrier. <br>  2. `Attached` takes effect, and is acknowledged, only when nothing of `L` is unacknowledged: not in the window, `held`, `settling`, `unacknowledged_held`, a flight or a refresh. Otherwise it waits at the lane's front. <br>  3. `Attached` is matched by `attempt`: an older attempt is discarded; the current attempt on a lane already open and rebuilding is a no-op (H2). <br>• **Cleared:** `parked_item`, the fresh-build ticket `rebuild`, F9 and its account, `walked` (reset), `held_for_seed`, the owed reindex with its retry, the index in `pending_reindex`, and the lane's `Fences` entries with `covered_below`. `deferred` is fixed, and `Fences.applied` is kept, since later keys are larger. <br>• **Kept, marked cut:** the outstanding backfill entry. <br>• **The barrier:** the detach waits until nothing of the lane is in flight: writes, probes, refreshes, seeds, records, restages. |
| R7 | **Ingest publishes nothing to a detached lane.** For a detached lane, `publish_live` does what it does for a lane with nothing built (a skip, no lane item); the skip's `register` does nothing, and `settle_build` still runs. At the detach, `L` is removed from every running walk and coalescing set of `I` (`Snapshot::requested`), and its request is acknowledged with the snapshot's. Another sink's walk goes on. A `Backfill` request for a detached lane is acknowledged, not served for that lane. |
| R8 | **Attach by quiesce (§1).** `Request::Attach { attempt }`, on its own route, whose `attempt` is the lane's current one in `Progress`. The builder of `I`: <br>1. quiesces; <br>2. calls `Progress::attach(L)`; <br>3. **then** publishes `LaneItem::Attached { attempt }`; <br>4. then acknowledges the request. <br>**A request finding `L` already attached for its attempt** publishes `Attached` again and is acknowledged (H2). That is the case of a run that died between steps 2 and 3, whose request comes back. **The reverse order** (publish, then attach) **loses data:** the sink would open `L` and ask for its backfill while `Progress` still skips `L`. A run dying in between lets the slot pass changes that never reach `L` and that pages read before them miss. At a process's first staging, no builder runs, and staging attaches directly. **Inside a process, `Progress`'s own state decides the lane.** The record decides only the sink side, and `Progress` only at the first staging (H1). **At the attach, `L.newest` is set to the newest position routed in this process** (`routed_ever`, kept by `forget_all`). Every change up to it was emitted before the attach, so the rebuild's pages, read after it, cover it. A lane that registered nothing then answers `Through(routed_ever)`, not `Idle`, and a quiet database keeps confirming (H7). |
| R9 | **At most one partial generation.** A rebuild starts only after deleting every unserved generation except `serving`. So does the attach at a start, which also keeps the generation the marker names. |
| R10 | **The rebuild starts when its probe lands (§6)**, on the doubling schedule, or at `reindex`. **It fails** when its lane is parked or restaging at that moment and its `stuck_since` has passed `rebuild_grace`. The clock was cleared at d3 (H1), so it starts at the first park or restage after `Attached`, and flapping keeps it (R2). The lane then detaches again, `attempts` unchanged (it counts rebuilds started, at r1c): the partial is deleted, and the marker is moved explicitly past every existing generation, conditionally, keeping its `detaching_from` (§6a). |
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
    "refused_keys": "[<RowKey>…]", "attempts": 2, "last_attempt": "…", "since": "…" }
  ```

- **`stuck:{hash_alias}`, the stuck clock:** `{ "since": "…", "seen_at": "…" }`.
  - Written once the lane has been parked or restaging for `grace`, with `seen_at` refreshed every `grace` while it stays so.
  - Deleted, conditionally, when R2 clears the clock.
  - A park resumes the recorded `since` only if `seen_at` is within `grace` of now. Otherwise the clock starts fresh and the document is rewritten.
  - Inside a process the clock is an `Instant`, plus the recorded offset taken once, so a jump of the wall clock can't shorten it.
  - Nothing is written while `enabled = false` (O22).

**`refused_keys`** are the parked item's `RowKey`s, taken by `keys_of` at d1 while the item is still in the window. They are empty when two keys render alike, and then the rebuild is its own probe.

**Staging reads both documents and the marker first, in one `_mget`,** on every staging path: at the top of `step`, before a rebuild's `reindex` and before `ensure_index`. Only a `404` or a missing meta index means "none", whether `_mget` answers with a `404` status or a per-document `index_not_found_exception`. Any other failure fails the staging step, which is retried as today, so a timed-out read is never taken for "no record" (H6). A late d2a from a dead process, landing just after a restore, leaves "`detached` + names `serving`" for the next staging, which detaches again. That is self-healing.

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
| r4 | `SnapshotComplete`: `mark_seeded` swaps the aliases, drops `serving`, `PUT`s the marker seeded (`detaching_from` goes with the whole-doc `PUT`), then deletes the record. It follows #479's seed rule: a seed refused by a block on `serving` parks the lane `blocked`, naming the block, and its probe retries the seed. flusso never deletes `serving` itself, under any block (§10a) |

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
- A rebuild starts at the first landed probe at or after `last_attempt + retry_initial × 2^attempts`, the wait capped at `retry_max`. With no attempt yet, the earliest start is `retry_initial` after the detach (O11). So each failed rebuild doubles the wait (decision 4). *Corrected in slice 2's opening: the formula used to read `2^(attempts − 1)`, which gave the first retry the same wait as the first attempt, and R10 counted an attempt again at the failure.*
- `attempts` resets only at a seed.
- With `rebuild = "operator"`, probes still run and are shown, but only `reindex` rebuilds.

## 6a. Operator actions on a detached or rebuilding lane

Written at the opening of slice 2, before any code. Two seams of the note are settled here:

- **`attempts` counts rebuilds started.** It is incremented at r1c only. A failure (R10) or a forced restart (below) doesn't count again.
- **Two kinds of marker move.**
  - **Re-running an unfinished first detach (R4's d2b)** leaves a marker that already carries `detaching_from` exactly as it is (H4).
  - **Detaching a lane whose rebuild failed or was restarted** must move the marker past the rebuild's generation. That is an explicit move, conditional on the `seq_no` read, which names the next generation past every existing one and keeps `detaching_from` as it is, as the demotion does (§4).

### `reindex` on a rebuilding lane (O20)

A rebuilding lane is in one of three conditions, and each gets one answer:

| Condition | What `reindex` does | HTTP answer |
| --- | --- | --- |
| waiting for `Attached` (r2–r3) | nothing: the rebuild is starting | see "How an ignored reindex is answered" |
| attached and open: the rebuild is progressing | nothing; logged `reindex ignored: the index is being rebuilt` | same |
| attached, parked or restaging (inside `rebuild_grace`) | restarts the rebuild now, as below | `202` |

**Restarting a parked or restaging rebuild:**

1. **Fail the rebuild as R10 would, without waiting for `rebuild_grace`.**
   - The lane detaches again (d1–d5, at the lane barrier) and its record becomes `detached`.
   - `attempts` is unchanged.
   - d2c deletes the abandoned partial, best effort.
   - The marker moves explicitly past every existing generation (above).
2. **Rebuild at once.** This bypasses the probe and the schedule, which is decision 5 and R13. It also bypasses `rebuild = "operator"`, since this is the operator.
   - r1a deletes every unserved generation but `serving`. If one won't go, the rebuild is not started: the lane stays detached, `reason` names the refusal, and the reindex is owed, as an owed reindex is today (X15): retried on the restage backoff.
   - Then r1b–r4 run as in §5. r1c counts the attempt, so the next automatic wait doubles from it.
3. **Under `--skip-backfill`** the operator's reindex still rebuilds, as `stage_reindex` does today (R13). O26 then demotes it at the next start.

**How an ignored reindex is answered.** *Decided by the owner on 2026-11-25.*

- The daemon reads the lane from `Status` and refuses at the HTTP edge with `409`, `reason: "the index is being rebuilt"` (or `"…starting"` while it waits for `Attached`).
- A reindex naming no sink goes on for the sinks where it applies, and its `202` body lists the lanes it skipped.
- The engine still ignores, and logs, one that races past the check.
- `flusso reindex` exits non-zero with the reason.
- The rejected alternative, `202` with only a log line, would have left an operator learning from the log alone.

### `flusso reattach --accept-gap` (O16)

**The surface:**

- **CLI:** `flusso reattach <index> --sink <name> --accept-gap`, with the same `--server` and `--admin-password` as `flusso reindex`.
  - `--accept-gap` is required. Without it, the command refuses and prints what the gap means.
  - `--sink` is always required, because a reattach is one lane. *Decided by the owner on 2026-11-25*, even when exactly one `backfill = false` sink holds the index detached.
- **Private HTTP:** `POST /indexes/{name}/reattach?sink=<name>&accept_gap=true`, under the private surface's Basic auth.
- **Daemon:** `DaemonControl::reattach(index, sink)`, which sends `SinkControl::Reattach { index }` to that sink's engine.
- **Port:** no new `Sink` method. It uses `detachment`, `ensure_index` and `forget_detachment` (§8).

**Refusals:** checked at the edge from `Status`, and checked again by the engine, which logs a refusal it meets.

| Condition | HTTP |
| --- | --- |
| unknown index or sink | `404` |
| `accept_gap` missing or false | `400`, naming the gap |
| the lane is not detached (open, parked, restaging, rebuilding) | `409`, `reason: "not detached"` |
| the sink has `backfill = true` | `409`, `reason: "this sink can rebuild: use reindex"` |
| the record is not `live_only`, or the marker doesn't name `serving` seeded (an unfinished demotion) | `409`, with the reason. The next staging finishes the demotion (§4); ask again after it |
| the deployment is stopping | `503`, `reason: "stopping"` |
| the sink's control queue is full | `503` busy, as `reindex` |

**Steps** (sink engine, between batches, at the lane barrier, as a reindex is staged):

1. **Read the documents** (`detachment`). Check: `live_only`, the marker on `serving` and seeded, the lane still detached. Otherwise refuse and log.
2. **`ensure_index`.** It adopts the serving generation (the marker names it, seeded), so the target is the hash alias, as for any seeded index.
3. **Delete the record** (`forget_detachment`, conditional on the `seq_no` read at step 1). **This is the commit point of "accept the gap".**
4. **Attach.** The sink engine keeps `reattaching: Some(attempt)` in the lane's state and publishes `Request::Attach { attempt }`. The builder quiesces, attaches the lane (`Progress::attach`), then publishes `Attached` (R8). No backfill is asked for.
5. **On `Attached`** (R6's acknowledgement rules), the lane opens as an ordinary open lane, not rebuilding. `reattaching` is cleared.

**A crash at each point:**

| Killed after | The restart finds | Result |
| --- | --- | --- |
| 1, 2 | the `live_only` record | detached, as before: the reattach is lost, nothing changed; ask again |
| 3 (record gone, not attached) | no record, marker seeded on `serving` | today's staging: the lane starts attached and open, live into `serving`. The gap now runs to the restart; it is accepted |
| 4, 5 | the same | the same |
| a sink engine run restart inside the process after 3 | `Progress`: still detached (R8: `Progress` decides in a process) | the lane's `reattaching` re-sends `Request::Attach`, so the attach is idempotent per attempt (R8) |

Why the record goes before the attach:

- **Deleted first,** a crash anywhere after it still ends attached. The operator's decision sticks, and the only cost is a longer gap, which the decision already accepts.
- **Deleted last,** a crash before the delete would bring the lane back detached after the operator accepted the gap, and it would stay so until they ask again.

**The fence after a reattach.** *Decided by the owner on 2026-11-25.*

- From `Attached` on, the lane serves `Writes`: a fence vouches normally for every position the lane reaches.
- The gap is named in the reattach's answer, in the log and in the runbook. Its changes are missing until their rows change again, so a row last changed in the gap reads stale.
- The rejected alternative, never vouching past the position frozen at the detach, would have failed `/wait` on that index and sink for good, since a `backfill = false` sink can never be rebuilt.

**`/status` after a reattach:** the lane is `open`, and carries `reattached: { at, gap_since }` until the process restarts, so the gap is visible. This is not a decision.

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
- `SinkControl::Reattach { index }` and `DaemonControl::reattach(index, sink)`, behind `POST /indexes/{name}/reattach` on the private surface and `flusso reattach --accept-gap` (§6a).

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
| S1 | the write check writes no document and is refused under: a write block, `read_only`, `read_only_allow_delete`, closed, a cluster block, a `fail` or `drop` pipeline, the alias form, a deleted generation, the NUL `_id`. Also that the refusal names its block (the `cluster_block_exception`'s reason), so the probe can tell `read_only_allow_delete` from the others, which O19 depends on | slice 4 (run in slice 3) |
| S2 | `mark_seeded` drops an old generation that is `read_only`, closed or write-blocked. If the call fails, the seed fails and full backfills loop. **Reachable on `main` today** through an operator `reindex` of a seeded index whose generation is blocked | issue #479; done: `read_only` and `read_only_allow_delete` refuse the `_aliases` call, and #483 parks the seed (§10a) |
| S5 | `Landed::Free` against Postgres: every lane detached, a transaction held for visibility (the `visibility` e2e's standby), and only keepalives arriving. The slot follows the horizon and never passes the held commit | slice 2 (its first test) |
| S6 | engine level, injected failures: an ingest run dying between `Progress::attach` and `Attached` (H2); a sink run killed while a discard's acknowledgement is pending (H3) | slice 2 (its first test) |
| S3 | `write_serving` through the alias against a deleted, closed or blocked serving generation | slice 4 (run in slice 3) |
| S4 | `max_slot_wal_keep_size` read as the deployment's role, and where the meta index's shard sits under flood stage | slice 4 (run in slice 3) |

### Slices

The owner accepted this order on 2026-11-24. Each slice is safe if nothing after it is built. Spike S2 is issue #479 and runs before slice 2.

1. **Read the documents; never half-stage.**
   - `stuck` documents are ignored.
   - A `detaching_from` with no record is restored. One this version can't read (not an object, or naming no generation) is logged loudly and the index staged as before (O24), never failed for it: failing would restage the index until someone edits the marker.
   - Each document of the `_mget` must answer for the id asked, found or not found; an entry saying neither, or naming another id, fails the read.
   - A `detached` or `rebuilding` record logs loudly and stages as today. That is a correct rebuild, since the marker moved first.
   - A `live_only` record logs loudly and stages as today: the log names the gap. Nothing restages forever (H3).
   - A malformed record is logged loudly and staged as today, like a record left by a newer flusso (O24).
   - What staging read is kept per lane in the sink engine, for slice 2 to act on. Nothing in `/status` changes yet; the record's fields there come with slice 2's observability. Corrected in slice 1: this line used to promise a `/status` flag here. No record can exist before a later slice writes one, so a rollback is the only way to meet one.
   - Cost: one `_mget` per index per staging step, restages and reindexes included. Only a `404` reads as none (H6). A read that fails fails that index's step: under `Resumed` the index is left restaging alone (#471); under `Fresh`, the staging fails as a whole, as any step failure does today.
2. **Detach and rebuild, reachable only from tests,** with `enabled = false`: R1, R4–R9, R11–R14, the quiesce, `Request::Attach` and `Attached`, `Landed::Free`, `routed_ever`, the fence, the `/status` fields of §9. It starts from S5 and S6. It follows #479 (S2). A failed read leaves the entry slice 1 keeps per lane (`SinkEngine::detachment`) as the last read left it, so nothing may act on kept state while the lane is restaging. Until slice 4 brings R10, a rebuilding lane that parks stays parked and holds the slot like any parked lane. That is harmless, since nothing in a deployment reaches slice 2's rebuild.
3. **Spikes S1, S3, S4.**
4. **Thresholds and the probe-gated rebuild,** with `enabled = true`: R2, R3, R10, the stuck clock, the probes, the schedule, the alert. Gated on slice 3. Carried in from the re-check of step 4's fix round (2026-11-26), to build here:
   - **`SeedBlocked` in the counts.** R2's thresholds count a `ParkReason::SeedBlocked` park (O28 keeps it under R2). R10's `rebuild_grace` does not, and flood stage never counts toward R10. Every place that matches a park for R2 or R10 names `SeedBlocked` apart from `Blocked`; `kind()` and `label()` give both `blocked`, so only `/status`'s `seed_blocked` tells them apart.
   - **`clear_failed_detach` deletes any `Detached` record it finds,** conditional only on the version it just read. Since the fifth re-check it runs only while this process holds the slot, so no other process can have written one meanwhile; naming the attempt in the record would still make it exact.
   - **Record operations between the first read again and the grant.** Settled by the fifth re-check's narrowing: every record write is gated on the slot or safe without it (§10a, "After the fifth re-check", the table).
   - **m-3: a lane held detached in memory never learns that another process reattached or rebuilt it** (the fourth re-check). Since the cut (2026-11-27) it learns at its next read while held, or at a `reattach`, and stops: the record is left as the other process wrote it. On the live checklist below.
   - **m-4: `check` and `--strict` (#486) read no index state.** A detached index shows in `flusso indexes`. For the docs agent, on the manual's `check` page once this slice makes detach reachable: "`check` doesn't read index state; a detached index shows in `flusso indexes`."

   **To prove live before slice 4 ships** (the fourth re-check's checklist, verbatim):

   A checklist for the issue. Each item is a Postgres and OpenSearch e2e (or a scripted run of the real binary), not a stand-in.
   - [ ] **Rebuild to seed.** Detach an index (a hand-written record), `reindex` it. `/wait` answers `rebuilding` and vouches only up to its freeze while the new generation fills. After the seed, reads see every change, and the record is gone.
   - [ ] **Rebuild resumed at a restart.** Kill mid-walk. The next start finds `rebuilding`, resumes the walk from its checkpoint, seeds, and the index equals the table.
   - [ ] **Reattach with a gap.** On a `backfill = false` sink: `reattach --accept-gap`. The gap's `_update` lands on the seed marker; `/wait` inside it is `409 gap`, before and after a restart; after it is vouched; a seed ends it.
   - [ ] **A reattach killed before each step** comes back detached, or with its gap.
   - [ ] **O29 with two real processes.** A holds the slot, B staged and waits (busy slot). A detaches a lane, drops, confirms, exits. B opens: the lane answers `starting` until B's read, then `detached`, and nothing of it is written by B.
   - [ ] **O29 with a reattach in between.** A reattaches while B waits. If B read the records before, B stops at its read once it holds the slot (exit 1, the message naming the index), and its restart reads the gap and refuses inside it. If B first reads after the reattach, it reads the gap and refuses inside it.
   - [ ] **B-2 live.** A process dies mid-rebuild, another takes the slot, then a third starts. With B-2's fix, no seed lands over dropped changes.
   - [ ] **B-1 live.** A restaging index (its target deleted) whose restage is in flight across a replication reconnect (drop the walsender with `pg_terminate_backend`) gets its records read before it writes. After the reconnect, every healthy lane passes `/wait` within a few seconds.
   - [ ] **No seed without the slot** (the fifth and sixth re-checks). A backfill whose stream ends before its `SnapshotComplete` (drop the walsender mid-backfill): no seed is written until the slot is held again, then the owed seed lands with no second walk. With another process's detach record or gap written meanwhile, that record survives and B stops; its restart backfills.
   - [ ] **Another process's rebuild met without the slot** (the sixth and seventh re-checks, the cut). B restarts a sink's run while A holds the slot and walks a rebuild: A's partial generation and checkpoint survive, and B holds the lane detached, undecided. If A finishes first, B stops at its read once it holds the slot; otherwise B demotes then.
   - [ ] **The stop** (the cut). Two real processes; A changes B's records while B doesn't hold the slot. B exits 1 with the message naming the index and the sink, `/status` shows the error until then, the slot's `confirmed_flush_lsn` is no further than B's stream start, and B's restart converges with every change indexed.
   - [ ] **A lost answer never stops one process** (the review of the cut). Drop flusso's network to OpenSearch (or delay its responses past the client's timeout) while a reindex of a detached index begins its rebuild, and while a reattach deletes its record; then restart Postgres's walsender. flusso goes on: no exit 1, the rebuild or the reattach finishes.
   - [ ] **A seed whose record delete fails** (block `DELETE` on the meta index for one request while a rebuild seeds): the record is deleted at the seed's read back or the next staging, and a later reconnect stops nothing.
   - [ ] **A stop during a failed reconnect** (the old walsender still holds the slot until `wal_sender_timeout`): the stop drains the lanes' backlog and exits at once, not at its deadline.
   - [ ] **A process that can't reach Postgres at its start** while another holds the slot: a `reattach` is refused, a `reindex` waits (restaging, with the reason), and no record changes.
   - [ ] **A reconnect on a quiet database.** After `pg_terminate_backend` of the walsender, `/wait` for an already-applied position passes within about 1 s of the reopen (the wake), at both levels.
   - [ ] **Meta index unreadable for one index** (blocked or closed) across a reconnect. That lane answers `starting`, its siblings pass, and it recovers when readable.
   - [ ] **Server-side slot release.** The slot taken by another process after B's walsender is killed: B's `Held` guard drops `holding`, and no gap end is settled while it isn't held.
   - [ ] **A lane held detached in one process, reattached or rebuilt by another** (m-3). B stops at its next read while held, or at its `reattach`, and leaves A's records as they are.
   - [ ] **The gap write's condition** (`record_gaps` conditional on the marker read) under a concurrent marker write.
5. **Runbook split and manual pages;** a docs agent writes the user pages.
6. **O10,** later, if measured worth it.

## 10a. Slice 2: what its opening settles

Written at the start of slice 2's implementation, after #479 (PR #483) merged. Slice 2 is built in four steps, each reviewed on its own. Step 1 is this section.

2. **The detach** (R4–R6, R11, R12 for a detached lane): the sink's records, `Progress::detach` and `Landed::Free`, the fence, and the sink engine's discard. Staging comes back detached after a kill.
3. **The attach by quiesce** (R7, R8): `Request::Attach`, `LaneItem::Attached`, the builder's quiesce, `routed_ever`.
4. **The rebuild** (R9, R13, R14): r0–r4, the staging rows for `rebuilding`, O20(c), O16(b), O26, O27.

**No `[detach]` key lands in slice 2.** Nothing in a deployment asks for a detach: only a test sends `SinkControl::Detach`. The table, its flags and env vars, and the per-index `detach = false` arrive with slice 4's thresholds. So slice 2 needs no `just schema-gen` and no designer change.

### r4 under #479's seed rule

#483 merged the rule r4 follows:

- **What refuses.** `mark_seeded` returns `SinkError::RetireBlocked` before changing anything when a generation it retires carries a block that refuses the `_aliases` call: `read_only` or `read_only_allow_delete`. A write block or a closed `serving` retires as before.
- **What the engine does.** It parks the lane `blocked` on its `SnapshotComplete`, naming the block. The seed is the lane's probe, on the lane's backoff: 1 s doubling to `[backoff] max_ms`. Nothing behind the completion is written meanwhile, since the completion is a barrier on its lane. So `L` holds the slot from its oldest live item, and its stall age grows.
- **What flusso never does.** It never deletes `serving` itself, under any block. The owner decided on 2026-11-26 not to build form (a). Under `read_only_allow_delete` an operator may delete `serving`. Reads of the index then answer `404` until the seed's next probe adds both aliases to the new generation.
- **A kill in that window.** The restart's staging finds `rebuilding`, the marker unseeded on the new generation, and no alias holder. `ensure_index` takes `first` and gives the new generation both aliases. `L` is attached at the first staging, its walk resumes from its mark at the walk's end, and the seed follows at once. Reads see the complete backfill, and the slot replays the rest.

**How it meets R10 and O17.**

- **The park is a park after `Attached`.** It starts the lane's stuck clock under R2's rules, and R10 fails the rebuild once the lane is still parked past `rebuild_grace`.
- **Failing it costs the whole generation.** The lane detaches again, d2c deletes the new generation, complete as it is, and the marker moves past it.
- **No rebuild starts while the block stands.** The next attempt waits on the doubled schedule. Its probe writes through the hash alias, so to `serving`, and the block that refused the seed refuses the probe too (§6).
- **When a rebuild meets this.** Only when a block lands on `serving` after r0, or when `reindex` forces a rebuild past a read-only `serving`, since the operator's reindex bypasses the probe.
- **Until slice 4** there is no R10: a rebuild parked on its seed stays parked, as the slice 2 row says.

**O28, decided by the owner on 2026-11-26 (§11): is a rebuild parked on `RetireBlocked` exempt from R10?**

- **As written:** the rebuild fails after `rebuild_grace` (10 min), and a complete generation is deleted. An operator who forces `reindex` past a read-only `serving` pays a full rebuild per attempt.
- **Exempt:** the lane stays parked with the new generation complete until the block on `serving` goes, and holds the slot for `L`'s live items behind the seed meanwhile. R2 still applies: at `stuck_for`, or the WAL half, slice 4 detaches the lane again, and that detach deletes the generation anyway.
- **The step 2 review agrees with exempting it,** with two additions:
  - `/status` and the log name this park apart from a write refusal: the seed waits for the block on the serving generation;
  - flood stage (`read_only_allow_delete`, O19) never counts toward R10, even under a forced `reindex`.
- **Decided:** exempt from R10, under R2, with both additions. R10 is slice 4's: the rule is recorded here and built there.

### What S5, S6 and slice 1 changed

- **S5 (#482).** `Landed::Free` exists. Postgres confirms through its horizon on it, and `reached` answers `live`. The live spike shows the slot following the horizon and never passing a held commit. R5 stands as written.
- **S6 (#482), both H3 and H2 confirmed.**
  - An acknowledged boundary does not shield an earlier unacknowledged delivery: the earlier one comes back first. So R6's second rule (`Attached` takes effect only with nothing of `L` unacknowledged) is required, not a precaution.
  - The request a dying run took comes back to the next run, and what it recorded in `Progress` survives `forget_all`. So R8's republish of `Attached` for an attempt already attached is required.
  - The two follow-up tests named in §10 are step 3's: `a_run_dying_between_attach_and_attached_republishes_it` and `a_run_restart_after_attached_never_writes_a_redelivered_earlier_item`.
- **Slice 1 (#481).** `Sink::detachment` returns `Detachment { record, stuck, restored, unreadable_marker }`. Step 2 widens it with what staging must compare: the record's `serving` and a version for its conditional delete, and the marker's generation and seeded state.

### Corrections the code made

1. **`detaching_from` records the marker as it was**, not the hash alias's holder. Slice 1 puts the marker back on it exactly as it was. The two differ only when a reindex was under way at the detach: the marker on the next generation, unseeded, the alias on the old one. Restoring then returns to that reindex, which the restart's staging resumes. The record's `serving` is the generation the hash alias holds, what reads see, and `null` when nothing holds it.
2. **r4 no longer says the seed deletes `serving`** (form (a), above).
3. **Where step 2 detaches (R3).**
   - **A parked lane** detaches at its next probe the destination answers refused, blocked or missing. A probe that lands drops the ask, and so does a staging that lands.
   - **A restaging lane** has no probe: its refusal was the missing target. It detaches when its next staging comes due, instead of staging. A failed staging is not taken for a refusal, since an `ensure_index` error can't be told from a sink-wide failure.
   - **An open lane** keeps the ask until it parks and probes.
   - Slice 4 revisits all of this with S1's write check.
4. **`LaneHealth::Detached { since, reason }`** is a new public health, kind `detached`. A lane whose d2 is in flight keeps its parked or restaging health until d2 returns.
5. **R7 lands in step 3, with the builder's other change.** In step 2 ingest still publishes to a detached lane, and the sink engine discards it:
   - `register` on a detached lane is a no-op;
   - the build's `settle_build` still runs;
   - a walk of `I` for `L` runs on and is acknowledged as before.
   It costs builds, not correctness.
6. **The discard is acknowledged one item at a time.** A detached lane takes its next delivery only once the previous discard's acknowledgement returned, in the run loop (R6, H3).

### Step 2 as built

**What it does.**

- **The ask.** `SinkControl::Detach { index }` sets `LaneState.detach_asked`. Only tests send it.
- **The record.** `Sink::detach` returns `Recorded::Kept` or, by default, `NotKept`. The OpenSearch sink writes d2b, d2a and d2c, each conditional on an `_mget` read just before it; a `409` reads again, at most five rounds.
- **The lane.** On `Kept`, in this order:
  1. the health becomes `LaneHealth::Detached`, and the daemon fence hears it first, freezing `Serving::Detached { through }`;
  2. `Progress::detach`;
  3. `forget_detached`: the sink's fence entries, a pending reindex of the index, and the outstanding backfill marked cut;
  4. everything of the lane in the window or held is acknowledged unwritten, then every later delivery.
- **Staging.** `AtStaging` reads the rows of §4 that a detach can leave:
  - `detached` → detached;
  - a seeded marker elsewhere → finished: the record is deleted on the version read (`Sink::forget_detachment`, new), and the index staged as before;
  - `detached`, not live only, with the marker on `serving` → `Sink::detach` again;
  - `rebuilding` and unreadable records → staged as before, until step 4. *Changed in step 3 by O30:* an unreadable record stages as before only beside a marker on a newer, unseeded generation, and is detached again otherwise; a `rebuilding` record with the marker on serving is detached again.
- **A staging during a run** (a restage, a reindex) that finds the index detached is adopted by the run, which detaches the lane instead of reopening it.

**Corrections the build made.**

7. **`refused_keys` is stored as JSON text** (§4 corrected). The meta index maps its fields dynamically, and keys of different types across indexes would refuse each other.
8. **The fence answers `detached` at once with the position it froze.** After a restart that is empty, since nothing was frozen (O2's "detached at once").
9. **`/status` shows the lane `detached`.** It is left out of `flusso.lane.parked`. `flusso.lane.detached`, the alert and the other §9 fields come with step 4 and slice 4.
10. **A reindex of a detached index is ignored and logged until step 4,** which makes it force the rebuild (R13).
11. **A failed record of a restaging lane waits the staging backoff** before its next due staging tries again, so a meta index refusing every write is not a loop.

**What the tests prove, and what they don't.**

- **Proven by the sink's kill matrix** (`detachment::tests::record`): over a stand-in meta store, a detach killed before each of its writes leaves, for the next staging's read:
  - before d2b: nothing kept, the marker untouched;
  - after d2b: the marker put back as it was;
  - after d2a: detached, the marker elsewhere unseeded, the stale generation still there;
  - after d2c: the same, with the stale generation gone.
  The serving generation is never touched.
- **Proven by the engine tests** (`tests::detach`, on paused time, over the real sink engine and channel stream):
  - the refused probe;
  - the restaging lane;
  - the drain through a lane of two;
  - nothing of a detached index reaching `write`;
  - a run dying mid-record, which asks again;
  - a restarted run, which writes nothing of the lane;
  - every staging row.
- **Not proven:** the kill matrix against a live OpenSearch (stand-in only), and a detach meeting a running backfill walk of the index on the ingest side (R7 is step 3's).

### After the review of step 2

The independent review (`target/review-482-step2/REPORT.md`) found the core sound: the lane leaves the minimum only after the record is durable, every write is conditional, and `Free` comes only with every lane detached. What it asked, and what was done:

- **B1, a detached lane drained one item per wake-up.** A discard taken at the end of a run turn left its lane out of the run's wait. `drain_detached` now acknowledges and takes again until no discard is pending, at most `LOOKAHEAD` rounds a call. Two tests fail every time on the old code: a thousand items through a lane of two, and a healthy second sink of the same index, each within 100 ms of paused time.
- **S1, a record can land although its call failed.** The lane then keeps `LaneState.detach_unsure`. Before its next probe, the sink engine deletes a `detached` record found, on the version read, and reads again, which puts the moved marker back (§5). The probe waits for this, so a lane that opens leaves nothing for the next staging to detach. A failed clear re-parks the lane on its doubled backoff.
- **S2, a live-only record read as a finished rebuild.** `MarkerAt` gains `Newer`: the marker is on a generation past the record's `serving`. `Elsewhere` now means an older generation, or no known `serving`. `Finished` needs `Newer { seeded: true }` and a record that is not live only (O16).
- **S4, the fence-first order was unguarded.** A test observer checks that each detach is heard before the lane leaves the minimum, in a run and at staging. Swapping either order fails it.
- **S5:** the stand-in's `"docs"` key is in `NOT_READS`.
- **m1:** `detaching_from` keeps the marker's `walked`, and the put-back restores it, so the marker comes back byte for byte.
- **m2:** a detach run again at staging that the sink keeps no record of fails the staging step, instead of staging the index attached.
- **m4:** a staging during a run says itself that it found the index detached (`Outcome::Restage(Ok(true))`, `Reindexed.detached`). The run no longer reads the shared lane state, which it may have overwritten meanwhile.
- **m5:** a lane held detached in the process stays detached at any later staging (`step` returns `Staged::Detached`: `Progress` decides inside a process). A backfill ask no longer turns its fence from `detached` to backfilling.
- **m6, not fixed, for step 3.** `Landed::Free.covered_seq()` is `Some(u64::MAX)`. While every lane is detached, `IngestEngine::confirm`'s `furthest.reached` moves to the horizon. After the attach, `advances` stays put until the attached lane reaches past it, which delays the supervisor's backoff reset.
  - **The failing test to write first:** one lane; detach it; confirm at horizon `H`; attach; publish and land an item; assert `advances` moved. It fails today.
  - **Step 3 either** resets `furthest.reached` at an attach, or counts an advance when the landed answer itself moves.
- **m7, for step 4:** `/status`'s `committed` lists the detached lanes it leaves out (O21), with the other §9 fields.
- **m8, m9:** `SinkControl` is `#[non_exhaustive]`; two doc lines fixed.
- **Mutation tests added.**
  - Turning off `adopt_detached` fails `a_restage_during_a_run_that_finds_the_index_detached_detaches_the_lane` and `a_reindex_whose_staging_finds_the_index_detached_detaches_the_lane`.
  - Acknowledging the parked item when the detach starts, before the record, fails `a_sink_without_records_never_detaches`, which now checks that very item lands once the block lifts.

**The bench.** CI's one run showed `live_drain_held/sinks2/1` at +6.0% and `/256` at +4.8%. With nothing detached, the per-item additions are:

- a `detached > 0` check in `queue`, `acknowledge` and `lane_wants_more`;
- one bool in `register` and `least_of`, under the lock they already take;
- an empty check on the discard list;
- two more empty lists in `lane_in_flight` and `writing`.

None of these can be taken out without the feature. Measured locally (base `efc49673` against head `40bd03cb`, pinned to CPUs 0–3 and 12–15, three alternating rounds, criterion medians):

| Bench | Base, ms | Head, ms |
| --- | --- | --- |
| `sinks2/1` | 81.4, 81.6, 81.8 | 81.2, 81.4, 81.6 |
| `sinks2/64` | 64.1, 63.8, 64.2 | 64.1, 63.9, 63.9 |
| `sinks2/256` | 67.3, 68.5, 69.1 | 67.4, 67.6, 68.6 |

`sinks1` is within ±1% at every size. The CI numbers read as one run's noise. CI's gate on this branch is the authoritative answer.

### Step 3: the attach, as designed before the code

**What asks for an attach in step 3.** Only a test: `SinkControl::Attach { index }`. Nothing in a deployment sends it until step 4 wires the rebuild (r2) and `reattach` (O16) to the same path. In step 3 an attached lane opens as an ordinary open lane, with no backfill: what O16's reattach does.

**Attempts.** The sink engine numbers its attach attempts from one counter, increasing for its life. `LaneState.attaching: Option<u64>` is the attempt it waits for, and `LaneState.attached: Option<u64>` the attempt it last opened on. Both outlive a run, so a run restarted while it waits sends the same request again. In step 4 the attempt becomes the record's (§4), which is durable and increasing.

**The ingest side.**

- **The route.** `Request::Attach { index, sink, attempt }` goes on a third channel of the builder's route, beside `Rebuild`. It is read while a walk runs, so an attach never waits behind the inbox gate (W1).
- **The quiesce.** A builder with an attach pending grants nothing (`Builds::pause`): no live batch, no backfill batch, no fresh build. It publishes what is ready as before. Once nothing of its index is granted and unpublished (no slot, no running build), it acts on every pending request:
  - `Progress::attach(lane, attempt)`, under the registry lock;
  - then `LaneItem::Attached { attempt }` is published on the lane;
  - then the request is acknowledged, and granting resumes.
- **Progress decides the attempt (H2).**
  - A detached lane is attached for an attempt above the one it last attached for. Its `newest` becomes `routed_ever`, the newest position routed in this process, which `forget_all` keeps (H7).
  - A lane already attached for the request's attempt answers `Again`: `Attached` is published again.
  - Anything else is stale and acknowledged with nothing published: an older attempt, or the attempt the lane was attached for and then detached again.
- **R7.** With a lane detached (`Progress::any_detached`, an atomic load that gates each look-up; see the review of step 3 for what else the paths pay):
  - `publish_live` registers a skip on it and puts no item there;
  - `publish_backfill` and the snapshot's end publish nothing to it;
  - a `Backfill` request for it is acknowledged unserved when its snapshot starts;
  - a running walk whose every requesting lane is detached is cut, and its requests acknowledged.

**The sink side (R6).**

- A detached lane keeps taking its deliveries and acknowledging them unwritten.
- An `Attached` naming the attempt the lane waits for is held at the lane's front until nothing of the lane is unacknowledged: the discards acknowledged, nothing in flight. Then the lane opens, `attached` becomes that attempt, and `Attached` is acknowledged, in the run loop.
- Any other `Attached` is acknowledged and discarded: an older attempt, or one that arrives while no attach is awaited.
- On a lane already open with `attached` equal to the attempt, a redelivered `Attached` is a no-op (H2, H3).

**Tests first.**

- `progress::tests::attach`: attach after a detach, `newest` from `routed_ever` after `forget_all`, `Again` for the same attempt, stale attempts, `any_detached`.
- `ingest` level:
  - the quiesce publishes nothing of the index during the attach and grants after it;
  - R7 for live items, a walk, a `Backfill` request and a cut walk;
  - a lone builder quiesces and keeps its batching (W2);
  - S6's two follow-ups:
    - `a_run_dying_between_attach_and_attached_republishes_it`;
    - `a_run_restart_after_attached_never_writes_a_redelivered_earlier_item`.
- `tests::detach` (the sink engine):
  - items before `Attached` are discarded and those after it written;
  - an older attempt's `Attached` is discarded;
  - a second `Attached` of the same attempt discards nothing;
  - the attach waits for the discards.
- m6's failing test, then its fix.

### Step 3 as built

Built as designed above, with these corrections from the code:

1. **No `LaneState.attached`.** An `Attached` reaching a lane that isn't detached and waiting for it is acknowledged and dropped, whatever its attempt. That covers H2's second `Attached` and H3's redelivered one, with nothing to remember.
2. **The attach's own wait for discards is redundant by construction.** `take_attached` runs right after `drain_detached`, which leaves no discard pending, and a detached lane has nothing in the window or in flight. A mutation removing the check survives every test for that reason; the check stays as the rule's statement (R6, second point).
3. **The fence.** A lane that opens after being detached serves `Writes` (`FenceObserver::on_lane_health`). Step 4's rebuild must turn that into `Rebuilding` until its seed (O2), since the open there comes with a backfill.
4. **R7's walk.** A walk that other lanes asked for too keeps going and sends the detached lane nothing, its completion included. A walk only detached lanes asked for is cut at the builder's next turn, its requests acknowledged. A `Backfill` asked for a detached lane is acknowledged unserved when its snapshot starts.
5. **The request lane.** The attach request is offered (`now_or_never`) and sent again on the next turn when not taken, as a fresh build's request is. A stop drops a pending attach unacknowledged: the request is served on the next start.
6. **m6.** `IngestEngine::confirm` measures from the new answer when the source's answer leaves `Free`, so a landing after the attach counts as an advance.
7. **Where O29 and O30 went.** O30 was built at the start of step 3. O29 waits for step 4, beside the attach at a process start.

**What the tests prove.**

- **`ingest::tests::attach`, the quiesce.** A lone builder holds the build of change 1. An attach asked then waits for it: nothing else is granted, and nothing reaches the detached lane. Once released, the lane is attached and `Attached` is its first item; the changes that waited follow it, and change 1, built while detached, never reaches it.
- **`ingest::tests::attach`, H2 and stale attempts.** A run that died between the attach and `Attached` gets `Attached` published again. Stale attempts publish nothing. A detached lane gets no item, no completion, and its backfill acknowledged unserved. A walk only it asked for is cut. m6 holds.
- **`progress::tests::detach`, the attach cases.** Attempts, `Again`, `Stale`, and `routed_ever` across `forget_all`.
- **`tests::detach`, the sink engine.** Items before `Attached` are dropped and those after it written. An older attempt's `Attached` is dropped. A second `Attached` of the same attempt drops nothing. A run restarted after the lane opened never writes the earlier item, and writes the later one, delivered again.

**Not proven.**

- The quiesce with several builders and the budget's fold.
- An attach while another sink's walk of the index runs (its grants pause; untested).
- An attach request lane that stays full.
- Any of it against a live Postgres or OpenSearch.
- That a lone builder keeps its batching after an attach (W2), beyond the quiesce test's three changes.

### After the review of step 3

The independent review (`target/review-482-step3/REPORT.md`) found nothing blocking. What it asked, and what was done:

- **S1, `Attached` was not the exact boundary after a run death.** The next run's builder could publish to the lane before the request that the dying run left came back. **Chosen: the builder publishes first.** `Progress` keeps `unannounced` beside the attempt; `attach` sets it, and the builder clears it once `Attached` is out. Every run's builder publishes `Attached` for each unannounced lane of its index before its first grant (`announce`).
  - **Rejected:** treating an unannounced lane like a detached one until the request returns. Its changes would be skipped while the lane counts in the minimum, which a `reattach` (no backfill) would lose.
  - **The guard,** `a_run_dying_between_attach_and_attached_publishes_nothing_before_it`, asserts the lane's whole sequence and that nothing stays registered unwritten on it. The H2 test now asserts `Attached` is first.
- **m1:** R7 inside a walk another sink shares, and a backfill for a detached lane alone starting no snapshot. The three surviving mutations now fail a test each.
- **m2:** the test spy marks an `Attached` forwarded while the progress still skips its lane, so R8's order reversed fails every attach test.
- **m3:** `fence::tests::a_lane_attached_again_serves_its_writes` (step 4 narrows it).
- **m4:** a live-only `rebuilding` record beside a marker on serving is detached again (O30).
- **m6:** `take_attached`'s check is a `debug_assert!` over R6's whole list: discards, window, held, settled-unacknowledged, settling, in flight. `advance` now acknowledges settled held items before it.
- **m7:** the per-item cost, said exactly. The publish paths add one `any_detached()` load each; `choose`, a bool; the sink's `queue`, an enum match per item; `advance`, a few emptiness checks. The builder's `select!` gains an `attaches.recv()` arm built and polled every turn. While any lane anywhere is detached, the publish paths and `cut_detached_walk` take the registry lock per requested lane per turn, in every builder. CI's in-process gate passed on the step 3 head.
- **The quiesce beside another builder** (not proven before): `an_attach_beside_another_builder_under_a_budget_pauses_only_its_index`. It has two indexes, four build permits, a 64 KiB budget with 20 KiB documents, and builds held on both. The other index keeps publishing while the attach waits, nothing deadlocks, and the attached index resumes after `Attached`.
  - With one build permit, the held build holds the engine's only permit, so the other index waits too. That is the permit, not the quiesce.
  - That the budget refused a batch during the attach is not asserted.

### Step 4: the rebuild and the reattach, as designed before the code

**What reaches it.** Still only operations, no threshold (slice 4):

- `reindex` of a detached index (R13) starts a rebuild;
- `flusso reattach --accept-gap` reattaches a `backfill = false` lane (O16).

Both are user-visible from here: the `reattach` command and endpoint, the `409` of O20, and `/status`.

**The rebuild** (r0–r4, §5), in the sink engine, at the lane barrier:

1. **r1, `Sink::begin_rebuild` (new).** One call does three things:
   - deletes every generation but `serving` (r1a); a refused delete fails the attempt before anything is read;
   - moves the marker past every existing generation, unseeded, with no checkpoint, keeping `detaching_from`, conditionally (r1b; #477's reason: a failed attempt's walk mark is never read again);
   - writes the record `rebuilding`, `attempts + 1`, `last_attempt`, conditionally (r1c).

   It returns the new `attempts`, which is the attach's attempt. **S2:** the record says `rebuilding` before `Attach` is sent, so a staging after the attach finds `rebuilding`, never `detached`.
2. **r2.**
   - `ensure_index` creates the generation;
   - the fence hears `on_index_rebuilding` and goes from `Detached { through }` to `Rebuilding { through }`;
   - `LaneState.rebuilding = Some(attempts)`;
   - `Request::Attach { attempt: attempts }` is sent.
3. **r3.**
   - The sink engine takes `Attached`: the lane opens and stays rebuilding;
   - it publishes `Backfill` under a new ticket, which replaces the cut outstanding entry (m5's half for the rebuild);
   - the fence stays `Rebuilding` until the seed (**S3**, O2), never `Writes`.
4. **r4.**
   - The seed's `mark_seeded` deletes the record after the marker's `PUT`, on the version it reads (OpenSearch; the port says so);
   - `rebuilding` is cleared;
   - the fence goes to `Writes` at `on_index_seeded`.
   - #479's rule holds: a seed refused by a block on `serving` parks the lane.

**A kill between the record write and `Attached`.**

| Killed after | The restart finds | What happens |
| --- | --- | --- |
| r1 (record `rebuilding`, no attach) | `rebuilding`, marker unseeded on a generation that may not exist | the process-start attach below |
| r2 (generation created, `Attach` sent) | the same | the same |
| r3 (`Attached` taken, walk running) | `rebuilding`, a walk mark | the same; the walk resumes from its mark |
| r4, before the record's delete | `rebuilding`, marker seeded on a newer generation | finished: the record is deleted (`AtStaging::Finished`) |

**The process-start attach of a `rebuilding` record.** `AtStaging::Rebuilding(attempts)`:

- `ensure_index`, R9's delete, the fence `on_index_rebuilding`, the lane `rebuilding`, the backfill asked (its walk resumes from its mark);
- the lane is attached already: `Progress` starts with every lane attached;
- under `--skip-backfill` or `backfill = false`, **O26**: `Sink::demote` (new) puts the record back to `detached`, moves the marker explicitly past every generation, forgets the walk and deletes the partial, and the index is staged detached.

**O27.** The rebuild's `Backfill` goes to the inbox, which is read only while no walk runs, so it waits behind another sink's walk of the index, with the lane attached meanwhile. Existing behaviour; a test pins it.

**O20(c), `reindex` of a rebuilding index.**

| The lane is | The engine | The HTTP edge |
| --- | --- | --- |
| waiting for `Attached` | ignores it and logs | `409 "the index is being rebuilt (starting)"` |
| open, rebuilding | ignores it and logs | `409 "the index is being rebuilt"` |
| parked or restaging, rebuilding | detaches again (d1–d5, `attempts` unchanged) and rebuilds at once | `202` |

A reindex naming no sink goes on where it applies, and its `202` lists the lanes it skipped. `/status` carries what the edge reads: per lane, `rebuilding { attempts }` beside its health, and `attaching`.

**`reattach --accept-gap`** (O16, §6a):

- **The surface:** `DaemonControl::reattach(index, sink)`, then `SinkControl::Reattach { index }`, behind `POST /indexes/{name}/reattach?sink=&accept_gap=true` on the private surface, and `flusso reattach <index> --sink <name> --accept-gap`. The refusals and their HTTP codes are §6a's.
- **The steps, at the lane barrier:**
  1. read the records: `live_only`, the marker on `serving` and seeded, the lane detached;
  2. `ensure_index`;
  3. `forget_detachment`, the commit point (**S2** for the reattach: no record is left to detach it again);
  4. `Attach` under a new attempt;
  5. on `Attached` the lane opens as an ordinary lane. The cut backfill entry is cleared (**m5**), so a later `reindex` is staged.

**S3 for the reattach, the gap.** The owner's rule, built literally:

- **The gap** runs from the fence's frozen position at the detach (`from`) to the source's answer for the lane at the moment it opens (`to`).
- **What a wait gets:**
  - a position at or before `from` is vouched as before;
  - one inside `(from, to]` is refused at once, as `Unserved::Gap`, naming the gap;
  - one after `to` is vouched as usual.
- **Exact inside the process:** the fence computes both ends itself, from its own source.
- **Durable across a restart:**
  - the sink engine writes the gap's two ends as the source's own text into the seed marker (`gap: { from, to }`, through `Sink::record_gap`, new). It gets the text from a seam the daemon hands it (`SinkEngine::with_positions`: the source's `reached` and `format_position`);
  - staging reads the gap back (`Detachment.gap`), and the fence hears it as `on_lane_gap`.
- **The crash window.** A crash between step 3 (record gone) and step 5 (`Attached`) leaves `to` unknown. The next process's lane starts attached: its gap ends at the position the slot resumes from at that start. That needs a read-only `ChangeCapture::resume_point()` (Postgres: the slot's `confirmed_flush_lsn`), read at staging. If that read turns out not to be exact, the build stops there and reports the options.

**O29: the record read once the slot is held.**

- **When.** The ingest engine's run, after `live()` opened the stream and before the first item is published, asks every sink engine to read each index's records again (`SinkControl::Reread`, answered over a oneshot), and waits.
- **What it changes.**
  - A lane the records say is detached is adopted as detached;
  - slice 1's put-back of a half-moved marker happens only at this read: the staging read takes `put_back: false` (`Sink::detachment` gains the argument);
  - since every read before the slot is held is only advisory, a half-moved marker that staging found is left for this read.
- **Cost.** One `_mget` per index per sink per live start.

**m7 of step 2.** `/status`'s `committed` lists the detached lanes it leaves out (O21), and the top level lists `detached: [{index, sink}]` (§9).

**Tests first, at least:**

- `an_attached_lane_stays_attached_across_a_sink_run_restart`, through the real staging, over a stand-in that keeps its records (S2);
- `a_fence_on_a_rebuilding_lane_vouches_for_no_change_dropped_while_detached` (S3);
- `a_reattached_lane_vouches_only_from_its_attach`, inside the process and after a restart (S3);
- `a_reindex_after_an_attach_is_staged` (m5);
- the kill walk r1–r4, and the process-start attach;
- O26's demotion, O27's wait, and O20(c)'s three rows;
- the `409`s at the edge;
- the reattach's refusals and steps;
- O29's adopt and its put-back.

### Step 4 as built

Every item of the design above is built: the rebuild to the seed, the process-start attach, O26, O27, O20(c) with its `409`s, `reattach --accept-gap` (endpoint, CLI, `DaemonControl::reattach`), S2, S3 for both, m5, O29 and m7. What the code changed in the design:

**S3's two ends need no new source method.**

- **The rule.** Both ends are `ChangeCapture::reached` of what the lane landed (`Progress::lane`), at one moment. `engine::SourcePositions` prints it as text, and the daemon's `CapturePositions` wraps the source.
- **`from`** is read when the detach starts, before the lane leaves the minimum. It goes into the record (`Detach.through`, `Detachment.through`) and onto `LaneHealth::Detached { through }`. The fence freezes there, at the lower of that and its own answer. So what a reattach later refuses starts exactly where vouching stopped.
- **`to`** is read when the sink engine takes `Attached`, before the lane opens. `Progress::attach` makes the lane answer through the newest change routed, or before its first item since: exactly "everything at or before it was dropped, everything after it reaches the lane".
- **At a restart** the lane starts attached, with nothing landed: `reached(Idle)` is the source's horizon at the stream's start, which is the slot's `confirmed_flush_lsn`. That is the same rule, so `ChangeCapture::resume_point()` was not needed.
- **When the restart completes it.** The source only answers once `live()` opened, so the read again of O29 completes the gap (`known_gap`), before the lane writes anything.

**The gap is a `Sink` method after all.**

- `Sink::record_gap` is a partial `_update` of the seed marker, `gap: {from, to}`. It is read back as `Detachment.gap`.
- `ensure_index` of an existing generation never rewrites the marker, so the gap outlives restarts. A seed or a reindex rewrites the marker whole and drops it, which is right: the index then holds every change.
- §6a's "no new `Sink` method" predates S3.

**The reattach's steps** grew one write each side of the commit point:

1. read the records;
2. `ensure_index`;
3. `record_gap { from, to: None }`;
4. `forget_detachment`;
5. attach.

At `Attached`, `to` is set, the fence hears the whole gap (`Observer::on_lane_gap`) before the lane opens, and the sink keeps it. A failure there fails the run, and the restage completes the gap at the same `to`, since nothing of the lane landed.

- **A second reattach** adds a gap to the list the marker keeps (`gaps`, after the review of step 4). Each gap is exact, and positions between them are vouched as usual.
- **The fence answers a gap with `FenceOutcome::InGap`, and `/wait` with `409`, `reason: "gap"`,** each lane naming its gap. It is not `503`, since no retry helps. The query SDK already refuses a `409` at once (`FenceRefused`) with the body, so it needs no change.
- **An unknown `to`** refuses everything past `from`, and an unknown `from` everything up to `to`. The seed clears the gap.

**O29 is a counter, not a control the ingest engine waits on.**

- **Why.** Waiting on every sink engine would stall every sink behind one whose destination is down: its run never answers, and it can't be told from a slow one.
- **The counter.** `engine::Rereads` counts the slot's openings. The ingest engine calls `live_opened` after `live()` and before publishing.
- **The read.** A sink engine whose last full read of the records began before then (`reread_due`, one atomic load) reads them again in `advance`, before any grant, and again before the grant after `fill`. `Run::reread`:
  - takes as detached a lane the records say detached, or rebuilding by another process;
  - detaches again on O30's rows;
  - completes a gap.
- **Staging counts as the read** (`records_read`).
- **The fence** vouches for no lane of a sink that hasn't read since (`Rereads::current`), and answers `Unserved::Starting` meanwhile. A sink that is down holds the slot and vouches for nothing until it reads; the others go on.
- **The put-back stays at staging.** `Sink::detachment` takes no `put_back` argument, and this read calls the same `Sink::detachment`, so it puts a half-moved marker back too. If that undoes another process's move and its record lands after, the record sits beside a marker on serving. The next read or staging then meets O30's row and detaches again. Nothing is lost meanwhile, since this process holds the slot.

**The edge.**

- The reattach refusals `409` "the lane is not detached" and "being attached already" come from `Status::reattach_refusal`. "Can rebuild" comes from `DaemonControl`, which now knows the `backfill = false` sinks (`ControlError::CanRebuild`).
- The record checks of §6a (not `live_only`, marker not on serving seeded) are made by the engine only. The edge reads no record, so it answers `202`; the engine logs the refusal and the lane stays detached.
- `/status` shows the gap per lane (`gap: {from, to}`) until a seed, across restarts, instead of §6a's `reattached: {at, gap_since}`.

**Tests**, beyond those the design listed:

- the engine's `a_reattach_keeps_the_gap_deletes_the_record_and_attaches_the_lane`, `a_reattach_is_refused_where_it_does_not_apply`, `a_reattach_cut_short_before_its_attach_completes_its_gap_once_the_slot_is_held`, `a_second_reattach_keeps_both_gaps_exactly`, `a_lane_the_records_say_detached_once_the_slot_is_held_is_taken_as_detached` and `a_staging_once_the_slot_is_held_counts_as_the_read`;
- the fence's `a_reattached_lane_vouches_only_from_its_attach` (in the process and after a restart), `an_open_gap_refuses_everything_past_its_start_and_a_seed_ends_it`, `a_detach_freezes_where_the_sink_recorded_it` and `a_sink_that_has_not_read_its_records_since_the_slot_opened_vouches_for_nothing`;
- the OpenSearch sink's `a_reattach_keeps_its_gap_with_the_seed_marker`;
- the CLI's `a_wait_inside_a_reattachs_gap_is_refused_with_409`, `a_reattach_is_refused_unless_it_applies_and_queued_when_it_does`, `status_shows_a_lanes_gaps_until_a_seed` and `a_reattach_without_accept_gap_is_refused_before_any_request`.

**Not proven:**

- No live run: there is no Postgres or OpenSearch e2e of a rebuild, a reattach, the gap's `_update`, or two overlapping processes (O29). Every test runs over stand-ins.
- The restart's `to` relies on the Postgres capture seeding its horizon from `confirmed_flush_lsn` at the stream's start, and on the read happening before the lane lands anything. Both are argued above and tested only with stand-ins.
- The second read-again check, before the grant after `fill`, has no test that fails without it: the first check catches every case the tests make.
- A `record_gap` failure at `Attached`, and a gap end that doesn't parse (read as unknown), are untested.

### After the review of step 4

The review (`target/review-482-step4/REPORT.md`) found one blocker, five should-fix findings and seven minors. Every item has a test that fails without its fix, then the fix, then a commit:

**B1. The O29 gate masks only `Writes`, and a read wakes the fence.**
- **Precedence.** Gap first, then `Detached`, `Rebuilding` and `Backfilling` as themselves, then `Starting`, then `Writes`. The middle three vouch only for what this process saw applied, or for nothing, and another process's detach can't take that back.
- **What `Starting` still vouches for.** An unread lane that serves its writes still vouches for what it had reached when the slot last opened (`Furthest.at_open`, frozen on `Reread::Opened`). Those writes are in the index, and no detach removes them.
- **The wake.** `Rereads::subscribe` lets the fence freeze `at_open` at each opening and wake its waiters at each read.
- **A sink whose destination is down** therefore answers `starting` only for positions past what it applied before the reopening. Earlier ones pass, as on `main`.

**S1.** O26 demotes only at a process's start: a rebuild running in this process (`LaneState.rebuilding` set) survives its sink's re-staging under `--skip-backfill`.

**S2.**
- **The new park reason.** A rebuild whose seed meets #479's retire block parks with `ParkReason::SeedBlocked` (kind `blocked`, as before).
- **The engine** ignores a reindex of it.
- **The edge** answers `409`, quoting the block and its exits (`Status::reindex_refusal`, now a `String`).
- **Why.** The complete new generation waits for the block instead of being deleted and rebuilt into the same block (O28's cost).

**S3.** `a_reattach_killed_before_any_step_comes_back_with_its_gap_or_detached`. It fails when the record is deleted before the gap is kept.

**S4.**
- **(a) The operator hears the engine's verdict.**
  - `SinkControl::Reattach` carries a `Reply<ReattachVerdict>`, and `DaemonControl::reattach` returns its receiver.
  - The endpoint waits for it, at most `REATTACH_VERDICT` (30 s), and answers:
    - `Started`: `202` with `started: true`;
    - `Refused(why)`: `409` with the reason, which names what works instead;
    - `Failed`: `503`;
    - a run that ended first (a stop, or a failure): `503`, "ask again";
    - no verdict in time: `503`, "may still start".
  - This also covers m6: a stop between the request and the reattach's start answers `503`, not a blind `202`.
- **(b) A `detached` record that isn't live only, found by a start on a `backfill = false` sink,** is put back live only with `Sink::demote(mapping, true)` (R14 for a detached record): its marker returns to the serving generation, so a reattach applies.
- **The refusals name the command that works:**
  - a reindex of a detached index on a live-only sink: `409`, "flusso reattach … --accept-gap";
  - a reattach on a sink that can rebuild: `409`, "flusso reindex … --sink …".

**S5.**
- Every grant now goes through `grant_once_read`, so the records are read before any grant after the slot opened again, the ordered sink's first `fill` included.
- `a_slot_reopened_while_a_reindex_stages_is_read_before_the_items_it_brought` fails without the check.

**The gap list.**
- **Where it lives.** The marker keeps `gaps: [{from, to}, …]` (`Sink::record_gaps` writes the whole list, replacing an older single `gap`; `read_gaps` reads either form), and so do `Detachment.gaps`, `Observer::on_lane_gaps`, the fence and `/status`.
- **What a reattach writes.** It adds its gap; at `Attached` it completes the last one.
- **What the fence answers.** A position inside any gap is refused with `reason: gap`, naming that gap; one between two gaps is vouched as usual.
- **What clears it.** A seed or a reindex.

**Minors.**
- **m1.** A rebuild resumed at a start calls `on_index_rebuilding` before its backfill is asked, so it serves `rebuilding`.
- **m2.** Built as §6a designs it:
  - a rebuild that can't begin is owed (`LaneState.rebuild_owed`) and tried again on the staging backoff (`retry_rebuilds`; `lane_wake` wakes for it);
  - the detached lane's reason says why and when it is next tried.
- **m3.** `a_lane_another_process_began_rebuilding_is_taken_as_detached_once_the_slot_is_held`.
- **m4. A start settles every gap end the source can't read** (`known_gaps`, `SourcePositions::parses`), logged loudly, and keeps the list again:
  - **`to`:** unknown, or one that doesn't parse, becomes where the lane stands at that start. That is sound because everything after it reaches the lane, which has landed nothing since, so the gap still covers what was dropped. It may over-refuse up to that point.
  - **`from`:** one that doesn't parse becomes unknown. The gap then reaches back from its `to`, refusing only past positions.
  - So nothing refuses forever.
- **m5.**
  - **Stopping first:** a reindex or reattach while stopping is `503 stopping`, before any other refusal.
  - **`reason`:** the "can rebuild" `409` carries one.
  - **`reattach --sink`:** given on the command line only, with no `FLUSSO_SINK`. This is an exception to the every-flag-reads-env rule, since an env var shouldn't pick the lane a reattach drops changes on.
- **m6.** Covered by S4(a).
- **m7.** The put-back wording is corrected above.

**Not proven.** Unchanged from step 4, plus:
- the B1 fix against CI's live `a_fence_holds_while_the_index_is_seeded_or_rebuilt` (no Docker locally; CI decides);
- the verdict wait against a real sink engine under load;
- `SeedBlocked` on a live OpenSearch.

### After the re-check of step 4's fix round

The re-check (`target/review-482-step4-fixes/REPORT.md`) found one blocker, three should-fix findings and five minors. Each got a test that fails without its fix (the reviewer's probes, turned into tests), then the fix:

**B1'. The window at every opening, and what `at_open` is (`757a9292`, with m-a).**
- **The invariant** (stated in `fence.rs`'s module doc). From the moment a new stream may answer until the sink has read its records, a lane vouches for nothing past what this process applied under the previous stream.
- **One way to open.** The ingest engine opens the stream only through `Rereads::open(source.live())`. Before the future runs, every sink counts as unread and the fence hears `Reread::Opening`, while only the previous stream answers.
- **`at_open`.** At `Opening` the fence takes each lane's answer from that previous stream, fresh, and freezes it as `at_open`. A cached answer lower than what the lane applied only vouches less (m-a).
- **Which reads count.** A read counts only when it began once `open` returned Ok (`read_begins`, the `held` epoch). One begun during the opening does not. A failed opening counts no read until one succeeds.
- **Guards:**
  - `a_reached_answer_taken_before_the_opening_is_counted_never_vouches_past_the_old_stream`;
  - `a_wait_between_the_stream_answering_and_the_opening_vouches_nothing_new`;
  - `a_read_begun_during_the_opening_does_not_count`;
  - `the_answer_frozen_at_an_opening_is_taken_fresh_from_the_previous_stream`.

  The first two fail when the old order (open the stream, then mark unread) comes back.

**S-a. A gap end is settled only while this process holds the slot (`18d9479c`, with S-b).**
- **Who settles.** A staging needs no slot, so it never settles an end; it hears the gap as kept and flags it. Only a read made while `Rereads::holding()` settles one (the stream of the last opening is open; the ingest run's `Held` guard clears it when the run ends). Until then an unknown `to` refuses past `from`.
- **The write.** `Sink::record_gaps` takes a decision over the list kept, not a list. The OpenSearch write is conditional on the marker's `if_seq_no` / `if_primary_term`, and reads again and decides again on a conflict, so another process's settled gap stands.
- **Guards:**
  - `a_staging_while_the_stream_is_closed_never_settles_a_gap_end`, which fails if the slot check is dropped;
  - `a_gap_write_meeting_another_writer_reads_again_and_decides_again`, which fails if the write is unconditional.

**S-b. One gap per detach, and the truth on a failed delete.**
- **One gap.** A reattach reuses an open last gap that starts at its record's `through`, instead of adding a second.
- **A failed delete is read again.** A record still there is `Failed`. One gone goes on as `Started`.
- **What `Failed` says.** Nothing accepted is lost, and `/status` tells whether the reattach took effect.
- **Guards:** `a_failed_forget_then_a_reattach_keeps_one_gap_and_completes_it`, `a_record_delete_that_landed_although_its_call_failed_goes_on`.

**S-c. A detach adopted while its sink is unread freezes at `at_open`, never at the new stream's start (`42ba878b`).** Guard: `an_adopted_detach_without_its_through_vouches_for_nothing_new`.

**Minors.**
- **m-b (`3838f072`).** The edge's `Failed` and timeout answers are tested (`PrivateState.reattach_wait` lets a test shorten the 30 s).
- **m-c (`df7eb7b7`).** A gap entry that can't be read reads as one with both ends unknown, logged loudly. It refuses every position until settled, and `/status` shows it.
- **m-d (`b8463702`).** The kill matrix asserts the fence's freeze (detached rows: at `1`) or open gap (attached rows), and has a row for a death after `Attached` before the gap is saved.
- **m-e.** The wording below and in `CLAUDE.md` and `pipeline.md`.

**Corrected wording.**
- **m4's reading** is "a read while this process holds the slot settles an unknown end where the lane stands". It is never "a start" or "a staging".
- **B1's `at_open`** is the fence's answer from the previous stream taken as the opening begins.

**Not proven.** As before, plus:
- B1' and S-a live, with two overlapping processes;
- the conditional `_update` against a real OpenSearch.

### After the re-check of the second fix round

The re-check (`target/review-482-fix-round-2/REPORT.md`) found the same place a third time: what an unread sink's lanes may vouch for. So the rule is now inductive and guarded by a property test, not patched again.

**B1'' (`17a06e85`). The rule.** What a lane may vouch for while its sink is unread only ever comes from answers the fence gave while that sink counted as read.

- **How it is kept.** `Rereads::open` takes, before its count moves, which sinks counted as read during the stream being left (`ReadBefore`), and hands that in `Reread::Opening { read }`. Only those sinks' lanes take a fresh answer into `at_open`; every other lane keeps what it froze before. For a sink that never read, that is nothing.
- **Why this way.** It is the single ordering that matters, and it lives in one function. A frozen value kept up to date continuously while read would need every answer-taking path to know the read state.
- **The bug it closes.** Before it, a second opening (a stream that ended, or a `live()` failing and retried) before the read re-froze the first stream's start: the slot's confirmed position, past another process's dropped changes.
- **Tests:**
  - `a_second_opening_before_the_read_keeps_what_the_first_froze`;
  - `a_failed_opening_before_the_read_keeps_what_the_first_froze`.

**The property test** (`fence::tests::gate_property::a_lane_vouches_only_for_what_it_covered_while_its_sink_was_read`, proptest, 512 cases of 1 to 40 events, on paused time).
- **The events** (one sink, one lane, a source that answers nothing before the first opening):
  - an opening that succeeds or fails, its new stream starting 0, 10 or 20 past where the source stood. A start past what this process covered is another process that dropped the lane's changes and confirmed them; its records then say detached, and the next read adopts it;
  - a read of the records beginning, and one ending: it counts for when it began, and adopts a detach the records hold;
  - a change applied, only while the sink counts as read and the lane is attached, as the engine guarantees;
  - the source reading on with nothing to deliver;
  - a `/status` read;
  - a wait for a position.
- **The model.** It keeps its own count of openings and reads, and the highest position the source answered for the lane at a moment the sink counted as read and the lane was attached.
- **Safety.** The fence never passes a wait for a position the model never saw covered.
- **Liveness.** At the end, once read with nothing detached, a change applied is vouched for.
- **What it rediscovers when the fix is reverted:**
  - **B1''**, by itself: two successful openings, then a wait for 0;
  - **B1'**, the old order of telling the opening after the stream opened;
  - **S-c**, a detach adopted while unread freezing at the new stream's answer.
- **What it doesn't find.** B1's precedence (an unread sink masking every state, not only `Writes`) is over-refusal, not a soundness bug, so the property doesn't find it; its unit tests keep it.
- **Seeds.** The minimal failing cases are kept in `proptest-regressions/fence/tests.txt`.

**S-a' (`cc435aef`).** The attach takes its gap by its start, open or not, and only ever moves its end later. Another process may have settled it at a stale place first.
- **Why another process can.** `holding()` is only this process's belief. Postgres can free the slot server side (`wal_sender_timeout` after a partition, `pg_terminate_backend`) before the client sees its stream end, and a cleanly ended stream counts as held until the run ends.
- **Both orders are safe.** The conditional gap write plus this rule make either order of the two writers keep the later end.
- **Test:** `a_gap_settled_by_another_writer_before_the_attach_keeps_the_attach_end`.

**The untested claims (`e2c9b418`).** Each test fails under its mutation:
- the `Held` guard: `an_ingest_run_that_ends_no_longer_holds_the_slot`;
- a failed opening counting no read: `a_failed_opening_counts_no_read_and_settles_nothing`;
- the read that settles a gap a staging flagged: `a_restaging_while_the_slot_is_held_settles_the_gap_without_a_new_opening`.

**Minors.**
- **m-f (`d2c7e7d5`).** After an attach, gaps kept with an unknown or unreadable end are flagged, and the run's next turn settles them while the slot is held (`a_corrupt_gap_kept_before_a_reattach_is_settled_after_it`).
- **m-g (`d7fbfc40`).** The log says an unreadable entry settles by itself at the next read while flusso holds the slot, that a detached lane keeps it until attached, and that a reindex on a sink that can rebuild clears it. `/status` shows `settling: true` on a gap whose end is unknown.
- **m-h (`d712c062`).** `the_live_stream_is_opened_only_through_rereads_open` scans every non-test source under `libs`, `apps`, `sdk` and `dev`, and fails on a call to `live()` that isn't inside `Rereads::open`. It fails when the ingest engine calls `live()` directly. The docs say "the one place outside tests".
- **m-i (`09e68150`).** When a delete's re-read also fails, the operator hears both errors and that a reattach finds out. A reattach on a lane held detached with no record kept finishes what the earlier one began, with the one gap (`a_reattach_after_a_delete_that_landed_unseen_finishes_it`).

**Not proven.** As before. Nothing in this round runs against a live Postgres or OpenSearch.

### After the third re-check

The re-check (`target/review-482-fix-round-3/REPORT.md`) found a fourth hole in what an unread lane may vouch for, in the sink engine's half this time. The coordinator simplified the rule instead of patching it again (a decision under the owner's correctness-over-speed rule, reported to him). The branch was first rebased onto `main` at `7ddff7e5` (#486).

**The rule, one line.** A lane answers `starting` from the moment a new stream may answer until its own records were read under that stream. `gap`, `detached`, `rebuilding` and `backfilling` still take precedence as themselves.

**Why the frozen answer went (item 2, `5b5cc99a`).**
- **The problem.** Three of the four holes were in the convenience that let an unread lane vouch for what it applied under the previous stream (`at_open`):
  - B1': the window at the opening;
  - B1'': a second opening re-freezing;
  - S-c: a detach adopted while unread.

  Its own statement was not literally true either (S-1: the freeze took the max of every answer, unread ones included).
- **What went.** `at_open`, `ReadBefore`, the opening's freeze, and their tests.
- **What stays.** `Rereads::open` as the one opening, marking every lane unread before the stream can answer.
- **The cost, for the docs:**
  - after each opening, a `/wait` on an already-applied position holds until that lane's first read: one `_mget`, normally well under a second. `flusso-query` retries `503` within its deadline;
  - while a destination is down across an opening, its lanes answer `starting`.
- **The read is prompt.** An opening wakes every sink engine's run (`Rereads::openings`, a watch), so a quiet stream doesn't leave its lanes unread until the next write or idle tick (`an_opening_on_a_quiet_stream_is_read_at_once`).

**The four holes, for the record.**

| Hole | Where | What it let through |
| --- | --- | --- |
| B1' | the opening | an answer from the new stream taken before the lanes counted as unread |
| B1'' | a second opening before the read | the first stream's start re-frozen |
| S-c | a detach adopted while unread | a freeze at the new stream's start |
| B1''' | the read count | a staging that couldn't read one index counted as a read of the whole sink, so that index's lane vouched up to the new stream's start, over another process's drop |

The world property then found a fifth of the same shape: a rebuild begun while unread froze `Rebuilding` at whatever answer was held. Fixed with it, in `256fea15`.

**B1''' (item 1, `4453a87f`): reads count per lane, never per sink.**
- **The count.** `Rereads::read(&LaneKey, at)` and `current(&LaneKey)`. `SinkEngine::step` counts its lane only when its step got through: staging, a restage or a reindex alike.
- **The run's read.** `Run::reread` counts each lane it read. A detached lane counts without a read, since its state answers. A restaging lane is left to its restage path, so a bad index never fails the sink's run (#471).
- **An index whose read failed** stays unread and answers `starting` while its siblings go on. It is not written while unread: a restage, reindex or attach that returns across an opening reads it again before its lane opens (B-1 of the fourth re-check; before it, a restage whose read spanned an opening reopened its lane unread).
- **Tests:**
  - `a_staging_that_left_an_index_unread_counts_no_read_of_it`, during a stop too;
  - `a_read_again_that_missed_a_lane_counts_no_read_of_it`;
  - `an_unread_lane_never_holds_back_its_read_sibling`.

**The world property (item 3, `256fea15`): `fence::tests::gate_world`.**
- **The setup.** It runs the real sink engines over a stand-in sink:
  - two sinks: `audit` with `backfill = false`, which reattaches; `primary`, which rebuilds;
  - two indexes, so four lanes.
- **The events:**
  - openings that succeed or fail, one of them past another process's drop of one lane (its detach record left in the sink, every other lane applied by that process);
  - sink-run restarts whose staging can't read some indexes, which stay unreadable for the run's own read until made readable;
  - changes applied and made searchable per lane;
  - a reindex (a rebuild to its seed when detached) and a reattach, through the engines' controls, the test playing the index's builder (`Attached`, the backfill);
  - `/status` reads;
  - waits at both levels, on one lane or a whole sink.
- **The oracle owes nothing to the engine's count.** The stand-in logs every read of a lane's records that got through, with the opening it began under; a seed counts as one. It also holds the records and gaps. A wait for `p` on a lane passes only if the lane covered `p` while it held all of these:
  - its records were read under the current stream, with no detach record in them;
  - it was neither detached nor rebuilding;
  - no gap the sink keeps holds `p`.
- **Liveness.** Once a healthy lane is read, a change applied on it is vouched for on that lane alone, beside an unread sibling.
- **Size.** 512 cases of 1 to 40 events, about 2 s. The `gate_property` test over the fence alone stays.
- **What it rediscovers when the fix is reverted, each time:**
  - B1''' (a lane counted though its step failed);
  - `Fence::started` turning a detached lane back into a writing one;
  - the rebuild freeze;
  - S-c;
  - B1' (the old order);
  - the unread-writes gate turned off.

**Minors (item 4).**
- **m-1 (`99f35b57`).** A token only `Rereads::open` can make can't be true by construction. The source port (layer 1) would have to name a type only the engine (layer 2) constructs, and Rust has no such visibility: any constructor the port exposes is open to every caller, at the cost of every capture's `live` signature. So the tripwire stays. It now also catches `ChangeCapture::live`, and skips comment lines. Its doc lists what it misses and what trips it falsely.
- **m-2 (`2955cfb5`).** The log of a reattach finishing with no gap open says it refuses the lane's whole past, and that only a seed (a reindex on a sink that can rebuild) clears it.

**Not proven.** As before: no live run of overlapping processes, a rebuild, a reattach, or the conditional gap write.

### After the fourth re-check

The re-check (`target/review-482-fix-round-4/REPORT.md`) found two blocking holes in the sink engine's half. The world property, rebuilt to find them, then found four more of the same family. Every fix below has a unit test, and each was shown to fail it with the fix reverted.

**The rule, sink-engine side.** A lane is written, and opened again after any staging, restage, reindex or attach, only if its records were read under the current stream: by a read that *began* after the last opening returned. With no stream current (an opening in flight, or one that failed), nothing is written or seeded. The sink engine's module doc lists every path that opens a lane or writes, with the check it uses. There are two checks:
- **Opening a lane** goes through `store_lane`. A lane stored open while it isn't current makes the run's read due (`opened_unread`) before anything is granted.
- **Writing** goes through `grant`/`grant_once_read`, or `substituted` for a substitute, which is written past `grant`. Each reads again first while a read is due, and grants nothing while `Rereads::opening_outstanding`.

**B-1 (`43747de0`): a step whose read spans an opening.**
- **Restage.** It reopened its lane with records read only for the earlier stream. Fixed by the `store_lane` check. Tests: `a_restage_that_spans_an_opening_reads_again_before_its_lane_opens` and `a_restage_across_an_opening_never_writes_a_lane_another_process_detached`.
- **Reindex.** Already safe: it stages inline on the run, so an opening meanwhile makes the read due. Its two tests pass with the fix reverted; they pin it.
- **Rebuild and attach.** The run's read again counted a detached lane as read without reading it, so a rebuild's attach opened onto a record the previous slot holder had put back to detached. Now a detached lane is read too: a rebuild the records no longer show is dropped (`drop_rebuild`) and its attach opens nothing. `take_attached` reads again first when the lane isn't current. A read failing on a detached lane leaves it unread; it never fails the run. Tests: `a_rebuild_that_spans_an_opening_reads_again_before_its_lane_opens`, `a_rebuild_across_an_opening_never_writes_a_lane_another_process_demoted`.
- **Substitutes.** A parked lane's substitute is written past `grant`, from a `select!` arm that has no openings arm, so `substituted` reads first when due. No test makes this deterministic.

**B-2 (`9cb95b13`): another process's rebuild is demoted before its lane drops anything.**
- **The fix.** The run's read again puts a rebuild another process began back to detached (`Sink::demote`, the conditional write staging uses), reads the records again and decides on what it finds. A failed demotion, or a record still rebuilding after it, fails the run.
- **The stand-ins.** They keep a walk checkpoint, and a demotion forgets it.
- **Test.** `a_rebuild_another_process_began_is_demoted_before_its_lane_drops_anything`.

**S-1 (`12f2b096`).** `a_step_whose_read_spans_an_opening_counts_no_read_for_it`: the stand-in holds a staging's read open across an opening, and neither lane counts as read under the new stream. It fails when a step takes its epoch after its read (M4).

**S-2 (`e7452fb6`): the world property, rebuilt.** `fence::tests::gate_world::every_lane_vouches_only_for_what_it_holds`.
- **The world.** The engines write real items to a stand-in. Reads and `ensure_index` can be held and released across openings. A target can go missing, which restages its lane. A backfill completes only at a `Seed` event.
- **Other processes.** Another process may drop a lane, or drop it and begin a rebuild with a walk checkpoint. A new process can start over the same destination, redelivered from what the slot confirmed: the source's answer for the real `Progress`.
- **Writes are judged by the stand-in, as they are made** (a seed's marker included). A write fails the property in three cases:
  - no stream is current;
  - the lane was last read under an earlier stream;
  - it lands onto a detach record.
- **Waits are judged from what the test itself did.** That means its changes, its drops, the seeds it answered (with where their walks began), the reattaches it answered, and the writes the stand-in saw. It never uses the engine's gaps or counts. A wait for `p` passes only if every change at or before `p` was one of:
  - written;
  - read by a seed whose walk began after it;
  - given up by a reattach attached after it, and never for a wait at that change itself.

  A wait inside a range another process dropped never passes without such a seed.
- **The rebuild's answer.** While the store says a rebuild is under way on a lane the engine rebuilds, an unserved lane must say `rebuilding`.
- **Liveness, every lane.** Once everything is readable, the slot held and the runs settled, every lane that is not detached or rebuilding passes a wait for the newest position.
- **Size.** 1024 cases, about 2.5 s.
- **Mutations.** Three runs each, the regression file removed before every run:

  | Mutation | Found |
  | --- | --- |
  | B-1: `store_lane` opens an unread lane without a read | 3/3 |
  | B-1: a detached lane counts as read without a read | 3/3 |
  | B-2: another's rebuild taken as detached, no demotion | 3/3 |
  | M4: a step takes its epoch after its read | 3/3 |
  | M6: `attached` turns `Rebuilding` into `Writes` | 3/3 (by the `rebuilding` answer check) |
  | No write gate after a failed opening | 3/3 |
  | Any rebuilding record taken as ours while rebuilding | 3/3 |
  | A detach heard again during a rebuild drops it (fence) | 1/3 |
  | No confirmation hold for an unread detached lane | 1/3 |
  | Staging keeps a rebuild the records no longer show | 0/3 (its unit test catches it) |

**What the property found, fixed in their own commits.**
- **`a29ec918`: nothing is written or seeded while no stream is current.**
  - **The hole.** After a failed opening (another process may hold the slot), the engines kept writing and seeding lanes read only under the previous stream. A seed erased another process's detach record.
  - **The fix.** `grant` and the substitute arm wait while `Rereads::opening_outstanding` (two atomic loads). The opening that gets through wakes the run, which reads again first.
  - **Test.** `a_failed_opening_writes_nothing_until_one_gets_through`.
- **`acf2328a`: whose rebuild a record is, its attempts say.**
  - **The hole.** The read again, and staging, took any `rebuilding` record as this process's while its lane rebuilt. A second process that put this rebuild back and began its own was then seeded over by the old backfill.
  - **The fix.** Only a record counting the attempts this process began or took over is ours; any other is demoted first. A staging also drops a lane's waiting rebuild that the records no longer show.
  - **Tests.** `a_rebuild_another_process_began_since_ours_is_demoted`, `a_staging_drops_a_rebuild_the_records_no_longer_show`.
- **`9d3c1800`: a detached lane holds confirmation until read under the current stream.**
  - **The hole.** B-2 through the slot. A lane held detached left the minimum at once, across an opening too. If another process had begun a rebuild meanwhile, the next start resumed its walk over the changes this one let the slot confirm past.
  - **The fix.** `Progress::landed` answers `Idle`, which keeps the confirmed position, while a detached lane's records are unread under the current stream (`hold_while_unread`, wired by the daemon). It costs one atomic load while nothing is detached.
  - **Test.** `a_detached_lane_holds_confirmation_until_its_records_are_read_again`.
- **`52d1d3d1`: a detach heard again during a rebuild keeps it a rebuild.**
  - **The hole.** The fence turned a rebuilding lane into plain detached whenever the detach was announced again, as a re-staging does. The attach then made it `Writes`: it vouched over what the old generation misses, or answered `backfilling` (O2).
  - **The fix.** Only the rebuild's end (`on_lane_rebuild` with none) makes the lane plain detached.
  - **Tests.** `a_detach_heard_again_during_a_rebuild_keeps_it_a_rebuild`, `a_rebuild_that_ends_without_its_seed_leaves_the_lane_detached`.

**m-1 (`ea0b7387`): a fail-safe beside the tripwire.**
- **The count.** `ChangeCapture::streams_opened` counts every `live()` before it does anything; the default is `None`, and Postgres counts. `Rereads::opened` counts the openings begun.
- **The check.** The fence reads a lane as unread while the source has opened more streams than were counted, so a `live()` called past `Rereads::open` makes every lane answer `starting`.
- **Test.** `a_stream_opened_past_the_counted_openings_vouches_for_nothing`.

**What changes for a user.**
- **Writes pause while no stream is current.** While the source reconnects after a stream ended with an error, or another process holds the slot, nothing is written. What is queued is written once an opening gets through and the records are read again: normally one `_mget` per index.
- **Confirmation holds while a detached index's records are unread.** After every opening, until the read: normally well under a second. While that index's records can't be read (its meta index blocked or closed), the slot holds its WAL, as a down destination already does.
- **A rebuild answers `rebuilding` until its seed,** also after its sink re-stages.

**Not proven.**
- **The world is a model.**
  - The ingest engine isn't in it: changes are published by the test, and confirmation is the source's answer for the real `Progress`, not an engine's `confirm`.
  - Stops and `--skip-backfill` aren't events.
  - A process that keeps writing after another one took the slot, with no failed opening of its own in between, isn't modelled: that is fencing across processes, outside O29.
- **Live checks.** Still no live run of overlapping processes, a rebuild, a reattach or the conditional gap write. §10's checklist lists them.

### After the fifth re-check

Two independent reviews (`target/review-482-fix-round-5/`, `target/review-482-fix-round-5b/`) found holes in one family: what a process may do while another may hold the slot. Both recommended the same change of design, and **the owner decided it on 2026-11-26: narrow the overlapping-process guard (O29).**

**The rule.** This process destroys or rewrites no record another process may rely on unless it holds the slot. Plain writes behave as on `main`. Two halves:
- **No record is erased, or loosened, without the slot** (`Rereads::may_erase_records`). The slot counts as held while this process's stream is open, or once its run ended cleanly (the source finished its stream, or a stop drained and confirmed: the deployment ends after the drain). A sink engine with no ingest engine beside it (nothing to open a stream, as in tests) has no slot to hold and behaves as on `main`. An ingest engine wired to a `Rereads` marks it (`will_open`), so a process whose `prepare` keeps failing holds no slot from the start.
- **What the records say is read again once the slot is held,** as before: after every opening, before any grant, and decided by the same function staging uses (`decide`, run by `act_on_records`). The fence side is unchanged.

**Every write of a record or marker.**

| Write | Without the slot | Why, and what retries it |
| --- | --- | --- |
| a seed (`mark_seeded`) | gated | its marker is written whole, erasing any record. The `SnapshotComplete` is acknowledged unseeded and its backfill cut; the next read while the slot is held asks for it again (idempotent) |
| the failed-detach clear (`clear_failed_detach`) | gated | it deletes the `Detached` record it reads, which may be another process's. The lane stays parked with its record (a conservative detach); every turn tries again |
| a reattach (its gap, then `forget_detachment`) | gated | it deletes the record. Refused with the reason; the operator asks again |
| a reindex (`sink.reindex`) | gated | it moves the index to a new generation. Owed, the lane restaging with the reason, retried on the restage backoff; the other lanes write on |
| a rebuild (`begin_rebuild`), from a reindex, an owed rebuild or O20's restart | gated | it rewrites the detach record into `rebuilding`. Owed through `owe_rebuild`, retried by `retry_rebuilds` once the slot is held |
| a detach (`detach`), and staging's `DetachAgain` | safe | it only tightens a record, to detached |
| a demotion (`demote`): O26, R14, another process's rebuild | safe | `rebuilding` to detached, or detached to live only: tighter. Staging at a process's start does them before any stream, by design (#120) |
| a finished rebuild's record deleted at staging (`forget_detachment`) | safe | on the version read, and only beside a newer seeded marker: the record is obsolete |
| a gap added, or its end moved later (`record_gaps`) | safe | a gap only grows; an unknown end is settled only while the slot is held (S-a) |
| a walk checkpoint written or forgotten (`record_walked`, `forget_walked`) | safe | it claims only this process's own pages, and a walk resumes from it only where no record stands against the index; forgetting one only makes a walk start earlier |

No path hangs or fails the run because a gated action was skipped: each is retried as the table says.

**Per finding.**
- **First report, B-1 (`6907c928` test, `df9a5192` fix):** the write gate of the last round (`Rereads::opening_outstanding` in `grant` and the substitute arm) is gone; the seed is gated instead. `a_stream_that_ended_seeds_nothing_over_a_detach_written_since`, `a_failed_opening_seeds_nothing_and_asks_the_backfill_again_once_held`.
- **First report, S-1:** a stop after a failed opening drains its lanes again: `a_stop_while_an_opening_failed_still_drains_the_lanes`.
- **Second report, B-2 (`21f28210` tests, `cb501209` fix):** the clear waits for the slot. `a_clear_waits_while_no_stream_is_held`, `a_clear_waits_after_the_stream_ended`.
- **The audit (`6406f7d5`):** a reattach, a reindex and a rebuild wait for the slot. `a_reattach_is_refused_while_no_stream_is_held`, `a_reindex_waits_for_the_slot_before_it_rewrites_a_record`, `a_reindex_of_an_open_index_is_owed_while_no_stream_is_held`.
- **First report, B-3 (`6d3e9ce1` tests, `d3edbe47` fix): one decision.** Staging and the read again both call `act_on_records`, which reads `decide`:
  - another process's rebuild is put back to detached (conditional, read again and decided again) before the lane drops anything, whatever the lane's state;
  - it is taken over only at a process's first staging (no stream opened yet), its lane not held detached, with backfill to finish it;
  - a rebuild this process began that the records no longer show is dropped.

  Tests: `a_staging_of_a_lane_held_detached_demotes_anothers_rebuild`, `a_staging_demotes_a_rebuild_begun_since_ours`.
- **Found by the world this round (`c662dd41`):** before its first opening, a process whose ingest engine will open holds no slot either. `a_process_whose_stream_never_opened_deletes_no_record`.
- **CI red (`19881617`): a stand-in error, not a product hole.**
  - Each process in the world now has its own capture: it starts closed at the slot's confirmed position, stays closed after a failed opening, and answers `Landed::Idle` as Postgres does. CI's seed is kept.
  - The port states what the engine relies on: `reached` answers `Idle` at or past where the current stream started, so at or past anything an earlier holder of the slot dropped. `idle_is_never_answered_below_the_streams_start` pins the Postgres half.
  - `known_gaps` debug-asserts that a settled end is not below its start (m-4). It caught the engine tests' `Seqs` stand-in breaking the contract the same way, now fixed.
- **The world's write oracle (`19881617`, `0352ca5b`).**
  - It no longer restates the code's gate. The world keeps its own record of whether this process holds a stream.
  - It judges writes only while one is held. With none held, plain writes behave as on `main`.
  - It states the narrowed rule: no record of another process is erased, nor rewritten into anything but detached, while this process holds no stream.
  - New events: the stream ends and another process acts before the next opening begins; this process detaches a lane itself, its detach sometimes failing after the record lands.
  - Reverting a gate, three fresh-seed runs each: the seed 3/3, the clear 1/3 (its unit tests catch it every time), the reattach 3/3, the rebuild 3/3, `will_open` 3/3.
- **The six unguarded paths (second report, Part 2).**
  - `any` against `all` in the confirmation hold: `a_detached_lane_unread_holds_confirmation_while_another_detached_lane_was_read` (`b347fbb7`).
  - The read again in `substituted`: `a_substitute_arriving_with_an_opening_is_written_only_once_read` (`6a36a66f`). It runs 16 times, since the run's `select!` takes either arm first; without the read again it fails 3 runs of 3.
  - `take_attached`'s own read again first, and its skip of an `Attached` for a dropped rebuild: **removed** (`b347fbb7`). Both were redundant:
    - the attach opens its lane through `store_lane`, which makes the read due before any grant;
    - an `Attached` for an attempt the lane no longer waits for is already discarded where it is received.

    Every engine and daemon test passes without them.
  - The staging half of `acf2328a`: now B-3's decision, `a_staging_demotes_a_rebuild_begun_since_ours`.
  - A failed read of a detached lane never fails the run: `a_failed_read_of_a_detached_lane_never_fails_the_run`.
- **Minors.**
  - **m-1 and m-2 (`504acbc3`):** `readable_wait` and `streams_opened` each have their own doc again. `streams_opened` says it fails open for a capture that wraps another and doesn't forward it: a backstop beside the tripwire.
  - **m-3, equal attempt counts:** not changed. Two processes can each read a `rebuilding` record under the same attempt count as their own. With the narrowed rule only the one holding the slot can seed, demote or rebuild over it, so the ambiguity no longer loses changes.

**Removed by the narrowing.**
- `Rereads::opening_outstanding`, and its gates in `grant` and the substitute arm.
- `take_attached`'s read again, and its skip.
- The world's write rules built on the code's own counters (`attempts`).
- The test that a failed opening writes nothing, replaced by one that it seeds nothing.

**The window that stays open, for the docs agent.** Postgres may already have dropped this process's walsender while the process hasn't noticed: about a status interval on a reset connection, up to a TCP timeout on a silent partition. In that window the process may still make late plain writes into an index another process detached. That is the same as `main`. Real fencing of two processes (an owner lease in the sink's meta index) is a separate issue the coordinator files. One process per slot (Helm's `Recreate`) stays the supported shape.

**What changes for a user.**
- **Writes during a reconnect or a stop:** as on `main`. The lanes' backlog is written while the stream reopens, and a stop during a failed reconnect drains at once.
- **A backfill that completes while the slot isn't held seeds nothing.** It is asked for again once the slot is held, so an index stays backfilling a little longer after a reconnect that came mid-backfill.
- **A reattach while the slot isn't held** is refused, with the reason; ask again once flusso holds the slot.
- **A reindex while the slot isn't held** waits, and the index shows restaging with the reason. A rebuild, and the clear of a failed detach, wait the same way.
- **A process that can't reach Postgres at its start** (its `prepare` failing) makes none of these record changes until its stream opens.
- **Unchanged:** the fence (a lane answers `starting` from each opening until its own records are read), the read again after every opening, and the confirmation hold while a detached lane is unread.

**Not proven.**
- **The world is a model.** The ingest engine isn't in it: changes are published by the test, and confirmation is the source's answer for the real `Progress`.
- **Stops and `--skip-backfill`** aren't events.
- **Live runs:** none, still. §10's checklist lists them.

### After the sixth re-check

The review (`target/review-482-fix-round-6/`) found the narrowed rule held only where it was checked, and checked too early. The rule is unchanged; what changed is where it is checked.

**One place checks the slot (B-1, `b743f0c3` tests, `e74fdfcd` fix).**
- The gates checked `may_erase_records` at the top of an action, then awaited a read, an `ensure_index` or a gap write before the request. A stream that ended meanwhile was not seen.
- Every request that may erase or loosen a record now goes through `sink_engine/slot.rs`'s `Erasing`. It checks the slot immediately before the request is sent, after every await in its caller. A gated method returns `NotHeld` instead of sending.
- The sink's erasing methods are called nowhere else in the engine. `erasing_requests_go_only_through_the_slot` scans the engine's sources and fails on a call outside `slot.rs`, so a new erasing call can't be written without the check.
- **A reattach is two requests**: its gap, then the record's delete. A refusal at the delete leaves the gap written and the record standing; the lane stays detached and the reattach is refused with the reason. Asked again once the slot is held, it reuses that gap (one per detach) and deletes the record.
- Tests: `a_reattach_whose_stream_ends_mid_way_deletes_no_record`, `a_clear_whose_stream_ends_after_its_read_deletes_nothing`, `a_reindex_whose_stream_ends_while_staging_rewrites_nothing`, and the reviewer's two world cases, kept as their shrunk event lists (`a_reattach_waiting_in_ensure_as_the_stream_ends_deletes_no_record`, `..._in_its_read_...`; `31bd7da9`). A seed in a regression file replays another event list once the strategy changes, which S-3's new event did.
- **Still open, closed by #490 (an owner lease):** a request already sent when the stream ends, and the time before this process notices Postgres dropped its walsender. In both, `holding()` still says held.

**A seed refused for want of the slot is owed (B-2 and S-1, `3150526e` tests, `f8404d7f` fix).**
- Before: the seed's backfill was cut and asked again at the next read while held. A lane parked at that read was never asked again (B-2), and every cut walked the whole index again (S-1).
- Now: the snapshot's documents are written and acknowledged as before; only `mark_seeded` waits, owed per index (`OwedSeed`, with its ticket).
- It lands once the slot is held, nothing of the lane is in flight, the ticket still answers the index's target, and the records read again show nothing written over the index (no record, no gap kept: `written_over`). Otherwise the backfill is asked again from the first key.
- A seed owed while its lane was open stays owed if the lane parks later, and lands once it opens. *(Corrected after the seventh re-check: a seed met on a lane already parked is not owed; it is the lane's probe, and spun until B-2's fix.)* A detached lane drops it: its rebuild asks for its own. A new backfill request drops it too.
- No index is marked seeded with documents missing: an owed seed lands only on the target its snapshot wrote.
- Tests: `a_seed_met_without_the_slot_on_a_lane_parked_at_the_read_lands_once_it_opens` (the reviewer's probe), `a_seed_met_without_the_slot_lands_once_held_without_a_new_walk`, `a_seed_owed_over_a_gap_written_meanwhile_asks_the_backfill_again`, `a_failed_opening_seeds_nothing_until_an_opening_gets_through`.

**A demotion and a detach wait for the slot (S-2, `e8bb18e9` test, `369b645e` fix).**
- Both delete every generation but the serving one, and a demotion the walk checkpoint: without the slot that can be the rebuild the holder is walking.
- **Decided: the whole action waits**, not only its destructive half. A record-only tightening would need a new sink request, and it buys nothing: this process takes the lane as detached anyway.
- Staging and the read again that find a record asking for one take the lane as detached and leave its read uncounted (`Acted::Unheld`). The fence answers `detached`, confirmation holds while the lane is unread, and the read once the slot is held acts.
- A detach this process decides itself (a refused lane) leaves the lane as it was: parked, restaging or open, detaching at the next refused probe once held.
- **A process's start** stages before any stream: the ingest engine is built, and calls `will_open`, before the first staging. A backfilling sink still takes over a rebuild it finds, which writes no record. A live-only sink's demotion waits: the lane is detached until the first opening's read again demotes it, so the start ends fence-safe.
- Tests (each with another process acting in between): `a_staging_without_the_slot_leaves_anothers_rebuild_to_the_holder`, `a_start_staging_before_the_stream_opens_demotes_nothing`, `a_detach_with_no_slot_held_waits_for_the_slot`; each fails with its gate removed.

**The world's oracle (S-3, `9ef321b8`).**
- `touch` no longer exempts this process's own records. While this process holds no stream, another may hold the slot and rely on any record or gap of the index, whoever wrote it. A request is judged when either is there.
- `reindex`, `demote` and `detach` are judged too.
- A composite event ends the stream (another process dropping or rebuilding a lane) and then acts with no stream held: a seed, a reattach, a reindex, a restart, a detach or an unblock. Each gated request's unheld case is drawn often.
- The case count reads `PROPTEST_CASES` (default 1024).
- **Each gate removed, the property at its default count, regression file aside, 3 fresh runs:** `mark_seeded` 3/3, `forget_detachment` 3/3, `reindex` 3/3, `begin_rebuild` 3/3, `demote` 3/3, `detach` 3/3.
- **`PROPTEST_CASES=20000`, regression file aside, fixed fresh seeds (`PROPTEST_RNG_SEED`):** `13210131549416412653`, `9493254513836626041`, `12409552872641094006`, `11880150436224788299`, `17304850351738856557`. All pass, about 47 s each.

**The guards no test held (Part 5, `c718de99`).**
- **`reread`'s own gate: removed.** Its three callers check `reread_due` first.
- **`reread_due`'s `may_erase_records`: removed.** A read erases nothing, every request that may goes through `Erasing`, a gap end settles only while held (`known_gaps` checks `holding()` itself), and what a read does on its own only tightens. A read made with no slot held counts for the last opening that returned, which an earlier read already counted for; the next opening makes it due again.
- **`retry_rebuilds`' gate: kept.** Tried with no slot held, each refusal doubles the staging backoff, so the rebuild began up to a minute late once the slot was back. `an_owed_rebuild_begins_at_once_when_the_slot_comes_back` fails without it.

**Every record, marker and generation write.** This replaces the fifth re-check's table. Its one home in the code is `slot.rs`'s module docs.

| Request (OpenSearch requests) | Without the slot | Why |
| --- | --- | --- |
| `mark_seeded` (aliases swapped, every other generation deleted, the marker written whole, a finished rebuild's record deleted) | gated | the marker erases any record and the gaps. Owed |
| `forget_detachment`: the failed-detach clear, a reattach | gated | it deletes a record that may be another process's. The clear waits; a reattach is refused and keeps its gap |
| `forget_detachment` of a rebuild a newer seeded generation finished | safe | the record is obsolete, and the delete is conditional on the version read |
| `reindex` (the marker moved to a new generation) | gated | waits, its lane restaging |
| `reindex` at a `Fresh` source's staging | safe | before `prepare` creates the slot (#120): no slot exists for another process to hold |
| `begin_rebuild` (other generations deleted, the marker moved, the record rewritten) | gated | owed; `retry_rebuilds` waits for the slot |
| `demote` (other generations and the checkpoint deleted, the marker and the record rewritten) | gated | may delete the holder's rebuild. The lane is taken as detached, its read uncounted |
| `detach`, staging's `DetachAgain` (the marker moved, the record written, other generations deleted) | gated | as `demote`. A refused lane stays as it was |
| `ensure_index` writing the marker whole | safe | only when no marker names an existing generation: nothing anyone holds is there (no rows, so no checkpoint or gap means anything; `detaching_from` matters only with no record, and losing it leaves an unseeded generation to backfill while reads stay on the serving one). As on `main` |
| `ensure_index` deleting an unseeded generation whose `index.uuid` isn't the recorded one | safe | a write under auto-create made it, no process built it. As on `main` |
| the read putting a half-moved marker back | safe | only where the marker carries `detaching_from` and no record stands, on the version read. A detach between its two writes is the holder's (or one in flight, #490); its record landing after the put-back still says detached, and the next read detaches again |
| `record_gaps` | safe | a gap only grows; an unknown end settles only while held |
| `record_walked`, `forget_walked` | safe | a checkpoint claims only this process's own pages |
| `write` (bulk) | as on `main` | plain writes of what the lanes hold |

**Minors.**
- **m-2:** `Held` says why a clean end may seed: a live stream ends `Ok` only when the deployment is over (Postgres: only through a stop's `close_live`). Kept for `Ended::LiveEnded` because the daemon's tests end their mock sources so.
- **m-5:** a stop while no slot is held can't clear a lane parked unsure of its detach. The drain waits on it until the deadline and the process exits 1 naming the sink; the next start reads the record and decides. Stated on `start_clears`.
- **m-6:** the doubled doc prefix in `sink_engine.rs` is gone.

**The cost now.**
- **A seed met without the slot** costs a wait, not a walk: the index stays unseeded until the slot is held and the records are read again. Only when another process wrote a record or a gap over the index meanwhile is the backfill walked again, from the first key.
- **A demotion found at a process's start** on a live-only sink costs a lane detached from the start until the first opening's read again.

**What changes for a user, for the docs agent.** This replaces the fifth re-check's list.
- **Writes during a reconnect or a stop:** as on `main`.
- **A backfill that completes while flusso doesn't hold the slot** is written in full; only marking it complete waits until flusso holds the slot again. If nothing else touched the index meanwhile, it completes then with no second walk; otherwise it runs again.
- **A reattach while the slot isn't held** is refused with the reason; ask again once flusso holds the slot.
- **A reindex, a rebuild, the clear of a failed detach** wait for the slot; a waiting reindex shows its index restaging with the reason. An owed rebuild begins as soon as the slot is back.
- **A detach flusso decides while the slot isn't held** waits: the index stays as it was (parked, say) and detaches once flusso holds the slot.
- **Another process's rebuild met while the slot isn't held** is left alone; the index shows detached on this process until its stream opens, then the rebuild is put back to detached. *(Corrected after the seventh re-check: the rebuild may finish meanwhile, and this round's stand-in detached the lane with no record, so it dropped unrecorded changes; see B-1 there.)*
- **A process that can't reach Postgres at its start** changes no record until its stream opens.
- **A stop while flusso can't reconnect**, with a lane parked unsure of its detach, waits its deadline and exits 1 naming the sink; the next start settles it.
- **Unchanged:** the fence, the read again after every opening, the confirmation hold while a detached lane is unread.

**Not proven.** As after the fifth re-check: the world is a model (no ingest engine, no stops or `--skip-backfill` events), and there are no live runs.

### After the seventh re-check

The review (`target/review-482-fix-round-7/`) confirmed the chokepoint, the six gates and the owed seed, and found two holes in code the sixth round added. The world, given the events the review asked for, then found five more. The rule is unchanged.

**B-1. A lane never drops a change no record covers (`d2804315` tests, `9b4c486e` fix).**
- **The hole.** S-2's stand-in took a lane as detached without writing a record. Once the holder finished its rebuild or reattached, the read while held kept the lane detached (`KeepDetached`) and counted it, so the slot passed changes nothing recorded.
- **The stand-in, chosen as (c).** When what the records ask waits for the slot (`Acted::Unheld`), the lane parks `ParkReason::AwaitingSlot`:
  - never granted, never probed, its items held unacknowledged, its read uncounted;
  - the read once the slot is held decides: it detaches the lane under the record, or opens it with every held item still to write. No reindex is forced when the record went;
  - a restaging lane stays restaging, its own staging being the read that decides; a reindex that meets a stand-in is owed;
  - `/status` shows it `blocked`, "waiting for the slot: …", with no next probe.
- **Which items may be discarded.** None, by the stand-in. A lane detached at a read while held discards only under the record that read found, and the record covers what it drops. Items of a stream that ended stay unacknowledged; nothing confirms them without the slot, so the next holder's stream delivers them again.
- **What bounds it.** No stream is held while it stands in, so no WAL is pinned beyond what the lane held already. The first opening's read again decides before anything is granted; a read that fails fails the run, which restages, as for any lane. A stop releases its items unapplied, as a restaging lane's, so the slot stays behind them and the next start replays them.
- **(b), for a lane truly held detached** (round 4's m-3). When its records, read while held, show no record or a finished rebuild, it is recorded detached again with this process's place before its read counts (`Decision::RecordAgain`). The exception is this process's own reattach between its delete and its attach.
- **Tests:**
  - `a_lane_taken_detached_without_the_slot_never_drops_unrecorded_after_the_holder_finishes` (P1);
  - `…_opens_after_the_holder_reattaches`;
  - `a_lane_held_detached_whose_record_another_process_removed_is_recorded_again`;
  - `a_stand_in_writes_what_it_held_once_the_records_ask_nothing`;
  - `a_stop_while_a_lane_awaits_the_slot_drains_at_once`.

**B-2. A parked seed refused for want of the slot waits for an opening (`2da96343` test, `b60c68db` fix).**
- The refusal is no probe result. The lane keeps its seed out of the grant while no slot is held (`seed_waits_for_slot`), and the opening wakes the loop.
- `a_parked_seed_without_the_slot_waits_without_spinning`: the sink writes its other lanes, at most two probes in 10 s, and the seed lands after the opening.
- **Owed like an open lane's? No.** A parked lane's completion is #479's barrier: nothing behind it is written until the old generation retires. Owing it would acknowledge the completion and lift the barrier.
- **The two mechanisms now meet.** An owed seed that meets a blocked old generation parks its lane `SeedBlocked` with the owed seed as the probe (m-3).

**Every `NotHeld` return, and what paces it.**

| Where | What follows | Paced by |
| --- | --- | --- |
| a seed on an open lane | owed, its completion acknowledged | `land_owed_seeds` returns at once while no slot is held; the opening |
| a seed on a parked lane | put back, kept out of the grant (B-2) | the opening's wake |
| an owed seed | stays owed | as the first row |
| a reattach's delete | refused to the operator | the operator |
| the failed-detach clear | the lane parks again | its probe backoff, doubling |
| a reindex (control or owed) | owed, the lane restaging | the restage backoff, doubling |
| a rebuild | owed (`rebuild_failed`); `retry_rebuilds` doesn't try while no slot is held | the restage backoff, and the opening |
| a demotion or detach at a staging or a read again | the lane stands in (B-1) | the next opening's read |
| the same at a restage | the lane stays restaging | the restage backoff |
| a refused lane's detach | the lane stays as it was | its probe backoff: each refused probe asks again |
| O20's restart of a rebuild | skipped | once per reindex control |

The parked seed was the only refusal that left an item grantable at once.

**S-1. The chokepoint is the type (`7732231c`).** The engine holds its sink as `slot::Plain`, which exposes only the requests that erase nothing and makes `Erasing`. The `Sink` inside is reachable from `slot.rs` alone. Both of the reviewer's bypasses (UFCS, a rebinding) fail to compile; the text test is gone.

**S-2. The open window, as it is.** The check is per sink call, not per HTTP request. Inside `mark_seeded`, OpenSearch refreshes the whole new generation (seconds on a large index), swaps the aliases and drops other generations, then writes the marker and deletes the record. `begin_rebuild` and `demote` delete generations before their conditional writes; `detach` deletes after its record. For #490 to close: a sink call already begun when the stream ends, and the time before this process notices Postgres dropped its walsender. A lease must be checked before the call's last write.

**S-3. The world's other process now removes records (`0b69d631`).**
- New events: another process finishes a rebuild (its seed: the record and the gaps go), and reattaches a lane (the record goes, every drop since the detach kept as one gap).
- Two composite events draw them often: B-1's path, and a gap over an owed seed (G8).
- **What they found**, each pinned as its event list (a seed replays another list once the strategy changes) and fixed:
  1. **A fresh gap cuts the backfill asked before it (`f3f28265`).** A completion granted as an ordinary seed after the reopen seeded over another process's gap; the owed-seed check alone missed it. A gap not there at the last read now cuts the outstanding backfill, and any read that leaves it cut asks it again (`40a9fb83`: a read returning early never asked, and the index stayed backfilling).
  2. **A removed rebuild is over, in the run too (`f3f28265`, `ee7d5260`).** A read showing no rebuild where this process keeps one drops it in the engine; the run's copy kept it, so its attach opened the lane as a rebuild.
  3. **A rebuild is ours only on its generation (`d7a39d84` test and port field, `ee7d5260`).** Attempts restart at one once a record is removed, so a later rebuild by another process read as this one's. `Detachment.marker_target` (the marker's `index.uuid` on OpenSearch) must match the generation this process fills, when both are known.
  4. **A walk resumes from its checkpoint only where no gap is kept (`44dcebcd` tests, `019143b7` fix).** Pages before the mark may predate a drop the gap records, and the seed drops the gaps. With a gap kept, the walk starts from the first key.
- The world cannot see a hot loop: B-2 stays a unit test.
- **Runs, regression file set aside, fresh seeds, default 1024 cases, 3 runs each:**
  - each gate removed: `mark_seeded` 3/3, `forget_detachment` 3/3, `reindex` 3/3, `begin_rebuild` 3/3, `demote` 3/3, `detach` 3/3;
  - B-1 reverted, both halves: 3/3. (b) alone reverted: 3/3. (c) alone reverted: 1/3, by design: with (b) still in place the drops are recorded at the held read, so no fence hole is left, only a needless rebuild, which the B-1 unit tests catch.
- **`PROPTEST_CASES=20000`, head `019143b7`, fixed fresh seeds (`PROPTEST_RNG_SEED`):**
  - `gate_world` passes on `14218805350463452646`, `15928503808928437918`, `5384963679989482508`, `17358906531569235568`, `848786780898807453` (about 64 s each). Before the last two fixes, eight of ten seeds failed, which found holes 1 and 4.
  - `gate_property` (which now reads `PROPTEST_CASES` too) passes on `16884303745865196505`, `14296209190638332650`, `4183942642373773587`, `16477602683537168056`, `5598586321357990883`.

**Minors.**
- **m-2:** `land_owed_seeds` no longer checks `current`. The read again runs first in the same pass, and a lane it leaves open was read then. `answers` stays as a second net: every path that cuts a backfill asks again at once, which drops the owed seed. `an_owed_seed_whose_target_changed_meanwhile_never_lands` pins the scenario, and passes without `answers` for that reason.
- **m-3:** `an_owed_seed_meeting_a_blocked_old_generation_parks_its_lane`.
- **m-4, not changed.** Reporting `on_index_rebuilding` after the reindex would break #379's order: the fence stops vouching before the sink moves its writes. It would change nothing either: a refused reindex is owed, and an owed reindex reports rebuilding by design (review of #470, 5).
- **m-5:** the `rereads.rs` pointer, `reattach`'s doc, and the sixth section's two sentences, corrected in place.

**The user-visible list, for the docs agent.** This replaces the sixth re-check's; the eighth re-check's cut replaces it in turn.
- Writes during a reconnect or a stop: as on `main`.
- **A backfill that completes while flusso doesn't hold the slot** is written in full; only marking it complete waits. If its lane was parked on that completion, it waits for the slot without retrying. Once the slot is back it completes with no second walk, unless another process left a detach record or a gap over the index meanwhile; then it runs again from the start. A backfill resumed from its checkpoint also starts from the beginning when a gap is kept over the index.
- **An index whose records ask for something flusso may not do without the slot** (another process's rebuild to put back, a detach to record) shows `blocked`, "waiting for the slot: …". Nothing of it is written or dropped meanwhile. Once flusso holds the slot it detaches the index, or, when the other process finished or reattached it, simply goes on with nothing lost.
- **An index flusso holds detached whose record another process removed** is detached again (recorded), and needs a reindex or a reattach as before.
- **A seed blocked by the old generation (#479) that completed during an outage** shows the lane `blocked` with the block's reason.
- **A reattach while the slot isn't held** is refused with the reason. A reindex, a rebuild and the clear of a failed detach wait for the slot; an owed rebuild begins as soon as it is back.
- **A stop while flusso can't reconnect** releases a waiting index's changes for the next start; with a lane parked unsure of its detach, it waits its deadline and exits 1 naming the sink.
- **Unchanged:** the fence, the read again after every opening, the confirmation hold.

**Not proven.** The world is a model: no ingest engine, no stops or `--skip-backfill` events. A rebuild's identity rests on the marker naming its generation's target. No live runs yet: §10's checklist.

### After the eighth re-check: the cut

The eighth review (`target/review-482-fix-round-8/`) found four blocking holes, three of them in the reconciliation rounds 7 and 8 added (the stand-in lane, `RecordAgain`, generation identity, the fresh-gap cut), and the world failing on the head. The owner cut it on 2026-11-27 (O29 in §11).

**The decision.** flusso stops instead of reconciling. When a read of the records (after an opening, the slot held) finds that another process changed a lane's records since this process last read or wrote them:
- the run ends with an error naming the index, the sink and what was found;
- the CLI exits 1;
- nothing past that opening is confirmed, so the next start replays from the records.

Unchanged: the narrow rule and its gates (`slot::Plain`/`Erasing`), detach, rebuild, `reattach`, the read again after every opening, the start's staging decisions, the `will_open` gate, the fence.

**"Changed by another process", defined once** (`sink_engine/known.rs`, `change`). Conservative: what this process can't show to be its own, or unchanged, is foreign.

| Known (last read, or own write) | Found by a read | Answer |
| --- | --- | --- |
| nothing (never read by this process) | anything | Same: adopted |
| anything | read under the same opening, slot held throughout | Same: adopted |
| exact fields (record, attempts, `through`, gaps; the version when both are known) | equal on each exact field | Same |
| an own write's `Any` field (the sink decides it) | any value there | Same |
| an own detach whose call failed (outcome unknown) | `Detached` with this process's `through` | Same |
| anything else | | **Foreign**: the stop |

- **Own writes** tell `known` what they leave, inside `slot` (so no write skips it): a detach its `through`, a rebuild its attempts, a demotion and a forget their record, a seed no record and no gap, a gap write its list, a created generation an empty gap list. A reindex leaves nothing to expect: the ensure that follows creates.
  - A write of the record (a detach, a rebuild, a demotion) is **read back at once** while the slot is still held (`slot`'s `read_back`): the read learns the version the write left, so an identical record another process writes later (its own rebuild under the same attempt count, after finishing this one's) is another version. A stream ending mid-call leaves the version unknown, which is #490's window (a call in flight as the stream ends).
  - A write of the marker alone (a created generation, a gap) keeps the record's version.
  - With nothing read yet (every read so far failed), a write that decides the record (a detach, a delete, a seed) is what this process knows.
- **Own in-flight actions** are recognised without guessing: a failed detach by its `through`; anything else by a read that follows it under the same held opening.
- **Why one deployment never stops.** (The review of the cut found two exceptions, an own write whose outcome was lost and the seed's best-effort record delete; both closed in the next section.) Only this process writes the records. Each write it makes is told to `known`; a read with the slot held is adopted; a read across a gap in holding finds only what this process wrote. Proven by `a_single_process_never_stops`: the world with every event of one process (openings, ends, detaches, restarts, unreadable reads, held calls, reindexes, reattaches, new processes) and no other, asserting no stop.

**The stop, and confirmation.**
1. The read finds Foreign. It is not counted (`rereads.read` is never called), so the lane stays unread.
2. Confirmation holds while a detached lane is unread (`Landed::Idle`: the stream's start). Attached and parked lanes write nothing past their own read either (`grant_once_read`; an owed seed needs `current`), so the watermark holds them. Nothing past the opening is confirmed.
3. No erasing request goes out on the way: the run returns at once from the read (`Run::advance` and `stage` return `EngineError::ForeignChange`).
4. The supervisor returns the error from `run_all`: the ingest task and every sink task are aborted, no drain, no restart with backoff. The other sinks stop with the process.
5. The CLI's failed-run path logs it, prints it, and exits 1. `/status` shows it as the sink's error until the exit.

A widened hold (every lane, not only detached ones) was weighed and not taken: it would pin WAL for #471's lanes restaging on a stale target.

**What was removed** (from `904079ae`, the engine, port and adapter, outside the new `known.rs`: +455 / −534; `known.rs` is +575, 208 of them its tests):
- the stand-in lane (`ParkReason::AwaitingSlot`, `staged_awaiting`, `release_awaiting`, `sync_rebuild`, `lane_awaits_slot`);
- `Decision::RecordAgain` and its `detach_again` path in the read again;
- the removed-rebuild drop in the run (`drop_rebuild`, the stale-rebuild clearing in `act_on_records`);
- generation identity: `Detachment.marker_target` on the port and in the OpenSearch adapter, `rebuild_target`'s update at staging;
- the fresh-gap cut and its re-ask (`gapped`, `written_over`, `asked_target`);
- the reattach's no-record branch;
- every `Acted::Unheld` path that took a lane as detached without the slot and counted its read.

**What stayed, as simple waits:** the owed seed (with `rereads.current` back: B-1), the owed reindex and rebuild, the parked seed waiting for the opening, the failed-detach clear waiting. A staging with no slot that finds a demotion or a detach to do takes the lane as detached and **undecided** (`LaneState.undecided`); the first read while held decides it again, now held. The read again runs only while held.

**Per finding.**

| Finding | Outcome | Guard |
| --- | --- | --- |
| B-1 an owed seed lands without a read under the stream now held | `current` back in `land_owed_seeds`, both branches | `an_owed_seed_waits_for_a_read_under_the_stream_now_held` (world, ends in a stop) |
| B-2 a reattach finishes another process's reattach | the no-record branch is gone: the missing record is foreign | `a_reattach_asked_over_a_record_another_process_removed_stops_the_deployment`; world `a_reattach_over_anothers_reattach_writes_nothing_onto_its_next_record`, `…_after_a_reindex` |
| B-3 `RecordAgain` records the horizon | moot: `RecordAgain` is gone; the lane's record vanishing is foreign | `a_lane_held_detached_whose_record_another_process_removed_stops_the_deployment` |
| B-4a a reindex whose generation another process seeded | a staging that finds the index seeded on another target than its outstanding backfill asked for stops | world `a_reindex_whose_generation_another_process_seeded_ends_in_a_stop` (fails with the check removed) |
| B-4b a removed rebuild dropped unheard | moot: the drop is gone; the removal is foreign | world `a_rebuild_another_process_finished_ends_in_a_stop` |
| S-1 a rebuild taken over at a start re-walks | the checkpoint is dropped only where a gap is kept (`known.keeps_gaps`) | `a_rebuild_taken_over_at_a_start_resumes_its_walk` |
| S-2 a lane held detached is asked a backfill | the re-ask is gone with the fresh-gap cut | `a_detached_lane_is_asked_no_backfill_at_the_next_opening` |
| S-3 a stop never drains a waiting seed | at a stop, a seed that can't land is cut: the owed seed dropped, the completion acknowledged unseeded (`lock_skipped`) | `a_stop_drains_a_lane_parked_on_a_seed_waiting_for_the_slot`, `a_stop_drains_a_lane_parked_on_a_blocked_owed_seed` |

The four reviewer replays (p8, p9, p11, p12) and p10 each end in a clean stop, asserted. Minors: m-1's two misplaced docs went with their items; m-2 is answered by B-1; m-4 and m-5 are moot (no generation identity, no fresh-gap cut); m-3 is not changed: a lane that detaches while its parked seed waits for the slot keeps `seed_waits_for_slot`, so after a later attach its probes wait for the slot in an outage; under the cut the read that detaches it reads records this process wrote, and the cost is a delayed probe, never a write; m-6 is moot (no stand-in); m-7 is one sentence in `slot.rs`.

**Found while cutting, by the world.**
- A staging that read an index's gaps before `ensure_index` created it again kept them at the fence: the read that found none told nothing. Every read now tells its gap list, empty included (`a_gap_heard_before_the_index_was_created_again_is_forgotten`).
- **The first 50,000-case sweep on the cut (`3842d8dd`) failed on all ten fresh seeds.** Three engine holes, all in `known`, each with a unit test that failed first (`known::tests`, `tests::detach`) and a pinned world case:
  - a lane detached here before any read of its records succeeded knew nothing, so another process's reattach was adopted and the lane stayed detached for good (`an_own_write_with_nothing_read_before_is_known`; world `a_lane_detached_here_before_any_read_ends_in_a_stop_once_reattached_elsewhere`);
  - a generation created here cleared the record's version, so another process's rebuild begun again under the same attempt count read as this one's (`a_marker_write_keeps_the_record_version`; world `a_rebuild_begun_again_while_a_generation_is_created_here_ends_in_a_stop`);
  - an own record write left its version unknown until a later read, the same ABA (`an_identical_record_written_again_by_another_process_after_our_rebuild_stops`).
- **And four model errors in the world**, each against how OpenSearch or Postgres behave:
  - every record had the version `1:1`; each write now gets a new one (`_seq_no`), and the delete is conditional on it (`a_rebuild_finished_and_begun_again_under_the_same_attempts_ends_in_a_stop`);
  - a generation created anew kept the walk checkpoint; its marker is written whole;
  - another process's turn started at the horizon, so a change this process published but held unconfirmed (a restaging lane) was replayed by nobody; it now starts where the slot confirmed, writes what this process left unwritten on the lanes it applies, starts its drops where reads stood, and moves the slot's confirmed position to its end.

**The world.** The other process keeps its whole repertoire; this process may now stop. The oracle:
1. no record erased or loosened without the slot (unchanged);
2. nothing confirmed past what is recorded or held: after every event, each change confirmed on a lane was written there, seeded after, given up by a later reattach, or covered by a record;
3. after a stop, a fresh process is started and must converge, the fence sound;
4. liveness from the world's own ledger, not the engine's reports: a lane whose store keeps a record is exempt;
5. a world with no other process never stops.

**Runs** (regression files set aside, fresh seeds from `/dev/urandom`, `PROPTEST_RNG_SEED`):
- **On the cut's first head (`3842d8dd`):** `gate_world` at 50,000 cases failed on all ten seeds, which found the three engine holes and four model errors above; `gate_property` at 20,000 passed 5/5; `a_single_process_never_stops` at 20,000 passed 3/3.
- **On `12d95131`:**
  - `gate_world`, 50,000 cases, 10/10 pass: `3971913483858755499`, `15935979820359600844`, `14472093542762707660`, `2461511017899595502`, `1432534355050435940`, `5835903624696254368`, `16535937334700589762`, `13503703424802941085`, `6872224147755757233`, `11231661418564181889` (about 215–435 s each);
  - `gate_property`, 20,000 cases, 5/5 pass: `4653536296031323287`, `7267688158388012166`, `15610940434591718234`, `15934232122219041705`, `14865457012761622090`;
  - `a_single_process_never_stops`, 20,000 cases, 3/3 pass: `17897760492528412284`, `9293259805703681585`, `17364344082128184665`;
  - the ten seeds that failed on `3842d8dd`, rerun: all pass.
- **Mutations, default 1,024 cases, 3 fresh runs each:** each gate removed (`mark_seeded`, `forget_detachment`, `reindex`, `begin_rebuild`, `demote`, `detach`) 3/3; the stop removed (a foreign read carried on over, as `Same`) 3/3. B-4a's check removed fails its pinned replay.

**What #490 inherits.** Reconciling with another process's changes, if a lease (#490) makes it worth it. The machinery removed here is in `904079ae` for reference. Four failures it must not repeat:
1. **A lane must never drop a change no record covers** (round 7, B-1): taking a lane as detached without a record of this process's own.
2. **A record written again must cover what the lane dropped** (round 8, B-3): a source position read after the opening is not where the drops began.
3. **A missing record is not proof of this process's own lost write** (round 8, B-2): another process's reattach looks the same.
4. **A rebuild's identity needs more than its attempt count** (round 7, hole 3): attempts restart at one once a record goes; and a seed must never land without a read under the stream now held (round 8, B-1).

**The user-visible list, for the docs agent.** This replaces the seventh re-check's; the review of the cut replaces it in turn (the error text changed).
- **Two flusso processes on one slot** (a manual run beside a deployment, compose and a host run): when one finds that the other changed an index's detach records while it didn't hold the replication slot, it stops.
  - The log, at `ERROR`: `another flusso process changed the index's detach records while this one didn't hold the slot: stopping the deployment, nothing past this stream's start confirmed`, with `index`, `sink` and `what`.
  - stderr, and the exit code 1: ``Error: sink `<sink>`, index `<index>`: another flusso process changed the index's detach records while this one didn't hold the replication slot (<what was found>). Stopping: flusso doesn't carry on over another process's record changes. Nothing past this stream's start was confirmed; start flusso again (one process per slot) and its start decides from the records``.
  - `/status` shows it as the sink's error until the process exits. A supervisor (Kubernetes, systemd) restarts it like any failed run.
  - **What to do:** run one process per slot. Stop the other one (or let it finish), then start flusso again: it decides from the records with nothing lost. Nothing past the stream's start was confirmed, so the restart reads those changes again.
- **A single deployment never stops this way.**
- **A backfill that completes while flusso doesn't hold the slot** is written in full; only marking it complete waits for the slot. Once the slot is back it completes with no second walk. A rebuild resumed at a restart resumes from its checkpoint; with a gap kept over the index, it starts from the first key.
- **An index whose records ask for a demotion or a detach while flusso doesn't hold the slot** is held detached until flusso holds it, then decided.
- **A stop during an outage**, with a backfill waiting to be marked complete, drains and exits 0 as on `main`; the next start backfills that index again.
- **A reattach while the slot isn't held** is refused with the reason. A reindex, a rebuild and the clear of a failed detach wait for the slot.
- **Unchanged:** the fence, the read again after every opening, the confirmation hold.

**Not proven.** The world is a model: no ingest engine, no `--skip-backfill`. No live run of two processes yet: §10's checklist.

### After the review of the cut

The review (`target/review-482-cut/`) found the cut the right shape (both owner rules hold under overlap, the stop safe and final, the world's corrections right, no leftovers) and its own promise broken twice: a single deployment stopped once, exit 1, when a reconnect followed an own write whose outcome it had lost. The design stays; `known` learns what every own write may have left.

**One mechanism for every own write.** `known` keeps a set of states the records may be in, and a read is the same when one state admits it.

| Own write (`slot`) | `Ok` leaves | Call failed: admitted until a read settles it |
| --- | --- | --- |
| detach, live only | `detached` (live only), from this `through`, attempts any, gaps as before | before; after |
| detach that can rebuild | as above, gaps as before or none (the marker moved, written whole, unless a detach moved it already) | before; after (either); before with the marker alone written (no gap) |
| rebuild (`begin_rebuild`) | `rebuilding`, attempts as returned (failed: one more than before), `through` kept, no gap | before; after; before with no gap |
| demotion | `detached` (live only as asked or as before), attempts and `through` kept, gaps as before or none | before; after; before with no gap |
| delete (`forget_detachment`, `forget_finished`) | no record, gaps as before | before; after |
| seed (`mark_seeded`) | no record, no gap; or, the record's best-effort delete failed, the record as before with no gap | before; after (both); before with no gap |
| gaps (`record_gaps`) | the list written | before; the list it was deciding |
| marker alone (a created generation, a reindex) | no gap, the record and its version kept | before; after |

- A write of the record, or any write that failed, is **read back** while the slot is still held: no other process writes then, so the read settles which state landed and learns the record's version. A marker write that returned `Ok` needs no read.
- **A failed write whose read back shows what it leaves landed**, and is taken as done: a detach record with this detach's `through`, a rebuild record, a demotion's detach record, no record after a delete. The caller then goes on as after a success (the lane detached under its own record, the rebuild begun), instead of staying on the failure path over records that say otherwise.
- **Every gated write stands on records read under the opening held now** (`slot`'s `held_fresh`). An own write keeps the opening what it stands on was read under; records read under an earlier stream are read again first, and the write goes out only when they are the same. Another process's change found by any read, the gate's or a read back included, stops the deployment (`KnownRecords` keeps the first).
- **Conservative:** only what this process's own write may have left, never "anything". With nothing known yet, a failed write leaves nothing known (the state before it can't be named), and a detach or delete that returned `Ok` is what is known.
- **Same opening** is `Rereads::held_or_alone`: held, or no stream ever opens here. A run that ended cleanly may still erase (`may_erase_records`, as a stop on `main`), but a read then is judged, never adopted (m-2).

The Same/Foreign table of the cut is unchanged otherwise: nothing known → adopted; read under the same held opening → adopted; one known state admits the read → Same; anything else → Foreign.

**Per finding.**

| Finding | Outcome | Guard |
| --- | --- | --- |
| B-1 a seed whose best-effort record delete failed stops a lone process at its next reconnect | the seed's outcome admits the record kept; its read back finds it and deletes it at once (`forget_finished`), as a staging would | `a_seed_whose_record_delete_failed_deletes_it_and_never_stops_one_process`; `known::tests`' `a_seed_may_leave_the_finished_rebuilds_record`; world `a_seed_that_left_its_record_never_stops_a_single_process` |
| B-2 an own write that landed but whose call failed stops a lone process | every write's unknown outcome kept and read back (the table above); a reattach asked again after its own delete landed unseen finds the record gone, judged the same, with its own open gap from where the lane was detached, and goes on to the attach | `a_rebuild_that_landed_unseen_never_stops_one_process`; `a_reattach_whose_delete_failed_finishes_either_way_without_a_stop` (the old stop test flipped: no stop and the attach, both ways the delete went); one `a_*_of_unknown_outcome_admits_before_and_after_only` unit test per write kind; world `a_rebuild_whose_response_was_lost_never_stops_a_single_process`, `an_own_write_of_unknown_outcome_never_stops_a_single_process` (every write kind, landing and not) |
| S-1 the world can't produce them | every own record or marker write can land then fail, or fail before landing (`Fails`); the seed's record delete can fail silently (`SeedKeepsRecord`); a reindex leaves its generation for the next ensure to create; a detach that can rebuild, a rebuild, a demotion and a reindex drop the gaps; `RebuildLost` and `SeedLeavesRecord` draw B-2 and B-1 often | on the cut's engine, `a_single_process_never_stops` fails 3/3 fresh seeds (each shrinks to `RebuildLost`) and the pinned B-1, B-2 and per-write cases fail; on this one they pass |
| S-2 `rereads.current` unguarded | the reviewer's probe as a test; the world gets reads that take their answer before they wait (`HoldReadsAnswered`) and `GapOverSlowRead` | both `an_owed_seed_never_lands_on_a_read_answered_under_an_earlier_stream` (engine and world); `gate_world` 3/3 without `current` ("orders@seed written with its records last read under stream 1, while 2 is held") |
| S-3 liveness skips every lane with a record | a lane is left out only when its records say detached, or rebuilding into a generation not yet seeded; a finished rebuild's record beside a seeded marker is judged | the oracle itself (no engine change behind it) |
| m-1 the error overclaims | "No change of this index was confirmed past what its records cover", in the error, its doc and the log | `a_change_by_another_process_stops_the_deployment` pins the wording |
| "ignore the version" caught 1/3 | the world draws the ABA often (`RebuildBegunAgain`), but ignoring the version there makes nothing an oracle can call wrong (the engine's target check cuts the old backfill): the stop is the conservative policy, not safety. Pinned by the tests that assert the stop | with it ignored, 8 fail: five `known` unit tests, `a_rebuild_finished_and_begun_again_under_the_same_attempts_ends_in_a_stop`, `a_rebuild_begun_again_while_a_generation_is_created_here_ends_in_a_stop`, `an_identical_record_written_again_by_another_process_after_our_rebuild_stops` |
| m-2 `finished` adopts a read | `Rereads::held_or_alone` decides adoption | `a_read_after_a_clean_end_judges_another_process_s_change` |
| m-3 two sinks, one cluster | refused offline (below) | `adapters::tests`' `two_opensearch_sinks_on_one_cluster_are_refused` |
| m-4 a read back that fails leaves the version unknown | not changed: the version can't be learnt without a read; a failed read back while held is the sink failing, and the version stays unknown until the next read, as for a call in flight when the stream ends (#490's window). Every field the write decides stays exact | — |
| m-5 gap-lane edges of B-2 | closed by the "marker alone written" state of every marker-writing call | the per-write unit tests |
| m-6 `Restaged::AwaitingSlot` | renamed `Restaged::WaitsForSlot` | — |
| m-7 (the author's m-3) | accepted, as described | — |

**Found on the way, by the world.** The first 50,000-case sweep on the fixes failed on 9 of 10 `gate_world` seeds and all 10 solo seeds: the new failure events reached paths no run had before. Each engine hole below has a test that fails without its fix, and each model error was checked against the adapter.
- **A rebuild's restart (O20) whose detach failed** after landing went on writing onto the detach record it left: the lane now waits for the failed-detach clear, as a probe's own failed detach does (`a_rebuild_restart_whose_detach_failed_writes_nothing_onto_its_record`, since superseded by the read back taking it as done).
- **A lane's own failed detach, read again before its clear,** was taken as detached and then cleared, detached for good: the read again leaves it to the clear (`a_failed_detach_read_again_before_its_clear_is_left_to_the_clear`).
- **A failed write that landed** left the engine on the failure path: the read back now takes it as done (`a_detach_that_landed_unseen_is_taken_as_done_from_its_read_back`).
- **A demotion on records read under an earlier stream**, and **an owed rebuild retried before the read again**, rewrote records another process had changed: every gated write now stands on fresh records, and a change found by the gate's read stops (`a_demotion_never_stands_on_records_read_under_an_earlier_stream`, `a_retried_rebuild_reads_records_from_an_earlier_stream_first`, `a_change_found_by_a_gated_write_s_read_stops_the_deployment`).
- **A lane taken detached on a read from an earlier stream** (a restage whose read returned after the next opening's read again) was never read again: such a lane makes the read again due (`a_lane_taken_detached_on_a_read_from_an_earlier_stream_is_read_again`).
- The model: a demotion with no record writes nothing (the adapter returns before the marker); another process's turn counts a change a seed walked past as in the index; its reattach starts the gap where the record says reads stood, with no start when it says nothing.
- The world's `begin_rebuild` wrote a fresh record and lost `through`; the adapter rewrites the record read, keeping it. Fixed in the model; `known` expects `through` kept, and a demotion keeps attempts and `through` too.
- A failed detach that overwrote a rebuild's record, then cleared by the failed-detach clear, leaves an unseeded generation with no record. Its backfill covers what it dropped, so the confirmation oracle now counts, on an unseeded index, the backfill that will seed it (asked after the change, or the one the next staging asks).
- The Foreign message names the reason against the state holding the record found, when one does.

**Two OpenSearch sinks on one cluster (m-3): broken on `main` too, now refused.** Every index, alias, generation, seed marker and detach record flusso keeps on a cluster is named by the deployment's `prefix` alone (`hash_alias`, `convenience_alias`, `meta_index`), never by the sink, so two sinks on one cluster already shared them on `main`: a reindex on one moved the shared marker, its seed dropped the generations the other filled, and per-sink `index_settings` applied only to whichever created the index. Under the cut each would also take the other's record writes for another process's and stop a single deployment. `flusso check` reports it and `flusso run` refuses to start (`adapters::distinct_clusters`), naming both sinks, when their URLs resolve to one cluster (scheme and host case-folded, the default port filled in, a trailing slash dropped, the path kept). `build` resolves no environment and doesn't check it. A breaking change for such a deployment.

**Runs** (regression files set aside, fresh seeds from `/dev/urandom`, `PROPTEST_RNG_SEED`):
- **On the cut's engine** (`942a0380`, with the corrected world): `a_single_process_never_stops` fails 3/3 fresh seeds (each shrinks to `RebuildLost`); the pinned B-1, B-2 and per-write world cases fail.
- **The first sweep on the fixes** (`6820a508`): `gate_world` failed 9/10 at 50,000, the solo property 10/10; `gate_property` passed 5/5. What it found is listed above; a second sweep (`c0d91ead`) passed the solo property 10/10 and `gate_world` 8/10, whose two failures were the last two items above.
- **On `f34c98b4`:**
  - `gate_world`, 50,000 cases, 10/10 pass: `14505590937704509809`, `8328152354279919408`, `8243136049864277044`, `17544380030640787217`, `7122052349086819789`, `7109168362158659214`, `746909242153028078`, `10926639883715590156`, `15986835245007642195`, `2022788067398891036` (about 225 s each);
  - `a_single_process_never_stops`, 50,000 cases, 10/10 pass: `8794503679367420561`, `17007995020621156217`, `14432500404984301053`, `15477235028446986029`, `18088754547490046131`, `9691304133450874234`, `15908243079865310575`, `5022801130033315065`, `9133570195975241908`, `10444670630233412093` (about 165 s each);
  - `gate_property`, 20,000 cases, 5/5 pass: `264410011143798754`, `10430097096019477661`, `12036578562343586838`, `3729341901395365656`, `1280131643207868299`.
- **Mutations on `f34c98b4`**, default 1,024 cases, 3 fresh runs each, `gate_world` and the solo property together:
  - each gate removed (`mark_seeded`, `forget_detachment`, `reindex`, `begin_rebuild`, `demote`, `detach`): 3/3 each;
  - the stop removed (a foreign read carried on over, the gate's found change ignored): 3/3;
  - "always Same" (every read judged the same): 3/3;
  - the unknown-outcome handling removed (no states kept, no read back after a failure): 3/3, by the solo property;
  - B-1's seed fix removed (no read back after the seed, the kept record not admitted): 3/3, by the solo property;
  - `rereads.current` removed from `land_owed_seeds` (S-2), run on `1a076bc1` when its world event landed: 3/3 by `gate_world`, and both pinned tests fail;
  - "ignore the version": 0/3 by the properties, by design (above); 8 tests fail.

**The user-visible list, for the docs agent.** This replaces the cut's.
- **Two flusso processes on one slot:** when one finds that the other changed an index's detach records while it didn't hold the slot, it stops.
  - The log, at `ERROR`, with `index`, `sink` and `what`: `another flusso process changed the index's detach records while this one didn't hold the slot: stopping the deployment; no change of this index confirmed past what its records cover`.
  - stderr, and the exit code 1: ``Error: sink `<sink>`, index `<index>`: another flusso process changed the index's detach records while this one didn't hold the replication slot (<what was found>). Stopping: flusso doesn't carry on over another process's record changes. No change of this index was confirmed past what its records cover; start flusso again (one process per slot) and its start decides from the records``.
  - `/status` shows it as the sink's error until the process exits; a supervisor restarts it like any failed run.
  - **What to do:** run one process per slot. Stop the other one, or let it finish, then start flusso again: it decides from the records with nothing lost.
- **A single deployment never stops this way**, also when a write's answer is lost (a timeout, a network drop) or a seed couldn't delete a finished rebuild's record: flusso keeps what its own write may have left, reads the records back while it holds the slot, and deletes a leftover finished record at once.
- **Two OpenSearch sinks on one cluster are refused** by `flusso check` and `flusso run`, naming both: `[sinks.<a>] and [sinks.<b>] write to the same OpenSearch cluster (<url>): every index, alias and seed marker flusso keeps there is named by the deployment's `prefix`, never by the sink, so the two would share them. Give each sink its own cluster, or run each in its own deployment with its own `prefix``. **Breaking** for such a deployment (it shared every index already); the upgrade page should say what stops and how to fix it.
- A backfill that completes while flusso doesn't hold the slot is written in full; only marking it complete waits. A rebuild resumed at a restart resumes from its checkpoint, unless a gap is kept over the index.
- An index whose records ask for a demotion or a detach while flusso doesn't hold the slot is held detached until it does.
- A stop during an outage, with a seed waiting, drains and exits 0, as on `main`.
- A reattach while the slot isn't held is refused with the reason; a reattach asked again after one whose answer was lost finishes it. A reindex, a rebuild and the clear of a failed detach wait for the slot.
- Unchanged: the fence, the read again after every opening, the confirmation hold.

**Not proven.** The world is a model: no ingest engine, no `--skip-backfill`. No live run of two processes, of a lost write answer, or of a failed seed delete yet: §10's checklist.

### After the review of round 10: who wrote it

The review (`target/review-482-round-10/`) found the slot gate and the stop safe, and two promises broken by the shape of `known`: a hand-written prediction, per write kind, of what OpenSearch leaves (B-1: a detach with no `through` keeps the record's, `known` predicted none; S-2: "taken as done" can't tell a landing from records that already were so), and admitted states that can't tell this process's write from an identical one by another process (B-2: the failed-detach clear deleted another process's record). This round replaces the predictions with identity. Designed here first, then built to it.

**The design.**
- **A stamp on every record and marker write.** A stamp is `{writer, seq}`: `writer` a random token made once per process start (never persisted on the host), `seq` that process's write counter. The engine issues it (`KnownRecords::stamp`); the sink stores it in the documents it writes and returns it on a read (`Detachment.record_stamp`, `Detachment.marker_stamp`). Both documents live in the sink's meta index, not in `flusso.lock`: no format freeze applies. A document without a stamp (written before this version, or by hand) is written by nobody this process knows.
- **A delete is a tombstone.** `forget_detachment` (and the seed's delete of a finished rebuild's record) writes `{state: "forgotten", stamp}` over the record, conditionally on the version read, instead of deleting it. A read reports a tombstone as no record, with its stamp. A detach over a tombstone overwrites it, conditionally on its version.
- **Same or Foreign.** Each document is judged on its own. The record (or tombstone): the same when it carries this process's writer, or is exactly what was last read (presence, version, stamp, record, attempts, `through`); else foreign. The marker: the same when it carries this process's writer, or its stamp and gaps are what was last read; else foreign. A read is the same when both are. Nothing known yet: adopted; a read under the same held opening: adopted (unchanged).
- **An own write with unknown outcome needs nothing kept.** The next read shows the write's own stamp (landed), or what was last read (not landed), or something else (foreign). "Taken as done" is exactly this write's stamp on the record found by the read back.
- **With nothing read yet, an own write of the record** makes the record known as "only this process's": the next read is the same only when the record carries this process's writer; the marker is still adopted (unknown).

**1. Deletes.** A plain delete leaves nothing to tell "I deleted it" from "another process deleted it", which is round 8's B-2 and this review's B-2. A tombstone keeps the deleter's stamp: this process's own delete, landed or not, is read as its stamp or as the version it targeted; another process's delete is its stamp: foreign. Conditional deletes on the version read alone were weighed and refused: another process reading the same version and deleting it leaves the same absence. A reattach asked again after its own delete landed unseen finds its own tombstone and goes on.

**2. Multi-request sink calls.** Every write that changes the record or the marker's gaps is stamped; the generation deletes and alias swaps around them change neither document. After a failure at any step each document is either exactly as last read or carries this process's stamp, so a read is the same whichever step the failure came at; the engine learns whether the call's own write landed from the stamp. Per call:
- `detach`: the marker moved (stamped), then the record (stamped), then stale generations deleted. A read after the marker alone finds the marker this process's and no record: the read's put-back of a half-moved marker (stamped) restores it.
- `begin_rebuild`, `demote`: generations deleted, the marker moved or put back (stamped), the record rewritten (stamped).
- `mark_seeded`: refresh, alias swap, the marker written whole (stamped), the record's tombstone (stamped, best effort).
- `reindex`, `ensure_index` when it creates: the marker written whole (stamped).
- `record_gaps`: the marker's gaps and stamp in one `_update`.
- `record_walked`, `forget_walked`, the marker's `uuid`: neither gaps nor stamp change; not judged.

**3. OpenSearch semantics relied on.**
- `if_seq_no`/`if_primary_term` make a write fail when the document changed since it was read (every record write, the tombstone, the put-back, `record_gaps`).
- `_update` keeps the fields it isn't given: `record_gaps` gives the gaps and the stamp together; the walk checkpoint and the `uuid` keep the stamp they don't give (correct: they change nothing judged).
- A `PUT` of the whole document keeps nothing it isn't given: every whole-document write of the record or marker carries its stamp; a detach with no `through` copies the record's (`record_body`), which the stamp makes irrelevant to the judgement.
- A no-op `_update` keeps the version.
Pinned by the adapter's unit tests over the request bodies (`detachment::tests`); the stand-in sinks (`tests::detach`'s `Gate`, the world's `Store`) store what they are given, stamps included, and predict nothing.

**4. The port.** `sink::Stamp { writer, seq }`. `Detachment` gains `record_stamp` and `marker_stamp`. The writes that keep records take the stamp: `detachment` (for the put-back), `detach`, `forget_detachment`, `begin_rebuild`, `demote`, `record_gaps`; `ensure_index_as`, `mark_seeded_as`, `reindex_as` are provided methods that default to the unstamped ones, for sinks that keep no records. The sink stores and returns; every decision is the engine's.

**5. B-2 again.** Another process's detach carries its writer: foreign, stop. The failed-detach clear deletes only a record carrying this process's writer, by a tombstone conditional on the version read; any other record is never touched by it.

**6. Restarts.** The writer is per process: after a process restart nothing is this process's, and staging adopts the records as before (nothing known). A sink engine's restart inside one process keeps `known` and the writer: a write whose outcome was unknown is read later as this process's stamp (landed) or as what was read before it (not landed); nothing is kept for it.

**7. What stays.** The slot gates (`slot::Plain`/`Erasing`), fresh records before every gated write (`held_fresh`), the stop and its ordering, the read back while held (now it reads the stamp), the simple waits.

**What goes.** The admitted state sets, `Expect`, the per-write `after` rows, their bound, the per-write `landed` predicates, and the "nothing known" special cases they needed.

**As built.** The design above, with these choices made in the code:
- **One writer per sink engine**, made at process start (`std`'s per-process random hash keys over the pid and the clock: no new dependency); a sink engine restarted inside the process keeps it. Two sinks of one process write under different writers, on different clusters as far as the cluster check tells them apart (since round 12, by `cluster_uuid` at `run`; by URL alone under `check --offline`).
- **The read stamps its put-back.** `detachment` takes a stamp, so a half-moved marker put back by any read is this process's own write.
- **The marker is judged by its stamp and gaps, not its version**: the walk checkpoint and the `uuid` are partial writes (`_update`) that keep the stamp and change nothing a read is judged on. An unstamped writer (a flusso from before stamps, a hand edit) that rewrites an unstamped marker without changing its gaps is not seen; one that changes a stamped marker or its gaps is. A marker an earlier version wrote stays unstamped until something rewrites it (a seed, a reindex, a gap, a detach), which for a seeded index may be never, so this holds for that long, not only before the upgrade (the review of round 11, m-4).
- **What `known` keeps per index**: the record as last read (or "only mine" after an own record write with nothing read), the marker's stamp and gaps as last read (or nothing, adopted), the opening it was learnt under, and whether a gap may be kept (for the walk checkpoint's rule). Own writes change nothing in it but those last two cases.
- **`slot::settled`**: after every own record write, read back while held; a failed call whose read back shows its own stamp is `Ok` with what the read shows (`Recorded::Kept`, the attempts, `()`).
- **The failed-detach clear** forgets only a record stamped by this process; **a lane's own landed detach** found by the read again is taken as detached, as staging takes it, and its clear dropped (m-8).
- **`slot::NotHeld`** names why a gated write waits: no slot, the read that failed, another's change (m-2).

**Per finding.**

| Finding | Outcome | Guard |
| --- | --- | --- |
| B-1 restart detach with no `through` | the stamp decides, not the content | `a_rebuild_restart_detach_with_no_through_never_stops_one_process`; world `a_rebuild_restart_whose_read_back_is_lost_never_stops_a_single_process` (`RestartLost`) |
| B-2 another's same-place detach taken for ours, deleted by the clear | another writer's stamp is foreign; the clear forgets only our stamp | `anothers_detach_from_the_same_place_is_never_taken_for_ours`; world `anothers_detach_from_the_same_place_ends_in_a_stop` (`SameDetachElsewhere`) |
| S-1 the 64-state bound | no states kept | `many_failed_rebuild_tries_never_stop_one_process` |
| S-2 a landing not told from records already that way | landed = this write's own stamp | `a_failed_live_only_demotion_is_not_taken_as_done` |
| S-3 liveness shorter than the backoff | last change at round 10 of 50 (80 s after it) | `a_lane_restaging_on_its_longest_backoff_is_served_by_the_end` (seed `14283768201718162447`, shrunk) |
| S-4 `731c3791` | kept, guarded | `a_rebuild_restart_whose_detach_is_unknown_writes_nothing_until_a_read` (fails without the arm) |
| S-4 fresh records | kept, guarded per gated write | `slot::tests::every_gated_write_reads_records_from_an_earlier_stream_first` (fails without `held_fresh`'s read) |
| S-4 `rereads.current` before an owed seed | removed: the seed's gate reads stale records again first | the test above, `mark_seeded` row |
| S-5 the world's model | the other process stamps its records, tombstones its forgets; `Block`, `Unreadable`, `RestartLost`, `SameDetachElsewhere` events | each property fails 3/3 on the round-10 head (solo: `RestartLost`; `gate_world`: `SameDetachElsewhere`) |
| Two sinks on one cluster | a `check` report finding (`shared_cluster`, error; `check_skipped` for an unresolved URL); `run` refuses as before; other clusters accepted | `check::tests`, `check_report`'s `two_sinks_on_one_cluster_are_a_finding_of_the_report`, `adapters::tests` |
| m-1 "any failed write is read back" | docs say "a write of the record" | — |
| m-2 refusal reason | named | `slot::tests::a_refused_gated_write_names_why` |
| m-3 O29's row | reworded (§11) | — |
| m-4 two hostnames for one cluster | documented on `ClusterFinding::Shared` and here; the deployment stops at the first read of the other sink's writes | — |
| m-5, m-6 | the `check` finding above | above |
| m-7 `known.rs` docs, `unsure`'s `at` | rewritten; `unsure` gone | — |
| m-8 two answers | one: taken as detached | `a_failed_detach_read_again_before_its_clear_is_taken_as_landed` |

**Lines.** `known.rs` 923 → 552 (production 498 → 315); `slot.rs` production 481 → 460 (+202 lines of tests). The OpenSearch adapter's `sink_impl.rs` churn is three bodies moved into stamped inherent methods.

**Runs** (regression files set aside, fresh seeds from `/dev/urandom`, `PROPTEST_RNG_SEED`):
- **On the round-10 head** (`b5ef0928`, with the round-11 world): the B-1, B-2, S-1 and S-2 engine tests fail; `a_single_process_never_stops` fails 3/3 fresh seeds (each shrinks to `RestartLost`), `gate_world` 3/3 (each shrinks to `SameDetachElsewhere`).
- **The first sweep on the stamps** (`a4819623`): `gate_world` failed its first seed, `15942218590030476190`, a model error the stamps' world change made (the other process's detach dropped the gaps, then its reattach, which the real one refuses on a rebuildable sink, left a drop uncovered): fixed in the world, pinned as `another_process_s_reattach_of_a_rebuildable_sink_keeps_the_gaps`.
- **On `37d98695`:**
  - `gate_world`, 50,000 cases, 10/10 pass: `2072050034091820154`, `11797069531290004157`, `3931616706816368429`, `4494877985668223751`, `9074526999428275027`, `15627589824227035274`, `8938140542608114701`, `10693614021994901921`, `4984724499296774687`, `16471890907364706056` (about 250 s each);
  - `a_single_process_never_stops`, 50,000 cases, 10/10 pass: `6557221320528905103`, `15891071873221261963`, `513315723070099337`, `5872695736433603885`, `11860941265330882808`, `762022264007779238`, `10511170882348564218`, `18269212866028036341`, `6568098226663435629`, `17624715706247919953` (about 190 s each);
  - `gate_property`, 20,000 cases, 5/5 pass: `8480548126065064286`, `11168978230363060072`, `17018900340830192549`, `9059612534937220374`, `923915185921491831`.
- **Mutations on `37d98695`**, default cases, 3 fresh runs each, `gate_world` and the solo property together:
  - each gate removed (`mark_seeded`, `forget_detachment`, `reindex`, `begin_rebuild`, `demote`, `detach`): 3/3 each (by `gate_world` every run; the solo property too, but for `reindex`);
  - the stop removed (a foreign read carried on over, the gate's found change ignored, no `foreign_error`): 3/3, by `gate_world`;
  - "every read is mine": 3/3, by `gate_world`;
  - "no read is mine": 3/3, by the solo property;
  - the delete rule removed (the world's forget deletes, no tombstone): 2/3, by the solo property; the adapter's `forgetting_a_record_leaves_a_tombstone_only_on_the_version_read` and `the_seed_forgets_the_finished_rebuilds_record` pin the tombstone itself;
  - the fresh-records requirement removed (`held_fresh` = `held`): 3/3, by `gate_world`, and `slot::tests::every_gated_write_reads_records_from_an_earlier_stream_first` fails.

**The user-visible list, for the docs agent.** This replaces the review of the cut's.
- **Two flusso processes on one slot:** when one finds that the other changed an index's detach records while it didn't hold the slot, it stops. Unchanged texts (log at `ERROR`; stderr and exit 1: ``Error: sink `<sink>`, index `<index>`: another flusso process changed the index's detach records while this one didn't hold the replication slot (<what was found>). Stopping: flusso doesn't carry on over another process's record changes. No change of this index was confirmed past what its records cover; start flusso again (one process per slot) and its start decides from the records``). *What was found* now names who wrote each document (`writer <token> (write <n>)`, or `nobody known (no stamp)`). **What to do:** one process per slot; start again.
- **New in the meta index:** each detach record and seed marker carries `stamp: {writer, seq}`; a forgotten detach record stays as `{state: "forgotten", stamp}` instead of being deleted. Nothing to do; an operator reading the meta index sees them.
- **A single deployment never stops this way**, also when a write's answer is lost or a seed couldn't forget a finished rebuild's record.
- **`flusso check` with two OpenSearch sinks on one cluster**: an error of the report (section *Sinks*, JSON `sinks[]` with `kind: "shared_cluster"`, counted in the summary), not a failure before it. A sink whose URL can't be resolved there (an unset environment variable) is a warning, `check_skipped`, naming the sink and why. `flusso run` refuses a shared cluster as before. A second OpenSearch sink on another cluster (a backup) is accepted. Two hostnames naming one cluster are not detected: such a deployment stops at the first read of the other sink's records.
- **A gated write that waits says why** in the reattach refusal and the logs: no slot, the read that failed, or another process's change.
- Unchanged: the fence, the read again after every opening, the confirmation hold, the waits for the slot.

### After the review of round 11

No blocking finding (`target/review-482-round-11/`). A closing round: no new mechanism.

| Finding | Outcome | Guard |
| --- | --- | --- |
| S-1 a gap lost when a rebuildable detach's marker comes back on serving | `detaching_from` carries the gaps; the read's put-back and the live-only demotion restore them; only a seed or a reindex drops them | adapter `a_gap_survives_a_detach_whose_marker_comes_back_on_serving`; world `a_gap_survives_a_backfill_flipped_on_and_off_again`, `a_gap_survives_a_failed_detach_s_clear_and_the_put_back`; Docker `a_gap_comes_back_with_the_marker_onto_the_serving_generation` |
| S-2 two unreal world steps cancelling | the other process's rebuildable detach carries the gaps in `detaching_from`; its reattach refuses a record a rebuildable sink wrote, as the real one; `backfill` can flip between processes (`FlipBackfill`, `BackfillFlips`, `PutBackAfterFlip`) | with the stand-in's restore removed, the first case above fails; the round-11 case re-pinned as `another_process_s_detach_of_a_rebuildable_sink_carries_the_gaps` |
| S-3 the put-back in neither model | the `Gate` and the world's `Store` keep `detaching_from` and put a half-moved marker back on a read that gets through, stamped by the reader | `a_rebuild_restart_whose_read_back_failed_once_is_cleared_and_goes_on`; the world's put-back removed fails `a_gap_survives_a_failed_detach_s_clear_and_the_put_back` |
| S-4 the cluster check compares URLs | `run` and an online `check` ask each OpenSearch sink's cluster its `cluster_uuid` (`GET /`); `check --offline` compares URLs and says so; a cluster that can't be asked is `check_skipped` in `check`, logged by `run` (its sink's staging then fails and retries as before, so no new refusal) | `sinks_answering_one_cluster_uuid_share_a_cluster`, `a_cluster_that_cannot_be_asked_is_a_skipped_check`, `urls_that_differ_pass_the_url_comparison`, adapter `a_sink_names_its_cluster_by_the_uuid_get_root_answers`; Docker `two_urls_of_one_cluster_answer_one_cluster_uuid` |
| S-5 weak guards | the B-2 test stamps the other's record and asserts the stop; the clear's own-stamp check has a test; m-8's reset was redundant (`LaneState::detach` starts the state anew) and is removed, its outcome asserted | `anothers_detach_from_the_same_place_is_never_taken_for_ours` (fails under "every read is mine"), `the_clear_forgets_no_record_another_writer_stamped` (fails without the check), `a_failed_detach_read_again_before_its_clear_is_taken_as_landed` |
| Item 7 an unstamped write path added unnoticed | every whole write of a record or marker goes through `put_if`/`put_meta`, which stamp it | `every_record_or_marker_body_carries_the_callers_stamp` (fails when `put_if` drops the stamp) |
| Found by this round's sweep (seed `5729423447252134060`, pre-existing): a detach over a record already detached took `through` from where reads stood, so a later reattach's gap missed what was dropped since the first detach | the record keeps its `through` as it keeps its `since` (adapter `record_body`; the stand-ins alike) | adapter `a_detach_over_a_detached_record_keeps_where_it_began`; world `a_detach_over_a_detached_record_keeps_where_the_drop_began` |
| Found by this round's sweep (seed `935851008211566009`, a model error): the stand-ins took a live-only detach's marker as on serving where an earlier detach had moved it, so a reattach the real one refuses went through | the `Store` and the `Gate` report the marker where it is | world `a_live_only_detach_over_a_moved_marker_leaves_it_moved` |
| Found by this round's sweep (seed `5275283102389846757`, a model error): the world's other process reattached beside a marker an earlier detach had moved | its reattach checks the marker too, as the real one | world `another_process_never_reattaches_beside_a_moved_marker` |
| m-1 | the reindex and rebuild refusals name the real `NotHeld` reason | `a_reindex_of_an_open_index_is_owed_while_no_stream_is_held` |
| m-2 unstamped defaults | the record methods move into `sink::Records`, reached through `Sink::records()`, with no defaults: a record-keeping sink can't write a marker unstamped | `slot::tests::a_record_keeping_sink_has_its_marker_writes_stamped` |
| m-3 `OnlyMine` | gone: a gated write with nothing read reads the records first | `slot::tests::a_gated_write_with_nothing_read_reads_first_and_never_stops_one_process` (fails on the head: the write went out before any read) |
| m-4 | an 0.18 marker stays unstamped until something rewrites it, which for a seeded index may be never: an unstamped writer's change to it that leaves its gaps is not seen that long | "As built" in round 11's section, corrected |
| m-5 | the Docker stamp test no longer claims the `uuid`'s `_update` | — |
| m-6 | "two sinks of one process write under different writers, on different clusters" holds where the cluster check can tell: by `cluster_uuid` at `run`; by URL alone offline | this section |

**Runs** (fresh seeds, regression files set aside). The sweep stopped three times before it passed, each a case pinned above: on `f5d964a8` seed `5729423447252134060` (the `through` hole, fixed in the adapter), on `ebf93b22` seed `935851008211566009` and on `2f9fbf44` seed `5275283102389846757` (both model errors of the new events, fixed in the world).
- **On `38b5be8a`:** `gate_world` 50,000 × 5 pass: `13652441168411726753`, `18059178798293708243`, `18435921287012619458`, `17364435703406401764`, `16476881974013753479`; `a_single_process_never_stops` 50,000 × 5 pass: `10882295765089759437`, `9500954212440262356`, `14074150961885632879`, `8273760688739192016`, `8750072589800257797`; `gate_property` 20,000 × 5 pass: `15225370711209625760`, `8149616318890584644`, `12627488218562671632`, `13320593749340421117`, `13678373583712747739`.
- **Reverts on `38b5be8a`**, each caught:
  - S-1's adapter fix reverted: `a_gap_survives_a_detach_whose_marker_comes_back_on_serving` fails (shown on the head before the fix);
  - S-1's stand-in restore removed in the world: both properties fail 3/3 at the default count, and both pinned gap cases fail;
  - S-3's world put-back removed: both properties fail 3/3, and `a_gap_survives_a_failed_detach_s_clear_and_the_put_back` fails;
  - S-3's `Gate` put-back removed: `a_rebuild_restart_whose_read_back_failed_once_is_cleared_and_goes_on` fails;
  - S-5: "every read is mine" fails `anothers_detach_from_the_same_place_is_never_taken_for_ours`; the clear's own-stamp check removed fails `the_clear_forgets_no_record_another_writer_stamped`.

### After the review of the closing round

The closing review (`target/review-482-closing/`) found the stand-ins drifting from the adapter: three holes (A, D, E) were reachable only by reading the code. This round gives the tests one copy of the adapter's document logic, then fixes what the world then shows.

**Item 1: one copy of the adapter's document logic.**
- **The plans.** The OpenSearch sink's decisions over its meta documents are pure functions (`sink-opensearch/src/detachment/plan.rs`, re-exported as `sink_opensearch::meta`).
  - In: the documents read (the marker; the record or its tombstone), the generation the alias serves, the generations that exist.
  - Out: a `Plan`, the writes in order (marker, record, generation deletes), then the best-effort deletes.
  - The HTTP code reads, asks for the plan and sends it, each write conditional on the version read. The writer adds the stamp, so no plan sends a write unstamped.
- **The in-memory meta index.** `meta::memory::MemoryMeta` (feature `testing`) runs the same plans. It models, once:
  - a version per write;
  - tombstones;
  - the gaps' partial `_update` against whole writes;
  - the alias dropped with its generation.
- **The world.** The daemon's world keeps its stand-in's documents there, for this process and the other alike.
  - Its hand-written copies are deleted: marker moves, the put-back, the `through` rule, gap carrying, tombstones, versions.
  - It keeps only the oracle's bookkeeping: who wrote a record under which stream, reads, writes, seeds.
- **Layering and publishing.** The world is layer 3 over the adapter (layer 2), so the layering holds. The daemon already dev-depended on `flusso-sink-opensearch` for the fence e2e, with its version.
  - With `features = ["testing"]` added to that versioned entry, `cargo package -p flusso-daemon` failed: it resolves the dev-dependency against crates.io, whose `flusso-sink-opensearch` has no `testing` feature until the libs train publishes it.
  - So the entry is now path only, as `flusso-test-postgres` is. `cargo package` then passes and strips it from the published manifest (checked: neither appears in the packaged `Cargo.toml`). The fence e2e uses the same entry in the repository.
- **The engine.** It is the adapter's layer, so it can't depend on it.
  - Its `Gate` keeps what it is asked to (records as written, a marker stamp, gaps from `record_gaps` until a seed) and nothing more.
  - The one test that leaned on the adapter's put-back moved to the world: `a_rebuild_restart_whose_read_back_failed_once_is_cleared_and_goes_on`. The world's `a_rebuild_restart_whose_read_back_is_lost_never_stops_a_single_process` and `a_gap_survives_a_failed_detach_s_clear_and_the_put_back` cover it.
- **What the world now reaches.**
  - a reindex and a reattach of either sink;
  - `backfill` flips between starts;
  - a rebuild begun from a live-only record (`RebuildFromLiveOnly`);
  - a failed best-effort forget at a seed (`SeedKeepsRecord`);
  - a live-only record beside a moved marker (`LiveOnlyBesideMoved`);
  - a target deleted with its alias (`Missing`).
- **Two new oracle rules.**
  - No write leaves the alias serving nothing (E).
  - On a sink that can't rebuild, a detached lane comes back by a reattach, or by a start and a reattach (D's way out).
- **Before the fixes.**
  - A failed 6 of 6 fresh seeds: both properties, 5,000 cases, three seeds each, each shrunk to `GapThroughRebuild { lane: 0, moved: false }`.
  - D and E failed their pinned replays.

| Finding | Outcome | Guard |
| --- | --- | --- |
| A a gap dropped by writes other than a completed seed | **A gap is dropped only by a seed that completed.** Rebuild and non-live-only demotion over a marker no detach moved: they make its `detaching_from`, gaps included. A created generation and a reindex: they rewrite the marker carrying `gaps` and `detaching_from` (`rewrite_marker`, conditional on the marker read). Live-only demotion and the put-back: they restore the gaps from either place. The seed's `write_index_meta` is the one write without them. `reindex` checked: it moved the marker whole, dropping both, and now carries them. | plan `only_the_seed_drops_the_gaps`; adapter `a_gap_survives_a_rebuild_begun_over_a_live_only_detach`, `a_gap_survives_a_begun_rebuild_put_back_live_only`; Docker `a_gap_comes_back_with_the_marker_onto_the_serving_generation` (two more cases); world `a_gap_survives_a_rebuild_put_back_live_only` |
| D a live-only record beside a moved marker never demoted | A start on a sink that can't rebuild puts a `detached` record back live only on the generation reads see. It does so when the record was written while the sink could rebuild, or when the marker is off that generation. The refusal's "restart flusso" is then true. | engine `a_live_only_record_beside_a_moved_marker_reattaches_after_a_start`; world `a_live_only_record_beside_a_moved_marker_reattaches_after_a_start` and the way-out rule |
| E a rebuild from a live-only record never finished | A rebuild is begun with `live_only: false`. `Finished` is decided by the marker alone: a seeded newer marker finishes any record. The seed's read back forgets any record left there. No rebuild or demotion deletes the generation the alias serves (a `debug_assert!` in the plan). A live-only demotion goes back on the alias's generation. | plan `a_rebuild_begun_from_a_live_only_record_is_not_live_only`, `no_plan_deletes_the_generation_the_alias_serves`; engine `staging_classifies_every_record_and_marker_pair` (two cases); world `a_rebuild_from_a_live_only_record_finishes_at_its_seed` and the alias rule |
| B `_na_` taken for a cluster | A `cluster_uuid` of `_na_` or empty names no cluster: the sink is not compared (`check_skipped`, logged by `run`). | CLI `two_unformed_clusters_are_not_one_cluster` |
| C the clusters asked one after the other before the listeners | `run` binds its listeners, then asks every cluster at once, each for at most 10 s (`CLUSTER_ASK_TIMEOUT`). | CLI `slow_clusters_delay_the_check_by_one_timeout` |
| m-e the run-side warning | It says what is at stake (see the user-visible list). | — |
| test gap: the online uuid path | Driven end to end with two stubs. | CLI `one_uuid_through_the_online_path_is_refused` |
| test gap: the put-back read's own gaps | Asserted on the read that puts the marker back. | adapter `a_gap_survives_a_detach_whose_marker_comes_back_on_serving` |
| m-a the stand-ins' `begin_rebuild` | Gone with item 1: the world runs the adapter's plan; the `Gate` stores what it is asked. | — |
| m-b `Plain::record_gaps` on a sink with no records | It tells `known` of the empty list, as any write that returned none. | — |
| m-c the PR body's item 16 | Corrected: no provided methods; the record methods are `sink::Records`'. | — |
| m-d `restored` dropped the uuid | With no `detaching_from`, a live-only demotion over a marker on serving keeps its uuid and seeded state. | plan `a_live_only_demotion_keeps_the_uuid_of_a_marker_on_serving` |
| Found by the world's way-out rule: a fresh `backfill = false` sink could never reattach | Such a sink never seeds its index, so its marker on serving is unseeded, and the reattach demanded it seeded. The reattach now takes the marker on the generation reads see, seeded or not. | engine `a_live_only_record_beside_an_unseeded_marker_on_serving_reattaches` |
| Found by this round's sweep (seed `2409886620174256796`, a hole): a detach over a `rebuilding` record took the later `through` | Round 12 kept `through` only over a `detached` record. A rebuild given up seeded nothing, so the serving generation lacks every change from the first drop. The record's `through` is now kept over any record. | plan `a_detach_over_a_rebuilding_record_keeps_where_the_first_began`; adapter `a_detach_over_a_detached_record_keeps_where_it_began` (both states); world `a_detach_over_a_rebuilding_record_keeps_where_the_drop_began` |
| Found by the final sweep (seed `17611438872220405175` of `gate_world` at 50,000 cases, a hole): a moved marker read as on serving once the generation it left was deleted | The rebuild moves the marker past every generation that exists, so with the serving generation gone it took that number again. The marker, still carrying `detaching_from`, read as on serving. A reattach added its gap at the top level, and the next read's put-back wrote the marker back from `detaching_from` without it. A marker carrying `detaching_from` now reads as moved (`Newer`), never on serving. The put-back and a live-only demotion bring back every gap from both places. | plan `a_marker_carrying_detaching_from_is_never_on_serving`; world `a_gap_kept_beside_a_moved_marker_on_a_reused_generation_survives` (both fail without the fix) |
| Found by the world, not fixed: a record written while nothing served (the generation deleted from under the sink) names no generation | `marker_at` reads every marker as `Elsewhere` against it. A rebuild from it is never taken for finished: each start takes it over again. A live-only one is never reattached. The refusal's "restart flusso" doesn't help there. Reached only with a generation deleted by hand under a detach. The world leaves such a record out of its liveness and way-out checks (`Store::names_no_serving`). | issue #492 |
| Docs pass: `flusso reindex` said "on every sink queued" when sinks were skipped | The `202` carries `queued_on`. The CLI prints the sinks queued, then each skipped sink with its reason, one per line. The exit code is as before. | CLI `a_queued_reindex_is_printed_from_its_json`, `http::tests::a_reindex_is_queued_on_the_index_path` |
| Docs pass: `/readyz` with a detached lane or a gap | Unchanged: readiness is the deployment's and each sink engine's phase alone. A detached lane, or a gap, stays ready, is shown on `/status` and is refused by the fence. | daemon `a_detached_lane_or_a_gap_leaves_the_deployment_ready` |
| Docs pass: the stop said "detach records" when a seed marker changed | Error and log name "the index's detach record or seed marker" (texts in the user-visible list). | daemon `a_change_by_another_process_stops_the_deployment` |
| Docs pass: `contribute/pipeline.md` | Brought to what is built: `Records`, stamps, the stop, the plans and the world. | — |

**Runs** (fresh seeds, the regression files set aside, one test binary of the final code commit `731915a0`).
- **The sweeps stopped three times before they passed**, each case pinned above:
  - on `2de1a554`, seed `2409886620174256796` of `a_single_process_never_stops` at 10,000 cases: the `through` hole;
  - on `2de1a554`, seed `10183128751732460969` of `gate_world` at 10,000 cases: no longer failing once the `through` fix landed;
  - on `eb661278`, seed `17611438872220405175` of `gate_world` at 50,000 cases: the moved marker on a reused generation number.
  - Earlier, the world's first runs found two model errors of the new store: the alias kept on a deleted generation, and a lane left without its gap past a way-out reattach. Both are fixed in the world.
- **On `731915a0`, everything passes:**
  - `gate_world` at 50,000 cases, 5 seeds: `5314952944604234503`, `15376243376204117371`, `1813758078143670455`, `11802617948610117132`, `9137624259147426461`;
  - `a_single_process_never_stops` at 50,000 cases, 5 seeds: `11548598505017312669`, `2580746782486355148`, `102763830326267967`, `14673866775106845812`, `235507850227250249`;
  - `gate_property` at 20,000 cases, 3 seeds: `3456071603121051413`, `17654448588881445174`, `16261749963614158946`.
- **Each fix reverted on `731915a0`** (both properties, 5,000 cases, three fresh seeds each, plus the pinned cases and the unit tests). Each is caught 3 of 3 by both properties:
  - A, reverted in the plans (`moved_past` keeps no gaps, `rewritten` carries nothing): `gate_world` and the solo property both shrink to `GapThroughRebuild { lane: 0, moved: false }`; caught by plan `only_the_seed_drops_the_gaps` and adapter `a_gap_survives_a_rebuild_begun_over_a_live_only_detach` and `a_gap_survives_a_begun_rebuild_put_back_live_only`;
  - D, reverted in the engine: `gate_world` shrinks to `LiveOnlyBesideMoved { lane: 0 }`; the pinned D cases fail; caught by engine `a_live_only_record_beside_a_moved_marker_reattaches_after_a_start`. D has no adapter part, so its unit test is the engine's;
  - E, reverted in the plan, `AtStaging` and the seed's read back: both properties shrink to `RebuildFromLiveOnly { lane: 0, flip_back: false }`; the pinned E case fails; caught by plan `a_rebuild_begun_from_a_live_only_record_is_not_live_only` and `no_plan_deletes_the_generation_the_alias_serves`.
- **Each smaller fix removed fails its test:** m-d, the put-back read's gaps, `through` over `rebuilding`, the unseeded reattach, the moved-marker rule.
- **m-b has no test.** Nothing reaches it: a sink with no records never reattaches.

### After the review of round 13

The review (`target/review-482-round-13/`) found one blocking hole, F, a start-up flake, H, and minors.

| Finding | Outcome | Guard |
| --- | --- | --- |
| F a live-only demotion carried another generation's uuid onto the serving one, and the next `ensure_index` deleted it | A reindex was under way when the lane detached. The demotion then put the marker back on serving with the reindex generation's uuid and `seeded: false`. A reattach's `ensure_index` read the serving generation as "not the one flusso created" and deleted it. Now the demotion takes `seeded` and `uuid` from `detaching_from` only when that names the serving generation, else from the marker itself when it stands there; otherwise the marker goes back unseeded with no uuid, and the next ensure records the live one. (Round 14's report said a uuid came only from the marker itself; the code, and its plan test, take it from a `detaching_from` naming the serving generation too, as the reviewer's fix proposed. Naming it by number is wrong only after the number was reused, which needs a hand deletion: #492.) The plan's doc states the rule: the marker's `uuid` always names the generation in `active_generation`, or is absent. The other carries of `uuid`, `seeded` and `walked` keep them on their own generation number: the put-back, `rewrite_marker` and d2b. | adapter `a_live_only_demotion_after_a_detach_during_a_reindex_keeps_the_serving_generation` (the reviewer's probe, committed failing first); plan `a_demotion_carries_no_details_of_another_generation`; world `a_detach_during_a_reindex_never_deletes_the_generation_reads_see`; Docker `a_detach_during_a_reindex_then_a_live_only_start_keeps_the_generation_reads_see` |
| Item 2: the model couldn't see F | `MemoryMeta` gives each generation a uuid at creation. It models `ensure_index` as the adapter does: an unseeded generation whose uuid differs from the recorded one is deleted and created again; the uuid is recorded after a create; the alias moves to a seeded generation, or to the first when nothing serves. The world keeps its own ledger of what reads see (`read_by`: the generation the alias serves and its uuid, moved only by a seed). A write that leaves reads on nothing, or on another generation or uuid, fails. `ReindexThenDetach` reaches F. With F's fix reverted on `b55d5324`, both properties fail 3 of 3 at 5,000 cases. The solo property shrinks to `ReindexThenDetach { lane: 2 }`, and `gate_world` to a path through `GapThroughRebuild`. The pinned case and both unit tests fail too. Real-cluster cases for D and E were added too. | Docker `a_live_only_record_beside_a_moved_marker_is_put_back_on_serving`, `a_rebuild_from_a_live_only_record_finishes_and_keeps_what_its_seed_serves` |
| H the marker rewrite's read refused while the meta index's shard recovers | The rewrite waits for the meta index's primary (cluster health `yellow`, at most 30 s), then reads once more. The adapter's other transient retries are the bulk's, from `max_retries`, which an operator tunes for busy clusters and the tests set to 0. A meta read elsewhere fails at once and the staging retries. So this waits for exactly the state OpenSearch reports. | adapter `a_marker_rewrite_waits_for_a_meta_index_still_recovering`, `a_marker_rewrite_fails_when_the_meta_index_never_gets_its_primary` |
| m-1 | Moving a marker that carries `detaching_from` keeps its own gaps too. | plan `moving_a_moved_marker_keeps_the_gaps_of_both_places` |
| m-2 | One drop kept settled in one place and open in the other comes back once, open. An open gap refuses a superset, and the next held read settles it again where the lane stands, which still covers the drop. Two settled ends of one start are both kept: a plan can't order the source's positions. | plan `a_gap_kept_settled_and_open_comes_back_once_open` |
| m-3 | The tautological `debug_assert!` is removed. | — |
| m-4 | A live-only detach moves no marker, and its deletes are best effort, so a failed listing no longer fails it. A rebuildable detach moves the marker past every generation and still needs the listing. | adapter `a_failed_listing_fails_only_a_detach_that_moves_the_marker` |
| m-5 | `bind_then_compare_clusters`: both listeners are bound, then the clusters compared. Its doc says what that buys: a taken or bad address fails at once. The `/healthz` claim is gone. | CLI `a_taken_port_fails_before_any_cluster_is_asked` |
| m-6 | `sink_opensearch::meta` is `#[doc(hidden)]` and says it is not a stable interface; `Plan`'s fields are documented. | — |
| m-7 | The seed's widened read back now has a test that fails without it. | engine `a_seed_forgets_a_live_only_rebuilding_record_its_delete_left` |
| Item 5: the unformed-cluster reason said "write failed" | `SinkError::Read` (no prefix) for a failed read or an unusable answer; `cluster_uuid` uses it. The reason now reads `its cluster_uuid could not be read: the cluster answers cluster_uuid "_na_", which names no cluster (no cluster manager elected yet)`. | CLI `two_unformed_clusters_are_not_one_cluster` (the exact text) |
| G, and two more shapes found by this round's world | All three need a generation deleted by hand, so they go to issue #492 (never reuse a generation number). <ul><li>G: a record naming a generation whose number was reused.</li><li>A `detaching_from` naming a number deleted by hand and taken again: its uuid then names the new generation's predecessor, so a demotion or put-back carrying it could delete the generation reads see.</li><li>A record naming a generation deleted by hand while the alias went to another: never on serving, never reattached.</li></ul> The world exempts a generation number a hand deletion freed from its ledger rule, and such records from liveness (`Store::names_no_serving`, `hand_deleted`). | issue #492 |

**Runs** on the final code commit `b55d5324` (fresh seeds, the regression files set aside), all passing:
- `gate_world` at 50,000 cases, 5 seeds: `14366876558027187610`, `18092216836314654213`, `3216978559052196460`, `9622539758342103720`, `4510213633514949636`;
- `a_single_process_never_stops` at 50,000 cases, 5 seeds: `1829981222936295500`, `14440101531066190450`, `8786220981485316306`, `13822541684002075021`, `8200598218082255423`;
- `gate_property` at 20,000 cases, 3 seeds: `824704552483484640`, `4431911670923886709`, `5561015251111656598`.

The world stopped twice before that, on the two hand-deletion shapes in the last row. Each needs a hand deletion and goes to #492; neither is a model error of the code under test. A 10,000-case sweep of both world properties on the exemption passed too.

### Decided by the owner on 2026-11-26, from the review of step 2

**O29: can a process that staged before another one's detach keep the lane attached? (S3)**

- **The case.** Two processes overlap (a manual run or compose, not Helm's `Recreate`). B stages while A still holds the slot. A then detaches `L`, and the slot passes `L`'s changes under a durable record. When A exits, B takes the slot and runs `L` attached, with a fence vouching for it. A variant: B's staging read lands between A's d2b and d2a, and B's put-back (slice 1) undoes A's marker move.
- **Why it happens.** A busy slot fails B's `live` (`ensure_slot_free`), and the supervisor retries the ingest engine without staging again.
- **The coordinator's recommendation, checked against the code: not sound as stated.**
  - *Re-staging every sink before each retry of a refused `prepare`* leaves a window. The staging before the try that succeeds runs while A still holds the slot, and A can detach, and write its record, until it lets go. The order "B stages, A writes the record, A exits, B's stream opens" stays possible.
  - Re-staging also means stopping each sink engine's run first, since staging never runs beside writes. That is new supervisor work, and it costs one staging round per index at every retry (the backoff is 1 s doubling to 60 s).
  - *Skipping the put-back while another process holds the slot* avoids undoing A's d2b. But B then stages as before over a moved marker: `ensure_index` creates the marker's new generation and backfills into it, which A's d2c may delete. It also needs the slot's holder known on the sink side, which the layering keeps from it (the daemon would hand it a flag).
- **What would be sound.** Read every index's detach records again once B's live stream holds the slot: after `ensure_slot_free` passed and `START_REPLICATION` opened, before the first item is published. Detach (adopt) any lane those records say is detached, and do slice 1's put-back only at that read. A cannot write a record that matters after it lost the slot:
  - A's `Progress` no longer confirms anything;
  - a late record of A's is the self-healing row of §4, one spurious detach and a rebuild.
  - **Cost:** one `_mget` per index per sink at every live start, and an adopt path between staging and the stream.
- **Options:**
  - (a) the read after the slot is held, as above;
  - (b) the coordinator's re-staging per retry, with the window named;
  - (c) document overlapping processes as unsupported, since they can now lose changes, not only write twice.
- **Decided:** (a). Built in step 4, beside the attach at a process start, which reads the records at the same moment (§11). Nothing reaches it before slice 4, since only tests detach.

**O30: what does an unreadable record stage as? (S6)**

- **As built (O24):** an `Unreadable` record is staged as before. That is safe only when the marker moved first: staging then rebuilds into the moved, unseeded generation.
- **Not safe otherwise.** With the marker still on the seeded serving generation (a live-only detach, or a record a hand or a newer flusso left beside a restored marker), staging as before reads the lane as attached and current over changes that were dropped, and only the log says so.
- **The reviewer's proposal:** split the row.
  - An unreadable record whose marker is `Newer { seeded: false }` stages as before, a rebuild.
  - Any other unreadable record runs `Sink::detach` again: the marker on serving, seeded or not, or `serving` unknown.
  - A `rebuilding` record with the marker on serving does the same (§4 says so; today it stages as before).
  - Nothing is lost either way. A detach again costs a rebuild, or, for live only, keeps the gap visible until an operator reattaches.
- **Options:** (a) keep O24 as built; (b) the split.
- **Decided:** (b). It narrows O24, which was decided for slice 1, when this version couldn't detach. Built in step 3 (§10a, step 3).

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
| O19 | Full disk (`read_only_allow_delete` met by the probe) | the rebuild waits for an operator. **Accepted knowingly:** a flood stage longer than `stuck_for` leaves every index on that node detached until an operator runs `reindex`, even after OpenSearch lifts the block itself. Reads stay frozen and the alert stays critical. The cause is classified from the block the live probe meets, which rests on OpenSearch naming the block in its refusal (S1) | automatic: writes a second full copy onto a full cluster and can block other tenants' indexes. A headroom check: heuristic, since the disk is shared |
| O20 | `reindex` on a rebuilding lane | restarts a rebuild that is parked or restaging; ignored, and logged, while it is open | always ignored: no way out but waiting for `rebuild_grace`. Always restart: throws away a healthy backfill |
| O21 | `committed` in `/status` | counts attached lanes only, and lists the detached ones it leaves out | frozen at the detach: the sink looks stuck while detached |
| O22 | Stuck clock while `enabled = false` | not written | written: meta writes for an unused feature |
| O23 | Where the attach happens | the index's builder quiesces (§1) | stopping the whole ingest engine: pauses every index, replays the unconfirmed tail, fails `/readyz` and fences during the restart, and restarts other indexes' backfills from where they were asked |
| O24 | Slice 1 on a record left by a newer flusso after a rollback | log loudly and stage as today: a rebuild, or for `live_only` resuming over the gap. **Narrowed by O30** from slice 2 on: only an unreadable record whose marker is on a newer, unseeded generation stages as today | restaging until upgraded: pins the slot until the WAL limit drops it for every index |
| O25 | The WAL half | only the lane `pinned_by` names, measuring the WAL its detach would free (its position to the next-least lane's, or to the horizon); no grace. Two lanes pinning the slot at the same position free nothing alone, so neither detaches by the WAL half; each waits for the time half | the lane's WAL behind as measured today: counts WAL not yet read and WAL another sink holds, so an outage causes a storm of rebuilds and detaches that free nothing. After a grace: the slot can be lost within it. A 90% hard bound: one more rule for the same case |
| O26 | `--skip-backfill` at a start that finds a running rebuild | demote it, with a loud log: the rebuild and its partial are dropped, the lane is detached | keep it attached and paused: pins the slot for the whole run |
| O27 | The rebuild's walk while another sink's walk of the index runs | it waits for that walk, with the lane attached and live meanwhile, so reads stay on the old generation for up to that walk's length | cut the other walk: costs the other sink its progress since its last mark |
| O28 | A rebuild whose seed parks on `RetireBlocked` (decided 2026-11-26) | exempt from R10's `rebuild_grace`, kept under R2's detach thresholds; `/status` and the log name this park apart (the seed waits for the block on the serving generation); flood stage never counts toward R10, even under a forced `reindex` | under R10: deletes a complete generation after 10 minutes, and each forced `reindex` past a read-only `serving` pays a full rebuild |
| O29 | A process that staged before another process's detach (decided 2026-11-26; **narrowed by the owner on 2026-11-26**, after the fifth re-check) | once this process's live stream holds the slot, before the first item is published, read every index's detach records again and decide from them with the function staging uses (adopt any lane they say is detached, put another process's rebuild back to detached); one `_mget` per index per sink per live start. **Narrowed:** this process erases or loosens no record unless it holds the slot (a seed, the failed-detach clear, a reattach, a reindex, a rebuild wait and are retried); plain writes behave as on `main`. A late plain write from a process whose walsender Postgres dropped before it noticed stays possible, as on `main`: real fencing (a lease) is its own issue. **Cut by the owner on 2026-11-27**, after the eighth re-check: when a read finds that another process changed a lane's records since this process last read them (`known`; since the review of round 10, judged by who wrote them: each write's stamp), the deployment stops (exit 1) with no change of the index confirmed past what its records cover, and the next start decides from the records. Reconciling with another process's changes is deferred to #490 (§10a, "After the eighth re-check: the cut") | Reconciling while running (rounds 7 and 8: a stand-in lane, recording a lane detached again, generation identity, a fresh gap cutting the backfill): each round found new overlaps it got wrong, and the world still failed on the head. Re-staging before each retry: a window remains, since the other process can write its record until it lets go of the slot. Documenting overlap as unsupported: an overlap would lose changes. Gating every write while no stream is current (the fourth re-check's round): covered half the window, stalled stops and reconnects for every deployment, and kept finding siblings |
| O30 | An unreadable record at staging (decided 2026-11-26) | stages as today only when the marker is on a newer, unseeded generation (a rebuild); any other unreadable record, and a `rebuilding` record with the marker on serving, runs `Sink::detach` again | as today for every unreadable record: one beside a seeded marker on serving reads as attached and current over dropped changes |

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
