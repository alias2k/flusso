# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status:** design only, for the owner's decisions (§13). Nothing is built.

This is the third revision of the note, after two independent design reviews of PR #478. §16 lists each review finding and where the note answers it.

**ADR 0012 §5** says: "Capping those costs by dropping and rebuilding one index is deferred to its own issue and design."

- Issue #394 holds the holes three review rounds found, and, in its comment, the owner's six decisions of 2026-11-19.
- The note designs against `main` at `8a4053b6`.
- Every claim about how `main` behaves names the code it was read in; §14 lists what was not verified.
- §17 lists what reading the files neither review reached turned up.

## 0. The owner's decisions this starts from (not reopened)

1. **Detach past a threshold, on by default, with an opt-out.** flusso stops holding the slot for the stuck index, marks it detached and in need of a rebuild in `/status`, and raises an alert louder than the stalled-lane one. Other indexes and sinks are unaffected.
2. **The threshold measures what hurts:** time stuck, or WAL held. It never counts errors.
3. **Rebuild automatically once the index accepts writes again** ("the lane's probe lands"), by a backfill into a new generation while the old one keeps serving reads.
4. **Flap protection:** a rebuild that fails returns the index to detached, and the next attempt waits twice as long, up to about an hour.
5. **A setting makes the rebuild wait for an operator;** `reindex` forces one at any time.
6. **A `backfill = false` sink cannot be rebuilt by flusso:** its index stays detached and reported until an operator deals with it.

The owner left four points open: the default thresholds (O1), what `/wait` answers (O2), `/readyz` (O3), and the alert and runbook (O4).

## 1. What exists on `main` that this builds on

### Confirmation (`progress.rs`)

- `landed()` is `over(self.marks.keys()).unwrap_or(Landed::Idle)` (524).
- `least_of` gives each lane the oldest of three things:
  - its own unlanded registrations;
  - `sent_to` (a build being published that the lane hasn't taken);
  - its index's `backlog_for` that sink.
- A lane with nothing outstanding answers `Through` the larger of its `newest` and `registry.routed`.
- Over an empty set of lanes the answer is `Idle`, and Postgres confirms nothing on `Idle` once something was emitted (`cdc/ack.rs:25-26`, `122-135`).
- The source takes the maximum of what it is told (`ack.rs:223`).
- Marks move only through `applied`/`searchable`. An item acknowledged unapplied raises nothing (`writes.rs:2126-2131`).
- `register` runs after a lane's own send returns (`commit.rs`'s `publish`), so a send blocked on a full lane registers late.
- Item keys are per-index atomics (`next_key`), taken in `publish_live` outside every lock. They are the same on every lane of the index and last for the process.
- Snapshot batches carry no key.

### The sink engine

- **A parked lane is not drained.** It "takes nothing more from its lane" (`writes.rs` module doc), so the ingest engine's send to it blocks (`publish_to`).
- **`Fences`** (`sink_engine/fences.rs`):
  - Every live item is entered as `Writing` when it is received (`writes.rs:1107`), before anything decides about it.
  - `advance_applied` stops at the first `Writing` entry (171-187), and `advance_searchable` pops only from the front.
  - The fences outlive the engine's runs.
- **A cut backfill's pages are still written.** `received` (around 1137) only withholds their walk mark (R5). A `SnapshotComplete` that doesn't answer is acknowledged without seeding (`grant`, `superseded`).
- **`answers`** is `true` when nothing is outstanding for the index (`sink_engine.rs:990`). Tickets come from `SinkEngine.tickets`, which outlives the engine's runs but not the process.
- **F9:** `LaneState.superseded` maps an id to the build of its landed substitute.
- **A substitute nobody waits for is acknowledged and dropped** (`substituted`, F8). The substitute lane is read only while some lane awaits a substitute (`awaits_a_substitute`). Its backstop is `LANE_BACKSTOP`, at most one substitute per lane.
- **Held items (X2)** sit in `held_for_seed` and are counted in `holding`.
- **Health** (`health.rs`):
  - `Parked { since: Instant }` and `Restaging { since: Instant }` are in memory.
  - `restage()` from parked resets `since` (345).
  - `reopen()` (374) goes through `Open` with no write landing.
- **`SinkEngine::reindex`**:
  - returns at once under `backfill = false` (1128);
  - skips an index only while its backfill is outstanding and its lane is open (1140-1146);
  - `stage_reindex` asks for a backfill whatever `skip_backfill` says (1218).
- **Paths that end in a new ticket:** `ask_again`, `restage`, `ensure_again`, `check_target`, and `TargetChanged` at a seed (`seed_returned`, which restages).

### Staging

- `step` calls `ensure_index`, then `seeding`; the `Seeding` match ends in `_ => Unseeded` (906-937).
- Every call staging makes to the sink goes through `step`.
- **The paths:**
  - `stage`, through `ensure_per_index`, then `rebuild_stale_seeds` and `forget_walks` on `Fresh`, then `request_backfill` unless `skip_backfill` or `backfill = false`;
  - `stage_while_stopping` (ensure only);
  - the restage and reindex paths.

### The OpenSearch sink (`sink_impl.rs`, `generations.rs`)

- **The seed marker** is the meta doc keyed by hash alias: `active_generation`, `seeded`, `walked` (tagged with its generation) and `uuid`.
- **Whole-document `PUT`s** of the marker happen in:
  - `ensure_index`, before creating a generation that doesn't exist (169), and when the doc is missing;
  - `reindex`, a no-op unless seeded (634);
  - `mark_seeded`.
- **Partial updates** with no `retry_on_conflict`: `update_walked`, `record_uuid`.
- **Aliases at creation:** `ensure_index` gives a new generation aliases only when nothing serves yet (`first`, computed at 162, used at 252). `mark_seeded` swaps the aliases and drops every other generation in one call.
- **Writes go through the in-memory target.** A write for an index with no target goes to the logical name, with `require_alias: false` (752).
- **Nothing reads another meta doc id,** and nothing retires an old hash alias.

### Ingest

