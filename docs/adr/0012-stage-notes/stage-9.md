# Stage 9 design: one lane fails, the sink goes on

**Status: §12 steps 1, 2, 3 and 6 built (PR #427, §13 "As built"); steps 4 (fresh builds, needs 8a) and 5 (reindex per lane) to come.**

**ADR 0012 §6:** "per-lane failure: parking with fresh builds; re-staging on `index_not_found`". Plus the budget's fifth rule ("a parked lane's item is released once its fresh build is requested") and §5's "a stalled lane pins the slot and alerts" (stage 8c). Dropping and rebuilding a stalled index stays with #394.

## 0. Where stage 7 left it

- **A failed write ends the sink's whole run.** The daemon restarts the sink engine with backoff (1 s doubling to `max_restart_backoff`).
  - The run records which lanes failed (`Blame::Lanes`). The next run puts a barrier on those lanes only: each writes its front item alone until one write on it lands.
  - `advanced()` counts only a landing on a blamed lane's front, so a healthy sibling doesn't reset the backoff of a poison item.
- **What stage 7's review deferred here:**
  - **Isolation.** A refusal (`on_error = stop`) or a missing index stops every index on the sink, not just its own (track B's notes, §13 "slots per lane": isolation at `writes = 1`, on an ordered sink, and with two or more stalled indexes needs parking).
  - **Reindex per lane.** It is sink-wide today, because `Sink::reindex` must not run beside a `write` (the OpenSearch sink's targets change under it). A pending reindex freezes every write of the sink while one index has a hung write.
  - **A lane re-staging after `index_not_found` answering `503`.** Now in place: #379's `Backfilling` state covers it once re-staging requests a backfill (`fence-rebuilding.md`).
  - **The write-block e2e** from stage 8c: a write-blocked index stalls alone, its sibling at age 0.

## 1. What fails a lane, and what fails the sink

A failure is the **lane's** when the destination refused something about one index and the other indexes are fine. It is the **sink's** when the destination itself is unreachable or refuses the client.

| Answer | Today | Stage 9 |
| --- | --- | --- |
| an item rejected, `on_error = skip` | quarantined, acknowledged | unchanged |
| an item rejected, `on_error = stop` | run ends, sink restarts | **the lane parks** (§2) |
| `index_not_found_exception` on an item | flush-wide error, sink restarts, `ensure_index` recreates it (#123) | **the lane re-stages** (§4): only that index |
| a write block: `cluster_block_exception` 403 (`index.blocks.write`, `read_only`, `read_only_allow_delete`) | item-level rejection: quarantined under `skip`, documents lost (#404) | **the lane parks as blocked** (§3): never `on_error`, nothing quarantined |
| item 403 `security_exception` (a role without write on that index) | rejection (#404 asks) | **the lane parks as blocked**: a misconfiguration of that index, not of the documents |
| item 429, item 503 | retried inside the write (#400) | unchanged |
| a disk flood-stage block (429) | retried inside the write | unchanged |
| transport down, TLS refused, auth refused (401, or a request-level 403), the whole `_bulk` refused | flush-wide, the sink restarts | **unchanged: the sink fails.** Every lane would fail alike, and restarting with backoff is the right shape |

**#404 decided: a write block is a lane stall, not a rejection.**

- **Why.** The documents are fine; the index is closed to writes for a while. Quarantining them loses data, and `stop` would end every index's run.
- **How the sink reports it.** As a new outcome on the lane's part: `Blocked { reason }`, beside `Rejected` (§9's port change). It never reaches `FailurePolicies`.
- **Engine and adapter.** The engine parks the lane and probes it (§5). The OpenSearch sink classifies `cluster_block_exception` and an item-level `security_exception` as blocked (track A's file: their change, specified in §9).

## 2. Parking

A **parked lane** is one the sink engine has stopped granting, while the run goes on.

- **What stops.** The lane's items stay delivered and unacknowledged in the channel's ledger, so a restart redelivers them as today. The lane is not offered to selection: no grant, no merge, no seed. Its in-flight writes, if any, finish and are acknowledged or parked by their own outcome.
- **What goes on.** Every other lane is granted as before, including merges among them. With `writes = 1` the sink's one slot goes to the others. On an ordered sink (stdout) the parked lane's items are simply not offered: the order across lanes is only per lane, so skipping one lane breaks nothing a reader relies on.
- **Its marks stay put.** The lane's `Progress` stays at the parked position, so `Progress::landed` and the slot stay pinned by exactly that lane. Stage 8c's `pinned_by` names it, and `stall_seconds` and `wal_behind_bytes` grow on it alone, so the alert fires for it. The lane's fences hold (positions after the parked one aren't applied); fences naming other indexes pass.
- **Backoff, per lane.** Stage 1's rule, scoped to the lane: 1 s doubling to `max_restart_backoff`. It resets only when a write **on that lane** lands (the stage-7 `advanced()` rule, now per lane). Another lane landing never resets it.
- **The post-failure barrier becomes parking.** Stage 7's next-run barrier ("a blamed lane writes its front alone until one write lands") is the shape of a probe (§5). It is replaced: a blamed lane starts the next run parked, with its backoff carried over in the run-outliving state the engine already keeps (beside `outstanding` and the `Fences`), and its first probe is due at once. A transport failure blames nobody, and every lane starts unparked, as now.
- **The run ends only for a sink-wide failure** (§1's last row), or the stop.

**State per lane** (sink engine, run-outliving, one small map):

```rust
enum LaneHealth {
    Open,
    Parked { why: Parked, since: Instant, backoff: Duration, next_probe: Instant },
    Restaging,               // §4
}
enum Parked { Refused { ids: Vec<String> }, Blocked { reason: String } }
```

## 3. A write block (#404) and other "blocked" lanes

- **Parked as `Blocked`.** The items are fine, so they are **not rebuilt**: the same item is probed again (§5) and lands once the block is lifted.
- **Its built documents stay, and keep their budget bytes.** That is honest: it is what will be written. The memory budget then paces the rest (rule 2, at most half per index, with 8a), as a slow sink does.
- **What the operator sees.** The log names the index, the sink and OpenSearch's reason (`index [x] blocked by: [FORBIDDEN/8/index write (api)]`). `/status` shows the lane `blocked`. Stage 8c's alert fires past 10 minutes.
- **Way out:** lift the block. The next probe lands and the lane opens. No restart is needed.

## 4. `index_not_found`: re-stage one lane

This replaces today's sink-wide restart (#123):

1. The lane goes to `Restaging`, no longer granted. Its in-flight writes finish.
2. At a lane barrier (nothing of that lane in flight), the engine stages that one index: `ensure_index` (which retracts the seeded marker of a generation that is gone and creates a fresh one), `is_seeded`, then a `Backfill` request for the index. #379's fence state goes to `Backfilling` (the generation is new and empty), and `/wait` answers `503 backfilling` for that index until it is seeded again.
3. **The lane's delivered items are redelivered after the snapshot request**, through the same ticket ledger. They may write before the snapshot lands, which is fine: per document, a later message on the lane is the newer state.
   - **The order needs checking with track A.** The snapshot's pages and the redelivered live items interleave on one lane. Stage 3's per-id rule and 8a's page dropping of ids waiting or in flight live are what keep a page from overwriting a newer live write. To check at implementation, with a test.
4. Back to `Open` once `ensure_index` returns. Seeding then follows the normal `SnapshotComplete` path.

**Concurrency with writes.** `ensure_index` on one index while other lanes write is new. Today the port says `reindex`/`ensure_index` never run beside a `write`.

- **To relax, per index:** "never beside a write **of the same index**". The OpenSearch sink's per-index targets are separate entries, so this is a change to the port's documented contract and a check in track A's sink that no shared state is touched.
- **Without that relaxation,** re-staging waits for every write in flight on the sink and freezes it for the duration of `ensure_index` (one round trip), as reindex does today.
- **The same relaxation makes reindex per lane** (stage 7 review 7), so both land together.

## 5. How a parked lane resumes

**Probes.** When its backoff is due, the parked lane is granted **one** write of its front item (alone, never merged): the stage-7 barrier, now driven by the timer.

- **Refused:** the lane writes its front item's **fresh build** (§6) rather than the item itself.
- **Blocked:** the item as it is.
- **The probe lands:** the lane is `Open`, its backoff resets, and the rest of its items follow.
- **It fails the same way:** the backoff doubles.
- **It fails another way:** take the new classification (a refusal that becomes a block, and so on).

**What an operator does:**

- **A block:** lift it. The next probe lands.
- **A refusal (a mapping conflict, an id too long, a script rejected by the destination):** fix the data or the schema, then:
  - **the data:** the next fresh build reads the fixed row and lands;
  - **the schema:** a config change means a new content hash, a restart and a new index, so the parked items are moot;
  - **switch the index to `on_error = skip`**, which needs a restart today: no live config reload.
- **A reindex of that index** (`flusso reindex <index> --sink <s>`) drops the parked items' relevance. The lane's queued live items are still written into the new generation. Reindex per lane (§4) makes this the general way out.
- **Nothing:** the lane stays parked and pins the slot. Past `max_slot_wal_keep_size` the slot is invalidated: the documented cost (8c's runbook) until #394.

## 6. Fresh builds (refused lanes only)

A parked item's documents go stale while the lane is parked, and hold budget. ADR §6: "each retry asks the index task for a fresh build of the parked ids for that lane only; the parked position stays outstanding, and lane items stay immutable".

**What the sink engine has.** The parked item: its envelopes' ids, its key and its oldest position. More items of the lane may queue behind it.

**The decision: rebuild the front item's ids only, not "everything the lane has not landed".**

- The items behind the front are newer builds of their own ids. They were built after the parked one, so they read newer state; what makes them stale is time, not the failure.
- Once the front lands, they are written in lane order as today. If one of them is refused, it parks the lane in turn.
- Rebuilding everything not landed would mean a replay by position range, which 8a doesn't keep per lane: pending sets are per index and drop an id once it is published.

**The interface needed from 8a (the index task).** A request kind on the index's request lane:

```rust
Request::Rebuild { index: IndexName, sink: SinkName, ids: Vec<String>, key: u64 }
```

- **Answer.** The index task builds those ids now (no admission wait beyond a free permit: it is one batch) and publishes a `LaneItem::Batch` **to that lane only**, marked as replacing `key`: `Batch.replaces: Option<u64>`.
- **Effect on the lane.** It becomes the front's substitute; the parked item is acknowledged when the substitute lands. The ledger releases its bytes then, and the budget's fifth rule releases them earlier, at the request (below).
- **No new pending entries.** The ids are not marked pending on the index (other lanes have them landed or queued): this is outside the per-index pending set by design. Its `oldest` is the parked item's, so `Progress` and fences see one item.
- **What 8a must offer:** this request kind, a single-lane publish, and the `replaces` key. The ids come from the sink: they are its envelopes' `id`s, the rendered document ids. The index task needs to turn them back into `DocumentId`s. Either 8a keeps a reverse map (ids are rendered from the `RowKey`), or the item carries its `DocumentId`s beside the envelopes; the second is simpler and costs a `Vec<DocumentId>` per item. **Open question for track B.**

**Budget (rule 5).** When the fresh build is requested, the parked item's `Hold` is released from the budget's accounting. Its memory isn't freed: the ledger keeps the `Arc` until the substitute lands, as the ADR's list of open costs says. So the fresh build can be granted even when the parked lane's bytes filled its half.

- **To build:** `Charge::forget`, which releases the bytes once and makes the eventual drop a no-op.
- **Test:** `a_parked_lane_with_a_full_budget_unparks` (ADR §8).

**A refused fresh build** parks again with doubled backoff. A deleted row is fresh-built as a tombstone, which lands, so the lane opens.

## 7. Merged writes

A merged write holds parts of several lanes.

- **A part refused under `stop`** parks only its lane; the other parts landed and are acknowledged. This is what stage 7 already does for the blame: `a_stop_rejection_in_a_merged_write_bars_only_its_own_lane`. Now the run doesn't end.
- **A part blocked** parks its lane the same way.
- **An `index_not_found` part** re-stages its lane; the other parts landed.
- **A flush-wide error** fails the sink, as now.
- **A parked lane is never merged.** A probe is alone, so a stuck lane can't tie the others' parts to its outcome. That is stage 7's "merge only among lanes with no write out", extended to parked lanes.

## 8. Interactions

- **Resume checkpoints (track A, `stage-8b-resume.md`).** A backfill's walked mark is recorded per lane from landed snapshot items.
  - **Parked during a backfill:** the mark stops at the last landed page, which is correct, and a restart resumes there.
  - **Re-staging (§4):** a new generation, so the old mark is forgotten (`forget_walked`), as a reindex does today.
  - **A fresh build of a snapshot item:** it carries no position; it replaces by `key`, and the walked mark moves when the substitute lands.
- **Ordered sinks and `writes = 1`.** Parking is what gives them isolation: stage 7's slot cap can't help with one slot, but a parked lane takes none. A probe takes the one slot for one write, then yields.
- **The stop (#371) with a parked lane.**
  - **The drain can't finish:** a parked lane's items never land, and the watermark is the minimum. So the stop's deadline bounds it, as for a stalled sink today.
  - **At the deadline:** exit `1`, and the message names the lane: `shutdown deadline of 30s passed while draining (primary: lane users parked since 12:03:11, refused: id 42 …)`.
  - **Probes during the stop:** the stop doesn't wait for the backoff. One last probe is made at once, so a block lifted meanwhile still drains.
  - **What the next start does:** it redelivers, and the lane starts parked (§2), so it goes through the same probe.
- **A reindex control while parked:** handled as §5 says. With reindex per lane, the parked lane is reindexed alone. Its parked and queued items are written into the new generation, then the snapshot seeds it.
- **Fences (#379):**
  - a parked lane is `Serving`: what it landed is readable, and later positions simply aren't applied, so a wait times out with `408` as for any stall;
  - a re-staging lane is `Backfilling`, so `503 backfilling`.
- **8a's per-index failure** (an index task restarting alone) is ingest-side and independent: a parked lane still asks its index task for fresh builds, and a restarting task answers once it is back.

## 9. Port changes

- **`sink::WriteReport` gains per-part outcomes for blocks:** `blocked: Vec<(IndexName, String)>`, the index and the reason, beside `rejected`.
  - The OpenSearch sink fills it for `cluster_block_exception` (403) and item-level `security_exception` (track A).
  - A part with any blocked item is blocked whole: its accepted items landed, and rewriting them is idempotent.
  - `index_not_found` moves from flush-wide to a per-part outcome: `missing: Vec<IndexName>`.
- **`Sink` contract:** `ensure_index`/`reindex` may run beside writes of **other** indexes (§4).
- **`stream::Request::Rebuild`** and **`Batch.replaces`** (§6).
- **`engine::Charge::forget`** (§6).

## 10. Observability

- **`/status`:** `sinks.<s>.lanes.<i>.health`, one of `open`, `parked`, `blocked` or `restaging`, with `parked_since`, `next_probe` and `reason`.
- **Metrics:**
  - `flusso.lane.parked{index,sink,why}` (gauge, 1 while parked);
  - `flusso.lane.probes{index,sink,outcome}` (counter).
- **Stage 8c's alert** fires for a parked lane through its stall age, and needs nothing new. The runbook ("A lane is stalled") gains the parked cases and their ways out (§5).
- **Logs:**
  - `warn!` at park, with the index, sink, reason, the ids for a refusal, and the backoff;
  - `info!` at each probe after the first;
  - `info!` at unpark, with how long it lasted.

## 11. Tests (red first)

- **The sink engine (mocks):**
  - a refusal parks only its lane, and the sibling lane writes and lands;
  - the run doesn't end;
  - probes with backoff on paused time;
  - backoff per lane, never reset by a sibling;
  - a probe of a refused lane is a fresh build;
  - a blocked lane probes the same item;
  - a lifted block opens the lane;
  - `index_not_found` re-stages one index (`ensure_index` for it alone, a `Backfill` request, redelivery);
  - a merged write with one refused part;
  - `writes = 1` and an ordered sink keep writing other lanes;
  - a stop with a parked lane: deadline, exit, message;
  - the seeded schedule (track B's) with random parks.
- **Budget:** `a_parked_lane_with_a_full_budget_unparks`, and `Charge::forget` releases exactly once.
- **The OpenSearch sink (track A), live:**
  - `index.blocks.write` set gives a lane `Blocked` and nothing quarantined under `skip`;
  - an item-level `security_exception`;
  - a deleted generation mid-run is per lane.
- **e2e:**
  - stage 8c's write-block case: the sibling at age 0, `pinned_by` names the blocked lane, and lifting the block drains it;
  - `pipeline`'s `generation_deleted_while_running_*` now per lane;
  - `shutdown` with a parked lane: exit 1, the lane named.
- **The fence:** a re-staging lane answers `503 backfilling` (#379's state, reached through §4).

## 12. Order of work, once 8a lands

1. Port: per-part outcomes, `Charge::forget`, the relaxed `ensure_index` contract (with track A).
2. Parking for refusals and blocks with same-item probes. The barrier becomes parking.
3. Re-staging on `index_not_found`.
4. Fresh builds (needs 8a's `Request::Rebuild`).
5. Reindex per lane.
6. Status, metrics and the runbook.

## Open points for the go

1. **Fresh builds of the front item only** (§6). Say if you want "everything not landed", which needs a replay design.
2. **The item carries its `DocumentId`s** for fresh builds (§6), or does 8a keep a reverse map? That's track B's call.
3. **The port relaxation** (`ensure_index`/`reindex` beside writes of other indexes) with track A, or keep re-staging freezing the sink for one round trip.
4. **A request-level 403** (the whole `_bulk`) stays sink-wide (auth). An item-level 403 on one index parks that lane. Agreed?

## 13. As built (steps 1, 2, 3 and 6)

What is in, and where it differs from the design above.

- **Port (§9).** `WriteReport::blocked` and `WriteReport::missing` are `Vec<IndexRefusal { index: IndexName, reason }>`, the logical index with the destination's reason (`missing` carries a reason too, for the log and `/status`). `Sink::write_parts`' default folds them. The OpenSearch sink classifies by `bulk::is_blocked` (`cluster_block_exception` at any status but busy, which is asked first, so the 429 flood-stage block stays busy; an item-level 403 `security_exception`) and `bulk::is_missing`, and maps each physical target back to its logical index from the write's own envelopes, not from the targets map; an index it can't place fails the write. A request-level 403 stays write-wide (the owner's call, open point 4). The staging contract is relaxed (open point 3): `ensure_index`/`reindex` may run beside writes of other indexes; the OpenSearch sink touches only that index's meta doc, aliases, generations and its own entry of the targets map, and a caught-up refresh of a deleted generation is `ignore_unavailable`.
- **Parking (§2, §5).** A held part goes back in the window at its receive order (`put_back`), still holding its delivery handle, rather than being dropped for the ledger to redeliver: redelivery would number it after items already taken, and a later build of one of its documents could then be written before it. The lane's state is `health.rs`'s `LaneState` (health, backoff, due, probes) in a map on `SinkEngine` (as designed, beside `outstanding` and the `Fences`); a lane absent from it is open.
  - **A refused lane probes the same item**, not a fresh build: §6 needs 8a's `Request::Rebuild`. Fixing the row therefore doesn't unpark the lane; the runbook says so (switch to `skip`, or fix the schema).
  - **The stage-7 barrier stays for sink-wide failures.** §2 says the barrier "becomes parking". As built, a run that fails as a whole (a write `Err`, a seed, a lane delivery or acknowledgement) still blames its lanes and the next run writes their fronts alone, as before: parking a lane for a transport error would show it `parked` in `/status` and on `flusso.lane.parked` for a failure that isn't the lane's, and a fresh engine would show every lane parked until its first write. The barrier is exactly a probe due at once, so the behaviour is the one §2 asks for; what changed is that a refusal, a block or a missing target no longer ends the run at all, so nothing of theirs reaches the barrier. A parked lane stays parked across a failed run, on its own schedule (not probed at once on the restart, since the backoff is the lane's).
  - **A parked lane takes nothing more from its lane** (it behaves as a barrier lane in `lane_wants_more`), so its items wait in the channel, not in the window.
  - **Merge wait.** A parked lane's items neither hold nor end a hold, and a due probe is never held (`merge_hold`); with every lane open the old path runs unchanged (`unhealthy == 0`).
  - **The probe's flight** is marked (`Flight.probe`), so a write of the lane that was already in flight when it parked moves nothing: landing doesn't open it, a second refusal doesn't double the backoff.
  - **Logs** as §10: `warn!` at park, `info!` at each probe after the first and at unpark (with how long). 
- **Restaging (§4).** `SinkEngine::restage` runs as a flight holding no write slot once nothing of the lane is in flight: `ensure_index` + `seeding`, then a `Backfill` request when unseeded, not outstanding, and backfill is on. A failed staging is retried on the lane's backoff; `stage()` (the daemon's restart path) reopens every restaging lane, having ensured every index. The order question of §4.3 needs no new rule: the held items stay in the window ahead of anything the snapshot brings later, so per-document admission keeps the order. **An outstanding backfill is not requested again** (as on `main` after a restart): a generation deleted mid-backfill loses the pages already written (sink reference, "Seeding", says so; out of scope here).
- **Stop (§8).** `probe_on_stop` sets every parked lane due at once when the stop is first seen (the loop watches the stop while a lane is unhealthy). The drain then waits for the CLI's deadline; `unfinished_sinks` names each lane that isn't open, with since and why.
- **Observability (§10).** `/status` `sinks.<s>.lanes.<i>.health` (`open`/`parked`/`blocked`/`restaging`), `parked_since`, `next_probe`, `reason` (from `Observer::on_lane_health`, instants turned to wall clock when reported). `flusso.lane.parked{index,sink,why}` (an observable gauge over `Status::unhealthy_lanes`; `why` `refused` or `blocked`, no series while restaging), `flusso.lane.probes{index,sink,outcome}` (`landed`, `refused`, `blocked`, `missing`, `failed`). The daemon passes `max_restart_backoff` as the probe backoff's ceiling.
- **Breaking.** `EngineError::DocumentsRejected` is removed (nothing ends a run on a refusal); `WriteReport` gains fields (it is `#[non_exhaustive]`); `Observer` gains two defaulted methods.
- **Not built:** §6 (fresh builds, `Charge::forget`, rule 5), §12 step 5 (reindex per lane), the per-index memory share (8a): a lane parked long enough fills the budget and paces every index, as a stalled sink does today. The live `shutdown` e2e with a parked lane and the seeded schedule with random parks (§11) are not added; the stop rules are covered by `tests::parking` on paused time.

Guards: `tests::parking`, `sink_engine::health::tests`, the reworked `tests::{concurrent,merge,wait,refresh}` stop-rejection cases, the daemon's `status_reports_each_lanes_health` and `a_stop_rejection_halts_its_lane_writing_several_items_at_once`, the CLI's metric and deadline-message tests, the sink's `bulk::tests`, `reindex`'s `generation_deleted_while_running_is_reported_missing_and_is_rebuilt`, and the `lane_parking` live e2e.
