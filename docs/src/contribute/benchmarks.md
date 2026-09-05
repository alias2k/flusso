# Benchmarks

flusso's performance is measured by **scenarios** that run the real binary against Postgres and OpenSearch containers, backed by **component benches** that say which stage moved. Every push to `main` records a data point; a PR is gated by the in-process benches alone.

The vocabulary (benchmark, component bench, scenario, headline metric, baseline, regression, soak) is in [`CONTEXT.md`](https://github.com/alias2k/flusso/blob/main/CONTEXT.md); the decision record is [ADR 0006](https://github.com/alias2k/flusso/blob/main/docs/adr/0006-benchmarks-real-binary-scenarios-with-published-history.md).

## The three layers

| Layer | What runs | Needs | Where | Job |
| --- | --- | --- | --- | --- |
| Scenarios | the shipped `flusso` binary as a child process, a seeded store, a seeded change writer | Docker | `dev/bench` (`flusso-bench`) | `main` pushes |
| Component benches, Docker-backed | Criterion over a live Postgres / OpenSearch: document build and resolve, bulk indexing, the full pipeline's change and burst paths | Docker | `benches/` in `source-postgres`, `sink-opensearch`, `engine` | `main` pushes |
| Component benches, in-process | Criterion with no I/O: the two engines over the channel stream with mocks, pgoutput decode over a recorded fixture, sink rendering | nothing | `benches/engine.rs`, `benches/pgoutput.rs`, `benches/render.rs` | every PR, A/B |

The scenario numbers are what gets watched. The component benches exist so that when a headline metric moves, the stage that moved is already on record for the same commit.

## Headline metrics

A scenario reports these under the series `<scenario>/<scale>/…`:

| Metric | Unit | Direction | How it's measured |
| --- | --- | --- | --- |
| `visible_latency_p50_ms`, `visible_latency_p99_ms` | ms | smaller | A paced trickle of changes; each stamped root-row update is timed from commit until a search on the index's alias returns the document with that stamp. Refresh included: this is what a user sees. Only root-row updates carry the stamp, so related-table changes are in the mix but not timed. |
| `drain_changes_per_s` | changes/s | bigger | Several writers commit a fixed burst as fast as Postgres accepts. Time runs from the first write until `changes_captured` covers every row written and every sink has nothing in flight. The report also records how long the writers themselves took, so a Postgres-bound run is recognisable. |
| `backfill_docs_per_s` | docs/s | bigger | Documents seeded divided by the time from spawning the binary until `/status` reports `live` with every index seeded. |
| `peak_rss_mib`, `cpu_seconds` | MiB, s | smaller | Sampled from the child with `ps` every half second. |
| `flush_p50_ms`, `flush_p99_ms` | ms | smaller | Attribution, not headline: the sink's flush-duration histogram from `/metrics` at the end of the run. |

Every point carries the container image tags and the scale parameters in its `extra` field.

## Scenarios and scales

| Scenario | Store | Change mix |
| --- | --- | --- |
| `reference` | the dev store (`dev/*.schema.yml`, three indexes), seeded deterministically | 70% root updates across users / products / orders, 20% related-table updates (line items, reviews, addresses, profiles), 10% inserts and deletes including soft-delete toggles |
| `complex` | one worst-case `users` document: 1:1, 1:N with a nested 1:N, M:N through a junction, seven aggregates, an object, transforms, a default, a constant, soft-delete | 40% line-item updates (the multi-hop resolve), 30% orders, 20% users (the timed change), 10% junction inserts and deletes |

Two scales, `ci` and `default`; the presets are in `dev/bench/src/scale.rs`. The scale is part of the series name, so a CI point and a local point are never compared. The seed derives every value from the row index, no `random()`, so a scale yields the same dataset every run. The dev root tables carry `updated_at` for this: the writer stamps it and the document exposes it as `updatedAt`.

Caps: a wall-clock cap for the whole run and an RSS cap on the child. Either one failing writes `failure.json` with the phase and reason and turns the job red.

## Running locally

```sh
just bench                         # in-process benches, no Docker, ~2 minutes
just bench main                    # same, saved as Criterion baseline `main`
just bench-compare main pr         # fail past 10% slower (what CI does for a PR)
just bench-components              # the Docker-backed component benches
just bench-scenario reference ci   # a scenario; also `complex`, and `default` scale
just load                          # the soak tool: watch it, it produces no comparable number
```

`bench-scenario` starts its own containers, builds a release `flusso` without the designer (or takes `FLUSSO_BIN`), and writes `target/bench/<scenario>-<scale>/{smaller,bigger,summary}.json`. `BENCH_PG_URL` and `BENCH_OS_URL` point it at an existing pair instead; the Postgres must have `wal_level = logical` and both must be empty.

To compare a branch by hand: run `just bench main` on `main`, `just bench pr` on the branch, then `just bench-compare main pr`. Criterion keeps both under `target/criterion`.

## What CI does

`.github/workflows/bench.yml` has two jobs.

- **On a pull request**, `in-process` checks out the PR's base, runs the three in-process benches saving baseline `base`, checks out the PR, runs them again as `pr`, and fails when any bench's median is more than 10% slower. Same runner for both sides, so there is no cross-run noise to argue with.
- **On a push to `main`**, `scenarios` runs the in-process benches, the Docker-backed component benches, and both scenarios at `ci` scale, converts everything to one list of data points, and stores them with `github-action-benchmark` on the `benchmarks` data branch. The Pages workflow folds that branch into the site at [`/bench/`](https://alias2k.github.io/flusso/bench/).

## What counts as a regression

The storage action's own alerting is off. `.github/scripts/bench-regression.py` reads the history and compares each new point to the **median of the previous five** points of the same series. A point is over when it is worse than that median by more than 25%, or 20% for peak RSS. A regression is the newest point being over **and** the one before it being over against its own window, two consecutive runs of `main`. One bad run is noise until the next confirms it. A change in the container image tags restarts the window.

When a regression is found the script opens one issue, "Performance regression on main", keeps it current on later runs, and closes it when every series is back within its threshold.

## Re-recording the pgoutput fixture

The decode bench reads `libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin`: the raw `XLogData` payloads a Postgres 16 sent for a fixed set of inserts, updates, deletes, and a truncate across tables keyed by `int`, `bigint`, `uuid`, and a composite key. Re-record it only when the decoder's input needs to change:

```sh
cargo nextest run -p flusso-source-postgres --test record_pgoutput --run-ignored all
```

Review the size, then commit. A new fixture is a new baseline for that bench.

## Related

- [Testing](testing.md), for the suites that run beside the benches.
- [Metrics](../reference/metrics.md), for the `/metrics` series the harness reads.
