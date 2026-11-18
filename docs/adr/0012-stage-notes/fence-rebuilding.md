# #379: a fence never vouches for an index a sink is rebuilding

**The bug.** `GET /wait` answers `200` for a position whose document a read can't see yet. This happens while a sink:

- seeds an index for the first time;
- rebuilds every seeded index after a `Fresh` start (the slot was lost);
- reindexes on demand.

ADR 0009 called this a documented limit while a client failed fast when flusso was away. Since #377 a `flusso-query` client rides out a restart, so a fenced read across a `Fresh` restart now ends in `200` and a stale or missing document: the one thing a fence must never do.

**The rule (ADR 0009, kept):** a fence never falls back to a stale read. It answers `200` only for what it can vouch for. Otherwise it answers `503`, with `Retry-After` and a `reason`, for as long as it can't.

Stage 7 gave each `(index, sink)` its own lane and its own marks in `Progress`, and a fence already names its indexes (`?index=`). So the fence can decide per lane and stall nothing it doesn't cover. That answers ADR 0009's old objection: "a fence is per sink, so holding it would stall every other index".

## 1. What each lane serves

Each lane is in one of four states. The fence keeps it beside the lane's `Furthest` positions, under the same per-lane mutex the check already takes, so the hot path gets no new lock. Engine events move it. They are rare: a few per index per run, none per batch.

| state | meaning | a read through the alias sees |
| --- | --- | --- |
| `Pending` | the lane's sink engine hasn't staged yet | nothing is known |
| `Serving` | seeded; live writes go where reads go | every write the lane applied |
| `Backfilling` | unseeded, a snapshot is filling the index | a partial index |
| `Rebuilding { through }` | a new generation is being built while the alias serves the old one, or nothing worth trusting | the old generation, exactly as it stood at `through`, or nothing when `through` is `None` |

**Events, in the order the sink engine emits them:**

- **`on_index_rebuilding(sink, index)` (new, `Observer`, no-op by default).** The sink engine emits it in its staging step `Step::Rebuild`, **before** `Sink::reindex`. That covers both callers: `rebuild_stale_seeds` after `Fresh`, and an on-demand reindex.
  - From `Serving`, it becomes `Rebuilding { through: Some(furthest) }`: the lane's furthest applied and searchable positions at that moment.
  - From any other state, it becomes `Rebuilding { through: None }`. A `Fresh` start is `Pending`: this process never saw that old generation written, and a `Fresh` source means changes are missing from it.
- **`on_backfill_requested(sink, indexes)`.**
  - From `Rebuilding`, the lane stays as it is.
  - From anything else, it becomes `Backfilling`. Two cases: a first seed, or a generation the sink found deleted and recreated empty. That second case comes from `Serving`, which is why only the explicit rebuilding event freezes `through`: here the old generation is gone, and freezing would vouch for an empty index.
- **`on_index_seeded(sink, index)`.** It becomes `Serving`. It is emitted after `Sink::mark_seeded` returned, and that call refreshes the new generation **before** it moves the alias. So everything the lane applied is readable, and searchable, once the alias moves.
- **`on_sink_started(sink)`.** That sink's `Pending` lanes become `Serving`. Indexes it requested are already `Backfilling` or `Rebuilding`, because `request_backfill` runs before `staged()`. The rest are seeded, or deliberately left as they are:
  - under `--skip-backfill`, or with `backfill = false` on the sink, a live write still lands where reads go, so the reader's own write is readable;
  - other rows may be missing, which the operator chose.

**Why a reindex can be honest without #6.** The on-demand reindex runs in `advance` only while no write is in flight. From `Sink::reindex` on, the OpenSearch sink writes to the new generation (the meta document names it, unseeded) and nothing more to the old one.

So the old generation holds exactly what the lane had applied when the rebuilding event fired: `through`. A wait for a position at or before `through` reads the old generation through the alias, and that holds the write. Anything later can't be vouched for until the swap. #6 (live writes reaching the serving generation during a rebuild) would let the fence pass sooner; it isn't needed for an honest answer.

