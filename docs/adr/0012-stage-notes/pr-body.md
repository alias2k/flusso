# Draft PR body: reopening #422

> Draft for the coordinator; it becomes the body of #422 when the branch reopens. Every claim points at the stage note that holds its numbers and tests.

## ADR 0012, first merge: concurrent builds and writes, a memory budget, one lane per index and sink

This merge takes in ADR 0012's stages 3–7 (stages 1–2, `Sink::write` and concurrent writes, are already on `main`), the backfill resume from 8b, the stall tracking from 8c, and #379. One ingest task still builds every index. 8a's reader and per-index tasks come later.

**The bar for this merge (owner, 2026-11-18):** stable, with no regression against `main`. Performance gains are welcome but not required; they continue on later branches.

## What is in

| Stage | What | Notes |
| --- | --- | --- |
| 3 | Keyset backfill: pages of root keys on the builder pool, no long transaction, bounded walk, the key index's own operators | `stage-3.md` |
| 4 | Up to `[concurrency] builds` builds at once (default 4, max 32), published in grant order; the build gate (live first, every fourth contested grant to backfill) | `stage-4.md` |
| 5 | Memory budget `[concurrency] max_built_mib` (default 512) in heap bytes; `[stream] capacity` deprecated | `stage-5.md` |
| 6 | Positions describe themselves; `Progress` confirms and fences; the stream keeps no watermark | `stage-6.md` |
| 7 | One lane per `(index, sink)`; per-index fences and `/wait?index`; ready items merged into one write; caught-up per lane | `stage-7.md` |
| seed | Cheaper seeding and generation creation (#410, #403) | `seed.md` |
| 8b (resume) | An interrupted backfill resumes after its landed prefix (`Batch.walked`, `Sink::seeding`) | `stage-8b-resume.md` |
| 8c | Stall age and WAL behind per lane, `pinned_by`, `FlussoLaneStalled`, the Helm `PrometheusRule`, the runbook "A lane is stalled" | `stage-8c.md` |
| #379 | A fence holds while an index is seeded or rebuilt; `503 backfilling` / `503 rebuilding` at its timeout | `fence-rebuilding.md` |
| CI | `alerts` job (promtool + helm from Docker): **not a required check** | `docs/src/contribute/testing.md` |

**8a groundwork, already on the branch** (inert or behaviour-neutral):

- the build permits (`ingest/gate.rs`, drawn by one task);
- changes resolved on a connection of their own (the connection count is still `builds + 2`);
- `Progress`'s per-index pending sets, which nothing marks yet.

## What is not in

- **8a: the reader and per-index ingest tasks.** Also the per-index budget half and its per-index estimate.
- **8b: per-index restart.** A failing build still restarts the whole ingest engine.
- **#396** (resolving a batch's changes concurrently on the reader).
- **Stage 9: per-lane failure.** It is designed (`stage-9.md`), not started.
  - Until it lands, a document refused under `on_error = "stop"`, a missing index or a write block still stops the sink's whole run. The daemon restarts it with backoff.
  - A write block under `skip` still quarantines documents (#404).
- **Dropping and rebuilding a stalled index** (#394).

## Behaviour changes users see

Each has its entry, with what to do, on `docs/src/operate/upgrade-to-0-18.md`:

- **A backfill reads key pages, with no long transaction.** No lock held across it, vacuum not held back, key order.
- **Several batches build at once** (`builds`), on `builds + 2` Postgres connections.
- **Memory is bounded in bytes** (`max_built_mib`).
  - `[stream] capacity` and `--queue-capacity` warn and are ignored.
  - Helm requests 512 Mi.
  - A warning at start when the budget reaches the cgroup or host memory.
- **One lane per index and sink.** A slow index holds up only itself. `/wait?index`; `flusso-query` waits on the indexes a read touches.
- **A fence holds while an index is seeded or rebuilt** (breaking, correct). It used to answer `200` there and the read was stale.
- **An interrupted backfill resumes.**
- **New `/status` fields:** `memory`, `pinned_by`, `lanes.*`. **New metrics:** `flusso.memory.*`, `flusso.lane.*`, `flusso.slot.pinned_by`, `flusso.backfill.checkpoints`, `flusso.sink.refreshes`.
- **The `FlussoLaneStalled` alert.**
- **Embedding the crates:** the `Sink`, `Stream`, `ChangeCapture`, `Position`, `Fence` and `Observer` ports changed. The upgrade page has the 0.17 → 0.18 tables.

## How it was verified

**At the tip:**

- `cargo fmt --all --check`, `cargo clippy --workspace`, and `cargo check --workspace --all-targets --features flusso-source-postgres/bench,flusso-sink-opensearch/bench`;
- `cargo nextest run --workspace`, plus the e2e binaries with `--profile ci --run-ignored all`;
- the CI scripts (`python3 -m unittest discover -s .github/scripts`);
- `mdbook build` and `check-manual-links.sh`;
- `.github/scripts/check-alerts.sh`.

The last full counts are in the stage notes' tables; the reviewers' runs replace them here.

- **The engine e2e** run at `builds = 4` (what ships), and once at `builds = 1` and under a 16 MiB budget (`FLUSSO_E2E_BUILDS`, `FLUSSO_E2E_BUDGET_KIB`).
- **Seeded schedule tests:** random build and write orders, failures and restarts, over several indexes, checking per-document order, lane order, the budget's bound, and that every byte is returned.
- **A standing leak guard:** every engine test and e2e counts what it builds, and nothing stays counted.
- **Measured with `flusso-bench`** (scenarios `reference` and `complex`, `--scale default`, three rounds where it says so):
  - concurrent builds: about +15% backfill on `reference` (`stage-4.md`);
  - the budget: 128 MiB keeps 512's backfill rate at half the RSS (`stage-5.md`, default left at 512 for the owner).
- **The no-regression comparison against `main`** is track B's scenario A/B (`bench-ab/concurrency`); its result goes here.
- **Independent reviews** of stages 4, 5, 7, 8c and #379, each with its item → commit → check table in the stage note.

## Known gaps

- **No soak run.** Nothing has run this branch for hours under steady writes and restarts.
- **Item-level 429 against a real OpenSearch is not exercised.** The retry (#400) is tested against stubs, and the flood-stage block's 429 by hand.
- **No e2e for a write-blocked index stalling alone.** That needs stage 9; today a write block stops the sink.
- **The budget default (512 MiB)** is the ADR's number; the owner decides with 8a's multi-index numbers.
- **The `alerts` CI job** is new and not required: a rule or chart change can merge with it red.
