# Stage 9 design: one lane fails, the sink goes on

**Status: §12 steps 1, 2, 3 and 6 built (PR #427, §13 "As built"). The second half (fresh builds, reindex per lane, a closed index, an auto-created impostor) is designed against `main` after 8a in §14; slice 1 (fresh builds) is built on `feat/lane-failure-2`.**

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
- **After review.** A lane opened by a staging keeps its backoff until one of its writes lands (`LaneState.unproven`): a target `ensure_index` can't cure (an ingest pipeline routing to an index auto-create may not make) otherwise looped staging and writing with no wait. `FlussoLaneParked` (`flusso_lane_parked > 0` for 5 m, `why` `refused`, `blocked` or `restaging`) pages for a parked lane, including a refused backfill page, which has no position and so never ages into `FlussoLaneStalled`; `/readyz` stays ready while a lane is parked. An ordered sink keeps order per lane, which is what the port promises (parking every lane of an ordered sink would let one index's block stall them all; stdout never reports a refusal, so it never parks).
- **Stop (§8).** `probe_on_stop` sets every parked lane due at once when the stop is first seen (the loop watches the stop while a lane is unhealthy). The drain then waits for the CLI's deadline; `unfinished_sinks` names each lane that isn't open, with since and why.
- **Observability (§10).** `/status` `sinks.<s>.lanes.<i>.health` (`open`/`parked`/`blocked`/`restaging`), `parked_since`, `next_probe`, `reason` (from `Observer::on_lane_health`, instants turned to wall clock when reported). `flusso.lane.parked{index,sink,why}` (an observable gauge over `Status::unhealthy_lanes`; `why` `refused` or `blocked`, no series while restaging), `flusso.lane.probes{index,sink,outcome}` (`landed`, `refused`, `blocked`, `missing`, `failed`). The daemon passes `max_restart_backoff` as the probe backoff's ceiling.
- **Breaking.** `EngineError::DocumentsRejected` is removed (nothing ends a run on a refusal); `WriteReport` gains fields (it is `#[non_exhaustive]`); `Observer` gains two defaulted methods.
- **Not built:** §6 (fresh builds, `Charge::forget`, rule 5), §12 step 5 (reindex per lane), the per-index memory share (8a): a lane parked long enough fills the budget and paces every index, as a stalled sink does today. The live `shutdown` e2e with a parked lane and the seeded schedule with random parks (§11) are not added; the stop rules are covered by `tests::parking` on paused time.

Guards: `tests::parking`, `sink_engine::health::tests`, the reworked `tests::{concurrent,merge,wait,refresh}` stop-rejection cases, the daemon's `status_reports_each_lanes_health` and `a_stop_rejection_halts_its_lane_writing_several_items_at_once`, the CLI's metric and deadline-message tests, the sink's `bulk::tests`, `reindex`'s `generation_deleted_while_running_is_reported_missing_and_is_rebuilt`, and the `lane_parking` live e2e.


## 14. Second half, designed against `main` (after 8a, `90073906`)

What 8a built changes three things the first half assumed:

- **One builder per index**, with its own inbox fed by the dispatcher, its own slots (published in grant
  order), the shared gate (`builds` permits) and the shared budget (rules 1–5 of `budget.rs`; note the
  budget's own rule 5 is "an index holding no live bytes gets one live batch", not the ADR's "parked item
  released").
- **A lane is a bounded FIFO.** A parked lane's front item heads it; everything the index published since
  queues behind it, up to the lane's bound, and the builder holds (`held_by: lane`) once it is full.
- **Pending sets drop an id once it is published**, so nothing on the ingest side remembers which ids an
  item carried.

### 14.1 Fresh builds (§6), re-derived

**The path a fresh build takes.** It cannot ride the parked lane: it would queue behind the very item it
replaces, past the window's lookahead, and a full lane would block the builder that publishes it (the
lane is full *because* it is parked). So substitutes get their own path:

- **`Stream::substitutes(sink)`** (port): one small lane per sink, downward, unbounded in practice (at
  most one substitute per lane of the sink is ever outstanding), with the usual delivery/ack/redelivery.
  Its items are `stream::Substitute { lane: LaneKey, ticket: Ticket, batch: Batch }`. The default
  implementation answers "no substitute lane" (`QueueError::Unsupported`); a sink engine over such a stream
  probes the same item, as today. The channel adapter implements it.
- **`Request::Rebuild { index, sink, keys: Vec<RowKey>, ticket }`** (port): up the request lane, routed by
  the dispatcher to the index's builder inbox like a `Backfill`.