`through` is the furthest the fence had already seen. The search level keeps its own `through`, so a reindex never claims the old generation searchable past what a refresh covered.

## 2. The answer

A wait names a level, a target, and a set of lanes (a sink or all sinks, by the named indexes or all). **One lane covers the target when:**

- it is `Serving` and its reached position at the level is at or past the target; or
- it is `Rebuilding { through: Some(t) }` and `t` at the level is at or past the target.

**What `/wait` answers:**

| situation at the check | answer |
| --- | --- |
| every selected lane covers the target | `200`, as today |
| some lane doesn't, and every such lane is `Serving` | wait; at the timeout `408`, as today |
| some lane doesn't, and one of them isn't `Serving` | wait (a seed may finish); at the timeout, `503` + `Retry-After: 1` + `reason` |

**The `reason`:**

- `rebuilding` if any uncovered lane is `Rebuilding`;
- else `backfilling` if one is `Backfilling`;
- else `starting` for `Pending`.

The body names the lanes: `{"error", "reason", "position", "lanes": [{"index", "sink", "state"}]}`.

**Why hold, not answer `503` at once.** A seed can finish within the wait, and the waiter is woken by the event, so a client sees `200` the moment the alias moves. A client's request asks flusso to wait for the time it has left (#377), so a long rebuild still ends in one `503` at the client's deadline, reported as `FenceUnavailable { cause: Rebuilding | Backfilling }`.

**Scope is the point of per-index fences:**

- A wait for every index of a sink is `503` while any of them rebuilds and doesn't cover the target.
- A wait naming only healthy indexes is `200`.
- A wait on a named sink never looks at another sink's lanes, so an unseeded sibling sink blocks only an all-sinks wait.

**Unchanged:**

- `400` and `404` for a bad request;
- `503 starting` before the source is live;
- `503 stopping` during a stop.

`starting` also stays the reason for a `Pending` lane: it can't happen once the source is live, since the daemon stages every sink before it opens the stream. It is kept for safety.

## 3. The query client

