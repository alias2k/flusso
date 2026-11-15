# Benchmarks

flusso's performance is measured by **scenarios** that run the real binary against Postgres and OpenSearch containers, backed by **component benches** that say which stage moved. Every push to `main` records a data point; a PR is gated by the in-process benches alone.

The vocabulary (benchmark, component bench, scenario, headline metric, baseline, regression, soak) is in [`CONTEXT.md`](https://github.com/alias2k/flusso/blob/main/CONTEXT.md); the decision record is [ADR 0006](https://github.com/alias2k/flusso/blob/main/docs/adr/0006-benchmarks-real-binary-scenarios-with-published-history.md).

## The three layers

| Layer | What runs | Needs | Where | Job |
| --- | --- | --- | --- | --- |
| Scenarios | the shipped `flusso` binary as a child process, a seeded store, a seeded change writer | Docker | `dev/bench` (`flusso-bench`) | `main` pushes |
| Component benches, Docker-backed | Criterion over a live Postgres / OpenSearch: document build and resolve, bulk indexing, the full pipeline's change and burst paths; the review cases (`cases`) | Docker | `benches/` in `source-postgres`, `sink-opensearch`, `engine` | `main` pushes; `cases` only locally |
| Component benches, in-process | Criterion with no I/O: the two engines over the channel stream with mocks, pgoutput decode over a recorded fixture, resolving the changes that need no query (root table, unread table, update of unread columns, a path walked from the key) over a builder whose pool never connects, sink rendering, script fields over built documents | nothing | `benches/engine.rs`, `benches/pgoutput.rs`, `benches/resolve.rs`, `benches/render.rs`, `benches/scripted.rs` | every PR, A/B |

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
just review-ab origin/main         # A/B the working tree against a base on those benches (see below)
just bench-scenario reference ci   # a scenario; also `complex`, and `default` scale
just load                          # the soak tool: watch it, it produces no comparable number
```

`bench-scenario` starts its own containers, builds a release `flusso` without the designer (or takes `FLUSSO_BIN`), and writes `target/bench/<scenario>-<scale>/{smaller,bigger,summary}.json`. `BENCH_PG_URL` and `BENCH_OS_URL` point it at an existing pair instead; the Postgres must have `wal_level = logical` and both must be empty.

To compare a branch by hand: run `just bench main` on `main`, `just bench pr` on the branch, then `just bench-compare main pr`. Criterion keeps both under `target/criterion`.

## Comparing a branch with its base

`just review-ab` measures the working tree against a base on the Docker-backed benches. Each side runs several times so the noise is measured too, and a case only counts as slower once a re-run confirms it. It's for reviewing a change to the source, the engine or the sink, where the in-process gate can't see the database.

```sh
just review-ab                                      # base origin/main, Postgres 16, every bench
just review-ab main --pg 14,16,18                   # one table per Postgres version
just review-ab main --bench cases --filter '^key/'  # one bench, a Criterion filter
just review-ab --clean                              # delete the warm base build and the results
```

| Flag | Default | Meaning |
| --- | --- | --- |
| `<base>` | `origin/main` | any commit-ish, as your clone has it: the command never fetches |
| `--pg` | `16` | Postgres versions, comma-separated (`postgres:<v>-alpine`) |
| `--bench` | `postgres,cases,pipeline` | bench targets; only these are built; `pipeline` also starts an OpenSearch |
| `--filter` | none | a Criterion filter (a regex over the case id) |
| `--limit` | `5` | the smallest change, in percent, that counts |
| `--rounds` | `3` | runs per side |
| `--own-benches` | off | run each side's own bench sources instead of the head's on both |
| `--bench-cpus`, `--server-cpus` | from `nproc` | core lists (`taskset -c` syntax) for the bench process and the containers; also `REVIEW_AB_BENCH_CPUS`, `REVIEW_AB_SERVER_CPUS` |
| `--no-pin` | | run unpinned; also `REVIEW_AB_PIN=0` |
| `--clean` | | remove `~/.cache/flusso/review-ab` and `target/review-ab`, then exit |

The first line names the base commit and its commit date. The command doesn't fetch, so `origin/main` is whatever was last fetched: run `git fetch` first when the latest `main` matters.

What it does:

1. **Runs one at a time.** It takes an exclusive lock on `~/.cache/flusso/review-ab/lock` for the whole run; a second run fails at once. Containers a killed run left behind (`flusso-review-<pid>-*` whose process is gone) are removed first.
2. **Builds the base in a warm tree.** `~/.cache/flusso/review-ab/tree` (or `$FLUSSO_REVIEW_HOME`) is the base checked out with a private git index: no worktree is registered, and only the files that changed since the last base are rewritten. Before each build it's reset to the base commit. It has its own target dir beside it, seeded once from the head's `release` dir with `cp -a --reflink=auto`: shared blocks on a filesystem with reflinks (btrfs, XFS), a full copy of about 2 GB on one without. One shared target dir would let cargo run one tree's binary for the other; `bench-provenance.py` checks each side ran its own.
3. **Reuses that build when it's current.** `review-ab-stamp.py` records the base commit, the toolchain, `RUSTFLAGS` and `CARGO_PROFILE_*`, `bench-in-process.sh`'s hash, the bench files laid over the base, and each bench executable's sha256. When all of them match, cargo isn't called for the base at all; otherwise the output says why it rebuilds.
4. **Runs the same harness on both sides.** The head's `postgres.rs`, `cases.rs` and `pipeline.rs` are copied over the base, so a base that predates a case is still measured. When a build fails in a bench target, the error suggests `--own-benches`. That flag refuses a base whose benches predate `REVIEW_AB_PG_URL`: they would run on their own testcontainer, Postgres 11, while the head ran on `--pg`.
5. **Starts named containers.** One `flusso-review-<pid>-pg<v>` per version, plus `flusso-review-<pid>-os` for `pipeline`. Each bench run gets a fresh database and an emptied cluster through `REVIEW_AB_PG_URL` / `REVIEW_AB_OS_URL`, variables only this command sets. A trap removes exactly these containers, on success, failure or Ctrl-C.
6. **Pins CPUs.** The bench process runs under `taskset -c` on one set of cores and the containers get `--cpuset-cpus` on a disjoint one, so the server and the client stop trading cores with each other and with the rest of the machine. By default core 0 is left alone and the next 2·k cores are split in two, with k = min(4, (nproc − 1) / 2): 1-4 and 5-8 on a 24-core machine. The output's third line names the cores. Without `taskset`, or with fewer than 5 cores, it runs unpinned and says so. Other processes can still run on those cores; pinning narrows the noise, it doesn't reserve the cores.
7. **Alternates.** Per version, per bench: base, head, base, head, base, head (`--rounds 3`). The order doesn't cancel a drift; the spread check below catches it.
8. **Re-runs what it flags.** A case is flagged when the head's fastest run is slower than the base's by more than `--limit` and more than twice the control spread. Each flagged case then runs again on its own (Criterion's `--exact`), base then head, and the PR gate's rule decides: it's **slower** when the re-run, or the mean of both passes, is past its threshold, and **flagged, cleared on re-run** otherwise.
9. **Prints one table per version**, from `bench-compare.py --repeats … --confirm …`:

```text
| case | base (fastest … slowest) | head (fastest … slowest) | change | control spread | verdict |
| --- | ---: | ---: | ---: | ---: | --- |
| `key/uuid` | 1.234 ms … 1.256 ms | 1.501 ms … 1.522 ms | +21.6% | 1.8% | slower ⚠️ (re-run +20.9%) |
```

The control spread is the gap between one binary's two fastest runs, as a share of the fastest, the larger of the two sides: the noise of the fastest run, which is what the change compares. With 3 rounds that's the range of all runs but the slowest, so one run hit by a burst doesn't widen it; that run still shows in the base and head columns. More rounds narrow it: the two fastest of more runs sit closer together, and one bad run more doesn't move them. A range over all runs would widen instead. A `faster` isn't re-run, so on a busy machine treat one as a hint, not a result. The command exits 1 only on a case that is **slower**, that is, confirmed by its re-run. When both sides' bench binaries are byte-identical, the output says so: every difference in the table is noise.

### How far to trust it

Measured on a shared development machine with another agent building and testing beside it, Postgres 16, 3 rounds, CPUs pinned:

- **Identical code**, the 22 `cases` (before `root/plain_baseline` was added; load average 2.4 to 14.7): exit 0. Two cases flagged, `key/float8` at +7.9% and `root/wide_composite_key` at +17.3%, and both cleared on re-run. One case, `batch/10`, read `faster` at −20.7%: `faster` isn't re-run.
- **Identical code, all 36 cases, before pinning** (two runs, load 2.9 to 14.6): exit 0 both times, one flag, cleared.
- **A 3 ms sleep added to every `build_many` call**, the 22 `cases` (load 2.6 to 11.9): **slower** on 18, each confirmed by its re-run. That's every case where 3 ms is more than the 5% limit; the true change ranged from +22% (`key/citext`, 13 ms) to +2500% (`batch/1`). The 4 left as `noise` are `batch/10000` and the ~150 ms composite-key, keyless and wide composite-key roots, where 3 ms is +2% to +3%.
- The same slowed run before pinning, with the spread taken over all runs, caught 4 of those 18. On the 4 to 11 ms cases the spread fell from 14%–61% to 1.7%–9.6%.

The smallest slowdown tested and caught on the 4 to 11 ms cases is +28% (`root/hash_partitioned`). Their spreads put the floor at 5% to 19%, depending on the case, but nothing below +28% was tested there. A change smaller than the 5% limit is never reported, and on a case whose spread is wide, the floor is twice that spread: read it in the row.

### When a case you care about reads `noise`

`noise` means "not past max(limit, 2 × spread)", not "unchanged". To look closer:

1. Run that case alone, with more rounds: `just review-ab main --bench cases --filter '^key/uuid$' --rounds 7`. One case takes a minute or two a round. What more rounds buy, simulated on identical code with the noise models of `SpreadSimulation` in `test_review_ab.py` (20,000 trials; the test pins the property with fewer): with 3% Gaussian noise the median spread goes from 3.4% at 3 rounds to 2.7% at 5 and 2.4% at 7. When 30% of runs are hit by a burst of 20–100%, it goes from 6.5% to 3.4% and 2.8%. The first-pass false-flag rate stays under 3% in every case.
2. Read the row's spread. If it's still wide, the machine is the problem: wait for other builds to finish, or move the run to a quieter machine. `--bench-cpus` / `--server-cpus` can steer it away from busy cores.
3. Lower `--limit` only for a case whose spread is well under it, and say so in the review.

What it can't see on a busy machine: a change smaller than the 5% limit; a change on a case whose spread is wide that run; and anything outside the benches, such as planner choices on other data sizes or another Postgres version than the ones asked for (`--pg`).

> **Tip:** a quiet machine matters more than more samples. Another build beside it shows up as a wide control spread, and the table then calls small changes noise, which is the honest answer.

### Disk

| Path | Size | Holds |
| --- | --- | --- |
| `~/.cache/flusso/review-ab/` | about 2 GB | the warm base tree, its target dir, the stamp, the lock |
| `target/review-ab/<timestamp>/` | a few MB each | Criterion output and the tables; the last 5 are kept |

`just review-ab --clean` deletes both. The cache is rebuilt by the next run.

### The review cases

`cases` (in `source-postgres`) is `build_many` over the ids a reviewer of a source change keeps asking about. Keys are read back as the WAL decoder reads them, and each case is checked before it's measured: one that doesn't build the expected number of documents is skipped with a message, so it shows as measured on one side only.

| Group | Cases |
| --- | --- |
| `batch` | 1, 10, 100, 1000, 10000 ids of a plain `int` root |
| `hit_ratio` | 0, 50, 100% of 1000 ids exist |
| `key` | `int`, `uuid`, `varchar`, `numeric`, `float8`, `char`, `citext` |
| `root` | `plain_baseline` (1,000 rows), hash- and range-partitioned on the key, composite key, keyless (`REPLICA IDENTITY FULL`), a wide row with a large unread column, the same with a composite key, a root and child never analysed (autovacuum off, never vacuumed) |

Every root has a `has_many` child keyed by its key type. Read the root shapes against `root/plain_baseline`, a plain 1,000-row root, not against `batch/1000`, which reads 1,000 rows of the 10,000-row `plain` table. `batch/1` is bimodal: on byte-identical binaries it read +29.4% against a 28.6% spread, so it can't show a small change, only a large one such as a round trip added per call. The composite-key and keyless roots declare `primary_key: id`, which isn't the table's key; `flusso run` warns about it and starts, and that warned-but-running shape is what they measure. `cases` runs only from `just bench-components` and `just review-ab`: the `main` history doesn't record it.

## What CI does

`.github/workflows/bench.yml` has two jobs.

- **On a pull request**, `in-process` checks the PR's base out into a second worktree, runs the five in-process benches there saving baseline `base` (a base that predates the script or resolve bench runs the others), then runs them on the PR as `pr`. Both sides run on the same runner, each from its own target directory, and write their baselines to one Criterion directory (`CRITERION_HOME`).
  - Each tree needs its own target directory. With one shared directory, cargo treats the base's build as fresh for the PR tree (a workspace member's artifacts are keyed by its path inside the workspace, and freshness is by modification time) and the PR side runs the base's binary. Before comparing, `bench-provenance.py` checks that each side ran executables from its own target directory and prints both sides' sha256 per bench. Identical hashes alone are not an error: a PR that changes only tests or comments compiles to the same bench binary.
  - A base with none of the in-process benches only warns: there is nothing to gate against yet. Any other failure on the base fails the job, and so does a comparison that finds no bench measured on both sides.
  - A bench whose median is more than 10% slower is a **breach**, not yet a failure. A shared runner still has noisy moments, and one sample past 10% is often just that.
  - `.github/scripts/bench-gate.sh` re-measures each breach on its own (Criterion's `--exact` filter on its full id, so `live_drain/sinks1/6` never runs `…/64`), base then PR, one bench at a time, saved as `base-confirm` and `pr-confirm`. Both trees and both target directories are kept, so cargo finds both builds fresh, and the provenance check proves each re-run executed the same binary as its first pass.
  - A breach is confirmed, and fails the job, when the re-run is past 10% or the mean of the first-pass and re-run changes is; it clears only when both are within 10%. A breach left unmeasured, because a re-run failed or the 600 s budget (`BENCH_CONFIRM_BUDGET_SECS`, checked before each benchmark) ran out, counts as confirmed: if most of the suite breaches, the gate re-measures it all and a broken runner still goes red.
  - The log and the step summary show the first-pass table, then one row per re-measured bench with the first-pass change, both re-run medians, the re-run change, the mean and the verdict.
- **On a push to `main`**, `scenarios` runs the in-process benches, the Docker-backed component benches, and both scenarios at `ci` scale, converts everything to one list of data points, and stores them with `github-action-benchmark` on the `benchmarks` data branch. The Pages workflow folds that branch into the site at [`/bench/`](https://alias2k.github.io/flusso/bench/).

## What counts as a regression

The storage action's own alerting is off. `.github/scripts/bench-regression.py` reads the history and compares each new point to the **median of the previous five** points of the same series. A point is over when it is worse than that median by more than 25% (20% for peak RSS) and by more than three times the window's own median absolute deviation, so a series that swings from run to run needs a move larger than its swings. A regression is the newest point being over **and** the one before it being over against its own window, two consecutive runs of `main`. One bad run is noise until the next confirms it. A change in the container image tags restarts the window.

When a regression is found the script opens one issue, "Performance regression on main", keeps it current on later runs, and closes it when every series is back within its threshold.

Around that rule, without changing what it flags:

- **Malformed entries.** Inside a readable data file, these are skipped with a `warning:` line instead of crashing the check:
  - a value that isn't a number (`true`/`false` still reads as 1/0, as before);
  - a bench without a name or that isn't an object;
  - a point that isn't an object, or whose `benches` isn't a list;
  - `"commit": null`, which is read as an unknown commit.
- **Unreadable history.** A data file that is missing, truncated or not a JSON object fails the step with an `error:` line before `gh` is called, and so does one whose `entries` isn't an object or holds a suite that isn't a list. The issue is never closed on a history that couldn't be read.

### Detection power

How large a lasting step does the rule actually catch? For each series the script measures it:

1. It injects a lasting step into the series' history at each of its last 20 runs, at 1, 1.2, 1.5, 2, 3, 4, 6 and 8 times its threshold.
2. It runs the rule on the step's second run.
3. The series' detection power is the smallest step caught there 9 times out of 10, or "not reliably caught" up to the largest step tried.

The full table, weakest series first, goes to the `scenarios` job summary. Under it, one line counts the series left out: too few runs to measure, or none in the last 50 runs (the rule no longer judges those). The header and the issue give two figures:

- how many series catch a lasting step of 1.5 times their threshold 9 times out of 10;
- the step the median series needs.

The issue gets those two figures in two sentences under the findings, with a link to the run whose summary has the table. A step of exactly the threshold is never the measure: the rule needs a point strictly past it, so such a step is caught about as often as not even on a quiet series.

On the current history a step of 1.5 times the threshold is caught reliably on 1 of 51 series, and the median series needs about +150%:

- the quietest (the `complex` scenario's p50 latency and peak RSS) need +38–40%;
- most series need +75–200%;
- the two p99 latencies, `complex/ci/cpu_seconds` and the four throughput series aren't caught 9 times out of 10 even at the largest step tried (+200%, or a 75% drop).

A silent series isn't one that's fine.

### Runner facts

Every data point records the machine it ran on, in its `extra`: the CPU model, the core count, and three calibrations of fixed work that runs no flusso code (a SHA-256 pass over 64 MiB, a 256 MiB memory copy, a synced 64 MiB disk write). `.github/scripts/bench-runner-facts.py` stamps them in the `scenarios` job, writing each file through a temporary file and a rename. It never fails the job: a calibration that can't run (a full or read-only disk, too little memory) is left out, and a file it can't read or write is left as it was, each with a warning. Nothing reads these facts yet; they're for the rule's next version. A point's image is read from `extra.images` only: an `extra` without an `images` key means "no image", the same as a point without `extra`, so a point carrying runner facts and no image keeps the same window as before. A reader that looks `images` up the way the script on `main` did (missing reads as `null`) would see a new image at the first stamped run and restart the window of every series without images.

### Known limits

The rule misfires in three known ways ([#317](https://github.com/alias2k/flusso/issues/317)):

- **A slow runner moves a whole run.** GitHub runs `main` on a few kinds of machine. On a slower one nearly every series crosses its threshold together, which isn't a regression of any of them.
- **A series alternating between two levels.** When a five-point window holds mostly one kind of machine, its median and spread sit on that level, and a point from the other kind looks like a move.
- **A lasting step absorbed by the median.** After a real step, the window's median reaches the new level within a few runs, so a regression flagged late, or whose issue was closed, isn't seen again.

Telling these apart needs to know which machine ran each point, which the history before runner facts doesn't record. The rule will be revisited once about 40 `main` runs carry them. Until then #317 stays open: the median series needs a lasting step of about +150% to be caught reliably, and only 1 of 51 series catches 1.5 times its threshold, so the rule is a coarse alarm for large steps, not a check at 25%.

The script's tests replay it over a snapshot of the real history (`.github/scripts/fixtures/`) next to a frozen copy of the rule as it stands, and CI's `scripts` job runs them:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

## Re-recording the pgoutput fixture

The decode bench reads `libs/2-adapters/source-postgres/benches/fixtures/pgoutput.bin`: the raw `XLogData` payloads a Postgres 16 sent for a fixed set of inserts, updates, deletes, and a truncate across tables keyed by `int`, `bigint`, `uuid`, and a composite key. Re-record it only when the decoder's input needs to change:

```sh
cargo nextest run -p flusso-source-postgres --test record_pgoutput --run-ignored all
```

Review the size, then commit. A new fixture is a new baseline for that bench.

## Related

- [Testing](testing.md), for the suites that run beside the benches.
- [Metrics](../reference/metrics.md), for the `/metrics` series the harness reads.
