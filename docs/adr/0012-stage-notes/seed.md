# Seeding: cheaper `mark_seeded` and generation creation (#410, #403), and seeding indexes at once

Track A, prefix `seed:`. Three parts:

1. Where `mark_seeded`'s time goes, and what made it cheaper (#410). Built: `d73e5aac`.
2. Creating a generation with the aliases it may carry (#403). Built: `d73e5aac`.
3. Seeding several indexes of one sink at once. Designed here. It needs a sink-engine change (track B's file), described in §3.4.

## 1. Where `mark_seeded`'s time went (#410)

`mark_seeded` used to send six requests, one after another, per index:

1. refresh the generation;
2. restore its `refresh_interval`;
3. look up the alias holders and the generations;
4. move the aliases;
5. delete the old generations;
6. write the marker.

Measured on base `01e7c7dc`, with a debug line timing each step. Three runs of each scenario at `--scale default`, pinned to cores 0-3,12-15, load 3 to 15. Times are milliseconds per index:

| Scenario | refresh | settings | lookups | aliases | delete | marker | seed, whole backfill |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| reference | 1–20 | 20–54 | 4–32 | 1–4 | 0 | 3–8 | 0.13–0.21 s |
| complex | 28–405 | 41–70 | 27–34 | ~5 | – | 8–11 | 0.11–0.53 s |

- **The settings update is the steady cost.** It is a cluster-state change: about 20 ms even on an idle cluster.
- **The refresh is the variable one.** It costs what the generation holds: up to 405 ms on `complex`.
- Every other step is a few milliseconds. The six run in sequence, so they add up.
- The 0.87 s that #409 reported is from before stage 2. That is an inference from its date and its numbers. None of these runs came near it.

## 2. What changed (`d73e5aac`)

`mark_seeded` (`libs/2-adapters/sink-opensearch/src/sink_impl.rs`) now:

1. Runs the refresh, the settings restore and the lookups **at once** (`tokio::try_join!`). The step costs the slowest of the three.
2. Sends **one** `_aliases` call. It adds both aliases to the new generation, removes them from the old holders, and drops the stale generations with `remove_index`. Readers see the old generation or the new one, never neither. The separate delete is gone.
   - If that call is refused (the convenience alias's name is held by an index, or a stale generation vanished meanwhile), the sink reads the holders again and moves the hash alias alone. The convenience alias stays best-effort, as before.
3. Writes the marker **last** (#354). A failure anywhere before it leaves the index unseeded, and the next start runs the idempotent backfill again.

`ensure_index` (#403):

- The **first** generation is created with both aliases in its create request. The convenience alias is left out when an index already holds it.
- A **rebuild's** generation is created bare. The old generation keeps serving until `mark_seeded` swaps the aliases.
- `create_index` reports whether it created the index, so a generation that already existed takes the old path.

Guards: `tests/seeding.rs`.

- `the_seed_marker_waits_until_the_old_generation_is_gone` (#354)
- `a_crash_after_any_request_of_mark_seeded_recovers_on_restart`: the sink dies after its *n*th request, for every *n*.
- `a_first_generation_is_created_with_both_aliases_in_one_request`
- `a_rebuilding_generation_is_created_without_aliases`
- `the_convenience_alias_another_schema_holds_is_moved_never_shared`
- `a_rebuild_swaps_both_aliases_and_drops_the_old_generation_in_one_request`
- `several_indexes_seed_at_once_beside_writes_to_others` (§3)

The unit tests for `plan_seed_actions` and `holders_of` are in `generations/tests.rs`.

### Measured before and after

See §4.

## 3. Seeding several indexes at once

Track C measured the backfill at `builds = 4`. The sink writes about three deep with no idle time. Then the `seed` steps run one after another: 27% of the backfill's wall time on `reference`.

Two things make the seeds serial:

- **The sink engine awaits `mark_seeded` inside its run loop.** `grant` calls `complete_snapshot` and waits for it (`libs/2-engine/src/sink_engine/writes.rs`). While a seed runs, nothing else happens on that sink: no write starts, and no write that returns is handled.
- **The seeds queue behind each other.** Since stage 7 each index's `SnapshotComplete` is an item on its own lane. Yet the engine still runs at most one seed at a time, and only between writes.

### 3.1 What `mark_seeded` needs to itself

It needs **its own index** to itself, and nothing more.

- **The marker** is one meta document per index, with the hash alias as its id (`generations.rs`'s `write_index_meta`). Two indexes never write the same document, so there is no write race.
- **The aliases and generations** an index's seed moves or drops are its own: `{prefix}{logical}_{hash}`, its generations `{hash alias}_{n}`, and `{prefix}{logical}`. A sink holds each logical index once, so the convenience aliases of two of its indexes differ.
- **Each `_aliases` call is atomic.** OpenSearch applies cluster-state updates one at a time. Two calls touching disjoint aliases and indexes don't interact.
- **The refresh and the settings update** are per generation.
- **The sink's in-memory state** is the `targets` map, behind a mutex, one entry per index.
- **A caught-up write of another index** may refresh this generation too, if earlier backfill writes left it unrefreshed. That costs one more refresh. The refresh skips missing indexes (`ignore_unavailable=true`), so a generation the seed just dropped fails nothing.

What still must never overlap, as today:

- a seed of an index beside a write of **the same** index: lane order already prevents it;
- a seed beside staging (`ensure_index`, `is_seeded`, `reindex`): staging waits for what is in flight.

`several_indexes_seed_at_once_beside_writes_to_others` pins the concurrent case. It runs on one sink, in one go:

- four rebuilt indexes and four first seeds call `mark_seeded`;
- at the same time, two other indexes take 20 writes each.

Every seeded index must end with only its new generation, both aliases on it, its marker seeded and its documents searchable. Every write must be found.

### 3.2 The bound

A seed takes **one of the `writes` slots** while it runs. A sink then never has more than `writes` operations against the cluster at once: writes and seeds together.

- A separate bound would let the seeds at the end of a backfill add to a full set of writes.
- A seed holds a slot for its slowest step, the refresh: tens of milliseconds, up to about 0.4 s on `complex`. When it ends, writes take the slot back.

### 3.3 What it buys

The seeds overlap each other, and they overlap other lanes' writes.

- **On `reference`:** about 0.1–0.2 s of serial seeding becomes about the slowest index's seed.
- **For a lane that finishes early,** its seed now runs during the other lanes' backfill instead of after it.

I expect most of the 27% to go. Measured numbers come after the engine change, in §4.

### 3.4 The engine change (track B's file, `sink_engine/writes.rs`)

What the sink engine needs to do:

1. **A seed runs in flight.** Once a lane's `SnapshotComplete` has nothing before it on its lane, waiting or in flight, `mark_seeded` starts as a flight, like a write. It is not awaited inside `grant`. Other lanes keep writing, landing and seeding.
2. **Several lanes' seeds may be in flight at once.** Each holds one write slot until it returns.
3. **The lane stays blocked until its seed returns.** Then the seed is acknowledged, the index leaves `outstanding`, `on_index_seeded` fires, and the `Stage::Seed` timing is reported for its index. That is what `complete_snapshot` does now, moved to when the seed returns.
4. **A reindex waits for seeds in flight**, as it waits for writes. Staging never runs beside `mark_seeded`.
5. **A stop:** a seed in flight finishes, like a write in flight. A `SnapshotComplete` whose backfill a stop cut short is not seeded, as today.
6. **A failure:** a seed that fails fails the run, like a failed write. The flights beside it are awaited, and its `SnapshotComplete` stays unacknowledged. The lane hands it back and `mark_seeded` runs again; it is idempotent, which the crash matrix pins.
7. **A seed in flight is busy time, not `idle`.**

The port doc in `libs/1-ports/sink/src/sink.rs` changes with it:

> `mark_seeded` may run at once for different indexes, and beside `write` for other indexes; never beside a write, a `mark_seeded` or staging of the same index, and never beside staging.

The OpenSearch sink needs no change (§3.1). The stdout sink's `mark_seeded` is the default no-op.

## 4. Measurements

Before is base `01e7c7dc`; after is the same base plus `d73e5aac`. Both are release builds, with a debug line timing each step.

Each run was pinned to cores 0-3,12-15. The two builds alternated: before, then after, for each scenario and round. The load average was between 6.8 and 23.8; other tracks were building.

### Backfill seeding, `--scale default`, three rounds

Time is the seed stage's busy seconds across all of a run's indexes. The share is that time over the backfill's wall time, from `attribution.md`.

| Scenario | Build | r1 | r2 | r3 | Median |
| --- | --- | ---: | ---: | ---: | ---: |
| reference (3 indexes) | before | 472 ms (5.4%) | 135 ms (1.9%) | 195 ms (2.9%) | 195 ms |
| reference | after | 159 ms (2.2%) | 102 ms (1.3%) | 155 ms (2.1%) | 155 ms |
| complex (1 index) | before | 508 ms (20.3%) | 75 ms (2.9%) | 1,071 ms (34.7%) | 508 ms |
| complex | after | 664 ms (23.0%) | 85 ms (2.9%) | 344 ms (15.9%) | 344 ms |

Per index, after the change:

- The concurrent step (refresh + settings + lookup) took 27–64 ms on `reference`. Before, the same three steps in sequence took 30–110 ms when the refresh was cheap.
- The `_aliases` call is skipped on a first seed: the aliases were set at create, so there is nothing to move.
- The marker took 4–12 ms.

**The refresh decides `complex`.** It took 3 ms to 939 ms for the same index on the same build: what OpenSearch still has to flush after the bulk writes. Running the steps at once hides the other steps behind it, but it can't shrink it. At three rounds, `complex` doesn't settle whether `after` is faster. `reference` is about 40 ms faster per backfill.

### Start-up, 24 indexes (`flusso-bench --startup 8`, `reference` at `ci` scale), two interleaved runs of three rounds

| Build | fresh, 6 starts | fresh median | restart median |
| --- | --- | ---: | ---: |
| before | 4.57, 1.82, 2.20, 2.42, 1.69, 1.71 s | 1.95 s | 0.21 s |
| after | 1.50, 1.16, 1.19, 2.34, 1.44, 1.35 s | 1.39 s | 0.25 s |

- **A fresh start is about 0.55 s faster with 24 indexes.** Almost all of it is the `stage primary` step, where generations are created: one create request now carries the aliases, instead of a create followed by an alias call.
- **A restart is unchanged.** It creates nothing. The 0.04 s gap is within the restarts' spread of 0.19–0.39 s.

### Seeding at once (§3)

Not measured: it needs the engine change in §3.4. Track C measured 27% of `reference`'s backfill wall time in serial seeding at `builds = 4` on the branch. That is the number to compare against once it lands.