`UnavailableCause` already has `Rebuilding` (added with #377) and maps the wire reason `rebuilding` to it. Changes to `sdk/query/src/fence.rs` and `error.rs` (track B's from stage 7: a small additive change, announced in the commit):

- **`UnavailableCause::Backfilling`, new.** It maps `backfilling`. The enum is `#[non_exhaustive]`, so this is additive.
- **Retries:** a `503` with either reason is retried like any `503`, within `fence_timeout`, paced by `Retry-After`.
- **Out of time:** `FenceUnavailable { cause, reason }` carries it.

## 4. What changes for users (breaking, correct)

A wait that answered `200` during a first backfill, a `Fresh` rebuild or a reindex, for a position the index didn't serve, now holds and answers `503` with the reason. A `408` at the timeout becomes `503` in the same states.

It goes on the upgrade page under breaking changes. ADR 0009 gets an amendment: the consequence "a backfill is not fenced, and neither is an index mid-reindex" is replaced by this answer.

## 5. Tests

- **The daemon's `fence::tests`** (a mock source and a real `Progress`), one per state:
  - a backfilling lane holds the wait and answers rebuilding/backfilling `503` at the timeout;
  - the seed event passes it at once;
  - a reindexed serving lane: a target at or before `through` passes, a later one waits, and passes on the seed;
  - a `Fresh` rebuild (`Pending` → rebuilding) vouches for nothing;
  - a recreated generation (`Serving` → backfill requested, no rebuilding event) doesn't freeze;
  - a wait naming only the healthy index passes while its sibling rebuilds;
  - a wait on a healthy sink passes while another sink backfills;
  - searchable keeps its own `through`;
  - `on_sink_started` serves `Pending` lanes.
- **The engine:** the sink engine emits `on_index_rebuilding` before `Sink::reindex`, on the `Fresh` path and the on-demand path. Checked with a recording sink and observer that log one ordered sequence.
- **HTTP (`http::tests`):** `503` + `Retry-After` + `reason` + `lanes` for both reasons.
- **The client (`tests::fence`, stub daemon):** `backfilling` → `UnavailableCause::Backfilling`; `rebuilding` stays `Rebuilding`; a `503 rebuilding` followed by a `200` passes within the deadline.
- **The `fence` e2e (real Postgres and OpenSearch):**
  - a wait during a first backfill whose snapshot is held back isn't answered `200` until the index is seeded, and then the document is read;
  - a wait during an on-demand reindex, for a change made after it started, is `503 rebuilding` until the swap, then `200`, and the document reads;
  - a change from before the reindex is `200` throughout;
  - the issue's `Fresh` reproduction: a commit made while flusso was stopped and the slot dropped is `503 rebuilding` until the rebuild is seeded.
- **Re-run:** `fence`, `fence_restart`, `shutdown`, `pipeline`, `reindex` and the query e2e.

## 6. Docs

- `operate/read-your-writes.md` and `query/read-your-writes.md`: drop the #379 warnings and say what a client sees.
- `reference/http.md`: `/wait`'s answers and the reasons.
- `operate/upgrade-to-0-18.md`: the breaking entry.
- ADR 0009's amendment, and `CLAUDE.md`'s fence invariant.

## Files

- `libs/3-daemon/src/fence.rs` (state, answer) and `fence/tests.rs`.
- `libs/2-engine/src/observer.rs`: the new event, forwarded by `FanOut`.
- `libs/2-engine/src/sink_engine.rs`: one emit in `step` (track B's file: one line, announced).
- `apps/cli/src/http/mod.rs`.
- `sdk/query/src/{fence.rs,error.rs}`.
- `libs/3-daemon/tests/fence.rs`.
- The docs above.

## As built

- **As designed, with two findings from the tests:**
  - **During a first seed a get by id already finds a live write.** The index exists and is just partial: `ensure_index` makes it reachable before the seed. The fence still answers `503 backfilling`: a read by id would be right, but a search would see a partial index, and the fence can't tell which read follows. The e2e asserts the answer, not the absence of the document.
  - **Only the rebuilding event reads the lane's positions** (to freeze `through`). The other events only set the state: reading positions there filled `Furthest` before the source was live, and a wait then answered from it instead of `503 starting` (caught by `http::tests` and `fence::tests`).
- **The client's `UnavailableCause::Backfilling`** is the only change to `sdk/query`.

| piece | commit | what was run | result |
| --- | --- | --- | --- |
| design | `e13fdaf7` | n/a | n/a |
| `Observer::on_index_rebuilding`, emitted before `Sink::reindex` | `6dbc91de` | `cargo nextest run -p flusso-engine -E 'test(an_index_is_announced_rebuilding)'` | 1 passed |
| the fence's per-lane state and `Unserved` | `76940d45` | `cargo nextest run -p flusso-daemon` (9 new `fence::tests`) | in the 634 below |
| `/wait` `503` with reason and lanes | `16daeb0a` | `http::tests` | in the 634 below |
| `UnavailableCause::Backfilling` | `e5c55c6a` | `flusso-query` stub tests | in the 634 below |
| e2e | `4f83d964`, `229e57ff` | `cargo nextest run --profile ci --run-ignored all -E 'binary(fence)'` | 3 passed |
| docs | `46ba4c54` | `mdbook build docs` | builds |

**Whole packages:** `cargo nextest run -p flusso-daemon -p flusso-cli -p flusso-query -p flusso-engine`, 634 passed.

**e2e from one archive** (`--profile ci --run-ignored all`):

| binary | passed |
| --- | --- |
| `fence` | 3 |
| `fence_restart` | 2 |
| `shutdown` | 13 |
| `pipeline` | 17 |
| `sink-opensearch` `reindex` | 9 |
| `flusso-query-e2e` | 16 (`array_columns` 2, `combined_search` 1, `cursor_paging` 4, `geo_points` 1, `map_keys` 3, `read_time` 4, `tls` 1) |
