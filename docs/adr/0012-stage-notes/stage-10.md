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
4. **Thresholds and the probe-gated rebuild,** with `enabled = true`: R2, R3, R10, the stuck clock, the probes, the schedule, the alert. Gated on slice 3.
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

- **A second reattach** keeps the first gap's `from`. The marker holds one gap, which then spans both and also refuses what lies between them. That is sound but not exact; see the report's options.
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
- **The put-back stays at staging.** `Sink::detachment` takes no `put_back` argument. Undoing another process's half-moved marker is healed by this read: that process's record, written after, sits beside a marker on serving, and O30's row detaches again.

**The edge.**

- The reattach refusals `409` "the lane is not detached" and "being attached already" come from `Status::reattach_refusal`. "Can rebuild" comes from `DaemonControl`, which now knows the `backfill = false` sinks (`ControlError::CanRebuild`).
- The record checks of §6a (not `live_only`, marker not on serving seeded) are made by the engine only. The edge reads no record, so it answers `202`; the engine logs the refusal and the lane stays detached.
- `/status` shows the gap per lane (`gap: {from, to}`) until a seed, across restarts, instead of §6a's `reattached: {at, gap_since}`.

**Tests**, beyond those the design listed:

- the engine's `a_reattach_keeps_the_gap_deletes_the_record_and_attaches_the_lane`, `a_reattach_is_refused_where_it_does_not_apply`, `a_reattach_cut_short_before_its_attach_completes_its_gap_once_the_slot_is_held`, `a_second_reattach_keeps_the_first_gaps_start`, `a_lane_the_records_say_detached_once_the_slot_is_held_is_taken_as_detached` and `a_staging_once_the_slot_is_held_counts_as_the_read`;
- the fence's `a_reattached_lane_vouches_only_from_its_attach` (in the process and after a restart), `an_open_gap_refuses_everything_past_its_start_and_a_seed_ends_it`, `a_detach_freezes_where_the_sink_recorded_it` and `a_sink_that_has_not_read_its_records_since_the_slot_opened_vouches_for_nothing`;
- the OpenSearch sink's `a_reattach_keeps_its_gap_with_the_seed_marker`;
- the CLI's `a_wait_inside_a_reattachs_gap_is_refused_with_409`, `a_reattach_is_refused_unless_it_applies_and_queued_when_it_does`, `status_shows_a_lanes_gap_until_a_seed` and `a_reattach_without_accept_gap_is_refused_before_any_request`.

**Not proven:**

- No live run: there is no Postgres or OpenSearch e2e of a rebuild, a reattach, the gap's `_update`, or two overlapping processes (O29). Every test runs over stand-ins.
- The restart's `to` relies on the Postgres capture seeding its horizon from `confirmed_flush_lsn` at the stream's start, and on the read happening before the lane lands anything. Both are argued above and tested only with stand-ins.
- The second read-again check, before the grant after `fill`, has no test that fails without it: the first check catches every case the tests make.
- A `record_gap` failure at `Attached`, and a gap end that doesn't parse (read as unknown), are untested.

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
| O29 | A process that staged before another process's detach (decided 2026-11-26) | once this process's live stream holds the slot, before the first item is published, read every index's detach records again, adopt any lane they say is detached, and do slice 1's put-back only at that read; one `_mget` per index per sink per live start | re-staging before each retry: a window remains, since the other process can write its record until it lets go of the slot. Documenting overlap as unsupported: an overlap would lose changes |
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
