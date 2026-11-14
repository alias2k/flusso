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

The storage action's own alerting is off. `.github/scripts/bench-regression.py` reads the history after every `main` run and decides. The hard part is the runners: GitHub runs `main` on a few kinds of machine, and each kind moves the numbers its own way. On one kind nearly every series is 40% faster; on another the Postgres benches are 20% faster while the OpenSearch ones are 20% slower. Compared to its own recent median, a series swings by 10–30% from run to run with no code change behind it. Compared to earlier runs on the same kind of machine, most series swing by 1–6%.

### Matching runs

Two runs *match* when they ran on the same kind of machine. That is told from evidence no flusso change can move, so a regression can't hide by changing which runs match:

- the CPU model, where both runs recorded one (`bench-runner-facts.py` stamps it into every data point's `extra`);
- the **floors**: `components/baseline/select_1` and `components/baseline/pg_select_1` (a bare `sqlx` `SELECT 1`) and `runner/sha256_64mib` (a SHA-256 pass over 64 MiB, recorded since #317). Two runs match when these differ by less than 5% on average, over the ones both have.

The floors are the runner's evidence, so they are never judged themselves.

### The rule

1. **Comparison.** A point is compared to the median of its series over its matching runs among the last 40. With fewer than three, it isn't judged.
2. **Swing.** A series' noise: the median change of that comparison from one judged point to the next, over the last 20, divided by √2. For steady noise that's the median absolute deviation; a step doesn't inflate it. It takes five judged points.
3. **Regression.** Two judged points in a row are each worse than their matches by more than three swings (*real*), and on average by more than 25%, or 20% for peak RSS (*large*).
4. **Hold.** From then on the series is held at the level before the step, which starts at the first of those points, or earlier if the points before them were already past three swings:
   - every later point is compared to its matching runs from *before* the step, however old;
   - a point with fewer than three of those is skipped. It doesn't end the hold or move it;
   - the swing stops taking in points.

   The hold ends when two judged points in a row are each back within half the threshold (12.5%, or 10% for RSS). It also ends on an image-tag change, or once the stored history (200 runs) no longer reaches back before the step.
5. **Image-tag changes.** The first judged point on a new image measures the image's own step against the old image's matches and isn't judged. From the next judged point on, the series is judged with that step taken out.

So a lasting regression stays in the issue until it goes away; nothing is absorbed quietly. There's no switch to accept one: a slowdown kept on purpose stays listed until an image change, or until the stored history no longer reaches back before it.

### What it can't see

- **The floors and the calibration.** A change that moves them (a `sqlx` or Postgres upgrade, a new GitHub fleet) reads as a new kind of machine. No run matches until three runs of the new kind are stored.
- **Runs on a kind of machine seen fewer than three times in the last 40.** They're skipped. The output says how many series the newest run was judged on, and a finding names the last run it was judged at.
- **The first judged run on a new image.** A regression landing in the same run as an image change is taken for the image's own step. The earliest call is on the image's third judged run, as before the change.
- **Series too noisy for the threshold.** When three swings are more than the threshold, a step just past the threshold can't count as real. Every run prints these series with the smallest step each can still catch. As of November 2026, in the `ci` scale history, they are the two visible p99 latencies (about +180% and +200%), `bulk_index/1` (+57%), `bulk_index/100` (+39%), the two `flush_p99_ms` (+30–38%), `change_burst/1` (+31%) and `reference/ci/peak_rss_mib` (+24% against its 20%). Their headline stages are covered by the p50 and the component benches.

> **Note:** "too noisy" is a fact about the `ci` scale, not about flusso. A longer confirmation (more runs in a row) would make these series slower to call and pass more of their spikes; more samples per run is the real fix.

The script's tests replay it over a snapshot of the real history (`.github/scripts/fixtures/bench-history.json`), and CI's `scripts` job runs them:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

When a series is held, the script opens one issue, "Performance regression on main". It keeps the issue current on later runs, with the last run each finding was judged at and the run its step started. It closes the issue when no series is held. The verdict also goes to the `scenarios` job's summary.

## Re-recording the pgoutput fixture

The decode bench reads `libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin`: the raw `XLogData` payloads a Postgres 16 sent for a fixed set of inserts, updates, deletes, and a truncate across tables keyed by `int`, `bigint`, `uuid`, and a composite key. Re-record it only when the decoder's input needs to change:

```sh
cargo nextest run -p flusso-source-postgres --test record_pgoutput --run-ignored all
```

Review the size, then commit. A new fixture is a new baseline for that bench.

## Related

- [Testing](testing.md), for the suites that run beside the benches.
- [Metrics](../reference/metrics.md), for the `/metrics` series the harness reads.
