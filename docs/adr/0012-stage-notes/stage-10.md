# Stage 10 design: detach a stalled index, rebuild it later (#394)

**Status:** design only, for the owner's decisions (§13). Nothing has been built.
This is the second revision, after the independent design review of PR #478; §16 maps each review finding to where it is answered.

**ADR 0012 §5:** "Capping those costs by dropping and rebuilding one index is deferred to its own issue and design."
Issue #394 holds the holes found by three review rounds and, in its comment, the owner's six decisions of 2026-11-19.

This note designs against `main` at `8a4053b6` (after stage 9's second half and #471). Where it says how `main` behaves, it names the code that was read. §14 lists what was not verified.

## 0. The owner's decisions this starts from (not reopened)

1. **Detach past a threshold, on by default, with an opt-out.**
   - flusso stops holding the slot for the stuck index.
   - It marks the index detached, and in need of a rebuild, in `/status`.
   - It raises an alert louder than the stalled-lane one.
   - Other indexes and sinks are unaffected.
2. **The threshold measures what hurts:** time stuck, or WAL held. It never counts errors.
3. **Rebuild automatically once the index accepts writes again** ("the lane's probe lands"), by a backfill into a new generation while the old one keeps serving reads.
4. **Flap protection:** a failed rebuild returns the index to detached, and the next attempt waits twice as long, up to about an hour.
5. **A setting makes the rebuild wait for an operator;** `reindex` forces one at any time.
6. **A `backfill = false` sink cannot be rebuilt by flusso:** its index stays detached and reported until an operator deals with it.

Left open, answered here: the default thresholds (O1), what `/wait` answers (O2), `/readyz` (O3), and the alert with its runbook (O4).

This note departs from decision 3 in one case. For a row the destination refuses, no probe cheaper than the rebuild exists once the lane's items are gone; §6 and O8 say so.

## 1. What exists on `main` that this builds on

### Confirmation

- **The slot follows the minimum over every lane.** `Progress::landed()` takes `over(self.marks.keys())` (`progress.rs`). Each lane answers `Before` the oldest of:
  - its own unlanded registrations (`Registered.unlanded`);
  - a build being published that it hasn't taken yet (`sent_to`);
  - its index's backlog for that sink (`pending.rs`'s `backlog_for`).

  A lane with nothing outstanding answers `Through` the greater of its `newest` (raised by every registration, skips included) and `registry.routed`. `pinned_by()` and `own_oldest()` are the same computation.
- **Marks move only through `Progress::applied`/`searchable`.** An item acknowledged unapplied raises nothing (`writes.rs:2126-2131`). `register` runs after the lane's own send returns (`commit.rs`'s `publish`), so a send blocked on a full lane registers late.
- **Item keys are per index** (`Progress::next_key`, an `AtomicU64` per index, taken in `publish_live` outside the registry lock). They are the same on every lane of the index. Snapshot batches carry no key.
- **The source never confirms backwards:** `cdc/ack.rs:223` takes the maximum (verified by the review).

### Sink engine

- **A parked lane is not drained.** `writes.rs`'s module doc says a parked lane "takes nothing more from its lane": its channel fills, and the ingest engine's send to it blocks (`publish_to`). Verified by reading; this is what the review could not reach.
- **Pages of a cut backfill are still written.** `received` (`writes.rs` ~1137) only withholds their walk mark (R5). A `SnapshotComplete` that doesn't answer is acknowledged without seeding (`grant`, `superseded`).
- **Outstanding requests.** `SinkEngine::answers` returns `true` when nothing is outstanding for the index (`sink_engine.rs:990`). Tickets come from `SinkEngine.tickets`, which outlives every run of the engine but not the process.
- **F9.** `LaneState.superseded` maps an id to the build of the substitute that landed for it. `past_superseded` and `supersede` strip older copies from items still waiting.
- **Items held for a seed** (X2): `LaneState.held_for_seed` holds them, and the run counts them in `holding` (`held_acknowledged`).
- **Health.** `LaneHealth::Parked { since: Instant, .. }` and `Restaging { since: Instant, .. }` (`health.rs`) live in memory. They outlive a run of the engine, not the process.

### Staging (`sink_engine.rs`)

- **`step` always calls `ensure_index` before `seeding`** (906-928). Its `Seeding` match ends in `_ => Unseeded` (937).
- **Staging paths:**
  - A start goes `ensure_per_index`, then `rebuild_stale_seeds` and `forget_walks` on `Continuity::Fresh` (only for `Staged::Seeded`, and for `Unseeded` with a walk), then `request_backfill`, except under `skip_backfill` or `backfill = false`.
  - `stage_while_stopping` ensures only.
  - A lane's restage and an owed reindex stage through the same `step` (`Step::Restage`, `Step::Rebuild`).

### OpenSearch sink (`sink_impl.rs`, `generations.rs`)

- **The meta doc** (`{prefix}flusso_meta`, id = hash alias) holds `active_generation`, `seeded`, `walked` (tagged with its generation; `Meta::read` drops a `walked` of another generation) and `uuid`.
- **Whole-document `PUT`s** of it (`write_index_meta`):
  - `ensure_index` before creating a generation that doesn't exist (169), and when the doc is missing;
  - `reindex`, which names `next_generation` unseeded, a no-op unless seeded (634);
  - `mark_seeded`, after the aliases swap.
- **Partial updates:** `update_walked`, `record_uuid`.
- **`ensure_index` gives a new generation aliases only when nothing serves yet** (`first`, 252). `mark_seeded` swaps the aliases and drops every other generation in one call.
- **Nothing ever reads or deletes another meta doc id.** Indexes under a previous content hash are left as they are: `grep` finds no retire path.

### Ingest

