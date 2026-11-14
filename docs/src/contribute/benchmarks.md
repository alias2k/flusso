# Benchmarks

flusso's performance is measured by **scenarios** that run the real binary against Postgres and OpenSearch containers, backed by **component benches** that say which stage moved. Every push to `main` records a data point; a PR is gated by the in-process benches alone.

The vocabulary (benchmark, component bench, scenario, headline metric, baseline, regression, soak) is in [`CONTEXT.md`](https://github.com/alias2k/flusso/blob/main/CONTEXT.md); the decision record is [ADR 0006](https://github.com/alias2k/flusso/blob/main/docs/adr/0006-benchmarks-real-binary-scenarios-with-published-history.md).

## The three layers

| Layer | What runs | Needs | Where | Job |
| --- | --- | --- | --- | --- |
| Scenarios | the shipped `flusso` binary as a child process, a seeded store, a seeded change writer | Docker | `dev/bench` (`flusso-bench`) | `main` pushes |
| Component benches, Docker-backed | Criterion over a live Postgres / OpenSearch: document build and resolve, bulk indexing, the full pipeline's change and burst paths | Docker | `benches/` in `source-postgres`, `sink-opensearch`, `engine` | `main` pushes |
| Component benches, in-process | Criterion with no I/O: the two engines over the channel stream with mocks, pgoutput decode over a recorded fixture, sink rendering, script fields over built documents | nothing | `benches/engine.rs`, `benches/pgoutput.rs`, `benches/render.rs`, `benches/scripted.rs` | every PR, A/B |

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
just bench-compare main pr         # fail past 10% slower (CI's first pass; CI then re-measures a breach)
just bench-components              # the Docker-backed component benches
just bench-scenario reference ci   # a scenario; also `complex`, and `default` scale
just load                          # the soak tool: watch it, it produces no comparable number
```

`bench-scenario` starts its own containers, builds a release `flusso` without the designer (or takes `FLUSSO_BIN`), and writes `target/bench/<scenario>-<scale>/{smaller,bigger,summary}.json`. `BENCH_PG_URL` and `BENCH_OS_URL` point it at an existing pair instead; the Postgres must have `wal_level = logical` and both must be empty.

To compare a branch by hand: run `just bench main` on `main`, `just bench pr` on the branch, then `just bench-compare main pr`. Criterion keeps both under `target/criterion`.

## What CI does

`.github/workflows/bench.yml` has two jobs.

- **On a pull request**, `in-process` checks the PR's base out into a second worktree, runs the four in-process benches there saving baseline `base` (a base that predates the script bench runs the other three), then runs them on the PR as `pr`. Both sides run on the same runner, each from its own target directory, and write their baselines to one Criterion directory (`CRITERION_HOME`).
  - Each tree needs its own target directory. With one shared directory, cargo treats the base's build as fresh for the PR tree (a workspace member's artifacts are keyed by its path inside the workspace, and freshness is by modification time) and the PR side runs the base's binary. Before comparing, `bench-provenance.py` checks that each side ran executables from its own target directory and prints both sides' sha256 per bench. Identical hashes alone are not an error: a PR that changes only tests or comments compiles to the same bench binary.
  - A base with none of the in-process benches only warns: there is nothing to gate against yet. Any other failure on the base fails the job, and so does a comparison that finds no bench measured on both sides.
  - A bench whose median is more than 10% slower is a **breach**, not yet a failure. A shared runner still has noisy moments, and one sample past 10% is often just that.
  - `.github/scripts/bench-gate.sh` re-measures each breach on its own (Criterion's `--exact` filter on its full id, so `live_drain/sinks1/6` never runs `…/64`), base then PR, one bench at a time, saved as `base-confirm` and `pr-confirm`. Both trees and both target directories are kept, so cargo finds both builds fresh, and the provenance check proves each re-run executed the same binary as its first pass.
  - A breach is confirmed, and fails the job, when the re-run is past 10% or the mean of the first-pass and re-run changes is; it clears only when both are within 10%. A breach left unmeasured, because a re-run failed or the 600 s budget (`BENCH_CONFIRM_BUDGET_SECS`, checked before each benchmark) ran out, counts as confirmed: if most of the suite breaches, the gate re-measures it all and a broken runner still goes red.
  - The log and the step summary show the first-pass table, then one row per re-measured bench with the first-pass change, both re-run medians, the re-run change, the mean and the verdict.
- **On a push to `main`**, `scenarios` runs the in-process benches, the Docker-backed component benches, and both scenarios at `ci` scale, converts everything to one list of data points, and stores them with `github-action-benchmark` on the `benchmarks` data branch. The Pages workflow folds that branch into the site at [`/bench/`](https://alias2k.github.io/flusso/bench/).

## What counts as a regression

The storage action's own alerting is off. `.github/scripts/bench-regression.py` reads the history after every `main` run and decides. The hard part is the runners: GitHub runs `main` on a few kinds of machine, and each kind moves the numbers its own way. On one kind nearly every series is 40% faster; on another the Postgres benches are 20% faster while the OpenSearch ones are 20% slower. Compared to its own recent median, a series swings by 10–30% from run to run with no code change behind it. Compared to earlier runs on the same kind of machine, most series swing by under 6%.

### Matching runs

Two runs *match*, for a series, when they ran on the same kind of machine:

- **With runner facts** (every point stored since #317): the same CPU model and core count, and calibrations within 5% (median over them). `bench-runner-facts.py` records these in each point's `extra`. The calibrations are fixed work that runs no flusso code and loads no flusso dependency: a SHA-256 pass over 64 MiB, a 256 MiB memory copy, and a synced 64 MiB disk write.
- **Without them** (the points stored before): the series of the *other* suites within 5%, as the median over them. The suites are `inprocess`, `components` and the scenarios. A step confined to one suite can't move the match its own series are judged by. A step that moves most series of every suite at once can, and only runner facts tell it apart from a slower machine.

### The rule

1. **Same-runner comparison.** A point is compared to the median of its series over its matching runs among the last 40. This needs at least three matching runs and five earlier same-runner comparisons. It is *real* past three swings. The swing is the median change of that comparison from one point to the next, over √2: the median absolute deviation for steady noise, which a step doesn't inflate.
2. **All-runners comparison.** Every other point is compared to the median of its series over the last 40 points on any runner, and is *real* when it is past every one of them, a move larger than anything the runners did. This needs two earlier points, so a new series is judged from its third run. Points already past everything (the run of them the point belongs to) are left out.
3. **Regression.** The two newest judged points are each real, and each worse than its comparison by more than 25% (20% for peak RSS).
4. **Hold.** When at least one of those two points is a same-runner comparison, the series is held from the start of its run of real points:
   - later points are compared to points from before the step, matching runs of any age or the last 40 on any runner;
   - the swing stops taking in points;
   - the hold ends when three judged points in a row are same-runner comparisons each within half the threshold (12.5%, or 10% for RSS). It also ends on an image-tag change, when the stored history (200 runs) no longer reaches back before the step, or when the series is missing from the newest three runs.

   A regression seen only across all runners isn't held: across all runners there's no level to hold to, since a new kind of machine also moves past everything before. It's reported while it lasts.
5. **Image-tag change.** A series restarts on a new image, and only points on the new image count. With no same-runner history on it yet, it's judged across all runners from its third run on the new image.

A finding names the basis it was judged against. For a hold, it also names the last same-runner run before the step that wasn't past the swing, and the first run of the step ("after X, by Y"), or "at or before Y" when no earlier run was judged.

### What it can't see

- **A step on every suite at once, before runner facts.** It moves each run away from its matches, so those runs are judged only across all runners: a doubling is caught, a 30% step isn't, and nothing is held. With runner facts it's caught and held like any other.
- **The first runs of a new image.** A step landing on the new image's first run becomes its baseline. A step on its second run is caught only if it's large (about ×2), because the comparison has one run before it. From the third run on, a doubling is reported within three runs. A lasting step there is held only once three same-runner runs exist on the new image.
- **Series too noisy for the threshold.** When three swings are more than the threshold, a step just past the threshold can't count as real. Every run prints these series with the smallest step each can still catch. On the `ci` scale history as of November 2026 they are the two visible p99 latencies (about +190% and +210%), `bulk_index/1` (+59%), `bulk_index/100` (+39%), `complex/ci/flush_p50_ms` (+30%) and the two `flush_p99_ms` (+28%).

> **Note:** "too noisy" is a fact about the `ci` scale, not about flusso. A longer confirmation (more runs in a row) would make these series slower to call and pass more of their spikes; more samples per run is the real fix.

There's no switch to accept a slowdown on purpose: a held series stays listed until it recovers, its image changes, or the stored history no longer reaches back before it.

The script's tests replay it over a snapshot of the real history (`.github/scripts/fixtures/bench-history.json`), and CI's `scripts` job runs them:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

When a series is flagged, the script opens one issue, "Performance regression on main", and keeps it current on later runs. The issue says which basis each finding used, whether it's held, the last run it was judged at, where its step started, and how the newest run was judged. The script closes the issue when nothing is flagged. An unreadable data file fails the step and leaves the issue alone. The verdict also goes to the `scenarios` job's summary.

## Re-recording the pgoutput fixture

The decode bench reads `libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin`: the raw `XLogData` payloads a Postgres 16 sent for a fixed set of inserts, updates, deletes, and a truncate across tables keyed by `int`, `bigint`, `uuid`, and a composite key. Re-record it only when the decoder's input needs to change:

```sh
cargo nextest run -p flusso-source-postgres --test record_pgoutput --run-ignored all
```

Review the size, then commit. A new fixture is a new baseline for that bench.

## Related

- [Testing](testing.md), for the suites that run beside the benches.
- [Metrics](../reference/metrics.md), for the `/metrics` series the harness reads.