- Backfill requests reach their index's builder (`dispatcher.rs`'s `forward`), then `start_snapshot` (`snapshot.rs:77`).
- `start_snapshot` loops over the deliveries (88-115), runs `plan_walk` (126), then `Walk::new` (135).
- Key pages are read only from the walk's first `step` (`reading_apart` only sets a flag, 457).
- Requests are acknowledged at `finish_snapshot`. A failed walk or a restarted builder hands them back (`into_requests`) with the same ticket, and they are served again.
- Live batches are granted while requests coalesce (`builder.rs:14`).
- `publish_live` decides a build's lanes at publish (`commit.rs:648-690`).

### Daemon

- **Stall** (`stall.rs`):
  - the age resets when the lane's oldest seq changes, and `since` starts at `Stalls::new`;
  - WAL behind = slot lag − (reached(lane) − reached(landed)).
- **Fence** (`fence.rs`):
  - `Serving::{Pending, Writes, Backfilling, Rebuilding { through }}`; `reached()` raises `furthest` from `progress.lane` on every call.
  - `wait` returns `Unavailable("the source is not following live changes yet")` as soon as any selected lane's `reached` is `None` (around 503).
  - `Unserved` is answered only at the timeout.
- **Readiness** (`is_ready`) is phases only.
- **Alerts:** `FlussoLaneStalled`, `FlussoLaneParked` and `FlussoIndexBacklog`, all `page`.
- **`max_slot_wal_keep_size`** is read nowhere.

## 2. The rules

| # | Rule |
| --- | --- |
| D1 | **Two new lane conditions.** `LaneHealth::Detached` is a lane that is out of the minimum and discards everything. **Rebuilding** is an ordinary health (`Open`, or parked or restaging during the rebuild) plus `LaneState.rebuild: Some(Rebuild { attempts })`, plus, for an attach inside one process, a floor in `Progress` (D10). Every `is_open()` gate in `writes.rs` then treats a rebuilding lane as the open lane it is (A8). The same index on another sink is untouched. |
| D2 | **Only a diagnosed lane can detach.** The lane must have been `parked` or `restaging` without a write of it landing, for `stuck_for`, or for `grace` with its WAL behind past `wal_held`. This is measured by an explicit `LaneState.stuck_since` (wall clock), set when the lane first parks or restages and cleared only when a write of the lane lands, never by `reopen` or `restage` (A7). It is durable (D12). An open lane is never eligible, whatever its age. |
| D3 | **A detach happens at a refusal.** The daemon asks (`SinkControl::Detach`). The sink engine detaches at the lane's next failed probe or failed restage, which is an answer from the destination. A probe that lands opens the lane and drops the ask. Nothing waits on the age of a run: after an outage a lane is probed first, and a lane whose cause went away opens (B4, O18). |
| D4 | **Recorded at the sink before it takes effect, marker first.** `Sink::detach` runs three steps. **d2b:** an `_update` of the seed marker to the next generation, unseeded, with `walked: null` and `uuid: null` (`retry_on_conflict`). **d2a:** a `PUT` of the detach record, its own document. **d2c:** delete partials, best effort. The lane leaves the minimum only once d2b and d2a both returned `Ok` in this process. A sink that can't record never detaches, and the ask is retried at the next refusal. Because a record exists only after its marker moved, a half-detach can never freeze reads (B1). An ambiguous d2a is retried. If the lane opens meanwhile, the record is deleted, best effort. A record left behind costs one rebuild, never data. |
| D5 | **The fence is frozen, then the lane leaves the minimum.** The fence first brings `furthest` up to date and serves `Detached { through }`, and `reached()` no longer raises it. Then `Progress::detach(lane)`, under the registry lock: the lane's registrations and steps are cleared, and `register` on it is a no-op. The lane is skipped by `least_of`, `pinned_by`, `own_oldest` and `committed`, and by a fence over all lanes. **With no attached lane, `landed()` answers `Through(routed)`, so everything read is confirmable (A2).** `landed()` only moves forward at a detach. |
| D6 | **Everything of the lane is discarded and released.** These are acknowledged unwritten and unapplied: its window items, its held items (`holding` decremented, `held_for_seed` cleared), and everything arriving later. A detached lane is always taken from, so a blocked send completes. Its `Fences` entries are removed (with `deferred` fixed), and a discarded item is never entered there: the discard is decided before `fences.received` (A3). Its fresh-build ticket, F9 map, owed reindex with its retry, and the index in `pending_reindex` are dropped. Its outstanding backfill entry is kept, marked `cut` (A2 of round 1). |
| D7 | **Ingest publishes nothing to a detached lane.** `publish_live` treats it as a lane with nothing built: `register` is a no-op, and `settle_build` still runs. An old backfill request is not served for that lane. An index detached on every sink is still built until slice 6 (O10). The #452 exemption sees it as free, because its items are acknowledged at once (§17). |
| D8 | **A rebuild always fills a new generation**, the one d2b names, and starts only once every unserved generation is gone. So at most one partial generation of the index exists. |
| D9 | **Attaching.** **Inside one process:** before publishing the rebuild's request under ticket `T`, the sink engine calls `Progress::expect_attach(lane, T)`. `start_snapshot`, in its loop over the deliveries and before `plan_walk` and `Walk::new`, calls `Progress::attach(lane, T)` under the registry lock. The first call for `T` sets the floor and `from` and consumes the expectation. A later serve of `T` (a failed walk, a restarted builder) finds the lane attached by `T`, keeps the first floor, and its pages go to the lane (B3). Any new ticket for a lane not attached yet (re-ask, restage, `check_target`, `ensure_again`) re-points the expectation first, in `request_backfill`/`ask_again` (A8). A request with an unexpected ticket for a detached lane is not served for it. **At a start:** a lane whose record says `rebuilding` is attached at staging, before the ingest engine runs, with no floor and no `from`. It is in the minimum from its first moment, which is today's resumed unseeded index (A1). |
| D10 | **The floor and the clamp** apply only to an attach inside one process. On such a lane, a live item keyed below the floor, and a page or `SnapshotComplete` not answering the lane's current request, are acknowledged unwritten, and `Progress` ignores a registration below the floor. Until its own answer first reaches `Through(from)`, the lane answers `Through(from)` whenever its own answer is lower. After that the clamp is gone for good, so lifting it never steps `landed()` back (B8). No seed API is needed. |
| D11 | **The rebuild starts once its probe lands** (decision 3, §6), on the flap schedule, or at `reindex`. **A rebuild fails** when its lane stays parked or restaging for `rebuild_grace` during it (B5, O17). The lane then detaches again, attempts + 1. Meanwhile it is an attached parked lane, holding the slot as any parked lane does. |
| D12 | **Flap state, refused ids and the clock are durable** in the detach record (`attempts`, `last_attempt`, `stuck_since`, `refused`). |
| D13 | **`reindex`.** On a detached lane it forces the rebuild. On a rebuilding lane, see O20. `--skip-backfill` blocks automatic rebuilds only. An operator's `reindex` asks its backfill, as `stage_reindex` does today. `backfill = false` refuses `reindex`, as today (1128). |
| D14 | **A `backfill = false` sink.** Its detach writes the record but skips d2b, so the serving generation stays the write target. Nothing reattaches the lane except an operator's explicit "accept the gap" (O16). |
| D15 | **No envelope of a detached index reaches `Sink::write`**, guarded by a test. With no target staged, a write would go to the logical name (A6). |
| D16 | **A full disk** (`read_only_allow_delete` in `why`) is rebuilt per O19. |