- **Backfill requests** reach the builder of their index (`dispatcher.rs`'s `forward`). They are gathered into `start_snapshot` (`snapshot.rs:77`), which loops over the deliveries (88-115), plans each index (126, a key read under the permit), and builds the `Walk` (135).
- **No key page is read before the walk's first `step`.** `reading_apart` only sets a flag (457).
- **Requests are acknowledged only in `finish_snapshot`.** A builder that fails hands its snapshot's requests back through `into_requests` (`builder.rs`, `builds.rs`, `commit.rs`).
- **`publish_live` decides a build's lanes at publish** (`commit.rs:648-690`): an item for each lane whose index the batch built, a skip registration for the rest.

### Daemon and observability

- **Stall** (`daemon/stall.rs`):
  - The age resets when the lane's oldest outstanding seq changes, and `since` starts at `Stalls::new`, so a process start resets every age.
  - WAL behind = slot lag − (reached(lane) − reached(landed)).
- **Fence** (`daemon/fence.rs`):
  - `Serving::{Pending, Writes, Backfilling, Rebuilding { through }}`.
  - `rebuilding` freezes `through` from `furthest`, which `reached()` raises from `progress.lane(lane)` on every call.
- **Readiness** (`status.rs`'s `is_ready`) reads the phases only.
- **Alerts:** `FlussoLaneStalled`, `FlussoLaneParked` and `FlussoIndexBacklog`, all `severity: page`.
- **`max_slot_wal_keep_size`** is read nowhere.

## 2. The rules

| # | Rule |
| --- | --- |
| D1 | **Detached is a lane state**, `LaneHealth::Detached`, per `(index, sink)`. The same index on another sink is untouched. |
| D2 | **Only a lane the sink engine has diagnosed can detach:** `parked` (refused or blocked) or `restaging`, continuously, past `stuck_for`, or past the grace with its WAL behind past `wal_held` (§10). An `open` lane is never eligible, whatever its age: one holding an item for a seed, one acknowledging a fan-out, a slow one. A sink that is down has no lane in those states, because its run fails before any write is decided per lane. |
| D3 | **A detach happens at a refusal.** The daemon asks (`SinkControl::Detach`). The sink engine detaches at the lane's next failed probe or failed restage, never on a timer alone. A probe that lands opens the lane and drops the ask. No detach starts in the first `grace` of a sink engine run (B3). |
| D4 | **Recorded at the sink before it takes effect.** `Sink::detach` writes a **detach record** of its own (§7), apart from the seed marker, and moves the seed marker to a new generation. The lane leaves the minimum only once that returned `Ok` in this process. A sink that can't record (unreachable, refusing the meta write, a sink with no record such as stdout) never detaches: the lane stays as it was and the ask is retried at the next failed probe. `/status` names why. |
| D5 | **The fence is frozen, then the lane leaves the minimum.** First the fence brings the lane's `furthest` up to date and serves `Detached { through }`, and `reached()` stops raising it (A4). Then `Progress::detach(lane)`, under the registry lock: the lane's registrations and steps are cleared, `register` on it is a no-op (a late send's included), and it counts in none of `landed`, `pinned_by`, `own_oldest`, `committed`, or a fence over all lanes. Its index's backlog stops counting for that sink. `landed()` may only move forward here. |
| D6 | **Everything of the lane is discarded and its memory released.** These are acknowledged unwritten and unapplied: its window items, its held items (`holding` decremented, `held_for_seed` cleared), and every item arriving later. A detached lane is always taken from, unlike a parked one, so a send blocked on it completes. These are dropped: its fresh-build ticket (a later substitute no longer matches, F8), its F9 map, its owed reindex with its retry, and the index's entry in `pending_reindex`. Its outstanding backfill entry is **kept, marked `cut`** (A2). |
| D7 | **Ingest publishes nothing to a detached lane.** `publish_live` treats it as a lane the batch built nothing for, but registers nothing: `register` is a no-op, and `settle_build` still runs, so the build stops holding that sink. An old backfill request for a detached lane isn't served for that lane (D9). An index all of whose lanes are detached is still built until slice 5 (O10). |
| D8 | **A rebuild is always a new generation,** named in the seed marker by the detach (D4), never the serving one. It starts only once the abandoned generations are gone, so at most one partial generation of the index is on disk at any time (B1). |
| D9 | **The lane attaches at its rebuild's backfill, gated by its ticket.** The sink engine notes the rebuild request's ticket in `Progress` (`expect_attach`) before publishing the request. `start_snapshot` attaches the lane (`Progress::attach`) only for that ticket. It does so in its loop over the deliveries, under the registry lock, before `plan_walk` and `Walk::new`, so before any key read. Attaching sets the lane's **key floor** (`next_key(index)` as it stands) and its **from** (`registry.routed`), raises both marks to the floor, and lets registrations count again. A request with any other ticket is not served for that lane, and is acknowledged with the snapshot's requests (A6). |
| D10 | **Below the floor, nothing is written.** On an attached lane the sink engine acknowledges unwritten every live item keyed below the floor, and every page or `SnapshotComplete` not answering the rebuild's ticket (A3, and the same hole for old pages). `Progress` ignores a registration keyed below the floor. Until its index is seeded, the lane's answer is never below `Through(from)`: every change up to `from` is in the pages, which read the database after the attach. |
| D11 | **The rebuild starts once its probe lands** (decision 3, §6), on the flap schedule. A cheap probe on the serving generation that writes no document stands for "the index accepts writes again", where one exists (a block, a closed index). For a refused row the probe is a fresh build of that row written into the serving generation while the process lives; after a restart, it is the rebuild itself. A failed rebuild (a park or restage during it) detaches again at once, attempts + 1. |
| D12 | **Flap state and the stuck clock are durable**, in the detach record (§7). A crash loop neither resets the doubling nor keeps a stuck lane from detaching (O13). |
| D13 | **`reindex` forces a rebuild** at once, whatever the schedule, the probe or `rebuild = "operator"`. |
| D14 | **A `backfill = false` sink or `--skip-backfill` never starts a rebuild.** Their lanes stay detached and reported. A `backfill = false` sink's detach doesn't move the seed marker (B4). Its `reindex` reattaches the lane live into the generation still serving reads, with the gap accepted and logged (O7). |

## 3. States and transitions

| State | Holds the slot | Written | Detach record |
| --- | --- | --- | --- |
| open | its unlanded items | yes | none, or `stuck` (the clock, O13) |
| parked (refused, blocked) | its parked item and everything behind it | probes | none, or `stuck` |
| restaging (target gone, owed reindex, left out) | its items | no | none, or `stuck` |
| **detached** | nothing | nothing; items discarded | `detached` |
| **rebuilding** | its items from the floor on, never below `Through(from)` | into the new generation | `rebuilding` |

`rebuilding` is `LaneHealth::Detached { rebuild: Some(ticket) }`: it discards until the attach, then behaves as open with D10. A park or restage during it returns it to `detached` (D11). Detaching is not a state: it is a flight at a lane barrier (nothing of the lane in flight), like a restage.

**Into `detached`.** Each source state is listed with what happens to its parts. The ask is executed at the lane's next failed probe or restage.

| From | What goes |
| --- | --- |
| parked, refused | the parked item is discarded; the fresh-build ticket is dropped; a substitute arriving later is acknowledged and dropped (F8); the F9 map is cleared |
| parked, blocked | the parked item and everything behind it are discarded |
| restaging, target gone | the restage stops being retried |
| restaging, owed reindex | `LaneState.reindex` and its retry, and the index in `pending_reindex`, are dropped: the rebuild replaces them |
| restaging, left out (#471) | the `unensured` mark is dropped |
| any of these with items held for a seed (X2) | the held items are acknowledged unapplied and `holding` is decremented; the outstanding entry is cut |
| any of these mid-backfill | the outstanding entry is cut and its pages arriving later are discarded |
| rebuilding, parked or restaging during the rebuild | detached at once, without waiting for the threshold (D11) |
| open | never |

**Out of `detached`:**

- **A probe lands and the schedule allows (§6), or `reindex`:** the rebuild starts (r1, §4). The lane becomes `rebuilding`.
- **`--skip-backfill`, `backfill = false`, or `rebuild = "operator"`** with no `reindex`: the lane stays detached.
- **On a `backfill = false` sink, `reindex`:** the record is deleted and the lane opens live into the serving generation (D14).

**Out of `rebuilding`:**

- **Its seed:** the lane is `open`.
- **A park or restage:** `detached`, attempts + 1.
- **A restart:** the rebuild continues (§4).

**Never** from detached to open without a seed, except D14.

## 4. The order of recording, and a kill at each step

### The detach record

The record is its own document in the meta index: id `detach:{hash_alias}` (OpenSearch). No existing code writes it, so `ensure_index`'s and `mark_seeded`'s `PUT`s of the seed marker can't erase it (A1).

```json
{ "state": "stuck" | "detached" | "rebuilding",
  "stuck_since": "…", "why": "blocked: index.blocks.write",
  "serving": 3, "attempts": 2, "last_attempt": "…" }
```

### Staging reads it first

`step` calls `Sink::detachment(mapping)` before `ensure_index`, for every `Step`. Its answers:

- **No record, or `stuck`:** today's staging.
- **`detached`, with the seed marker unseeded at a generation other than `serving`:** a complete detach.
  - `Staged::Detached`: no `ensure_index`, no `seeding`, no backfill.
  - `Progress::detach` runs before the ingest engine starts (the daemon stages every sink first), and the fence serves `Detached { through: (None, None) }`.
- **`detached`, with the seed marker still seeded at `serving`:** the detach died between its two writes.
  - `Sink::detach` is run again (idempotent), then the index is staged as above.
  - If that fails, the index is left restaging with why (#471's path), and holds the minimum.
- **`rebuilding`, with the seed marker unseeded:** the rebuild continues.
  - Staged as an unseeded index, so its walk resumes: `ensure_index` adopts or creates the named generation, `seeding` answers `Unseeded { walked }`, and the backfill is requested under an expected ticket (D9).
  - `Fresh` continuity forgets its walk (`forget_walks`, unchanged).
  - Under `skip_backfill` or `backfill = false` it is staged `Detached` instead, with no `ensure_index`.
- **`rebuilding`, with the seed marker seeded at a generation other than `serving`:** the rebuild finished; the record is deleted and the index staged normally.

Two more rules:

- **`rebuild_stale_seeds` never sees a detached index:** it takes only `Staged::Seeded`. `stage_while_stopping` stages a detached index `Detached`.
- **The new `Staged` variant and the `Seeding` match are matched by name,** so a variant added later can't compile into "unseeded, backfill" (the `_ =>` arm at 937 is replaced).

### Detaching lane `L` of index `I` on sink `S`

| Step | What happens | Where |
| --- | --- | --- |
| d0 | the daemon finds `L` eligible (D2) and sends `SinkControl::Detach { index }` | daemon, memory |
| d1 | `L`'s next probe or restage fails, and nothing of `L` is in flight | sink engine |
| d2a | `PUT detach:{alias}` with `state: detached`, `serving: G` (the generation the aliases serve, or the seed marker's), `attempts`, `why` | **sink, durable** |
| d2b | (not for `backfill = false`) partial update of the seed marker: `active_generation` = the next past every existing generation, `seeded: false`, `walked: null` | **sink, durable** |
| d2c | delete every generation of the alias but `G` that no alias serves: the partials (B1); best effort | sink |
| d3 | the fence serves `Detached { through }`, frozen at both levels (D5); the observer is told; `LaneHealth::Detached` | daemon and engine, memory |
| d4 | `Progress::detach(L)` (D5) | memory |
| d5 | discard (D6) | memory, channel |

**A kill after each step, then a restart.** The replay from the slot is what the new process reads.

| Killed after | The restart finds | Correct because |
| --- | --- | --- |
| d0, d1 | no record (or `stuck`) | the slot never passed `L`'s changes; `L` parks again at its first write, and its clock goes on from `stuck_since` |
| d2a | `detached`, the seed marker still seeded at `G` | staging completes the detach before `L` takes part; nothing was confirmed past `L` |
| d2b, d2c | a complete detach | `L` starts detached; nothing written to it; the rebuild reads the database |
| d3–d5 | the same | the slot may have passed `L`'s changes; the record and the seed marker force a new generation, and its pages read every row as it is |

- **A timed-out d2** (the #477 shape, verified safe by the review): if it was applied, `L` simply stays in the minimum this run, and a later restart rebuilds once more than needed. A retried detach `PUT`s the record whole, from the engine's own count, so an attempt is never counted twice.
- **A downgrade after d2b:** an older flusso ignores the record, adopts the next generation from the seed marker, backfills and seeds it. That is a correct rebuild. The newer flusso then finds `detached` with the seed marker seeded at a generation other than `serving`, and deletes the stale record.

### The rebuild of `L`

| Step | What happens | Where |
| --- | --- | --- |
| r0 | the probe landed on schedule, or `reindex`, or a restart found `rebuilding` | engine |
| r1a | `Sink::begin_rebuild`: delete every unserved generation but `serving`; if one won't go, this attempt fails (attempts + 1, no backfill read) | sink |
| r1b | the seed marker names a generation that doesn't exist yet, unseeded (already true after d2b) | **sink, durable** |
| r1c | `PUT detach:{alias}` with `state: rebuilding`, `attempts + 1`, `last_attempt` | **sink, durable** |
| r2 | `ensure_index` creates it (its `PUT` of the seed marker can't touch the record); the outstanding entry becomes ticket `T`; `Progress::expect_attach(L, T)`; fence `Rebuilding`, keeping `through`; `Request::Backfill { T }` published | sink, engine, stream |
| r3 | `start_snapshot` attaches `L` for `T` before any key read (D9) | ingest, memory |
| r4 | `SnapshotComplete { T }`: `mark_seeded` swaps the aliases, drops `G` and the rest, then `PUT`s the seed marker seeded, then **deletes the record** | **sink, durable** |

| Killed after | The restart finds | Correct because |
| --- | --- | --- |
| r0, r1a, r1b | `detached` | nothing started; the schedule resumes from `last_attempt` |
| r1c, r2, r3 | `rebuilding`, unseeded | the rebuild continues from its recorded walk; changes before the restart are in the pages, those after are live, attached at the new backfill |
| r4 before the record's delete | `rebuilding`, seeded at a generation other than `serving` | finished; the record is deleted |
| r4 | no record | done |

**A failed rebuild** (`L` parks or restages during r2–r4) runs d2–d5 again with the attempts kept. d2c deletes the abandoned partial. The outstanding entry for `T` stays, cut.

## 5. Interactions

- **The race between pages and the stream:**
  - A change marked before the attach is emitted only once visible (#343), so every page, which reads after the attach, sees it.
  - A change committed after a page's read is emitted after it, marked after the attach, and published to `L`.
  - The per-document build order (ADR §2) orders a build against a page.
  - An item keyed below the floor (built before the attach) is dropped: its change is in the pages.
  - The floor is read under the registry lock, and `publish_live` takes the key before it checks the lane (which takes the same lock). So an item keyed at or above the floor is published after `L` attached.
- **A send blocked on `L` before the detach** completes once D6 drains `L`. Its registration is a no-op (D5); its item is discarded.
- **Ingest side and budgets:**
  - A detached lane is never `held_by`.
  - Discarded items release their budget bytes with the last lane's acknowledgement, as today.
  - An index detached on `S` and healthy on `T` is built for `T` alone. Its pending set doesn't count for `S`, so it isn't exempt or stuck for #452 because of `S`, nor restarted for #456.
- **Several sinks:** the slot follows `T`. A fence naming `S`, or naming no sink, answers `detached` for `I` (O14).
- **A whole sink stuck.**
  - Unreachable: its run fails, no lane is parked or restaging, nothing detaches (D2, D4), and the slot stays pinned, as today.
  - A cluster block that also covers the meta index (`read_only_allow_delete` at flood stage, usually): D4 refuses, so a full disk never detaches.
  - Reachable, refusing writes to its data indexes only: each lane is diagnosed and detaches on its own (O5).
- **A reindex control:** while detached it forces the rebuild (D13). While rebuilding, it is the rebuild's own: ignored, logged.
- **R5 and #477:** a `walked` names its generation and `Meta::read` drops one of another generation. d2b always names a number past every existing one, so a late checkpoint of the abandoned walk is never read (A8). The invariant to keep: **d2b and r1b never reuse a generation number.**
- **A schema change while detached.** A new content hash means a new hash alias, a new seed marker and no record: the index is unseeded and backfilled into its own generation, so the detach is moot. The old alias's record and generations are left, as every old hash alias's indexes are today (O15).
- **Counters.** Quarantine and `on_error` counters are untouched; a rebuild's backfill may quarantine the same rows again. A detached lane is left out of `committed` (B6).

### The stop (extends §15.3 of the stage 9 note)

| Item \ lane | detaching (d2 in flight) | detached | rebuilding |
| --- | --- | --- | --- |
| d2 or r1 in flight | finishes (sink timeouts bound it); recorded: as detached; not: its previous row | — | — |
| live item | discarded once recorded; else its previous row | discarded | as open: written, or held per §15.3 |
| backfill page | discarded | discarded | `skip_backfill`; the record stays `rebuilding` |
| `SnapshotComplete` | cut | discarded | as open |
| a probe landing, or a rebuild due | not started once stopping | not started | — |

A detached lane holds nothing, so the drain never waits for it.

## 6. The rebuild trigger

**The probe, by the cause recorded in `why`:**

| Cause | Probe | Writes |
| --- | --- | --- |
| blocked: write block, `read_only`, closed generation, cluster block | `Sink::accepts_writes(mapping)`. OpenSearch: `PUT /{serving}/_doc/__flusso_probe?if_seq_no=0&if_primary_term=1` with `{}`. A block or a closed index is refused before the version check; an open, writable index answers `409 version_conflict` (no document) | nothing |
| refused row | the fresh build of the refused documents (stage 9 F3) written alone into the serving generation, while the process that detached it lives. After a restart the ids are gone, and the rebuild itself is the probe | the rows' current state: a deleted row's build is a delete |
| restaging: target gone, owed reindex, left out | the rebuild itself: its r1–r2 fail before reading any page if the cause is still there | nothing until r2 succeeds |

- **The serving generation is never written by a blocked-cause probe.** A refused-cause probe writes current rows into it, which only makes it more current.
- **An operator's deliberate `index.blocks.write` or close on the serving generation holds the rebuild** until it is lifted, or until the operator forces it with `reindex`. A timed rebuild would route around it, and its seed would delete that generation (B2).

**The schedule.**

- Probes run on the lane's parked backoff.
- A rebuild starts at the first probe landing on or after `last_attempt + retry_initial × 2^(attempts − 1)`, capped at `retry_max`. With `attempts = 0`, that is the first probe at least `retry_initial` after the detach.
- The record holds `attempts` and `last_attempt` (D12), so a restart keeps the doubling.
- `attempts` goes back to 0 only at a seed.
- `rebuild = "operator"`: probes still run and `/status` shows whether one landed, but only `reindex` starts a rebuild.

## 7. Port changes

**`Sink`** (defaults keep every existing sink as it is):

- `detachment(&IndexMapping) -> Result<Option<Detachment>>`: default `Ok(None)`.
- `detach(&IndexMapping, &Detachment) -> Result<Recorded>` (d2a–d2c): default `Ok(Recorded::NotKept)`, so stdout never detaches. `SinkError` gains no variant.
- `record_stuck(&IndexMapping, since) -> Result<()>` and `clear_stuck`: the clock (O13). Default no-op.
- `begin_rebuild(&IndexMapping, &Detachment) -> Result<()>` (r1a–r1c).
- `accepts_writes(&IndexMapping) -> Result<bool>`: default `Ok(false)`, so the rebuild is its own probe.
- `mark_seeded` deletes the record after its seed-marker `PUT`.
- `ensure_index`, `seeding`, `reindex` and `write_index_meta` are unchanged: the record lives outside the seed marker. The detach's own update of the seed marker is partial (`_update`), never a `PUT`.

**Other changes:**

- **`Staged::Detached`** in the engine; `step` calls `detachment` first. The `Seeding` match names every variant.
- **`Progress`:**
  - `detach(lane)`;
  - `expect_attach(lane, ticket)`;
  - `attach(lane, ticket) -> bool`;
  - `is_detached(lane)`, and `floor(lane)` for the sink engine.
  - `register` checks both the detached state and the floor.
  - `committed`, `least_of`, `pinned_by` and `own_oldest` skip detached lanes.
- **`SinkControl::Detach { index }`**, sent by the daemon's evaluation. No public endpoint in this design (O12).
- **The fence:** `Serving::Detached { through }` and `Unserved::Detached`. `reached()` doesn't raise `furthest` while detached. `rebuilding()` keeps a detached `through`.
- **`Observer`:** `on_lane_detached(sink, index, why)`, `on_lane_attached`, `on_detach_refused(sink, index, why)`.
- **`ChangeCapture::wal_limit() -> Option<u64>`:** Postgres reads `current_setting('max_slot_wal_keep_size')`, with `-1` meaning unlimited and read as `None`.

## 8. Configuration

**`[detach]` in `flusso.toml`.** It adds keys only (the format is frozen for the major). It is the machine's, as `[backoff]` is: `#[serde(skip)]`, never written into the lock. Each key has a flag and env var by the existing rule (`--detach-stuck-for-secs`, `FLUSSO_DETACH_STUCK_FOR_SECS`), with flag > env > file.

| Key | Default | Meaning |
| --- | --- | --- |
| `enabled` | `false` in slice 2, `true` from slice 3 | decision 1's opt-out |
| `stuck_for_secs` | 14400 (4 h) | time `parked` or `restaging` past which a lane detaches |
| `wal_held_mib` | half of `max_slot_wal_keep_size`, else 8192 | WAL behind past which a lane detaches once past the grace |
| `grace_secs` | 600 | the shortest diagnosis before any detach, and no detach this soon after a sink engine run starts (B3) |
| `rebuild` | `"auto"` | `"operator"` waits for `reindex` (decision 5) |
| `retry_initial_secs` | 300 | the first rebuild's earliest start after a detach |
| `retry_max_secs` | 3600 | the cap of the doubling (decision 4) |

**Per index:** `detach = false` on an `[[index]]` entry, beside `on_error`. It goes in the lock and is **written only when set**, so every existing lock stays byte-identical (`golden_lock`). That is a constraint, not a choice (O6).

## 9. Observability

- **`/status`:**
  - per lane: `health: "detached" | "rebuilding"`, `why`, `detached_since`, `serving: "frozen at the detach"`, `rebuild: { attempts, earliest, probe: "landed" | "refused: …" | "none", waiting_for: "probe" | "schedule" | "operator" | "backfill = false" | "skip_backfill" }`;
  - `detach: { asked, refused: "…" }` on a lane whose detach is asked or couldn't be recorded;
  - a top-level `detached: [{ index, sink }]`.

  A detached lane reports no stall age or WAL behind, is never `pinned_by`, and is out of `committed`.
- **`/indexes`** (private) shows the same per index.
- **`flusso check`** says nothing about it: as far as was read, it does not read a sink's state (O15).
- **`/wait` (O2):**
  - A `detached` lane vouches up to the `through` frozen at the detach. Past that it answers `503` with `reason: "detached"`, at both levels.
  - From r2 on it answers `rebuilding`, with the same `through`, until seeded.
  - After a restart while detached, `through` is unknown: `detached` for every position.
- **`/readyz` (O3):** unchanged.
- **Metrics:**
  - `flusso.lane.detached{index,sink}` (gauge, 1 while detached or rebuilding);
  - `flusso.lane.detaches{index,sink}`;
  - `flusso.lane.rebuild_attempts{index,sink,outcome}` with `outcome` = `seeded`, `parked`, `refused_to_start`;
  - `flusso.lane.detach_refused{index,sink}`.
- **Alert (O4):** `FlussoIndexDetached`, `expr: max by (index, sink) (flusso_lane_detached) > 0`, `for: 0m`, `severity: critical`.
  - It sits above the `page` of `FlussoLaneParked` and `FlussoLaneStalled`, because the slot was let go.
  - Its text: "reads of {index} on {sink} are frozen at the detach; changes and deletes since are not applied; a rebuild is needed (runbook)".
  - `FlussoLaneStalled` stops firing for the lane, since it has no age.
- **Runbook.** `docs/src/operate/stalled-lane.md` is at its size limit (339 lines). Proposed split, no pages written here:
  - `stalled-lane.md` keeps finding the lane and reading its health;
  - the per-health cures move to `operate/parked-lane.md` (refused, blocked, closed) and `operate/restaging-lane.md`;
  - a new `operate/detached-index.md` covers what detached means, what reads see, what the rebuild restores, forcing or holding it, a `backfill = false` sink, and the stale record a schema change leaves.

## 10. Thresholds: what is measured, where, and across a restart

- **The clock is the lane's diagnosis, not its oldest item.**
  - It starts when the lane parks or restages (`LaneHealth::{Parked, Restaging}.since`). It runs while the lane stays in either state, and stops when a write lands and the lane opens.
  - Holding an item for a seed, a fan-out acknowledged steadily, and a slow sink are `open`, so they never start it (A7).
  - The stage 8c stall age is unchanged and still drives `FlussoLaneStalled`.
- **Durable (O13).** Once the clock passes `grace`, the sink engine records `stuck_since` in the detach record (`state: stuck`, best effort), and clears it when the lane opens.
  - After a restart, a lane that parks again within `grace` of the run's start resumes its clock from the record, so a crash loop can't keep a stuck lane from ever detaching.
  - Without a record (the meta write refused), the clock is memory-only and restarts with the process.
- **WAL:** `stall.rs`'s per-lane WAL behind, already sampled each second from `ChangeCapture::lag`. It is meaningful here because the lane is parked or restaging.
- **Evaluated** in the daemon's sampling task, which reads the lanes' health from `Status`. It sends `SinkControl::Detach` once per lane per episode, and the sink engine executes it at the next refusal (D3).
- **Eligibility:** `enabled`, and the index not `detach = false`, and the lane `parked` or `restaging` for at least `grace`, and (clock ≥ `stuck_for` or WAL behind ≥ `wal_held`), and the sink engine run up for at least `grace`.
- **Never an acknowledging lane:** it is `open`.
- **Never an unreachable sink:** no lane of it is diagnosed, and the record can't be written.
- **A sink back from an outage:** its parked lanes probe first (D3). A lane whose cause went with the outage opens and is never detached.

## 11. Tests (red first)

### Engine (`sink_engine`, `progress`, `ingest`)

- `progress`:
  - `a_detached_lane_leaves_the_minimum_and_its_index_backlog_too`;
  - `a_registration_on_a_detached_lane_is_ignored`;
  - `an_attach_sets_the_floor_and_never_moves_landed_back`;
  - `a_registration_below_the_floor_is_ignored`;
  - `a_detached_lane_counts_in_no_committed`.
- `a_detach_is_recorded_before_the_lane_leaves_the_minimum` (a sink fake logs the order: record, fence, progress); `a_refused_detach_changes_nothing`.
- `a_detach_drains_the_lane`: a send blocked on a full parked lane completes; its item is discarded; budget, `holding` and F9 all end at zero.
- `a_detach_keeps_the_outstanding_backfill_cut`: its late `SnapshotComplete` seeds nothing, before or after the rebuild starts (A2).
- `an_item_below_the_floor_is_never_written_into_the_new_generation`: published before the detach and still on the channel at r2 (A3).
- `an_old_backfill_request_does_not_attach_the_lane` (A6), including one handed back by a failed builder.
- `the_attach_precedes_every_key_read`, with one builder and with several.
- `a_park_during_a_rebuild_detaches_again_with_the_wait_doubled`; `a_failed_rebuild_leaves_no_partial_generation`.
- `the_kill_walk`: a sink fake that dies after each of d2a–d2c and r1a–r4, and a restart over the same fake, for `Fresh` and `Resumed`.
- `staging_reads_the_record_before_ensuring`, for every `Step`, `stage_while_stopping` and `rebuild_stale_seeds`.
- `rebuild_operator_waits_and_reindex_forces`; `backfill_false_stays_detached_and_reindex_reattaches_live_into_the_serving_generation`; `skip_backfill_stays_detached`.
- The stop table of §5.

### Daemon

- `an_open_lane_never_detaches`: held for a seed through a long reindex, a fan-out with steady acknowledgements, a slow sink.
- `a_parked_lane_detaches_past_stuck_for_at_its_next_failed_probe`; `a_probe_that_lands_drops_the_ask`.
- `past_wal_held_only_after_the_grace`; `an_unreachable_sink_detaches_nothing`; `no_detach_within_the_grace_of_a_run_start`.
- `a_crash_loop_resumes_the_clock_from_the_record`.
- `fence::a_detached_lane_vouches_up_to_its_frozen_through`, then `rebuilding` with the same `through` (A4).

### Live (`lane_parking`, OpenSearch)

- `a_detached_index_is_rebuilt_into_a_new_generation_without_rows_deleted_meanwhile`.
- `a_restart_while_detached_comes_back_detached_and_keeps_its_attempts` (A1).
- `a_restart_mid_rebuild_continues_it`.
- `an_operator_block_holds_the_rebuild_until_lifted_or_reindexed` (the probe).
- `the_probe_writes_no_document` (asserts the conditional write's answers under `blocks.write`, `read_only`, closed, and none).
- `a_backfill_false_lane_stays_detached_across_a_restart_and_live_writes_reach_readers_after_reindex` (B4).

## 12. Slice plan

Each slice leaves `main` correct and releasable if nothing after it is ever built.

1. **Restart safety first: the record and staging.**
   - Adds `Sink::detachment`, OpenSearch's record, `Staged::Detached`, `step` reading the record first, and the named match.
   - Nothing writes a record yet, so behaviour is unchanged.
   - Tests: `staging_reads_the_record_before_ensuring` over a hand-written record, and the restart half of the kill walk.
2. **Detach and rebuild, reachable only from tests.**
   - Adds D4–D10 and D13–D14, `Progress::detach`/`attach`, the fence, the discard, `begin_rebuild`, `mark_seeded` deleting the record, and `[detach]` with `enabled = false`, no evaluation.
   - Tests: the engine set and the live rebuild test, driven by `SinkControl::Detach`.
   - Nothing detaches in a deployment.
3. **Thresholds and the probe-gated automatic rebuild, together.**
   - Adds D2, D3, D11, D12, the clock, `accepts_writes`, the schedule, `wal_limit`, the metrics and the alert, and `enabled = true`.
   - The trigger and the rebuild ship as one, so a deployment never detaches without the means to come back.
   - Tests: the daemon set, and the live probe and restart tests.
4. **The runbook split and the user pages** (a docs agent, before the PR is ready).
5. **Not building an index all of whose lanes are detached (O10),** if measured worth it.

## 13. Decisions for the owner

| # | Question | Options | For an operator | Recommended |
| --- | --- | --- | --- | --- |
| O1 | Default thresholds | (a) 4 h parked or restaging, or WAL past half of `max_slot_wal_keep_size` (8 GiB if unlimited) after a 10 min grace; (b) 1 h; (c) WAL only | (a) leaves four hours after the parked page; (b) frees WAL sooner with more rebuilds; (c) never detaches on a quiet database | **(a)** (unchanged) |
| O2 | `/wait` for a detached lane | (a) vouch up to the frozen `through`, then `503 detached`, then `rebuilding` from r2; (b) `rebuilding` throughout | (a) tells "nothing is kept" from "being rebuilt", and reads older than the detach still pass | **(a)** (now with a frozen `through`) |
| O3 | `/readyz` | (a) unchanged; (b) not ready while a lane is detached | (b) restarts or drops a process that serves every other index, and a restart cures nothing | **(a)** |
| O4 | The alert | `FlussoIndexDetached`, `for: 0m`, `critical` | pages at once, louder than parked or stalled | **as proposed** |
| O5 | Every lane of a reachable sink refusing writes to its data indexes | (a) each detaches on its own; (b) treat it as an outage and detach nothing | (a) frees the slot, then each index is rebuilt once its probe lands; plan maintenance with `enabled = false`; (b) the slot fills, and past `max_slot_wal_keep_size` every index of every sink is rebuilt | **(a)**; a sink that is down, or a cluster block covering the meta index, never detaches (D2, D4) |
| O6 | Where `[detach]` lives | (a) the machine's (not in the lock), with `detach = false` per index in the lock, written only when set; (b) all in the lock | (a) thresholds follow each database's disk and are tuned per environment with flags | **(a)**; "written only when set" is required either way |
| O7 | A sink with no record (stdout), and a `backfill = false` OpenSearch sink | (a) stdout never detaches; a `backfill = false` lane detaches without moving the seed marker, and `reindex` reattaches it live into the serving generation, gap logged; (b) stdout detaches in memory only | (a) keeps "recorded first" without exception, but changes the documented "`backfill = false` ignores `reindex`" for a detached lane; (b) a consumer misses changes silently across a restart | **(a)** (now names the generation, B4) |
| O8 | The rebuild trigger (decision 3) | (a) probe-gated: a no-write check on the serving generation for a block or close; for a refused row, its fresh build while the process lives, then the rebuild itself; on the doubling schedule; (b) a timer, with the rebuild as the probe | (a) holds the rebuild while an operator's block or close is in place, and costs no backfill until the cause is gone; (b) is simpler, but rebuilds around an operator's deliberate block and its seed deletes the blocked or closed generation; a refused row costs a backfill per attempt either way | **(a)**. This departs from decision 3 only for a refused row after a restart |
| O9 | Flap state | (a) in the detach record; (b) in memory | (b) means every restart rebuilds at once | **(a)** |
| O10 | An index all of whose lanes are detached | (a) stop reading its changes until a lane attaches; (b) build it for no lane | (a) saves builds and reads | **(a), in slice 5 once measured** |
| O11 | First rebuild attempt | (a) no earlier than 5 min after the detach, at a landed probe; (b) at the first landed probe | (b) usually meets the cause again within minutes | **(a)** |
| O12 | **New:** an operator `detach` command | (a) none; slices 1–2 are tested through the engine's control; (b) `POST /detach` and `flusso detach` | (b) is a new way to drop an index's changes on purpose, undone only by a full rebuild; the owner's decisions cover automatic detaching only | **(a)**; add it later if asked |
| O13 | **New:** the stuck clock across restarts | (a) recorded at the sink once past the grace (`stuck`), resumed by a lane that parks again within the grace of a start; (b) memory only | (b): a process restarting more often than `stuck_for` (OOM, eviction) never detaches, and WAL grows to `max_slot_wal_keep_size`, which loses the slot for every index | **(a)** |
| O14 | **New:** a fence naming no sink while `I` is detached on `S` only | (a) it waits for every sink and so answers `detached`; (b) it skips detached lanes and names them in the answer | (a) fails fenced reads of `I` even for a client reading from the healthy `T`, until it names `T`; (b) passes a reader of `S` that reads frozen data | **(a)**, documented: name the sink you read from |
| O15 | **New:** what is left behind and reported | (a) `flusso check` unchanged, `/status` and `/indexes` report; quarantine counters untouched; a schema change leaves the old alias's record and generations as old aliases are left today, listed in the runbook; (b) sweep old aliases' records and generations at start | (b) deletes data flusso no longer reads, which nothing does today for any old alias | **(a)** |

**Earlier recommendations that changed:** O2 now vouches up to a frozen `through`. O7 says which generation a reattached lane writes. O8 is now a decision, recommending the probe over the timer. O11 now waits for a landed probe.

## 14. Not verified

- **The conditional write as a probe:** that OpenSearch refuses `PUT _doc/<id>?if_seq_no=0&if_primary_term=1` under `index.blocks.write`, `read_only`, a closed index and a cluster block before the version check, and answers `409` with nothing written otherwise. This comes from OpenSearch's block levels, not tried; the live test `the_probe_writes_no_document` decides it.
- **Deleting a blocked generation.**
  - `lane_parking`'s reindex tests seed past a closed and a write-blocked generation, so the seed's alias call that drops them didn't fail. They don't assert that the old generation is gone.
  - Under `index.blocks.read_only` a delete is expected to be refused (the reason `read_only_allow_delete` exists); not tried. d2c and r1a treat a refused delete as best effort, and as a failed attempt.
- **Whether a cluster-wide `read_only_allow_delete` covers the meta index,** so that a full disk never detaches (D4): expected, not tried.
- **`current_setting('max_slot_wal_keep_size')`** on Postgres 13+ for a non-superuser: expected readable (the review agrees), not run. Before 13 it doesn't exist, and `wal_limit` answers `None`.
- **That `flusso check` reads no sink state:** inferred from its description, not read line by line.
- **The cost of O10:** not measured.

## 15. Not in this stage

- Closing #477 (it is already harmless here, §5).
- A config reload.
- Detaching on any signal but time and WAL (decision 2).
- Rebuilding a `backfill = false` sink (decision 6).
- A public detach command (O12).
- Sweeping a previous hash alias's record and generations (O15).

## 16. Changes after the design review

| Finding | Answered in |
| --- | --- |
| A1 restart erases the record | §4 "The detach record" and "Staging reads it first": a record of its own, read before `ensure_index`; the new rebuilding/stuck states; the kill walks redone on the real call order (`Fresh`, `Resumed`, per-index staging, `stage_while_stopping`); D14 for `backfill = false`/`--skip-backfill` |
| A2 a dropped outstanding entry answers everything | D6 (kept, cut), r2 (the rebuild's ticket replaces it before the lane stops discarding) |
| A3 pre-detach items written into the new generation | D9–D10: key floor set under the registry lock at attach; old pages and completions dropped by ticket; §5 race |
| A4 the fence vouches past the old generation | D5, §7, §9: `Serving::Detached { through }` frozen first, `reached()` not raised, `rebuilding` keeps it |
| A5 discarded items pin the slot after attach | D5 (registrations cleared, `register` a no-op), D9 (marks raised to the floor), D10 (`Through(from)` floor until seeded) |
| A6 an old request attaches early | D9: `expect_attach` ticket; others not served for the lane |
| A7 the age is not "stuck" | D2, §10: the clock is the parked/restaging diagnosis, durable (O13) |
| A8 smaller corrections | §5 (#477 invariant), §7 (no `Unsupported` variant), §1 (`max_slot_wal_keep_size`) |
| B1 partial generations pile up | D8, d2c, r1a: at most one partial, a delete that fails fails the attempt |
| B2 timer vs probe | §6, O8 |
| B3 detach on return from an outage | D3, `grace_secs`, §10 |
| B4 `backfill = false` reattach writes an unread generation | D14, d2b not run, O7 |
| B5 where the attach goes | D9, §1 (`start_snapshot` loop, before `plan_walk`/`Walk::new`; reads start at `step`) |
| B6 leftovers | D6 (`holding`, owed reindex and its retry, `pending_reindex`), D5 (`committed`) |
| C1–C5 silent decisions | O12, O8, O14, O6, O15 |
| Verify: is a parked lane drained? | §1: no; D6 drains a detached one actively |
| Verify: F9 against a detach | D6: cleared. An item a cleared entry would have stripped has an older build than the substitute, so it was published before it, before the detach, and before the attach. It is either live and keyed below the floor, or a page of a cut ticket, and D10 drops it either way |
| Verify: deleting a blocked generation | §14 |