**The keys (open point 2, decided: the item carries its build's ids; §15.0 as built).** An envelope's `id` is
the key rendered as text, composite parts joined with `:`; that is not reversible (a text part may hold `:`),
and a reverse map would have to remember every unacknowledged document. So each lane item holds, by refcount,
the id vector its build was handed (`Batch.ids: Option<Arc<Vec<DocumentId>>>`, `#[serde(skip)]`), and the
sink engine finds a parked item's keys in it by rendered id when it asks for a fresh build. `DocumentId` and
`RowKey` move from the `source` port to the kernel (kernel types only; `source` re-exports both), so the
stream port can name them.

**Build numbers.** `Batch.build: Option<u64>`: the index's build number, from one counter per index that
outlives builder restarts and engine runs (on `IngestEngine`), taken at publish. Slots publish in grant
order, and two builds of one document never overlap (admission), so for one document a larger number is a
later build reading newer state. Live, backfill and fresh builds share the counter.

**The rules** (each pinned by the test named; `T:` = engine test, `S:` = sink-engine test):

| # | Rule | Test |
| --- | --- | --- |
| F1 | A lane parked on a **refusal** whose front item carries keys asks, when its probe is due, for a fresh build (`Rebuild` with the front item's keys and a new ticket) instead of writing the item; at most one is outstanding per lane | S `a_fixed_row_unparks_its_lane` |
| F2 | A **blocked** lane, and a refused lane whose item carries no keys or whose stream has no substitute lane, probes the same item (first half) | S `a_lifted_block_opens_the_lane_and_a_block_loses_nothing_under_either_policy`, `a_lane_without_substitutes_probes_the_same_item` |
| F3 | The builder serves a `Rebuild` as one build of those keys, granted by the gate like a live batch and admitted only when no running build holds one of its ids; it publishes the result to the substitute lane of that sink only, and acknowledges the request once published | T `a_rebuild_is_built_once_and_sent_to_its_sink_only` |
| F4 | No pending entry, no `Progress` registration, no `Outstanding` record: the parked item stays the one item its position and fences wait on | T `a_rebuild_is_built_once_and_sent_to_its_sink_only` (nothing reaches a lane) |
| F5 | The substitute is written alone as the lane's probe in the parked item's place: landed, the parked item is acknowledged (its key applied, its walk mark recorded, as if it had landed) together with the substitute, and the lane opens | S `a_fixed_row_unparks_its_lane` |
| F6 | A refused substitute parks again with doubled backoff; the parked item stays as it was; the substitute is acknowledged and dropped | S `a_fixed_row_unparks_its_lane` |
| F7 | A deleted row is fresh-built as a delete, which lands | S `a_deleted_row_unparks_its_lane_with_a_delete` |
| F8 | A substitute naming another ticket than the lane awaits (an older one, a lane opened meanwhile) is acknowledged and dropped | S `a_stale_or_unanswered_fresh_build_falls_back_to_the_item` |
| F9 | **Superseded builds.** Once a substitute with build number `b` lands, every item of that lane with a build number below `b` is written without the substitute's ids (they would write an older build over a newer one). The record lives on the lane's state (it outlives a run) until an item numbered past `b` was received | S `an_older_build_queued_behind_the_parked_item_is_not_written_over_its_substitute` and 14.9's; random schedules |
| F10 | A rebuild for an index whose builder is down waits in its inbox and is served after the restart (8a keeps the inbox) | T `a_failing_rebuild_restarts_its_index_and_is_served_after` |
| F11 | A rebuild whose build fails (an error, a panic, a `Failed` document under `stop`) is the **index's** failure, as a live build's is: the builder restarts alone (or the run, with one index); the request, unacknowledged, is served again | T `a_failing_rebuild_restarts_its_index_and_is_served_after` |
| F12 | A refused backfill item (no position) is replaced the same way: its walk mark is recorded when the substitute lands | S `a_fixed_row_unparks_its_lane` (its backfill case) |
| F13 | At a stop, a refused lane's last probe writes its item as it is: builders don't serve rebuilds once the stop is requested (they are left on the request lane) | S `a_stop_probes_a_refused_lane_with_its_own_item` |
| F14 | The budget counts a fresh build like a live batch of its index but never refuses it: it is one item per parked lane, and the item it replaces can't be released before it lands | T `a_rebuild_is_served_with_the_budget_held_full` |

**Rule 5 of the ADR (`Charge::forget`) is not built.** It would release the parked item's bytes from the
accounting while the ledger still holds them, so the budget, the exemption and `Outstanding`'s record would
all read less than is held. F14 gets the same outcome (the fresh build is never waited on the parked item's
bytes) and keeps the count honest: the bound grows by one fresh build per parked lane, the size of the item
it replaces.

**Order, case by case.**

- *Items queued behind the parked one* (§6's decision stands: the front item's ids only). They are older
  or newer builds of their own ids; F9 drops exactly the older builds of the substitute's ids, so a row
  updated several times while broken unparks once, and no document goes back to an older build.
- *A live build of the same id granted meanwhile.* Admission keeps the two builds apart. Granted before
  the rebuild, it is older, queued behind the parked item, and F9 drops it there; granted after, it is
  newer and written after the substitute in lane order.
- *A backfill page holding the id.* The same, by build number.
- *Other sinks.* The substitute goes to one lane; the other sinks' lanes see nothing.

**Per-index restart (8a).** F10, F11. A builder restarting drops its fresh builds in flight with its other
builds (8a's `abandon`); their requests stay unacknowledged in its inbox, so they are served after it.

### 14.2 Reindex per index and lane (§12 step 5)

Today (`main`): a reindex is staged once every write of the sink returned (`take_pending_reindex` waits for
`flights.is_empty()`), then `reindex` + `ensure_index` + a `Backfill` request. A parked lane stays parked on
its item, which its next probe writes into the new generation; a restaging lane is staged by both.

| # | Rule | Test |
| --- | --- | --- |
| X1 | A reindex of one index on one sink waits only for that lane's writes, not the sink's (the port already allows staging beside other indexes' writes, §13) | S `a_reindex_waits_only_for_its_own_lanes_writes` |
| X2 | A parked lane's reindex acknowledges its parked item unwritten (the snapshot it requests reads every row after the item's changes), opens the lane, and its queued items are written into the new generation | S `a_reindex_unparks_a_refused_lane`; e2e `lane_parking` |
| X3 | A blocked lane's reindex is the same: the new generation carries no block | S, e2e |
| X4 | A restaging lane's reindex replaces its staging | S |
| X5 | `POST /indexes/<name>/reindex?sink=` answers as today; `/status` shows the lane `open` once staged | CLI |

### 14.3 A closed index is `blocked` (§1)

OpenSearch answers a write to a closed index item by item with `index_closed_exception` (400). Today that is
a rejection: `skip` quarantines the documents, `stop` parks the lane as `refused`. Rule C1:
`bulk::is_blocked` takes `index_closed_exception` (at any status) as a block, so the lane parks `blocked`
and the same item lands once the index is opened. Live e2e `a_closed_index_blocks_its_lane_until_opened`
(close the generation, write, `/status` `blocked`, nothing quarantined under `skip`, open, lands).

### 14.4 An auto-created impostor

What still gets through, to be established by test before any change (`reindex` e2e):

- Seeded writes go through the hash alias with `require_alias`: a deleted generation is `index_not_found`
  → `missing` → restage. Safe.
- **Unseeded generations are written by name.** Deleted mid-backfill, the next write auto-creates an index
  of that name (dynamic mapping, no alias, default settings) and the lane goes on writing into it.
- `ensure_index` finding the name taken: to check whether it adopts what it finds.

Rules (proposed): I1, every write goes through an alias with `require_alias` (an unseeded generation gets
its own write alias, created with it); I2, `ensure_index` refuses an index of a generation's name that
doesn't carry flusso's marker (mapping `_meta`) and the lane stays `restaging` with the reason naming it;
I3, the impostor is **left alone** and named in the log and `/status` (recommended: it holds whatever was
written into it, and deleting what flusso can't prove it created is not flusso's to do; the operator
deletes it, the next staging goes on).

### 14.5 What each state looks like

| State | `/status` lane `health` | `reason` | metrics |
| --- | --- | --- | --- |
| parked on a refusal, fresh build asked | `refused` | the refusal, plus `rebuilding: true` while a fresh build is outstanding | `flusso_lane_parked{why="refused"}` 1; `flusso_lane_probes{outcome}`: a fresh build's outcome as any probe's |
| substitute landed | `open` | — | parked 0 |
| blocked (incl. closed) | `blocked` | OpenSearch's reason | `why="blocked"` |
| impostor | `restaging` | names the impostor index | — (as restaging) |

`ingest.indexes.<name>`: a fresh build is a build of the index (`working`, `held_by` unchanged); a failing
one is the index's live failure (`state: down`, `last_error.live`, `restarts.live`).

### 14.6 Slice plan

1. **Fresh builds** (14.1): port (`RowKey` to the kernel, `Batch.keys` (now `Batch.ids`, §15.0), `Batch.build`,
   `Request::Rebuild`, `Stream::substitutes`, `Substitute`), channel adapter, ingest (dispatcher, builder,
   publish), sink engine (ask, substitute probe, F9). Also the seeded schedule with random refusals and
   parks, and the live `shutdown` e2e with a parked lane (§11's two not added).
2. **Reindex per lane** (14.2).
3. **A closed index** (14.3), small: may ride with 2.
4. **The impostor** (14.4): the test first, then I1–I3.

Each is a PR correct alone. Slice 1 adds no work to the paths `main` measures: keys are moved, not cloned;
the build number is one atomic add per published item; the substitute lane is polled only while a lane
awaits a substitute.

### 14.7 Decisions for the owner (each built as recommended, behind one predicate)

| # | Decision | Recommended | Other way | Predicate |
| --- | --- | --- | --- | --- |
| O1 | A fresh build that fails on the ingest side | the index's failure (restart, `/status` down), as the same row's live build fails the same way | the lane's: park again, the index untouched (needs a failure answer on the substitute lane) | — (`builds.rs`'s `land`: `Job::Rebuild` halts as `Job::Live`) |
| O2 | Older builds of a substitute's ids queued behind it | dropped (F9): one unpark, no document going back | written in order: each refused again parks again | `sink_engine::substitute::supersedes` |
| O3 | A stop's last probe of a refused lane | the item as it is | a fresh build (needs builders serving rebuilds while stopping) | — (F13) |
| O4 | A reindex of a parked lane (slice 2) | acknowledges the parked item unwritten | writes it into the new generation | slice 2 |
| O5 | An impostor (slice 4) | left alone, named, lane restaging | deleted and replaced | slice 4 |
| O6 | Rule 5 | F14 (never refused, not counted) | the ADR's `Charge::forget` | — |

### 14.8 Slice 1 as built (`feat/lane-failure-2`)

What differs from 14.1, and why:

- **F14: not counted.** The fresh build asks the gate with no budget part, which the gate admits without
  touching the budget: it is never refused and never counted. One per parked lane, the size of the item it
  replaces, and gone once it lands or is refused. Counting it would need a budget reservation that can't be
  refused, which the budget has no path for.
- **F15 (new): a fresh build is waited for at most the lane's backoff and `SUBSTITUTE_WAIT` (5 s)**; then the
  item itself is the probe (today's behaviour), and the next probe asks again; a substitute arriving after
  that is stale (F8). Found by `ingest::tests::builders`: an index's builder blocked publishing to its own
  parked, full lane can't serve its inbox, so the substitute never comes. In production the lane's bound is
  the 2^20-item backstop and the budget binds first (a builder held by the budget keeps serving), so this is
  the test-sized case; the fallback keeps the engine from waiting on it. Cost: a fresh build slower than the
  wait is wasted and the lane probes its item.
- **F16 (new): the request is offered, never waited for** (`now_or_never`): a request lane nobody reads (the
  ingest engine stopping, or not draining) would otherwise stall every lane of the sink. Not taken, the probe
  writes the item. Found by the same test (its sink engine isn't told about the stop).
- **One gate ask per pass.** The builder offers a fresh build inside `Builds::choose`, as the live part of the
  one ask, ahead of the live candidate. A separate ask joined and left the live line on every pass, and each
  change of the lines woke the builder to ask again: a spin a random schedule found (seed 58, `builds = 1`).
- **`/status` unchanged.** A lane waiting for its substitute shows `refused` with `next_probe` at the end of
  the wait; no `rebuilding` field (14.5's row is not built).
- **O1 has no predicate**: the other way needs a failure answer on the substitute lane.

**Tests.** `tests::parking`: `a_fixed_row_unparks_its_lane` (F1, F5, F6, F12, live and backfill),
`a_deleted_row_unparks_its_lane_with_a_delete` (F7), `a_stale_or_unanswered_fresh_build_falls_back_to_the_item`
(F8, F15), `an_older_build_queued_behind_the_parked_item_is_not_written_over_its_substitute` (F9),
`a_stop_probes_a_refused_lane_with_its_own_item` (F13); the existing parking tests (items without keys) pin
F2. `ingest::tests::rebuilds`: `a_rebuild_is_built_once_and_sent_to_its_sink_only` (F3, F4, F7's ingest half),
`every_item_carries_its_keys_and_a_growing_build_number`, `a_failing_rebuild_restarts_its_index_and_is_served_after`
(F10, F11). `ingest::tests::builders` (F15, F16). The random schedules (refused `orders` stretches over two
indexes) now go through fresh builds end to end and assert some were published; their per-seed checks (no
version going down, the documents equal the rows) cover F9 against the real builder.

**Not built in slice 1:** the live `shutdown` e2e with a parked lane (the stop rules are pinned on paused time).
F14's test and the live e2e came in fix round 1 (14.9).

**Mutations** (each compiles, none hangs):

| Mutation | Killed by |
| --- | --- |
| F1: no fresh build is ever asked | `a_fixed_row_unparks_its_lane`, `a_deleted_row_*`, `a_stale_or_unanswered_*`, `a_stop_probes_a_refused_lane_*`, `an_older_build_*` |
| F3: a `Rebuild` routed to the backfill inbox | `rebuilds::a_rebuild_is_built_once_*`, `rebuilds::a_failing_rebuild_*`, four `builders` tests |
| F3: admission ignored (a fresh build beside a running build of its documents) | both random schedules |
| F5: the item written in place of its substitute | `a_deleted_row_unparks_its_lane_with_a_delete` |
| F8: a substitute for another ticket accepted | `a_stale_or_unanswered_fresh_build_falls_back_to_the_item` |
| F9: older builds written whole | `an_older_build_queued_behind_the_parked_item_*`, both random schedules |
| F13: fresh builds asked while stopping | `a_stop_probes_a_refused_lane_with_its_own_item` |
| F15: no fallback (a day's wait) | `a_stale_or_unanswered_*`, eight `builders` tests |
| F16: the request awaited | `builders::a_stop_with_an_exempt_index_still_stuck_does_not_end` |
| a delete's key left out | `rebuilds::a_rebuild_is_built_once_and_sent_to_its_sink_only` |
| no build number on a live item | `rebuilds::every_item_carries_its_keys_and_a_growing_build_number` |

Not mutated: F11's halting (a failed fresh build fails its builder through `publish_slot` whether or not it
halts further grants, so no mutation of that line changes an outcome), F14 (no test, above).

### 14.9 Fix round 1 (review of #458 at `67921532`): F9 redesigned

F9 as built kept one (build, ids) record per lane, replaced by each landed substitute and dropped as soon as
an item numbered past it was **received**. Both are wrong: a second substitute erased what the first one
superseded (B1), and an older item received before the newer one can still come back whole when a run fails
and the lane redelivers it (B2). Either way an older build of a document was written over a newer one.

**F9 (replaces 14.1's).** Per lane, a map from document id to the highest build number of a substitute that
landed with that document. Merged into (the maximum per id), never replaced. An item numbered `b` is written
without every document whose entry is above `b`. An entry `s` is dropped only once no item numbered below `s`
can be delivered again: the lane delivers in build order, so that holds once the run has received an item
numbered at least `s` and every item of the lane it received numbered below `s` is acknowledged (the smallest
build number still unacknowledged, or, with none, one past the largest received, is at least `s`).

- **Where it lives:** on the lane's `LaneState`, which the `SinkEngine` keeps across its runs. The daemon
  restarts a failed sink engine by running the same `SinkEngine` again (`supervise.rs`), so the map outlives
  every run a redelivery can follow; a process restart loses the in-process lanes with it. A restarted run
  knows nothing it received before, so it keeps every entry until its own deliveries prove one can go: never
  early. Pinned by `review_a_redelivered_older_item_is_still_written_without_the_substitutes_documents` (a run
  failing after the substitute) and a daemon-path test restarting the sink engine.
- **Memory bound:** one entry per document a substitute landed with, kept until the lane's items built before
  that substitute are acknowledged. Each parked item is fresh-built at most once per probe, so the map holds at
  most the documents of the parked items fresh-built while older items of the lane were still unacknowledged:
  in practice the refused items' documents, a few ids each.
- **A substitute refused for some of its documents** (found by the extended schedules, seed 2): its other
  documents landed, at its build. They join the map too, so the parked item, older, is written without them
  and the next fresh build asks only for what is left. Without it the item's own probe (F15's fallback) wrote
  its older builds over them.
- **Cost while no substitute landed:** none per item; a run-wide count of lanes with entries is checked per
  received item (one integer compare), and the prune runs only on a lane with entries.

**The rest of fix round 1.**

| Item | Change | Tests |
| --- | --- | --- |
| B3: the in-process gate (+13 to +16% on `live_drain_held`) | the sink engine's open-lane path is `main`'s again: a fresh build's handle rides on its probe's `Flight` (not on every `Part`), the substitute lane is polled inside the lane-wake branch (no extra branch future), and a received item is looked at for F9 only while some lane has entries (a run-wide count). Ingest: a build of one index (every build since 8a) publishes its envelopes and keys as they are, no regrouping (which on `main` also cloned each envelope's index name) | the gate |
| B4: the live e2e | `lane_parking`'s `a_row_opensearch_refuses_unparks_its_lane_once_fixed_or_deleted`: a `date` field over a text column holding no date parks the lane (`stop`); an `UPDATE` unparks it at the next probe, no restart; a second row, deleted, unparks it with the delete; the sibling stays current. Red with fresh builds turned off | live |
| 5: F14 | adopted | `a_rebuild_is_served_with_the_budget_held_full` |
| 6: memory nobody counts | stated, not counted: (a) the keys, one `RowKey` per document (a column name and the value, tens of bytes beside a document of KiBs), now live until every lane acknowledged the item instead of being freed at build; (b) `without` copies the envelopes it keeps of an item older than a landed substitute, while the ledger holds the original until the item is acknowledged: only items of a lane with F9 entries that hold one of those documents, each once; (c) a fresh build's documents, outside the budget (F14): one per lane, see 7 | — |
| 7: stale fresh builds | at most one fresh build waits per lane in a builder (`Builds::ask_rebuild`: a newer ticket replaces the waiting one, the replaced request is acknowledged), so a builder blocked for a while builds one, not one per timed-out ask; the substitute lane then holds at most one late answer per lane, read and dropped the next time the lane waits for one | `a_newer_request_for_a_lane_replaces_the_one_waiting` |
| 8: O3 | a stop forgets the fresh builds not granted (`Builds::drop_rebuilds` in `grant`); their requests are served on the next start | — (a race: no directed test) |
| minor | the table above names real tests; F2's `Unsupported` branch is tested; a request that strays into a snapshot is acknowledged with it (under the routing mutation the tests now fail at once instead of spinning to nextest's kill); an ask and its fallback count one probe | `a_lane_without_substitutes_probes_the_same_item` |

**O1, a caveat.** With one index a failing fresh build fails the whole run, on every probe, so the lane's probe
backoff and the run's restart backoff run side by side; with several, the index's builder restarts alone on its
own backoff.

**Remaining cost per item on the open path.** Ingest: the keys moved into their own vector and an `Arc<[RowKey]>`
per index per build (one allocation and a copy of 24 bytes per document), one relaxed `fetch_add` per published
slot, and `Batch` 32 bytes larger (copied per lane and per redelivery ledger entry). Sink: one integer compare
per received item. A throwaway local A/B of `live_drain_held` (pinned, three rounds) was within its noise either
way; the CI gate decides.

**Mutations** (fix round 1; each compiles, none hangs):

| Mutation | Killed by |
| --- | --- |
| B1: a substitute replaces the map | `a_second_substitute_*`, `a_backlog_past_the_lookahead_*`, the open-stream schedules (`a_fixed_row_unparks_its_lane` failed under it only through a side effect, corrected in round 2) |
| B2: entries pruned past unacknowledged items | both random schedules |
| a refused substitute's landed documents forgotten | the closed-stream schedules |
| F14: a fresh build asks the budget | `a_rebuild_is_served_with_the_budget_held_full` |
| 7: a newer request queued beside the older | `a_newer_request_for_a_lane_replaces_the_one_waiting` |
| F3: a `Rebuild` routed to the backfill inbox | four `rebuilds` tests (fail at once now) |

### 14.10 Fix round 2 (review of #458 at `8b87df0d`): when an F9 entry may go

The round-1 drop rule read the lane's oldest unacknowledged item from the window and the writes in flight
only. Two handles were outside it: one dropped unacknowledged by a failed write (its item comes back with the
next run), and one waiting for its lane's caught-up write (`refreshing`). A sibling write landing afterwards
dropped the entry, and the redelivered older item was written over the substitute.

**The rule.** While a lane has F9 entries, the run keeps **one account** of the lane's items received and not
yet acknowledged: `seq → build number`, on the lane's `LaneRun`.

- An item enters it where it is received (`queue`, the one place a delivery becomes an item of the run), and
  leaves it only where its acknowledgement returned `Ok` (`acknowledged`, the one place a lane item's handle is
  acknowledged: live and snapshot parts, a stop's skipped snapshot items, completions, and items that waited for
  a caught-up write all go through it). A handle dropped unacknowledged (a failed write, a failed run, the run's
  end) never leaves the account, so the floor never passes it.
- When a lane's map becomes non-empty in the middle of a run (its first substitute lands), the account is seeded
  from every place a received, unacknowledged item of the lane can be: the window, the parts of writes in
  flight, and the items waiting for a caught-up write. Those are the only places a lane item's handle is held
  between `queue` and `acknowledged`; anything else is a handle already dropped, which happens only when the run
  fails, and **once the run has failed nothing is pruned** for the rest of it.
- A run starts with an empty account for a lane that has entries (the account is the run's; the map is the
  engine's). An entry `s` goes when the account's smallest build number reaches `s`, or, with the account
  empty, when the run has received an item numbered at least `s − 1` (the next one the lane can bring is
  numbered past it). Proof sketch: a lane delivers in build-number order and redelivers what was left
  unacknowledged before anything new; so by the time a run has received an item numbered `b`, every
  unacknowledged item numbered below `b` was received in this run before it, and sits in the account unless it
  was acknowledged. An item with no build number blocks every drop.
- **Boundary.** With the oldest unacknowledged item numbered `s − 1` the entry `s` stays; numbered `s`, it goes.
- **Memory.** Once every item of the lane received before the substitute is acknowledged, the map empties, the
  account is dropped and the lane's state is released (`LaneState::is_kept`).
- **Cost.** With no entry anywhere, none: the account exists only while its lane has entries.

**Why no path bypasses it: every place a lane item's handle lives in a run** (`sink_engine/writes.rs`).

| Where | Created, moved, dropped or acknowledged | Covered by |
| --- | --- | --- |
| `queue` | a delivery becomes a `Queued` in the window | enters the account here (while the lane keeps one) |
| `take_from_window` → `grant`/`pick` | moved into a `Part` of a `Flight` | still in the account; seeding reads `parts_in_flight` |
| `substituted` | the parked item's handle moved into the probe's `Part` | the same |
| `put_back` | a held part back into the window | the same (it never left) |
| `landed` → `acknowledge` → `acknowledged` | a landed part | leaves the account only after `ack` returned `Ok` |
| `acknowledge` → `start_refresh` (`Refreshing.handle`) → `refresh_returned` → `acknowledged` | an item waiting for its lane's caught-up write | stays in the account until then; seeding reads `refreshing` |
| `skip_backfill` (stop) → `acknowledge` | a snapshot item acknowledged unwritten | through `acknowledged` |
| `seed`/`seed_returned`/`put_back_complete`, the completion acknowledged in `grant` | a `SnapshotComplete` | no build number: not an item of the account |
| `landed`'s `Err` arm, `fail`, the run's end | a handle dropped unacknowledged | never leaves the account; nothing is pruned once the run failed; the next run starts again from the redelivery |
| `grant`'s `parts` (window → `launch`), `landed`'s `landed_parts` during its acknowledgement loop, `refresh_returned`'s `waiting` | short-lived local holders inside one `&mut self` call | no seed runs while they hold a handle: seeding happens only in `supersede`, which `landed` calls after its acknowledgement loop (every landed part already acknowledged or moved to `refreshing`), and neither `grant` nor `refresh_returned` calls it |

**A write mixing a block and refusals for one index.** `held_indexes` lets a block win over refusals of the
same index, and a blocked part's report names no documents, so a substitute in such a write adds nothing to
the map. That is right in every case OpenSearch produces: a write block (`index.blocks.*`) and a role without
write permission refuse every item of the index in the request, so none of its documents landed. The only way
some could have landed is a block set while the `_bulk` request was being applied; then those documents are
written again by the parked item's next probe (an older build over a newer one, until the row next changes) —
the one case the map can't see, because the sink port doesn't say which items a block refused.

**Tests** (round 2): the two reproductions, adopted (`a_sibling_write_landing_after_a_failed_one_keeps_the_map`,
`an_item_waiting_for_a_caught_up_write_keeps_the_map`); the boundary and memory bound
(`an_entry_stays_while_an_older_item_is_unacknowledged_and_goes_after`: an item numbered 11 prunes while 9 is in
flight, the entry 10 stays; once 9 is acknowledged it goes and the lane keeps no state); the seed's caught-up wait
(`the_parked_item_is_never_rewritten_whole`, the review's strengthened version, round 3: a lane isn't probed while
its own caught-up write is out, so the item waiting for one when its substitute lands is the parked item itself,
whose acknowledgement waits for the quiet lane's caught-up write; the test waits for every acknowledgement and
counts the writes holding the document). The random schedules: the seeds refusing documents write up to four
items at once and, two in five, report writes searchable within 50 ms; after a substitute the sink's next write
fails now and then (a part, a sibling's, or a caught-up write); they assert lanes asked again after a substitute
and writes failed after one.

**Mutations** (round 2; each compiles, none hangs):

| Mutation | Killed by |
| --- | --- |
| a received item left out of the account | both random schedules |
| a caught-up wait left out of the seed | `the_parked_item_is_never_rewritten_whole` (round 3; before, nothing killed it reliably) |
| an entry dropped one item too early (`> floor + 1`) | `an_entry_stays_while_an_older_item_is_unacknowledged_and_goes_after` |
| an entry never dropped | the same |
| pruning after the run failed | none: redundant. A handle dropped by the failure never leaves the account, so the floor can't pass it either way; the check stays as a second barrier |

Not done: under the routing mutation (a `Rebuild` sent to the backfill inbox) the open-stream schedule still
runs to nextest's kill; the directed `rebuilds` tests fail at once.

### 14.11 Fix round 3 (review of #458 at `ff8b7b43`)

- **F9 depends on the stream port's order**, now written into `Consumer`'s contract (`stream/src/queue.rs`):
  items come in publication order, and a delivery returned (nacked or dropped) comes back in that order among
  the returned ones, before any item not yet delivered. 14.10's proof uses exactly this ("by the time a run has
  received item `b`, every unacknowledged item below `b` was received in this run"). The channel adapter's
  `a_returned_delivery_comes_back_before_anything_not_yet_delivered` pins it.
- **m2:** the substitute lane is not read once the run failed, so no probe write starts after a failure.
- **m3:** `supersede` returns at once for an empty set of documents (no account seeded for nothing).
- **m4:** a landed substitute's documents (and those of one refused for others) join the map before its
  delivery is acknowledged.
- **m1:** after every refusal stretch following a substitute the sink's next write fails; the schedules count
  each armed failure a write consumed and assert at least 20 (62 in the closed-stream test).
- **The bench.** Nothing on the open path changed between `8b87df0d` (gate green) and `ff8b7b43`: `queue` and
  `acknowledged` each test `superseding > 0` first, the account lives on the per-run lane state, and
  `ff8b7b43` removed a per-landing `Vec` `8b87df0d` allocated. A throwaway pinned local A/B of `live_drain_held`
  against `origin/main` was within its noise (the machine's own load moved the base by 2× between rounds).

## 15. Slice 2 (`feat/lane-failure-3`)

### 15.0 Part 0: the per-batch cost slice 1 left

The in-process gate read slice 1's one-change batches (`live_drain_held/sinks1/1`) 8 to 13% over `main` on two
of three runs. What slice 1 added per published batch: a `Vec<RowKey>` of the documents' keys and its copy into an
`Arc<[RowKey]>` (two allocations and a copy of 24 bytes per document), one refcount pair per lane for that `Arc`,
one relaxed atomic add for the build number, and a `Batch` 32 bytes larger.

A first answer (`d054ce47`) moved each key into its envelope (`Envelope.key`). It cost no allocation, but every
envelope grew 24 bytes, and every pass a sink engine makes over its envelopes (admission hashing, rendering,
the F9 checks) walked the wider stride. With two sinks that is paid twice: the gate read `sinks2/1` +7.6%,
`sinks2/256` +9.1%, `snapshot/rows` +6.5% over `90073906`. It is replaced.

**As built: the item carries its build's own id vector.**

- **What an item carries.** `Batch.ids: Option<Arc<Vec<DocumentId>>>` (`#[serde(skip)]`): the vector of ids the
  build was handed, the one the builder already made, shared by refcount with every lane of the build (one
  lane per sink: each index has its own builder, so a build's ids are one index's). `Envelope` is `90073906`'s again, byte for byte.
- **Matching.** `writes.rs`'s `keys_of` renders each id (`DocumentId::rendered`, the same text as the envelope's
  `id`) and looks each envelope up by its id and index (the index a guard: §15.3), in envelope order. The vector
  is a superset: documents left out as failed under `skip` (#198), and documents an F9 strip (`without`) took
  out, are in it and unmatched; `without` keeps the same `Arc`, no copy. An envelope with no match, or an item
  with no ids, gives no keys: the probe writes the item itself (F2), never with a wrong key. Only a parked
  item's probe pays for the matching (one map of the build's ids).
- **Deletes and backfill pages** carry ids like live upserts: a delete is built from its id like any document.
- **A serializing stream** carries none (`#[serde(skip)]`; none exists, the channel moves items).
- **The reuse pool is kept.** `main` reused a build's id vector at once (`Arc::try_unwrap` → `spare`). Now the
  vector is lent to the items, so `Builds::land` keeps a handle in `lent` (at most `LENT` = 64) and `spare()`
  takes the oldest back once only that handle holds it (`Arc::strong_count == 1`, then `try_unwrap` and
  `clear`). Giving the pool up would have cost one `Vec` allocation per build; keeping it costs none in steady
  state. It allocates when the oldest lent vector is still held by an unacknowledged item, or more than 64
  are out (a stalled sink): one `Vec`, as before slice 1 when a build outran its spare. The `DocumentId`s' own
  heap (the key's values) is freed at `clear`, in the builder, as `main` freed it.

**What a published batch pays over `90073906`** (the path where nothing is refused):

| Cost | Per |
| --- | --- |
| One relaxed `fetch_add` (the build number) | build |
| One refcount increment for the pool's handle, one decrement when the build's own handle is dropped after publishing | build |
| One refcount increment per lane item, its decrement when the item is acknowledged | lane |
| An atomic load (`strong_count`) and a `try_unwrap` when the vector comes back | build |
| `Batch` 24 bytes larger (`build`: 16, `ids`: 8) | lane item |
| Nothing per envelope, no allocation in steady state | — |

That is more than "one atomic add": the ids ride on refcounts, uncontended, on the cache line the build just
wrote. The envelopes already took one refcount per lane on `main`; the ids take a second. A sink engine's pass
over its envelopes costs what it did on `90073906`.

**Two sinks.** What made two sinks cost more was the wider envelope, walked by each sink engine; it is gone.
Everything else per lane while every lane is open is integer work: the F9 strip is skipped while no lane of the
engine is superseding (`superseding == 0`), the held-item lists are empty, and `keys_of` runs only for a parked
lane's probe.

Pinned by: `every_lane_of_a_build_shares_its_one_id_vector` (two sinks: one vector, by pointer),
`a_parked_items_keys_come_from_its_builds_ids_by_rendered_id` (superset in envelope order, an unmatched
envelope, another index, no ids), `a_stripped_item_shares_its_ids_and_still_finds_its_keys` (`without`),
`an_id_list_lent_to_items_is_reused_once_they_are_gone` (the pool),
`every_item_carries_its_keys_and_a_growing_build_number` (live and backfill items carry ids),
`a_rebuild_is_built_once_and_sent_to_its_sink_only` (a delete's key), `stream`'s
`an_item_read_back_from_bytes_carries_no_ids`. Mutations, each caught: `keys_of` ignoring the index; the pool
never taking a vector back; a live item, then a backfill page, carrying no ids; `without` copying the ids; each
lane copying the ids; `ids` serialized (`DocumentId` has no serde: does not compile).

### 15.1 Part 1: reindex a parked, blocked or restaging lane (O4 as recommended)

**Today** (`main` at `6aa85913`): `SinkControl::Reindex` waits for every write of the sink to return, then for
each index: an index whose backfill is outstanding is **ignored** (warned about), the rest are staged
(`reindex` + `ensure_index`) and a backfill requested. The lane's health is untouched: a parked lane keeps
probing its item (now into the new generation), a restaging lane keeps its staging.

| Lane state | Today | Now |
| --- | --- | --- |
| open | new generation, backfill | unchanged |
| parked, refused | keeps probing the refused item (refused again in the new generation) | opens; the parked item is held unwritten until the index is seeded, then acknowledged; items behind it are written into the new generation |
| parked, blocked | keeps probing until the block lifts on the old generation's name | the same as refused |
| restaging | keeps its own staging and backfill beside the reindex's | opens; its held items are written into the new generation |
| backfill outstanding, lane open | reindex ignored (the backfill is already rebuilding) | unchanged (`gone`'s `a_reindex_while_the_fresh_backfill_runs_stages_nothing`) |
| backfill outstanding, lane parked, blocked or restaging | reindex ignored: the lane stays stuck | the outstanding backfill is cut and asked for again from the first key, its checkpoint forgotten (as a restage does) |

**Rules.**

| # | Rule | Test |
| --- | --- | --- |
| X1 | A reindex of a lane not open opens it, its backoff forgotten | `a_reindex_opens_a_parked_lane_and_holds_its_item_until_seeded` |
| X2 | The parked item is not written: held, unacknowledged, until the reindexed index is seeded (its `SnapshotComplete` for the new request), then acknowledged and reported applied | the same |
| X3 | Items behind it are written into the new generation, in lane order; their positions are applied only after the held one's (fences report in key order), so nothing counts as landed before the new generation holds the documents | the same |
| X4 | A blocked lane: the same | `a_reindex_opens_a_parked_lane_and_holds_its_item_until_seeded` (its blocked case) |
| X5 | A restaging lane opens; its held items are written (they were held only for a missing target) | `a_reindex_opens_a_restaging_lane_and_writes_its_items` |
| X6 | With the lane not open, an outstanding backfill is cut and asked for again, its walk checkpoint forgotten; with the lane open, the reindex is ignored as before | `a_reindex_during_a_backfill_cuts_it_and_asks_again`, `a_reindex_while_the_fresh_backfill_runs_stages_nothing` |
| X7 | A run failing before the seed: the held item comes back and is held again (the lane's state remembers its build number), so it is never written | `a_held_item_redelivered_after_a_failed_run_is_held_again` |

**Why confirmation and fences stay right.** The held item's position is reported applied only at the seed. Until
then the lane's applied mark stays before it, so the watermark, and so the slot, stays before its change, and
#379 already answers `503 backfilling` for the index. At the seed the new generation holds every row as read after
the reindex, its change included. A crash before the seed: the slot is before the change, the generation is
unseeded, and the next start requests its backfill; the change is replayed and rebuilt. An engine run failing
before the seed: the item comes back and is held again.

**F9 across a reindex.** A reindex's backfill pages and later live items are built after the reindex, so they are
numbered past every entry the lane's map holds; the map and the account are left as they are (the held item is in
neither the window nor the flights, and stays in the account until its acknowledgement at the seed).

### 15.2 Part 1, fix round 1 (review of #459 at `dfc9ec1a`)

| # | Rule | Test |
| --- | --- | --- |
| X8 (B1) | A held item's identity stays on the lane's state (seeded) until its acknowledgement returned `Ok` (`acknowledged`), so an item redelivered after a failure between the seed and that acknowledgement is acknowledged unwritten, never written | `a_failure_between_the_seed_and_the_held_items_ack_never_writes_it` |
| X9 (S1) | A lane holds a list: a second reindex while items are held adds to it; the next seed releases every held item; a redelivery matches any entry | `a_second_reindex_keeps_the_first_held_item_unwritten` |
| X10 (S2) | Once stopping, held items are acknowledged without their positions applied: confirmation follows the applied marks (`Progress::landed`), never the acknowledgements, so the slot stays before their changes, the drain ends, and the next start replays them into whatever generation is then backfilled | `a_stop_acknowledges_a_held_item_without_applying_it` |
| X11 (S4) | *Replaced by X14–X18 (§15.3).* A reindex that cannot stage its new generation failed that reindex only, logged, the sink's run going on; it was not retried, and the lane was not left as it was: the fence had been told the index was rebuilding | — |
| X12 | A parked lane records its parked item's identity when it parks, so a reindex staged while that item is not in the window (the first pass of a run after a failure) still holds it when it is redelivered | `a_reindex_staged_before_the_parked_item_is_redelivered_still_holds_it` |
| X13 | A held item counts in `is_oldest` (items behind it are not the supervisor's progress) and in the F9 account's seed | — (stated) |

**Why X10 never confirms past a held change.** At a stop the engine confirms what `Progress::landed()` says
(`drain_lanes`, then the final confirm before `close_live`). `landed` is the minimum over the lanes' applied marks,
which move only through `Progress::applied` (a write that returned, or a held item settled at its seed). A held item
acknowledged at a stop never reaches `applied`, so its lane's mark, and the slot, stay before its change. The
acknowledgement only empties the channel's ledger, which is what lets `stream.is_idle()` end the drain.

**What a reindex cures (S3).** A reindex recreates the index from the same schema and mapping. It cures what lives
in the generation: a write block on that index, settings or a mapping changed by hand, `index_settings` changed and
the process restarted. It does not cure a row the mapping refuses (the new backfill's page is refused, the lane
parks on it, the index is never seeded, and the held item waits until the row is fixed), nor a cluster-wide block
(X15: the lane is `restaging`, the reindex retried until the block lifts).

**The held state in `/status`.** A lane holding an item reads `open`; it pins the slot until the seed
(`pinned_by` names it) and its stall age grows, so `FlussoLaneStalled` can fire during a long reindex backfill. Not
changed here: a `held` health value (excluded from the alert) is the follow-up.

### 15.3 Review of `main` at `9b70cd6e`: a reindex whose staging fails is retried

#459's commits reached `main` by a mistaken push; it was reviewed there and kept. Its fixes, on
`fix/lane-failure-followup`:

**Keys (review 1, 3).** `keys_of` looks an envelope up by rendered id **and index**; two different keys of its
index under one rendering (composite parts holding `:`) give no keys, so the probe writes the item itself (F2).
Each index has its own builder (`ingest.rs`, `Builds::of_index`), so one build's ids hold one index: the index
check is a guard, not a fix of something a run could reach (`each_index_sharing_a_rendered_id_gets_its_own_fresh_build`
passes with the old code too). Alike renderings within one index are reachable:
`an_item_whose_keys_render_alike_probes_itself` (end to end), `two_keys_rendering_alike_in_one_index_give_no_keys`.

**Reindex staging (review 2).** X11 dropped a reindex that could not stage, after `on_index_rebuilding` had told
the fence the index was rebuilding: nothing moved the fence back, so every later fence on the index timed out until
a restart. A momentary error cancelled a reindex the caller got `202` for, and one index failing dropped the
backfill of another that had staged. X11 is replaced:

| # | Rule | Test |
| --- | --- | --- |
| X14 | A reindex keeps an outcome per index. Every index that staged gets its backfill requested (cut and asked again when outstanding, X6), and its lane is handled as X1–X5 say | `a_reindex_of_two_indexes_stages_one_while_the_other_is_retried` |
| X15 | An index whose staging failed, or was not started after another failed, is **owed** its reindex: its lane becomes `restaging`, reason `the reindex could not stage: <error>`, and the reindex is retried on the lane's restage backoff (the first wait, doubling to the longest), as a missing target's staging is (§4). Other lanes and the sink's run go on | `a_reindex_whose_staging_fails_is_retried_until_it_stages` |
| X16 | A retry that stages is the reindex: the lane is handled as X1–X5 say, a parked lane's item held until the seed (the lane remembers it was parked). The fence's `Rebuilding`, announced before each attempt and again when an index becomes owed (so also for an index never started after another failed: review of #470, 5), stays true until that seed | the same; `an_index_never_started_is_announced_rebuilding_once_owed` |
| X17 | The owed reindex lives on the lane's state, so a failed run keeps it: staging at the next run's start leaves the lane `restaging` for the retry instead of reopening it | `an_owed_reindex_outlives_a_failed_run` |
| X18 | *Revised (review of #470, 1).* Asking for an index's backfill (`request_backfill`, or `ask_again`'s `forget_walked`, a sink write the same block refuses) failing makes **that index** owed, with the error as its reason; it never fails the run. Every index of the call is recorded staged or owed before anything returns, so no ordering of failures drops one. A run failing for another reason keeps them (X17) | `a_failed_ask_for_a_backfill_leaves_every_index_of_the_reindex_owed`, `a_reindex_whose_backfill_request_fails_is_owed` |

| X19 | *Reviews of #470, 2 and second review 1, 2.* Once the stop is asked, nothing is staged or asked for: no `restaging` lane is staged again (its target gone, or owed its reindex), a reindex still waiting to be staged is dropped (logged), and a staging already out when the stop came (a restage, an owed retry, the control's inline reindex) finishes as **state only**: it opens no lane and its lane stays `restaging` (owed). No backfill is asked for once stopping; an ask already in progress when the stop came completes and is never served (the ingest engine serves no request once stopping). A `restaging` lane's live items and `SnapshotComplete` are acknowledged unwritten and unapplied (`release_restaging_on_stop`), its index left unseeded, read and released until nothing more is ready (a lane not open takes one item at a time), also while its staging is out, so the drain ends at once | `a_stop_while_a_reindex_is_owed_drains_at_once_and_stages_nothing`, `a_reindex_pending_at_a_stop_is_not_staged`, `a_stop_releases_the_completion_waiting_on_an_owed_lane`, `a_stop_releases_a_lane_restaging_a_missing_target`, `a_stop_during_an_owed_retry_records_it_and_writes_nothing_more`, `a_retry_returning_staged_during_a_stop_leaves_the_lane_owed` |
| X20 | *Owner's decision (review of #470, 3; corrected by the second review, 3).* The owed reindex lives in memory: it outlives a failed run of the sink engine (X17), not the process. What the next start finds depends on how far it got. (a) The sink recorded a new generation (OpenSearch: `reindex` wrote the meta doc naming it, unseeded) and only a later step failed: staging finds the index unseeded, announces it `rebuilding` and backfills it. (b) The index was already unseeded (X6: its backfill outstanding; OpenSearch's `reindex` is a no-op then): staging finds it unseeded and resumes that backfill from its walk checkpoint, which the failed `forget_walked` may have left. (c) The sink recorded nothing on a seeded index: it is seeded as before and the reindex is gone; the operator asks again. In (a) and (b) the lane's stuck item is not held (X2 needs the reindex): it is replayed and written like any item | — (stated) |
| X21 | *Third review of #470, 2.* A sink run that fails during the stop is still restarted by the daemon (a transient write error must not turn a clean stop into exit 1), but staging under a stop only ensures (`stage_while_stopping`): no backfill is asked for, no restaging lane reopens, and an index that can't be ensured (or isn't tried after another failed) doesn't fail the staging: its lane is left `restaging` with why, and the restarted run releases it like any other. The restart still waits the daemon's restart backoff first A stop that lands while a staging is already ensuring is looked at again once the ensures returned, before any ask or reopen, so that staging ensures only too (fourth review of #470, 3); an index that failed to ensure then fails that staging, and its retry takes the stop path. It is reached only by a run restarted during the stop: the first staging never stages under a stop (X22) | `staging_after_a_run_failed_during_the_stop_only_ensures`, `an_open_lane_not_ensured_under_the_stop_is_released`, `a_stop_during_a_staging_makes_it_ensure_only` |
| X22 | *Fourth review of #470, 1.* The source is prepared only after every rebuild was staged (#120), whatever the stop does: the daemon's first staging looks at the stop before staging and again once staging returned (`biased`, stop first, then a check), so the engines never start behind a stop, and the ingest engine checks the stop before `prepare` and ends without preparing. A first staging under a stop therefore stages nothing (it would only ensure, skipping a `Fresh` rebuild). The check after staging is not pinned on its own: the other two make its absence unobservable | `a_stop_before_the_first_staging_never_prepares` (daemon, deterministic: nothing ensured, nothing prepared), `a_shutdown_racing_the_first_staging_never_prepares_without_the_rebuild` (daemon, 60 runs), `a_stop_requested_before_the_run_prepares_and_reads_nothing` (engine) |

**Known limit, outside a stop (third review of #470, 2; to be filed).** `stage()` fails as a whole when one index
can't be ensured. So after any failed run, an index whose cause persists (an owed reindex's `ensure_index` still
refused, a target that still can't be created) blocks the restart of the whole sink, and its other lanes wait with
it, until the cause lifts. X17 assumes the restart's `ensure_index` succeeds. Unchanged here; only staging under a
stop leaves such an index restaging instead (X21).

**One rule for every reindex path (third review of #470, 1).** The control's reindex is staged inline in the run's
`advance`; an owed retry is a flight. Both end in `settle_reindex`, so a stop seen after staging records the staged
indexes as owed and opens no lane, whichever path staged them
(`a_first_reindex_staging_as_the_stop_comes_opens_nothing`, `a_retry_returning_staged_during_a_stop_leaves_the_lane_owed`).
The inline staging is awaited by the run's loop, so a stop that comes while it is out waits for that round trip
before the drain ends (the loop would have to be restructured to make it a flight); the retries are flights and
don't hold the drain.

**Why X19 keeps confirmation and fences right.** A lane's applied mark moves only through
`Progress::applied`, which only a write that returned (`write_returned`) or a held item settled at its seed calls.
A released item is never written: it is acknowledged (`acknowledge`, whose last argument is the caught-up-write
hint, not a position), which settles it in the channel's ledger and the run's own accounts and moves no
applied mark. So the lane's applied mark, `Progress::landed`
and the confirmed slot stay before its change, and the next start replays it. Its `Fences` entry stays `Writing`,
so nothing behind it on the lane can raise `applied` past it either. A fence on the lane never sees it applied;
once stopping, `/wait` answers `503 stopping` anyway.

**One backfill request per index, as before.** `stage_reindex` asks for each index's backfill on its own
(`request_backfill`/`ask_again` per index) so a failed ask owes only its index. That changes no request: a
`Request::Backfill` names one index, and `request_backfill` has always published one per index, so the ingest
engine sees the same requests and coalesces them as before (per index, into its builder's snapshot), and
`outstanding` holds the same entry per index. `forget_walked` was already one write per cut index. Only the
observer's `on_backfill_requested` now comes once per index instead of once for the set; the fence takes it per
index anyway.

**What releases each item at a stop.** Every `Queue` variant in the run's window, every lane state; "released" is
acknowledged unwritten and unapplied. Items elsewhere are in the list after the table.

| Item \ lane | open | parked (refused, blocked) | restaging (target gone) | owed (restaging, a reindex owed) | holding (X2) |
| --- | --- | --- | --- | --- | --- |
| live `Batch` | written as usual, applied when it lands | the front item is probed once (`probe_on_stop`; a fresh build asked is not waited for, F13): landed, the lane opens and the rest is written; refused or blocked again, it stays and holds the drain until the deadline (exit 1), as before this PR: under `on_error = "stop"` it neither lands nor is dropped | released (`release_restaging_on_stop`) | released | the held item: released (`release_held_on_stop`, X10); items behind it: written as usual |
| backfill page (`Batch`, no position) | released by `skip_backfill`, its index marked cut short (left unseeded) | the same | the same | the same | the same |
| `SnapshotComplete` | run once nothing of the lane is in front: seeds nothing if a page was cut short (`cut_short`) or the request was superseded, else seeds | the same, once the probe cleared the front | released, seeds nothing | released, seeds nothing | the same as open |
| item read after the stop | as above | as above | read and released until nothing more is ready, also while a staging is out | the same | as above |
| a staging out (restage, owed retry) | — | — | finishes as state only: lane stays restaging, no new ask (one in progress completes, unserved) | the same, lane stays owed | — |
| the control's reindex, staged inline | the same as a retry: staged indexes owed, no lane opened (`settle_reindex`); the drain waits for that round trip | the same | the same | the same | the same |
| the run fails during the stop | restarted after the daemon's backoff; staging only ensures (X21); the restarted run treats each lane as this table says | the same | the same; one that can't be ensured stays restaging | the same | the same |

**Where else an item can be at a stop** (third review of #470, traced):

- **In flight** (a write's `Part`): it lands, and its position is applied only up to the first entry of its lane
  still `Writing` (a released item stays `Writing` in `Fences`); or it comes back (a missing target puts it back in
  the window, released as above; a refusal parks the lane: the parked row).
- **Held** (`run.held`, X2): released unapplied by `release_held_on_stop` (X10); one redelivered later is held again
  on arrival, then released.
- **Waiting on a caught-up write** (`Refreshing`): acknowledged when that empty write returns; a failure fails the
  run, which the daemon restarts (X21).
- **A seed in flight** (`Seeding`, its `SnapshotComplete` taken): it finishes, seeding the index or not, and the
  completion is acknowledged after it, as without a stop.
- **Still on the lane**: read and treated as the table says (a restaging lane's are read and released until none
  is ready).

For every sink shape: releasing and skipping run in `advance`, before any grant, and take no write slot, so an
ordered sink or `writes = 1` drains the same (`a_stop_releases_the_completion_waiting_on_an_owed_lane` runs both).
A release may make its lane quiet: `acknowledge` then starts one empty caught-up write (`owed_refresh`) only when
the lane has a written position not searchable yet (written before it stopped being open), as any quiet lane gets
at a stop; the item is acknowledged when it returns, and its failure is the run's, as for any lane. The parked row is
the one place a stop can still wait for the deadline, and it was so before this PR (slice 1, item 6).

A lasting failure (a cluster-wide block) shows as the lane `restaging` with its reason in `/status` and
`FlussoLaneParked` covers it, while the other lanes run (S4's goal).

**A wasted backfill after a failed run (review of #470, 6; kept).** When the new generation was recorded and a
later step failed, a failed run's restart stages it and asks for its backfill, then the owed retry finds that
backfill outstanding and cuts it and asks again from the first key: one backfill read for nothing. It is correct
(the second covers the same generation from the first key, and the first's pages land in that generation too) and
rare; telling the restart's request apart from an older one needs more state than it saves.

**X13, stated:** a held item acknowledged at a stop still counts in `is_oldest` until that acknowledgement
returned; items behind it are not the supervisor's progress.

## User-visible, second half

What the documentation agent writes the manual from, per slice. Nothing else in this note is meant for it.

### Slice 1: fresh builds (PR #458)

1. **A row fixed in the database unparks its lane.** A lane parked because the destination refused a document
   (`on_error = "stop"`) asks, at each probe, for that item's documents to be built again from the database and
   writes them in its place. A fixed row lands and the lane opens with no restart; a deleted row is written as a
   delete, which lands.
2. **The probe schedule is unchanged:** the first wait, doubling to the longest (`[backoff]`).
3. **The fresh build is waited for at most the backoff, and never more than 5 s.** Past that the probe writes
   the item as it was, and the next probe asks again.
4. **Documents never go back to an older version.** Once a fresh build lands, older copies of its documents
   still on that lane are skipped, including any a failed run sends again. Documents of a fresh build that
   landed while others were refused again count too: the parked item is written without them, and the next
   fresh build covers only what is left.
5. **Only refused lanes rebuild.** A blocked lane (write block, missing permission) still writes the same item
   until the block is lifted.
6. **At shutdown** a parked lane's last probe writes the item as it was; no fresh build is made once stopping.
7. **A fresh build failing on the database side counts as the index's failure.** `/status` shows
   `ingest.indexes.<name>.state: down`, with `last_error.live` and `restarts.live`. With one index that fails
   the whole run on every probe.
8. **No new `/status` fields or metrics.** A lane waiting for its fresh build shows health `refused`, with
   `next_probe` at the end of the wait; `flusso_lane_probes{outcome}` counts each probe once.
9. **Runbook, refused lane:** "fixing the row doesn't unpark the lane" is no longer true. Fix the row and the
   next probe lands; switching to `on_error = "skip"` or changing the schema is no longer the only way out.
10. **Nothing to configure.** For users of the stream port as a library: `Request` gains `Rebuild`, `Stream`
    gains `substitutes` (with a default), `Batch` gains `build` and `ids` (the build's document ids, shared
    by every lane, never serialized; slice 2 replaced slice 1's `keys`), `RowKey` and `DocumentId` move to the
    kernel (still re-exported by `source`), and `Consumer`'s contract states the redelivery order. Nothing
    else about how keys travel is user-visible.

### Slice 2: reindex as the way out of a stuck lane (PR #459, follow-up PR #470)

1. **`flusso reindex <index> --sink <sink>` (`POST /indexes/<name>/reindex?sink=`) now unsticks a lane that is
   parked (refused or blocked) or restaging.** The lane opens at once; the item it was stuck on is not written
   again: its documents come from the new generation's backfill, which reads every row after the reindex. Items
   behind it are written into the new generation as usual. **Caveat:** a reindex recreates the index from the
   same schema. It cures what lives in the index (a write block on that index, settings or a mapping changed by
   hand), not a row the mapping refuses (the new backfill is refused at that row, the lane parks again, and it
   unparks once the row is fixed).
2. **A reindex that cannot create its new generation is retried until it can** (#470). The HTTP call has already
   answered `202`. The lane's `/status` health becomes `restaging`, with the destination's error as its reason
   (`the reindex could not stage: …`), and the reindex is tried again on the lane's backoff (`[backoff]`: the
   first wait, doubling to the longest). A momentary error heals by itself; a lasting one (a cluster-wide block)
   shows as that lane restaging, with `flusso_lane_parked{why="restaging"}` at 1 so `FlussoLaneParked` fires,
   while the sink's other lanes keep writing. Once the destination accepts it, the reindex goes on as in 1. The
   wait is kept in memory: a failed run of the sink keeps it, a restart of flusso does not. After a restart: if
   the new generation was created (only a later step failed), the next start backfills it; if the index was
   still being backfilled when the reindex was asked, the next start resumes that backfill; otherwise **the
   reindex is gone, so ask for it again**. Either way the item the lane was stuck on is replayed and written
   like any other, not held.
3. **A reindex of several indexes** goes ahead for each index that can be staged, even when another cannot; each
   that cannot waits and is retried as in 2.
4. **Until the new generation is seeded**, the index's fences answer `503` with reason `rebuilding` (as for any
   reindex, including while a retry waits, and for an index whose staging never started because another failed
   first), and the stuck item's change and everything after it on that lane are
   not counted as landed: the replication slot stays before them. Once the backfill completes, the item is
   acknowledged and the lane's positions move on.
5. **A reindex while the index's backfill is still running** is ignored when the lane is open (that backfill is
   already rebuilding), but on a parked, blocked or restaging lane it restarts the backfill from the first key.
6. **`/status`:** the lane's health goes from `refused`, `blocked` or `restaging` to `open` once the reindex is
   staged (at once, or at the retry that succeeds); no new fields. `flusso_lane_parked` drops to 0 for the lane.
7. **A stop before the new generation is seeded** drains normally (exit 0): the stuck item's change is not
   confirmed, so the next start replays it and requests the backfill again. **A stop while a lane is `restaging`**
   (a reindex waiting for its retry, as in 2, or an index whose target went missing) also drains without waiting
   for the lane: nothing is staged, retried or asked for once stopping (a staging already under way finishes but
   opens nothing; an ask for a backfill already in progress completes and is never served), a reindex not yet
   staged is dropped, a sink run that fails during the stop is restarted to ensure its indexes only, and the lane's
   waiting changes are left unconfirmed, so the next start stages the index and replays them (what it does about
   the reindex: 2). **Two waits remain:** a reindex asked for just before the stop and still creating its new
   generation holds the stop until that one round trip with the destination returns; and a sink run that failed
   during the stop is restarted only after the daemon's restart backoff, which doubles while the sink keeps failing
   without progress, up to `max_restart_backoff` (60 s by default), past the default 30 s `--shutdown-timeout-secs`,
   so such a stop can end at its deadline (exit 1). A lane parked on a
   refused or blocked item still writes it once at the stop and, refused again, holds the stop until its deadline,
   as before.
8. **Runbook, a stuck lane:** a reindex of that index on that sink is the way out for a write block on that index
   that won't be lifted, a target that can't be staged, or a mapping or settings change made by hand; for a row
   the mapping refuses, fix the row (or switch the index to `on_error = "skip"`). A lane `restaging` with
   `the reindex could not stage: …` is waiting for the destination: lift what the reason names (often a
   cluster-wide block) and the next retry completes the reindex.
9. **During a reindex backfill** the lane reads `open` but holds the replication slot until the backfill
   completes (`pinned_by` names it), so `FlussoLaneStalled` can fire for a long backfill.
10. **A parked lane's fresh build** (slice 1) works for every index; an item whose documents' keys can't be told
    apart from their ids (composite keys whose parts hold `:`, rendering alike) is probed as written instead of
    rebuilt. Nothing to configure.
