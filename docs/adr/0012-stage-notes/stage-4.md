# Stage 4: the build gate and concurrent builds on the one ingest task

ADR 0012, stage 4. Still one ingest task, one lane per sink. Up to `[concurrency] builds`
(default 4, at most 32) `build_many` calls run at once. Their results are published in the order
they were granted, so each lane still sees what it saw before: build order is delivery order, and
live positions only grow along a lane.

Line references are to `f01b10ad` (the module split).

## Where it is today

- The loop (`libs/2-engine/src/ingest.rs:253`) awaits each build inline: `commit_live`
  (`ingest/commit.rs:27`) and `commit_snapshot` (`ingest/commit.rs:139`) call `build`
  (`ingest/commit.rs:92`), which awaits `DocumentBuilder::build_many`, then `publish`
  (`ingest/commit.rs:70`). Nothing is read, resolved or paged while a build runs.
- `build_many(&self, ids)` (`libs/1-ports/source/src/document/builder.rs:188`) takes `&self`, and the
  trait is `Send + Sync`. The Postgres builder runs each statement on a pooled connection
  (`PgDocumentBuilder::connect`, sqlx's default 10, `apps/cli/src/backends.rs:156`). Its caches are
  locks or atomics. `ScriptedBuilder::build_many` (`libs/2-script/src/builder.rs:65`) runs the
  scripts after the inner build, with no `await` between setting and reading its thread-local
  `RUNNING` (`libs/2-script/src/scripts.rs:77`). Concurrent calls are safe.

## 1. What runs at once, and why every document ends right

**At once:**

- backfill builds with each other (up to `builds`);
- live builds with backfill builds;
- live builds with each other, only while the live batch waiting behind a live build is full (the
  ADR's "a second only while at least `max_changes` are pending"). So a trickle still batches
  itself.
- `resolve` (on the loop task) and key pages (the snapshot arm) with all of them.

**Each build runs on its own spawned task.** A build is polled once inline. When that finishes it
(the in-process mocks), nothing is spawned. Otherwise the boxed future is handed to `tokio::spawn`.
Spawning matters twice:

- the decode, coercion and scripts in `build_many` are CPU work, which then runs on other workers;
- a spawned build makes progress while the loop is busy publishing to a full lane.

**The ordering argument.** Every grant gets the next sequence number `seq`. Items are published
strictly in `seq` order. For one document id:

1. **Builds never overlap.** A batch is granted only when no build in flight holds any of its ids.
   Ids are compared as 64-bit hashes of `(index, key)`, kept sorted per build (stage 2's idea): a
   collision only holds a batch back.
2. **A later build reads newer state.** The later one is granted after the earlier one's
   `build_many` returned. So every statement it runs starts after the earlier one's statements
   ended. Under Read Committed on the primary, each statement's snapshot is then a superset (ADR §2).
3. **A later build is published later.** The later one was granted later, so it has a larger `seq`.

What counts as in flight for (1) is the **build**, not publication: a build that has returned but
waits in the reorder buffer holds nothing back. (2) needs the earlier read to have ended, and (3)
comes from `seq`.

Within one batch, ids stay deduplicated (`PendingBatch::add`). **Across batches**, the per-id rule
above applies. A live batch that conflicts waits, and keeps accepting changes while it's open: a
change added later only adds ids, and the batch reads the database when it starts.

Positions: live batches are granted in the order they form, one candidate at a time, so live `seq`
order is position order. Each lane gets the same interleaving of live and snapshot items it got
before; only more of them are built at once.

## 2. The reorder buffer

- **Shape.** A `VecDeque` of slots in `seq` order, the front the oldest unpublished one. A slot is
  a live batch (position, change count), a snapshot batch (the requesting lanes' index sets), or a
  snapshot's `SnapshotComplete`. Each holds its build's output once it lands.
- **Publishing.** After every landing, the front is published while it is ready, each through
  today's `publish`.
- **`SnapshotComplete` is a slot of its own.** It is ready at once, so it is published right after
  the snapshot's last batch, wherever the builds finished. The next snapshot can start while the
  last one's builds are still in flight.
- **Unpositioned backfill items sit at their grant's place**, deterministically. They need no
  position: the watermark reads only positioned items. A lane that didn't ask for an index gets no
  slice, as today.
- **Bound: `2 × builds` slots, in flight or built, plus a snapshot's completion** (queued without a free slot; it holds no documents). No grant while the buffer holds that many. A
  slow build at the front can hold at most `2 × builds − 1` built batches behind it, each at most
  `max_changes` documents: 2,048 documents with the defaults, a few MiB on `reference`. That is on
  top of what the lanes hold. This is the only new memory until stage 5's budget.
- **Why `2 ×`.** At `1 ×`, one slow build stops every grant, and the concurrency is lost exactly
  when it pays.

## 3. The gate

Two candidates at most:

- **Live:** the pending batch. It is ready once it is closed (full, or past `max_delay`) or the
  stream would block. It is admissible when it is ready, its ids are free, and either no live build
  is in flight or the batch is full.
- **Backfill:** the snapshot's batch. It is ready when it is full, or the walk is done and the batch
  isn't empty.

A slot is free while fewer than `builds` builds run and the buffer is under its bound.

- **Contested** (both admissible): live wins, except every fourth contested grant, which goes to
  backfill.
- **Uncontested:** the one admissible candidate goes. The counter doesn't move.

**The counter never resets.** As on the sink side (stage 2), it counts only contested grants, so
while no backfill waits it stays put.

At `builds = 1` the gate gives live, live, live, backfill under a steady trickle during a backfill,
so a backfill still progresses. The gate runs after:

- a change is buffered and the stream would block;
- the snapshot takes keys or finishes its walk;
- a build lands;
- a publish frees a slot.

## 4. Batching

- **A lone change on a quiet stream commits at once (#135).** `drain_ready` is unchanged. With no
  live build in flight the batch is granted the moment the stream would block, as today.
- **A new batch starts building while earlier ones build.** That is where the gain is. The live arm
  is enabled while the pending batch is open (`PendingBatch::is_open`). When the batch is closed and
  can't be granted, the arm is off and the stream waits, as it does today during a build.
- **The deadline arm goes.** On `main` it never fires: a batch is committed in the arm that filled
  it. Kept, it would spin on a batch past its deadline that is waiting for a build.
- **A stop must still be noticed while the live arm is off.** The stop arm is enabled whenever the
  live arm isn't.
- **The snapshot arm** is enabled while its batch has room and its walk isn't done. Otherwise a full
  batch waiting for a slot would spin on `Step::Keys`.

## 5. Failure and stop

- **A build that fails** — `build_many` errs, the task panics, or a `Document::Failed` meets
  `on_error = stop` — makes `seq` k the failure:
  1. No new grant.
  2. The builds in flight are awaited, so no DB work outlives the run.
  3. Every slot before k is published in order. On `main` these were already published, because
     builds ran in order.
  4. Nothing from k on is published, then the error is returned.

  `Document::Failed` is decided when its slot is published (in order, on the loop task), so `stop`
  publishes nothing at or after it and `skip` drops just the document, as today (#198).
- **What is retried.** Live: everything from k on, since its positions were never acknowledged, so
  the stream replays them. Backfill: the request is not acknowledged, so the snapshot is served
  again.
- **The stop (#371).** No new read and no new grant, except the buffered live batch, which is
  granted once its ids are free (as "the buffered batch is published as a normal commit"). Builds
  in flight are finished and published in order. Then the lanes drain as today. A snapshot batch
  that wasn't granted is abandoned with the snapshot. Snapshot builds already granted are published,
  and a stopping sink skips them.
- **The live stream ending** waits for every build and publication before the run returns.

## 6. Connections

- **The builder pool is sized `builds + 2`:** the builds, one `resolve`, and one key page, all on
  the one pool (stage 3 put key pages there). sqlx's default 10 covered `builds ≤ 8`, so this
  shrinks it at the default and grows it past 8.
- **Stage 8a** moves `resolve` to its own connection (ADR §5).
- **A key page takes no build permit in this stage.** It has its own connection in the `+ 2`. ADR
  §5's "a key page takes a backfill permit" is left to the index tasks (8a), where a page is read by
  the task that builds it. Here the one snapshot reads at most one page at a time.
- **Behind PgBouncer**, transaction mode: up to `builds + 2` server connections are taken at once.
  Session mode: that many client and server connections.
- **`ScriptedBuilder`** is safe (section "Where it is today").
- **`resolve`** runs only on the loop task, one change at a time, as today.

## 7. Timings (#409)

- **`build`:** one per build, measured inside the build's task from start to return. With several
  in flight the sum passes the wall, as the sink's `write` does since stage 2.
- **`idle`:** open only while no build is in flight and no snapshot runs. While builds are in flight
  the loop is waiting on them, and that time shows as `build`.
- **`page`:** as today, but closed only while no build is in flight. Waiting with builds in flight
  belongs to them.
- **`publish` and `lane_wait`:** per published item, as today. `resolve` too.

## 8. Per-item cost at `builds = 1`

The in-process mocks finish on the first poll, so at `builds = 1` the live path adds, per batch:

- **One allocation:** the boxed `async move` owning the ids. The ids `Vec` comes back with the
  output and is reused.
- **Two atomic operations:** the `Arc` of the builder.
- **One `VecDeque` push and pop.**
- **Neither of these:**
  - no spawn, no task wake and no `Flights` entry: those happen only when the build doesn't finish
    on its first poll;
  - no id hashing: ids are hashed only when another build is in flight.

The loop loses the deadline arm and gains a landing arm whose future is built only when polled, as
stage 2's `next_delivery` is. Counted in-process against `main` at the end of the stage.

`FuturesUnordered` is not used. Spawned builds are awaited through `Flights`
(`sink_engine/flights.rs`, reused), whose per-flight waker polls only the builds that woke.

## Measured

`flusso-bench`, scenario `reference`, scale `default`, release build of `532308cc`, `writes = 4`.
Two interleaved rounds of builds 1, 2, 4, 8, `flusso-bench` and flusso pinned to CPUs 0-3,12-15;
Postgres and OpenSearch unpinned. Other agents were building and testing on the machine: load
average 5–11 in round 1 and 9–27 in round 2.

| builds | backfill docs/s | burst changes/s | latency p50 / p99 ms | peak RSS MiB |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 10,312 · 9,881 | 2,131 · 1,853 | 5.0 / 30.0 · 12.9 / 32.3 | 124 · 97 |
| 2 | 11,662 · 11,671 | 2,321 · 1,588 | 12.7 / 33.5 · 13.9 / 41.5 | 300 · 209 |
| 4 | 11,811 · 10,048 | 1,747 · 942 | 13.2 / 38.5 · 14.7 / 72.7 | 329 · 345 |
| 8 | 11,288 · 9,385 | 1,802 · 823 | 12.8 / 37.4 · 15.6 / 68.3 | 389 · 417 |

**Where the backfill's time went** (round 1; busy/wall, ms per batch):

| builds | wall s | ingest `build` | ingest `idle` | sink `write` (writes = 4) |
| ---: | ---: | --- | --- | --- |
| 1 | 6.01 | 0.88 (21.7) | 0.00 | 3.29 (81.4) |
| 4 | 5.25 | 1.27 (27.4) | 0.39 | 2.24 (48.5) |
| 8 | 5.49 | 1.41 (31.8) | 0.60 | 2.37 (53.5) |

**What it says:**

- **The backfill is sink-bound once builds overlap.** At `builds = 4` the ingest task builds 1.27
  deep, then sits `idle` 0.39 of the wall while the sink drains the backlog. The sink writes about
  2.3 deep with `idle` near 0. That is about +15% backfill throughput at 2 and 4 builds, not the
  prototype's 3×: the prototype built with no sink behind it.
- **A build gets slower under concurrency.** Per batch it went from 21.7 to 27.4 ms (4 builds) and
  31.8 ms (8). Postgres, OpenSearch and flusso share the 8 pinned threads plus the rest of a loaded
  machine.
- **Peak RSS rises:** 124 → 300–417 MiB. Built batches outrun the sink and wait in its lane, up to
  the lane's `capacity`, because nothing yet bounds built bytes. Stage 5's budget is what caps this.
- **Burst and latency.** The burst stays resolve-bound: ingest `resolve` is 0.97 of the wall at every
  setting. Its round-2 drops (942 and 823 changes/s) ran at load 21–27. Latency p50 at `builds = 1`
  was 5.0 then 12.9 ms across the two rounds, so the p50 spread here is noise, not the setting.

**Per-item cost at `builds = 1`**, counted in-process: 20,000 changes, `max_changes = 1`, one
sink, instant mocks, a counting global allocator and a poll-counting wrapper on the ingest run.

| | `origin/main` (`ffd54d22`) | branch (`532308cc`) |
| --- | ---: | ---: |
| allocations per change | 41.16 | 42.16 |
| ingest polls per change | 0.016 | 0.016 |

The one extra allocation per batch is the boxed build future, as designed. There is no spawn, no
extra poll and no hashing at `builds = 1`. Locks were not counted. The CI gate's timing is the
verdict.

## Found while measuring

The first scenario run failed its latency phase. At `builds = 1` a spawned live build was never
counted as landed, so after the first one no partial live batch was granted again on an open
stream. Unit tests missed it twice over: their mocks finish inline, and their streams end, where
`settle` grants anyway. Fixed in `6c271ed5`, with the red test
`changes_apart_on_an_open_stream_each_land_after_a_spawned_build` first.

## For the next stages

**Stage 5 (memory budget).**

- Concurrent builds turn a sink-bound backfill into built batches waiting in the lane: peak RSS
  roughly tripled at the default `builds = 4`.
- The reorder buffer adds at most `2 × builds` built batches. The lane's `capacity` is the real
  holder.
- The grant is the natural place for the budget check: `Builds::choose`, beside `has_room`.

**Stage 8a (index tasks).**

- `Builds` is per engine today; per index it moves into each index task.
- Admission (sorted id hashes per running build) and the grant-order queue carry over unchanged.
- The live-first rule and its every-fourth counter are per gate. A deployment-wide gate across
  index tasks needs the counter shared.
- The `2 × builds` bound becomes per index.

## After the review

**The per-item cost holds only for builds that finish on their first poll** (review item 5). The Postgres builder never does, so every batch, a lone change included, is spawned. Each one then pays:

- the task;
- the landing box;
- the `Woken` `Arc`;
- the `Flights` entry;
- one extra wake-up hop.

Counted with the live-path counter (20,000 changes, `max_changes = 1`, one sink, `builds = 1`, no budget). The second build's `build_many` yields once, as a query does. Branch head is `87d4f247`, which includes stage 7.

| per change | `main` (`ffd54d22`) | branch, spawned | branch, polled in place |
| --- | ---: | ---: | ---: |
| allocations, build finishes on its first poll | 41.16 | 57.48 | 57.48 |
| allocations, build pending on its first poll | 41.00 | 67.00 | 59.46 |
| ingest-task polls, build pending on its first poll | 1.000 | 1.000 | 0.016 |

- The +16 allocations in the first row came after stage 4's own count of 42.16 at `532308cc`, so they are most likely stage 7's per-lane split; I have not counted that separately.
- Spawning costs about 7.5 allocations per batch over polling in place, plus the hop.

**What spawning buys**, measured (`reference` and `complex`, builds 4, writes 4, two interleaved rounds, load 6–20):

| | `reference` backfill docs/s | build ms/batch | `complex` backfill docs/s | build ms/batch |
| --- | --- | --- | --- | --- |
| spawned | 15,139 · 11,478 | 18.8 · 27.2 | 2,238 · 2,643 | 49.3 · 50.2 |
| in place | 15,151 · 11,213 | 24.0 · 34.2 | 2,558 · 2,392 | 72.4 · 69.1 |

- Each build is 22–45% faster spawned: its decode, coercion and scripts run on other cores.
- End to end, the backfill is the same, because the sink bounds it (stage 4's attribution).
- **Kept spawning.** Its cost is a few µs per batch against a build of milliseconds. Its gain shows as soon as the sink stops being the bound: track A's seeding work, stage 7's per-lane writes, 8a's index tasks. Revisit if the in-process gate disagrees.

**Batch cutting differs from `main`** (review item 6). Resolving now overlaps the build, and the pending batch is granted the moment the previous build lands. So in a resolve-bound burst a batch holds what resolved during one build.

Measured: `reference`, default scale, `writes = 4`, three interleaved rounds, load 4–11 (another track's e2e ran between rounds; under the asked-for ~6 for part of it). Branch head is `87d4f247`:

| | burst changes/s | builds per burst | changes per build | sink writes per burst | docs per write | `resolve` busy s | p50 / p99 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `main` | 1,676 · 1,511 · 1,704 | 198–213 | 94–101 | 199–213 | 120–129 | 9.3–10.5 | 5.6–12.7 / 35–42 |
| branch, builds 1 | 1,767 · 1,593 · 1,878 | 237–273 | 73–84 | 696–811 | 31–37 | 11.5–13.7 | 12.2–13.1 / 32–50 |
| branch, builds 4 | 1,650 · 1,650 · 1,737 | 248–264 | 76–81 | 739–788 | 33–35 | 12.4–13.1 | 12.6–12.9 / 35–42 |

- **About 20% more, smaller builds, as the reviewer said.** The burst drain is not lower for it: median 1,676 on `main`, 1,767 at builds 1, 1,650 at builds 4.
- **`resolve` itself got slower.** It takes about 27% more busy time per change, because it now overlaps the builds against the same Postgres. The overlap gives that back in wall time.
- **Smaller writes are stage 7's.** The sink's writes per burst went up 3.5× and its documents per write down 4×, from one lane per `(index, sink)`. Track B's `fa6a2071` merges ready items for that.
- **No batch-filling rule added.** Smaller batches cost no throughput here, so #135's semantics stay as they are.