## 3. States and transitions

| State | Health and marks | Holds the slot | Written | Record |
| --- | --- | --- | --- | --- |
| open | `Open` | its unlanded items | yes | none, or `stuck` |
| parked / restaging | `Parked` / `Restaging` | its items | probes / no | none, or `stuck` once past `grace` |
| **detached** | `Detached`, out of `Progress` | nothing | nothing; items discarded | `detached` |
| **rebuilding, waiting to attach** (one process, r2 to r3) | `Detached`, `expect_attach(T)` | nothing | nothing | `rebuilding` |
| **rebuilding** | `Open` (or `Parked`/`Restaging` during `rebuild_grace`) + `rebuild` + floor/`from` if attached in-process | its items at or past the floor, never below `Through(from)` until passed | into the new generation | `rebuilding` |

**Into detached.** The detach is done at a lane barrier: nothing of the lane in flight, checkpoint flights included.

| From | What goes |
| --- | --- |
| parked, refused | the parked item; the fresh-build ticket (a later substitute is dropped, F8); the F9 map; the refused ids go into the record (B7) |
| parked, blocked | the parked item and everything behind it |
| restaging, target gone | the restage retry |
| restaging, owed reindex | `LaneState.reindex`, its retry, and the index in `pending_reindex` |
| restaging, left out (#471) | the `unensured` mark |
| any, with held items (X2) | the held items, acknowledged unapplied; `holding` decremented |
| any, mid-backfill | the outstanding entry, kept cut; its pages and completion are discarded |
| rebuilding, parked or restaging past `rebuild_grace` | the same as parked or restaging; attempts + 1 |
| open | never |

**Leaving detached:**

- **To rebuilding**, when the probe lands and the schedule allows, or on `reindex`, unless `rebuild = "operator"`, `--skip-backfill` or `backfill = false` keeps it detached.
- **To open, live only**, on a `backfill = false` sink, only by O16's explicit verb.

**Leaving rebuilding:**

- **Seeded:** the lane is open, `rebuild` is cleared, the record is deleted, and `attempts` resets.
- **Fails (D11):** the lane is detached again.
- **A restart:** the lane is attached at staging (D9). Under `--skip-backfill` or `backfill = false` it is demoted instead: `Sink::detach` runs again (d2b names a fresh generation and forgets the walk; d2c deletes the partial), and the lane is staged detached. Otherwise a later start would resume a walk that missed the changes made while it was detached.

## 4. The order of recording, and a kill at each step

### The detach record

The record is its own document, `detach:{hash_alias}` in the meta index (OpenSearch). No existing code writes it. Each meta `_update` uses `retry_on_conflict`.

```json
{ "state": "stuck" | "detached" | "rebuilding",
  "stuck_since": "…", "why": "blocked: index.blocks.write",
  "refused": ["<id>", "…"], "serving": 3, "attempts": 2, "last_attempt": "…" }
```

`serving` is the generation the hash alias serves at d2a. It is the seed marker's generation if no alias serves one.

### Staging reads it first

`step` calls `Sink::detachment(mapping)` before `ensure_index`, for every `Step` and in `stage_while_stopping`. The record and the seed marker are classified by whether the marker names `serving` (B2):

| Record | Marker | Meaning | Staging |
| --- | --- | --- | --- |
| none, or `stuck` | any | not detached | today's staging; a lane that parks again takes `stuck_since` from the record |
| `detached` or `rebuilding` | names `serving` (seeded or not) | the detach died before d2b, or was restored | run `Sink::detach` again, then as `detached`; if it fails, the index is restaging with why and holds the minimum |
| `detached` | names another generation, unseeded | detached | `Staged::Detached`: no `ensure_index`, no backfill; `Progress::detach` and the fence's `Detached { through: unknown }` before the ingest engine starts |
| `rebuilding` | names another generation, unseeded | mid-rebuild | under `--skip-backfill`/`backfill = false`, demote (§3); otherwise attach at staging (D9), `ensure_index`, `seeding` → `Unseeded { walked }` (`Fresh` forgets it), and request the backfill, resuming |
| `detached` or `rebuilding` | names another generation, seeded | finished: by r4, or by an older flusso | delete the record; today's staging |
| anything else | — | unknown | run `Sink::detach` again |

- **`rebuild_stale_seeds` never sees a detached index**, since it only takes `Staged::Seeded`.
- **`stage_while_stopping`** stages a detached index as detached, and a rebuilding one as attached with ensure only.
- **The `Seeding` and `Staged` matches name every variant.**

### Detaching lane `L` of index `I` on sink `S`

| Step | What happens | Durable? |
| --- | --- | --- |
| d0 | the daemon finds `L` eligible and sends `SinkControl::Detach { index }` | no |
| d1 | `L`'s next probe or restage fails; lane barrier | no |
| d2b | seed marker `_update`: next generation, unseeded, `walked`/`uuid` null (skipped for `backfill = false`) | **yes** |
| d2a | `PUT detach:{alias}`: `detached`, `serving`, `attempts`, `stuck_since`, `why`, `refused` | **yes** |
| d2c | delete every unserved generation but `serving`, best effort | yes |
| d3 | fence `Detached { through }` (frozen); observer; `LaneHealth::Detached` | no |
| d4 | `Progress::detach(L)` | no |
| d5 | discard (D6) | no |

**A kill after each step, then a restart.** The new process replays from the slot.

| Killed after | The restart finds | Correct because |
| --- | --- | --- |
| d0, d1 | the old marker, no record or `stuck` | the slot never passed `L`; `L` parks again, its clock resumes, and its next failed probe detaches it |
| d2b | the marker on an unseeded generation that doesn't exist, no record | today's staging creates and backfills it: an unrequested rebuild while `G` serves, with `L` in the minimum throughout; no data is lost |
| d2a, d2c | `detached`, marker elsewhere | `L` starts detached; the slot never passed `L` in the dead process, and the rebuild reads every row |
| d3–d5 | the same | the slot may have passed `L`; the new generation is backfilled from the database |

- **A timed-out d2a:** retried. Applied while the lane later opens, the lane is still in the minimum and current. The record is deleted at the open, best effort; failing that, the next start rebuilds once, which costs no data.
- **A downgrade:** an older flusso ignores the record, creates the generation the marker names, backfills it and seeds it. That is a correct rebuild, and the record is stale by the table above.

### The rebuild of `L`

| Step | What happens | Durable? |
| --- | --- | --- |
| r0 | the probe landed on schedule, or `reindex` | no |
| r1a | `begin_rebuild`: delete every unserved generation but `serving`; a refusal fails this attempt (attempts + 1, nothing read) | yes |
| r1b | the marker names a generation that doesn't exist, unseeded (already so after d2b) | **yes** |
| r1c | `PUT` record `rebuilding`, attempts + 1, `last_attempt` | **yes** |
| r2 | `ensure_index` creates it; outstanding → `T`; `expect_attach(L, T)`; fence `Rebuilding` keeping `through`; `Request::Backfill { T }` | |
| r3 | `start_snapshot` attaches `L` for `T`: floor, `from`; `LaneHealth` `Open` + `rebuild` | no |
| r4 | `SnapshotComplete { T }`: `mark_seeded` swaps the aliases and drops `G`, `PUT`s the marker seeded, then deletes the record | **yes** |

**A kill or a stop at each step, then a restart:**

| Ended after | The restart finds | Correct because |
| --- | --- | --- |
| r0–r1b | `detached` | nothing started; the schedule resumes |
| r1c, r2 (`L` never attached in that process) | `rebuilding`, no walk mark | `L` is attached at staging and the walk starts from its first key: every page is read after this attach |
| r3, pages p1…pk landed, mark at k, then a change `c` to a row in p1…pk, published to `L`, not landed | `rebuilding`, mark k; `c` replayed (the slot stayed before `c`, which is past `from`) | `L` is attached at staging, so `c` is delivered and written. Rows up to k were read after the first attach, so changes up to `from` are in them; changes after it were confirmed only once landed. Rows after k are read by the resumed walk (A1) |
| a graceful stop during r3 | the same | the stop acknowledges `L`'s live items unapplied (§15.3), so they are replayed, as above |
| r4 before the record's delete | `rebuilding`, marker seeded elsewhere | finished; the record is deleted |

**A failed rebuild** (`L` parked or restaging for `rebuild_grace` after r3) runs d2b–d5 again. d2b names the next generation, d2c deletes the partial, and the record goes back to `detached` with the attempts kept.

## 5. Interactions

- **The race between pages and the stream, inside one process.**
  - A change marked before the attach is emitted only once visible (#343), so every page, which reads after the attach, sees it.
  - A change committed after a page's read is emitted after it, marked after the attach, and published to `L`.
  - An item keyed below the floor was built before the attach, and its change is in the pages.
  - `publish_live` must take the key before it checks the lane under the registry lock. That is a requirement on new code, sufficient by the review.
- **A send blocked on `L` before the detach** completes once D6 drains `L`; its registration is a no-op.
- **Ingest and budgets:**
  - Discarded items release their bytes with the last lane's acknowledgement.
  - `index_states`' lane hold ends when the blocked send goes through.
  - The #452 exemption (`outstanding.rs`, `reader.rs`'s `someone_free`) sees the index free: a detached lane acknowledges at once, so the index's `Published` guards drop.
  - An index detached on `S` and healthy on `T` is paced by `T` alone.
- **Substitutes.** A detach drops the lane's ticket, so a substitute still on the lane is dropped when read (F8). The substitute lane is read only while some lane awaits one, and its backstop holds one per lane, so a stale substitute waits harmlessly. The refused-row probe (§6) reads its own ticket, apart from a parked item.
- **Several sinks.** The slot follows `T`. On fences, see O14.
- **A whole sink.**
  - Down: its run fails, no lane is diagnosed, nothing detaches.
  - Refusing writes to its data indexes only: each lane detaches on its own (O5).
  - Flood stage: the per-index `read_only_allow_delete` covers the meta index only if its shard sits on a full node. A full disk may or may not detach, and its rebuild is O19.
- **A reindex control:** see D13 and O20.
- **R5 and #477:** a walk checkpoint names its generation, and `Meta::read` drops one of another. d2b and r1b always name a number past every existing generation; keep d2b before d2c.
- **A schema change while detached:** the new hash alias has its own marker and no record. The old alias's record and generations stay, as old aliases do today (O15).
- **Counters:** quarantine counters are untouched. `committed` is per O21.

### The stop (extends §15.3 of the stage 9 note)

| Item \ lane | detaching (d2 in flight) | detached | rebuilding |
| --- | --- | --- | --- |
| d2, r1 in flight | finishes (bounded by the sink's timeouts); recorded: detached; not: its previous row | — | — |
| live item | discarded once recorded; otherwise its previous row | discarded | acknowledged unapplied, replayed by the next start (attached at staging) |
| backfill page | discarded | discarded | `skip_backfill`; the record stays `rebuilding` |
| `SnapshotComplete` | cut | discarded | as open |
| a probe landing, or a rebuild due | not started once stopping | not started | — |

## 6. The rebuild trigger

### The probe, by the cause in `why`

| Cause | Probe |
| --- | --- |
| blocked: write block, `read_only`, closed, cluster block | `Sink::accepts_writes(mapping)`: a one-item `_bulk` index op `{"index": {"_index": <hash alias>, "_id": <probe id>, "if_seq_no": 0, "if_primary_term": 1, "require_alias": true}}` with `{}`, read by the existing `read_bulk_response`. Landed means the item failed with `version_conflict_engine_exception`; any other answer means refused |
| refused rows | a fresh build of `refused` (the ids in the record, parsed back to root keys; B7) through `Request::Rebuild` under a probe ticket, written alone through the hash alias with `require_alias`, never the in-memory target (A5). It writes the rows' current state; a deleted row's build is a delete |
| restaging, or `refused` empty | the rebuild itself: r1–r2 fail before any page is read while the cause remains |
| full disk (`read_only_allow_delete`) | per O19 |

**Hazards** (review E1), each settled before slice 4 by the spike test `the_probe_writes_no_document`:

- **Order of checks.** OpenSearch runs ingest pipelines, then auto-create, then the global `WRITE` block, then resolving the concrete index (a closed index refuses here), then the index blocks, and only then the shard's version check. A conditional index op on an absent id therefore answers `409` with nothing written, and costs one write-pool task.
- **The alias form.** Using the hash alias with `require_alias` stops a deleted generation from being auto-created by the probe. A missing target then answers `index_not_found`, which is classified as missing.
- **Pipelines.** `default_pipeline`/`final_pipeline`, or the sink's `pipeline`, run first. A `fail` processor refuses a writable index, and a `drop` may answer success. The spike decides whether a pipeline on the index disables the write probe; `pipeline=_none` does not skip a final pipeline.
- **The probe id.** It contains U+0000, which no Postgres `text` value holds, so no root key renders it. OpenSearch accepting such an `_id` is part of the spike; the fallback is a fixed id with the conditional check's tiny overwrite risk, documented.
- **Never a delete.** A delete is allowed under `read_only_allow_delete`, so it would land under flood stage.
- **A one-item `_bulk`.** It uses the permission flusso already writes with, not `indices:data/write/index`.
- **Fallback if the ordering proves wrong:** a read-only probe of `GET <generation>/_settings/index.blocks.*`, the cluster blocks, and open/closed. It misses permission refusals and pipelines, which are left to the rebuild-as-probe.

**What a probe never does:** a probe for a block writes nothing. A probe for a refused row writes only current rows into the serving generation, which can only make it more current. An operator's block or close on the serving generation holds the rebuild until it is lifted or `reindex` forces it.

### The schedule

- Probes run on the lane's parked backoff.
- A rebuild starts at the first landed probe at or after `last_attempt + retry_initial × 2^(attempts − 1)`, capped at `retry_max`. With no attempt yet, the wait is `retry_initial` from the detach.
- `attempts` resets only at a seed.
- `rebuild = "operator"`: probes still run and `/status` shows them, but only `reindex` starts a rebuild.

## 7. Port changes

**`Sink`** (defaults leave every existing sink unchanged):

- `detachment(&IndexMapping) -> Result<Option<Detachment>>`: default `Ok(None)`.
- `detach(&IndexMapping, &Detachment) -> Result<Recorded>` (d2b, d2a, d2c): default `Ok(Recorded::NotKept)`, so stdout never detaches.
- `forget_detachment(&IndexMapping)`: a lane that opened after an ambiguous d2a.
- `record_stuck(&IndexMapping, since)` and `clear_stuck`: default no-ops.
- `begin_rebuild(&IndexMapping, &Detachment)` (r1a–r1c).
- `accepts_writes(&IndexMapping) -> Result<Probe>`: default `Probe::Unknown`, so the rebuild is its own probe.
- `write_serving(&IndexMapping, &[Envelope])`: the refused-row probe, through the hash alias with `require_alias`.
- `mark_seeded` deletes the record after its marker `PUT`.
- `SinkError` gains no variant.

**Source:**

- `DocumentBuilder::key_of(&DocumentId) -> Option<RowKey>`, to rebuild refused ids after a restart. Single-column root keys are validated at start, so an id parses back; this is unverified (§14).
- `ChangeCapture::wal_limit() -> Option<u64>`: `current_setting('max_slot_wal_keep_size')`, where `-1` reads as `None`.

**Engine:**

- `Staged::Detached` and `Staged::Rebuilding`. `step` reads the record first, and the matches name every variant.
- `Progress`: `detach`, `expect_attach`, `attach(lane, ticket) -> bool` (idempotent per ticket), `attach_at_start(lane)`, `floor(lane)`, `is_detached(lane)`. `register` checks the detached state and the floor. `landed()` with no attached lane answers `Through(routed)`.
- `Fences::forget_lane(lane)`.
- `SinkControl::Detach { index }`, not exposed publicly (O12).

**Fence:**

- `Serving::Detached { through }` and `Unserved::Detached`.
- `vouched` checks `Detached` before the `reached` early return and answers at once (A4).
- `slowest` skips detached lanes.
- `rebuilding()` keeps a detached `through`.

**Observer:** `on_lane_detached`, `on_lane_attached`, `on_detach_refused`, `on_rebuild_failed`.

## 8. Configuration

`[detach]` in `flusso.toml` adds keys only. It belongs to the machine, as `[backoff]` does: `#[serde(skip)]`, never written into the lock. Every key has a flag and an env var (`--detach-stuck-for-secs`, `FLUSSO_DETACH_STUCK_FOR_SECS`), and the order is flag > env > file.

| Key | Default | Meaning |
| --- | --- | --- |
| `enabled` | `false` until slice 4, then `true` | decision 1's opt-out |
| `stuck_for_secs` | 14400 | diagnosed this long → detach |
| `wal_held_mib` | half of `max_slot_wal_keep_size`, else 8192 | WAL behind past which a lane diagnosed for `grace` detaches |
| `grace_secs` | 600 | the shortest diagnosis for the WAL branch, and when `stuck_since` is first recorded |
| `rebuild_grace_secs` | 600 | how long a rebuilding lane may stay parked or restaging before the rebuild fails (O17) |
| `rebuild` | `"auto"` | `"operator"` waits for `reindex` |
| `retry_initial_secs` | 300 | earliest first rebuild after a detach |
| `retry_max_secs` | 3600 | the cap of the doubling |

- **Per index:** `detach = false` on `[[index]]`, beside `on_error`. It is in the lock, and **written only when set**, so every existing lock stays byte-identical (`golden_lock`). That is a constraint.
- **While `enabled = false`:** no `stuck` record is written (O22).

## 9. Observability

- **`/status`:**
  - per lane: `health: "detached"`, or `rebuilding: { attempts, attached }`;
  - `why`, `detached_since`, `serving: "frozen at the detach"`;
  - `rebuild: { attempts, earliest, probe, waiting_for }`;
  - `detach: { asked, refused }`;
  - top level: `detached: [{ index, sink }]`.
  - A detached lane has no stall age or WAL behind, is never `pinned_by`, and counts in `committed` per O21.
- **`/indexes`** shows the same. **`flusso check`** is unchanged: it is not known to read any sink state (§14).
- **`/wait` (O2, O14):**
  - A `detached` lane vouches up to its frozen `through`. Past it, the lane answers `503 detached` **at once**, not at the timeout.
  - From r2 on it answers `rebuilding` with the same `through`.
  - After a restart `through` is unknown, so the lane answers `detached` at once for every position.
  - A detached lane never makes a wait answer "not following live changes yet".
- **`/readyz` (O3):** unchanged.
- **Metrics:**
  - `flusso.lane.detached{index,sink}` (gauge: detached or rebuilding);
  - `flusso.lane.detaches`;
  - `flusso.lane.rebuild_attempts{outcome = seeded | failed | refused_to_start}`;
  - `flusso.lane.detach_refused`.
- **Alert (O4):** `FlussoIndexDetached`, `max by (index, sink) (flusso_lane_detached) > 0`, `for: 0m`, `severity: critical`. Its text: "reads of {index} on {sink} are frozen at the detach; changes and deletes since are not applied". `FlussoLaneStalled` stops firing for the lane.
- **Runbook split** (no pages written here):
  - `stalled-lane.md` (339 lines, at its limit) keeps finding the lane;
  - `operate/parked-lane.md` and `operate/restaging-lane.md` take the cures;
  - a new `operate/detached-index.md` covers what reads see, the rebuild, forcing or holding it, `backfill = false`, a stale record after a schema change, and a full disk (O19).

## 10. Thresholds: what is measured, where, and across a restart

- **Clock:** `stuck_since` (D2), not the stage 8c stall age. Held items, fan-outs and slow sinks never start it. The stage 8c stall age still drives `FlussoLaneStalled`.
- **Durable:** past `grace`, `stuck_since` goes into the record (`stuck`, best effort). It is cleared at the first landed write of the lane after a start (A7). A crash loop resumes the clock, and the first failed probe after a start detaches the lane once it is past the threshold (B4, O13).
- **WAL:** `stall.rs`'s per-lane WAL behind.
- **Evaluated** in the daemon's sampling task, from `Status` (health, `stuck_since`) and `Stalls` (WAL). The ask is sent once per episode and executed at the next refusal (D3).
- **Eligibility:** `enabled`, and not `detach = false`, and diagnosed, and (`now − stuck_since ≥ stuck_for`, or (`≥ grace` and WAL behind ≥ `wal_held`)).
- **Never:** an open lane, a sink that is down (no lane diagnosed, no record writable), or a lane whose cause went with an outage (its probe lands first).

## 11. Tests (red first)

**Engine:**

- `progress`:
  - `a_detached_lane_leaves_the_minimum_and_its_index_backlog_too`;
  - `with_every_lane_detached_everything_read_is_confirmable` (A2);
  - `a_registration_on_a_detached_lane_is_ignored`;
  - `an_attach_is_idempotent_per_ticket` (B3);
  - `the_clamp_lifts_once_passed_and_landed_never_steps_back` (B8);
  - `a_detached_lane_counts_in_no_committed`.
- `a_detach_writes_the_marker_then_the_record_then_leaves_the_minimum`; `a_refused_detach_changes_nothing`.
- `a_detach_drains_the_lane`: a blocked send completes; budget, `holding`, F9 and `Fences` end empty; the first item after the attach is reported applied (A3).
- `the_outstanding_backfill_is_kept_cut_and_its_late_completion_seeds_nothing`.
- `an_item_below_the_floor_is_never_written`; `an_old_page_is_never_written_into_the_new_generation`.
- `an_old_backfill_request_does_not_attach_the_lane`; `a_reask_before_the_attach_moves_the_expectation` (A8: reindex, restage, `check_target`, `TargetChanged`).
- `the_attach_precedes_every_key_read`, with one builder and with several.
- `a_restart_mid_rebuild_attaches_at_staging_and_loses_no_change` (A1): the change after a landed page and before the stop. Also `a_graceful_stop_mid_rebuild_loses_no_change`.
- `skip_backfill_demotes_a_rebuilding_lane`.
- `a_short_park_during_a_rebuild_does_not_fail_it`; `a_park_past_the_rebuild_grace_detaches_again_with_the_wait_doubled`.
- `nothing_of_a_detached_index_reaches_write` (A6).
- `the_kill_walk`: a sink fake dying after each of d2b–d2c and r1a–r4, then a restart, for `Fresh` and `Resumed`.
- `staging_classifies_every_record_and_marker_pair` (B2).

**Daemon:**

- `an_open_lane_never_detaches`: held through a long reindex, a steady fan-out.
- `the_clock_survives_reopen_and_restage_and_clears_only_at_a_landed_write` (A7).
- `a_crash_loop_detaches_at_the_first_failed_probe_past_the_threshold` (B4).
- `an_unreachable_sink_detaches_nothing`; `a_sink_back_from_an_outage_probes_before_detaching`.
- `fence::a_detached_lane_answers_at_once` (A4); `a_wait_naming_no_index_is_not_starting_because_of_a_detached_lane`.

**Live (`lane_parking`, OpenSearch):**

- `a_detached_index_is_rebuilt_without_rows_deleted_meanwhile`.
- `a_restart_while_detached_comes_back_detached_and_keeps_its_attempts`.
- `a_deploy_mid_rebuild_loses_no_change`.
- `an_operator_block_holds_the_rebuild_until_lifted_or_reindexed`.
- `the_probe_writes_no_document`, the spike: write block, `read_only`, `read_only_allow_delete`, closed, cluster block, a `fail` and a `drop` pipeline, the alias form, a deleted generation, the NUL id.
- `a_refused_row_probe_after_a_restart_uses_the_recorded_ids`.

## 12. Slice plan

Each slice is safe if nothing after it is ever built.

1. **The record is understood and never half-staged.**
   - `Sink::detachment` and OpenSearch's record reader.
   - A record of any state makes its index restage with why ("detached by a newer flusso; this version cannot rebuild it"). It never stages it detached or rebuilding (review D), so an index never runs with no target.
   - Cost: one meta `GET` per index per staging.
   - Tests: staging over hand-written records.
2. **Detach and rebuild, reachable only from tests.**
   - D1, D4–D10, D13–D15; the fence, the drain, `Fences`; `begin_rebuild`; `mark_seeded` deleting the record (best effort: a failure fails the seed, and the next staging finishes it).
   - `[detach]` with `enabled = false`.
   - Tests: the engine set and the deploy-mid-rebuild live test, driven by `SinkControl::Detach`.
3. **The probe spike.**
   - `the_probe_writes_no_document` against OpenSearch, with no feature behind it.
   - It decides the write probe or its read-only fallback.
4. **Thresholds and the probe-gated rebuild, together.**
   - D2, D3, D11, D12, D16; the clock and its record; `accepts_writes`; the refused-row probe; the schedule; `wal_limit`; the metrics and the alert; `enabled = true`.
   - Gated on slice 3.
5. **The runbook split and the user pages**, written by a docs agent before the PR is ready.
6. **Not building an index detached everywhere (O10)**, if measured worth it.

## 13. Decisions for the owner

New and changed rows are marked.

| # | Question | Options | For an operator | Recommended |
| --- | --- | --- | --- | --- |
| O1 | Default thresholds | (a) 4 h diagnosed, or WAL past half of `max_slot_wal_keep_size` (8 GiB unlimited) after 10 min; (b) 1 h; (c) WAL only | (a) 4 h after the parked page; (b) sooner, more rebuilds; (c) never on a quiet database | **(a)** |
| O2 | `/wait` on a detached lane | (a) vouch up to the frozen `through`, then `503 detached` at once, `rebuilding` from r2; (b) `rebuilding` throughout | (a) tells "nothing kept" from "being rebuilt", and answers without waiting | **(a)** *(changed: at once)* |
| O3 | `/readyz` | (a) unchanged; (b) not ready while a lane is detached | (b) restarts a process that a restart doesn't cure | **(a)** |
| O4 | Alert | `FlussoIndexDetached`, `for: 0m`, `critical` | pages at once, louder than parked or stalled | **as proposed** |
| O5 | Every lane of a reachable sink refusing writes to its data indexes | (a) each detaches; (b) none, as an outage | (a) frees the slot (it now does: A2), rebuilt as probes land; (b) the slot fills | **(a)** |
| O6 | Where `[detach]` lives | (a) on the machine, with `detach = false` in the lock written only when set; (b) all in the lock | (a) tuned per environment | **(a)** |
| O7 | stdout | (a) never detaches (nothing to record); (b) detach in memory | (b) misses changes silently across a restart | **(a)** *(changed: the `backfill = false` half moved to O16)* |
| O8 | The rebuild trigger | (a) a probe: the no-write check for a block, the recorded refused rows' fresh build, else the rebuild itself, on the doubling; (b) a timer | (a) holds while an operator's block is in place and costs no backfill until the cause is gone; (b) routes around a deliberate block and its seed deletes that generation | **(a)**, now faithful to decision 3 in every case *(changed: refused ids recorded, B7)* |
| O9 | Flap state | (a) in the record; (b) in memory | (b): every restart rebuilds at once | **(a)** |
| O10 | An index detached on every sink | (a) stop reading its changes; (b) build it for no lane | (a) saves builds | **(a), slice 6, once measured** |
| O11 | First rebuild | (a) ≥ 5 min after the detach, at a landed probe; (b) the first landed probe | (b) usually meets the cause again | **(a)** |
| O12 | Operator `detach` command | (a) none; tested through the engine's control; (b) `POST /detach` + `flusso detach` | (b) drops an index's changes on purpose, undone only by a rebuild | **(a)** |
| O13 | The stuck clock across restarts | (a) recorded past the grace, resumed after a start; (b) memory | (b): restarts faster than `stuck_for` never detach, and the WAL limit then loses the slot for every index | **(a)** |
| O14 | A fence naming no sink (or no index) while `I` is detached on `S` only | (a) it answers `detached` at once; (b) skip detached lanes | (a) fails fenced reads of `I`, and a raw `/wait` naming no index fails for every index, until the caller names the sink or index it reads (`flusso-query` names its indexes); (b) passes readers of frozen data | **(a)**, documented *(changed: wider reach, A4)* |
| O15 | What is left and reported | (a) `check` unchanged; `/status` and `/indexes` report; a schema change leaves the old alias's record and generations as old aliases are left; (b) sweep them | (b) deletes data, which nothing does today | **(a)** |
| O16 | **New:** reattaching a detached `backfill = false` lane | (a) `reindex` does it; (b) a distinct verb, `flusso reattach --accept-gap` / `POST /reattach`; (c) `reindex --accept-gap` | **The gap:** every build published between the detach and the reattach, and the items discarded at the detach. Deletes in it leave ghost documents for good; updates in it stay stale until the row changes again. Documents marked but not built at the reattach are written. (a) makes `reindex`, which means "rebuild", mean "accept this"; (b) and (c) make it explicit, and the answer names the gap | **(b)** |
| O17 | **New:** a park during a rebuild | (a) fails the rebuild after `rebuild_grace` (10 min) parked or restaging; (b) at once; (c) after `stuck_for` | (b) throws away a multi-hour backfill for a 30 s block (flood-stage flapping, a snapshot); (c) holds the slot hours per attempt | **(a)** |
| O18 | **New:** what gates a detach after an outage | (a) the lane's next failed probe (an answer from the destination); (b) time since the destination last answered; (c) the age of the sink engine's run | (c) never detaches in a crash loop or with a flaky sink; (b) adds a clock; (a) is already the rule | **(a)** |
| O19 | **New:** a full disk (`read_only_allow_delete`) | (a) the rebuild waits for an operator for that cause; (b) automatic, with a headroom check (free space on the data nodes ≥ the serving generation's size plus a margin, from `_cat/allocation`); (c) automatic, no check | (c) writes a second full copy onto a disk-pressed cluster and can block every other index on the node, so others are no longer unaffected; (b) is heuristic, and other tenants share the disk | **(a)** |
| O20 | **New:** `reindex` on a rebuilding lane (decision 5: "at any time") | (a) ignored and logged: a rebuild is running; (b) restart the rebuild into a fresh generation | (b) throws away the backfill so far | **(a)** |
| O21 | **New:** `committed` in `/status` | (a) counts only attached lanes, and the sink's entry lists the detached ones it leaves out; (b) frozen at the detach | (a) "landed on everything flusso keeps"; (b) the sink looks stuck for as long as it is detached | **(a)** |
| O22 | **New:** the `stuck` record while `enabled = false` | (a) not written; (b) written, so turning detach on uses the history | (a) no meta writes for an unused feature | **(a)** |

**Earlier recommendations that changed:** O2 now answers at once. O7 is narrowed to stdout, and its `backfill = false` half became O16. O8 is now faithful to decision 3. O14 states its full reach.

## 14. Not verified

- **The probe.** The ordering, the alias form, pipelines and the NUL `_id` are reasoned from OpenSearch's semantics, not run. Slice 3 decides them.
- **Deleting old generations.** That `mark_seeded` actually deletes a closed or write-blocked generation (`lane_parking`'s reindex tests seed past one but don't assert the delete). That a delete under `index.blocks.read_only` is refused.
- **Flood stage.** Where the meta index's shard sits under flood stage, so whether a full disk detaches at all.
- **The source reads.** `current_setting('max_slot_wal_keep_size')` for a non-superuser. That `ParkReason::Refused` keeps ids that parse back to root keys (`key_of` is new).
- **`flusso check`:** that it reads no sink state, inferred from its description, not read line by line.
- **The cost of O10.**

## 15. Not in this stage

- #477, which is harmless here (§5).
- A config reload.
- Detaching on errors (decision 2).
- Rebuilding a `backfill = false` sink (decision 6).
- A public detach command (O12).
- Sweeping old aliases (O15).

## 16. Changes after the design reviews

### Round 1 (revision `6df13f8d`)

| Finding | Answered in |
| --- | --- |
| A1 restart erases the record | §4: a record of its own, read before `ensure_index` |
| A2 a dropped entry answers everything | D6: kept, cut |
| A3 pre-detach items in the new generation | D9–D10, floor |
| A4 fence vouches past the old generation | D5 |
| A5 discarded items pin the slot | D5, D9, D10 |
| A6 an old request attaches early | D9 |
| A7 the stall age isn't "stuck" | D2, §10 |
| A8 smaller corrections | §1, §5, §7 |
| B1 partials pile up | D8 |
| B2 timer vs probe | §6, O8 |
| B3 a detach on return from an outage | D3, O18 |
| B4 `backfill = false` writes an unread generation | D14, O16 |
| B5 where the attach goes | D9 |
| B6 leftovers | D6, D5 |

### Round 2 (revision `cf44c7ea`)

| Finding | Answered in |
| --- | --- |
| A1 restart or stop in `rebuilding` loses changes | D9 (attach at staging, no floor), §3 (demote under `--skip-backfill`), §4 (rebuild kill walk, the stop row) |
| A2 every lane detached: `landed()` is `Idle` | D5: `Through(routed)` with no attached lane |
| A3 `Fences` keep discarded items | D6: `Fences::forget_lane`; discard before `fences.received` |
| A4 the fence answers "not following live changes" | §7 fence, §9, O14 |
| A5 the refused-row probe auto-creates the partial | §6: through the hash alias with `require_alias` (`write_serving`) |
| A6 no target: writes go to the logical name | D15; slice 1 never half-stages |
| A7 `since` isn't continuous | D2: explicit `stuck_since`, cleared only at a landed write |
| A8 `reindex`/re-ask strand a rebuilding lane; rebuilding isn't open | D1 (open plus marks), D9 (a new ticket re-points the expectation), O20 |
| A9 flags, uuid, 409s, §1 lines | D13, d2b (`uuid: null`, `retry_on_conflict`), §1 corrected |
| B1 a failed d2b leaves a `detached` record | D4: marker first, record second, delete on a later open |
| B2 unlisted record/marker pairs | §4 table, by "names `serving`"; anything else re-detaches |
| B3 attach idempotent per ticket | D9 |
| B4 the run-start grace defeats O13 | D3 (no run grace), O18 |
| B5 any park aborts a rebuild | D11, `rebuild_grace`, O17 |
| B6 a rebuild after flood stage | D16, O19 |
| B7 refused ids after a restart | D12, §6, O8 |
| B8 the clamp's end | D10 |
| C1–C6 silent decisions | O16, O20, O17, O18, O22, O21 |
| D slices | §12: slice 1 restages on a record; slice 3 is the spike; slice 4 is gated on it |
| E probe | §6 hazards and the spike |

## 17. What reading the unreached files found

- **`ingest/outstanding.rs` and `reader.rs`'s `someone_free` (#452).**
  - An index is stuck while it has items published and none acknowledged for 5 s.
  - A parked lane makes its index stuck. Once detached, its items are acknowledged and the index is free again, so it may let another held index grow exempt. This is correct: it is making progress for its other sinks, or building for none.
  - Nothing else to change.
- **`index_states.rs`.**
  - Its lane hold (`lane_full`) clears when the pending send goes through. The drain in D6 ends it.
  - "Stuck" in `/status` is read from `Outstanding`, as above. No change.
- **`ingest/live.rs`.**
  - `Runs.floor` is the newest position a builder's grant carried, so positions never go down per builder.
  - It is unrelated to the lane floor and unaffected: a detached lane changes no grant.
- **`ingest/together.rs`.** Scheduling only. No interaction.
- **`sink_engine/walked.rs`.**
  - A mark is recorded only for the current ticket's walk, and a release holds the walk.
  - With D9's restart attach, the walk mark recorded in a previous process is exactly what R5 resumes from. Pages before it were read after that process's attach.
  - A stop's unapplied items release and hold the mark, as today.
- **`writes.rs`, F9 / `substituted` / `seed_returned`.**
  - `substituted` drops a substitute whose lane is open or awaits another ticket (F8), so a detached lane's stale substitute is dropped once the substitute lane is read. It is read only while a lane awaits one, and the refused-row probe awaits its own ticket.
  - `seed_returned` on `TargetChanged` restages the lane, which cuts and re-asks under a new ticket. For a rebuilding lane already attached, that is today's unseeded restage, which is safe. Before the attach it re-points the expectation (D9).
  - `ask_rebuilds` reads the parked item's keys from the window, so a detached lane, which has no window, needs the record's ids (B7).
