# Stage 5: the memory budget

ADR 0012, stage 5: the global limit, the backfill half, and the one-batch grant. Still one ingest task. **Not in this stage:**

- the per-index half (8a, with index tasks);
- `max_pending_documents` (8a: it bounds the pending sets, which don't exist yet).

Line references are to `d082b9e5`.

**Why now.** On `reference` at `builds = 4`, peak RSS went from 124 to 329–345 MiB (stage 4 notes). Builds outrun a sink-bound backfill, and the built batches wait in the lane up to its `capacity` of 1,024 items (`libs/2-adapters/stream-channel/src/config.rs:25-29`). Nothing bounds them in bytes.

## 1. What is counted, and how

**Bytes of a batch: a size walk, on the build's own task.**

- `budget::heap_bytes(&Envelope)` walks the document tree and sums what each node owns on the heap:
  - a `String`'s and a `Vec<u8>`'s capacity;
  - a `Vec<GenericValue>`'s capacity × `size_of::<GenericValue>()`;
  - per `BTreeMap` entry, `size_of::<(String, GenericValue)>()` plus the node overhead amortised per entry (a B-tree leaf holds up to 11 entries; the estimate is `(key + value) × 12 / 11` plus 8 B per entry for edges);
  - a `BigDecimal`'s digit `Vec`.
- Each envelope adds its id `String`, the index name and `size_of::<Envelope>()` itself.
- `GenericValue` is defined at `libs/0-kernel/src/common/generic_value.rs:32-64`, `Envelope` at `libs/0-kernel/src/envelope.rs:114`.

**Where the walk runs, and what it costs.**

- It runs inside the build future (`ingest/builds.rs`, `Builds::start`), after `build_many` returns, so a real build walks on its spawned task, beside the decode and scripts, never on the loop.
- In the in-process bench the build finishes inline, so the walk does land on the loop. It is a visit per node (the bench's `users`-like body is about 30). It is counted and timed against the stage-4 numbers before this ships.
- **Rejected:** JSON length × a factor (the ADR's 8–10× varies per index, so it is wrong by up to the factor's spread), and an allocator-based count (needs a global allocator hook in the binary and attributes nothing to a batch).

**Validated twice:**

1. **Against a counting allocator, per document.** A test binary with a counting `#[global_allocator]` builds representative documents and compares the walk to the allocator's count. The limits stated there are the walk's honesty claim. Allocator bookkeeping is not counted, so RSS is expected above the walk by 10–20% (ADR §5).
2. **Against the process.** On `reference` and `complex` backfills at a small budget, held-bytes peak is compared with RSS growth over the idle baseline, and the factor is reported.

**Counted once, released when the last holder drops it.**

- **One `Hold` per batch.** A batch is shared by every lane's copy of the `LaneItem` and by the channel's redelivery ledger, which keeps a clone until the ack (`libs/2-adapters/stream-channel/src/lib.rs:208-236`). The bytes go back when the last copy goes, with no lock on the path. `Batch` (`libs/1-ports/stream/src/items.rs:41-51`) gains a `hold: Hold` field: an `Option<Arc<dyn Send + Sync>>`, serde-skipped, `PartialEq` always true, `Debug` as its byte count.
- **The guard.** The engine puts its guard there; the guard's `Drop` returns its bytes to the budget with one `fetch_sub`, and wakes the loop only when the loop has said it waits (an `AtomicBool`).
- **Why that is the right moment.** A lane item is in the ledger from delivery to acknowledgement. So the last copy drops once every lane has acknowledged the item, the ingest engine's copy is gone, and nothing redelivers it:
  - a nack or a dropped handle leaves the ledger's copy;
  - a sink engine restart re-takes the ledger's copy.

**Stage 2's sink engine keeps the batch until the ack. Checked per site in `sink_engine/writes.rs`:**

- `Queued`/`Queue::Batch` (`:93-104`) holds the `Batch` while it waits.
- `launch` (`:575-623`) moves `batch.envelopes` into the write and drops the rest of the `Batch`, including its `Hold` clone. But the ledger's clone is alive until the ack, so the bytes stay counted.
- `Documents` (`:106-115`) clones the envelopes `Arc`, not the `Batch`, and lives in the flight until it lands.
- `Flights` holds the write future, which holds the envelopes.

**The overcount window.** The envelopes can outlive the `Hold` by the instant between the ack and the write future's drop, which is microseconds after the write returned. Never the other way round: no `Hold` is released while a lane may still redeliver.

**Reservation and settle (ADR §5).**

- A grant reserves an estimate: the batch's ids × the running average bytes per document of its kind. One average for live and one for backfill, not per index. A backfill batch is mostly one index (`Snapshot::take_keys` fills from the walk's front index). A live batch mixes indexes. A per-id map lookup at grant would be a new per-item cost on the loop.
- When the build lands, the reservation is settled to the walked size.
- Whatever finishes the reservation returns its bytes: the settle, a failed build, an abandoned slot at a stop.
- The average starts at 16 KiB per document (the ADR's `users` measurement) and moves as an exponential average, 1/8 per batch.

**A floor per batch.** Every batch counts at least 1 KiB, empty ones included (a burst of unread-column updates publishes empty batches with positions). So the budget bounds the item count too: at most `max_built_mib` × 1,024 items. That is what lets the lane's item `capacity` go (section 5).

## 2. Where the budget blocks, and why it can't deadlock

**The check sits in `Builds::choose`** (`ingest/builds.rs`), beside `has_room`. A candidate is admissible only if the budget can reserve its estimate. A refused candidate is simply not admissible; nothing waits inside the gate.

**The rules:**

- **Global:** reserved + held + estimate ≤ `max_built_mib`.
- **Backfill:** backfill's own reserved + held + estimate ≤ half.
- **One batch:** a candidate whose kind holds nothing at all is always admitted. For live: live holds nothing. For backfill: backfill holds nothing.
  - **Stage 5's form of rule 4.** The rule is "an index holding nothing gets one batch"; per index it lands in 8a, and stage 5 has no index tasks, so it applies per kind.
  - So a batch larger than the whole budget still builds, alone, once its kind's bytes are back to zero.

**Waking.** When a candidate is refused for the budget only, the loop arms a `released` arm, a `Notified` on the budget's `Notify`, enabled only while something was refused. A guard's `Drop` notifies when that flag is set. So a refusal costs a wake-up per release, and no release costs anything while nothing waits.

**No deadlock, by cases:**

- **The loop holds nothing that a release needs.** Releases come from sink engines acknowledging, on their own tasks.
- **What is held is on its way to being acknowledged.**
  - A healthy sink writes every published item and acks it, so held bytes always fall.
  - Built-not-published slots in the reorder queue are bounded by `2 × builds` and are published as the front lands. Their builds don't wait on the budget: the reservation was taken at grant.
- **The one-batch rule** means a kind with nothing held is never refused, so a batch bigger than the budget can't wait for a release that never comes.

**A stalled sink.** It holds its unacknowledged bytes for ever. The budget fills, live and backfill builds stop, and the ingest engine stops reading once its pending batch is closed. That is the documented backpressure ("a stalled sink eventually paces ingest").

- **Not a regression.** It also stops live changes for the healthy sink, exactly as today: `publish` awaits every lane (`ingest/commit.rs`, `publish`), so one full lane already blocks the next publish for every sink.
- **What stage 9 changes.** A parked lane's item is released once its fresh build is requested (ADR §5's extra rule), so a lane parked by `on_error = stop` stops pinning the budget.
- **What stays the same.** A merely slow or unreachable sink still holds its bytes until it acknowledges, as the ADR says ("the other indexes keep going while memory allows").

**The bound.** Held never passes `max_built_mib` + `builds` batches:

- each running build may settle above its estimate by at most its own size;
- an over-budget kind then gets no new reservation until it is back under.

## 3. The backfill half

- **Tracking.** A reservation and its `Hold` carry their kind. The budget keeps two counters, total and backfill, both atomics. The guard's `Drop` subtracts from both when it is a backfill hold.
- **The gate's counter.** "Every fourth contested grant" counts a grant as contested only when both kinds are admissible, budget included. A backfill batch the gate would pick but the budget refuses is not admissible, so the grant is uncontested and the counter doesn't move. That is the stage-2 rule: the counter moves only when both kinds could take the slot.
- **Live always has room.** With the backfill half full, live still has the other half.

## 4. Memory before a build, not counted here

- **The live buffer.**
  - One `PendingBatch` of at most `max_changes` changes' ids (`ingest/live.rs`).
  - The capture's read-ahead of `max_changes × 16` row images (CLAUDE.md, #343), bounded in count as before.
  - Nothing else: the reorder queue holds only granted batches, and those are counted.
- **Pending ids and `max_pending_documents`** come with 8a's pending sets. Stage 5 adds no reader-side bound.

## 5. Lane capacity: deprecated

**`[stream] capacity`** (and `--queue-capacity` / `FLUSSO_QUEUE_CAPACITY`) still loads (format freeze), warns once at start that it is deprecated and ignored, and does nothing.

**The lanes and the request lane get a fixed backstop of 1,048,576 items** (`stream_channel::LANE_BACKSTOP`).

- It never binds before the budget at the 1 KiB floor and the default 512 MiB: 524,288 items.
- A tokio bounded `mpsc` doesn't preallocate its capacity, so the backstop costs nothing.

**The channel adapter is the stream's, not mine.** Stage 7 (track B) reworks lanes per `(index, sink)`, so the change there is two lines (the default, and the backstop in `ChannelStream::new`), coordinated with B first.

## 6. Configuration and operations

**`[concurrency] max_built_mib`** (`--max-built-mib` / `FLUSSO_MAX_BUILT_MIB`):

- default 512, at least 16, at most 1,048,576;
- never written into the lock;
- the daemon's `DEFAULT_MAX_BUILT_MIB`, and `IngestEngine::with_budget(Arc<Budget>)` (the engine's own default: unlimited, so embedders and the in-process bench keep today's behaviour unless they opt in).

**What the binary exposes:**

- metrics:
  - `flusso.memory.built` (gauge, bytes, `work` = `live` | `backfill`, read from the budget at collection);
  - `flusso.memory.refused` (counter, `work`): grants refused for the budget;
- `/status` gains `memory: { limit_bytes, built_bytes, backfill_bytes }`;
- documented in `metrics.md`, `status.md` (or wherever `/status` is documented), `config-toml.md`, `cli.md`, `environment.md`.

**The Helm chart:**

- `resources.requests.memory: 512Mi`;
- the limit commented out, with the formula beside it: `max_built_mib` + `builds` batches + ~100 MiB baseline + 10–20% allocator overhead;
- the chart README updated.

**Also:** the dev Grafana panel (built bytes by `work`, refusals rate), the designer's field next to `builds`, `config.schema.json`, the golden fixture, and the upgrade page.

## 7. Tests (red first)

**The accountant, `budget::tests`:**

- reserve and settle; release on drop;
- the backfill half;
- the one-batch rule per kind, and a batch bigger than the budget;
- the refused-then-woken path;
- the floor;
- no leak when a reservation is dropped unsettled.

**The walk, its own test binary with a counting allocator:** per-document walk vs allocated bytes, within a stated factor for strings, arrays, nested maps, decimals and bytes.

**The engine (`ingest::tests`):**

- A **teardown helper**, `assert_released(&budget)`, waits until held bytes return to 0 after the lanes drain. Every `ingest::tests` case gets it:
  - a failed build;
  - a failed write (the sink restarts, the item is redelivered, then acknowledged);
  - a stop;
  - an abandoned snapshot.
- A **seeded schedule under a tiny budget** (a few batches), mixing live and backfill with a slow sink. It asserts held ≤ budget + `builds` batches at every observation, nothing deadlocks, and the end state equals the rows.
- `live_keeps_room_beside_a_backfill_holding_its_half`.
- `a_batch_bigger_than_the_budget_still_builds`.
- `a_stalled_sink_paces_builds_and_releases_on_recovery`.

**One live check, through `flusso-bench`:** `reference` backfill at `builds = 4` with `max_built_mib = 64` stays under a stated RSS. A scenario, not a CI test.

## 8. Measurement plan

- **Budget settings:** `reference` and `complex`, `builds = 4`, `writes = 4`, at budgets 512 / 128 / 32.
- **Per run:** backfill docs/s, peak RSS, held-bytes peak (scraped from `/metrics`) vs RSS.
- **The stage-4 burst question:** three interleaved rounds at builds 1 and 4 when load is under 6, with counts per burst: builds, mean documents per batch, resolve samples, builds in flight, and grants held back by the per-id rule.

## The sink's depth during a backfill (stage-4 numbers, #409 timings)

- **Writes are saturated.** At `builds = 4` the sink's `write` is 2.24 deep on average over a 5.25 s backfill, while its `idle` is 0.02.
- **The other 1.43 s is seeding.** The three `seed` steps (0.47 + 0.19 + 0.77 s) run one after another at the end, with no write in flight: 27% of the wall.
- **What the average means.** Over the writing part alone, the sink is about 3 deep, close to `writes = 4`, with no idle. The writes themselves are saturated (48 ms each), so the time left over is seeding, not admission. Admission can't be the cause anyway: backfill items share no ids, so it never holds one back.
- **Where the next gain is:** the seed (refresh, settings, alias move), track A's #410/#403. After that, OpenSearch's bulk throughput.
