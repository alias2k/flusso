# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

flusso keeps OpenSearch in sync with Postgres from declarative config. You describe a
search document in YAML (`*.schema.yml`); flusso derives the index mapping, seeds it,
then follows Postgres logical replication so the index stays current. Read `README.md`
for the full picture. The user manual is an mdBook under `docs/`, published to GitHub Pages twice (issue #209):
the site root (`alias2k.github.io/flusso/`) serves `docs/` as of the **newest release tag of any
train**, `/main/` serves `main`'s tip with an "unreleased" banner and `noindex` (overlays in
`docs/channels/`, built by `.github/scripts/build-manuals.sh`); the versioned editor schemas sit
beside them under `/schemas/`. `pages.yml` deploys only from `main`: a push to `main` rebuilds both,
and a release tag (`flusso-{config,cli,query}-v*`, pushed by the release-plz App so it fires
workflows) dispatches a `main` run, which moves the root to that release. So a docs change reaches
`/main/` at merge and the root at the next release.
`docs/src/SUMMARY.md` is the page tree: seven parts (Start here, Author, Deploy, Operate,
Query, Reference, Contribute) under one rule, **one fact, one home**. Every `*.schema.yml`
key, `flusso.toml` key, adapter option, env var, CLI flag, Helm value, metric, and HTTP
endpoint is documented once, on a page under `docs/src/reference/`, and the how-to parts link
there instead of restating it. Adapter option tables are **generated** (`just schema-gen` →
`docs/src/reference/generated/`) and included by the adapter pages; edit the doc comment on the
adapter's config struct, never the table. Docker recipes are `docs/src/deploy/docker.md`, the query
manual is the sixteen chapters under `docs/src/query/`, the designer is
`docs/src/author/design-visually.md` + `designer-reference.md`, and the human-facing
architecture tour is `docs/src/contribute/` (this file stays the agent-facing index). Page
types, templates, and the ownership rule are in `docs/STYLE.md`. Every crate has its own
`README.md` — its crates.io/docs.rs landing, wired via `#![doc = include_str!("../README.md")]`
(the `sdk/query` README is a landing too, kept separate from its `//!`) — and `libs/README.md`
maps the crate layering. When you change a doc's content, update the Reference page or README
that owns it; when you move a page, add an `[output.html.redirect]` entry in `docs/book.toml`.
CI's `docs` job builds the book, runs `lychee --offline --include-fragments` on it, and asserts
every `alias2k.github.io/flusso/<path>` URL in the repo's markdown and Rust resolves to a built
page (`.github/scripts/check-manual-links.sh`; a `/main/<path>` URL is checked against the same
book). Deep links in crate READMEs and Rust docs stay at the root (they reach readers through
crates.io at release, when the root has the page); links in `plugin/**` point at `/main/`, since the
plugin is installed from the repository and describes `main`.

## Commands

Common workflows are wrapped in a `justfile` — run `just` to list them (e.g. `just up`,
`just run`, `just check`, `just test`, `just lint`, `just bench`, `just load`, `just status`). It needs
[`just`](https://just.systems) (`cargo install just --locked`). The raw commands it wraps
are below.

Tests run with [`cargo-nextest`](https://nexte.st) (`cargo install cargo-nextest --locked`).

```sh
cargo nextest run                          # fast tests: unit + parse/convert, no deps
cargo nextest run --run-ignored all        # + Postgres e2e (needs a running Docker daemon)
cargo nextest run -E 'test(name_substr)'   # a single test by name
cargo test --doc                           # doctests — nextest does NOT run these
cargo clippy --workspace                   # lint (NOT --all-targets; see below)
just bench                                 # in-process benches (engine loop, pgoutput decode, resolve, document reader, sink render, script fields); no Docker
just bench-components                      # Docker-backed Criterion benches (postgres, cases, opensearch, engine pipeline)
just review-ab origin/main                 # A/B the working tree vs a base on the Docker-backed benches (warm base build)
just review-digest                         # same documents as the base? one digest per index (review-ab --digest)
just review-plans                          # same plans? plans per call, root scan, pruning (review-ab --plans)
just bench-scenario reference ci           # a scenario on the real binary (dev/bench); see docs/src/contribute/benchmarks.md
cargo +nightly fuzz run pgoutput_decode    # fuzz the WAL decoder (from libs/2-adapters/source-postgres)
```

- **Fuzzing** needs nightly + `cargo install cargo-fuzz --locked`. The one target,
  `pgoutput_decode`, lives in `libs/2-adapters/source-postgres/fuzz/` (its own workspace, kept out
  of the strict `[workspace.lints]`) and drives the otherwise crate-private pgoutput decoder
  via the `source-postgres` `fuzzing` feature (`fuzz_pgoutput_decode`). Contract: the
  decoder must never panic on arbitrary bytes — an `Err` is the correct outcome. Run from the
  crate dir; a crash lands in `fuzz/artifacts/`.
- **Benchmarks are three layers** (ADR 0006; the vocabulary — benchmark, component bench,
  scenario, headline metric, baseline, regression, soak — is `CONTEXT.md`'s Measuring section; the
  chapter is `docs/src/contribute/benchmarks.md`). (1) **Scenarios** (`dev/bench`, `flusso-bench`,
  unpublished) run the real release `flusso` binary as a child process against testcontainers
  (or `BENCH_PG_URL`/`BENCH_OS_URL`) through `reference` (the dev store, three indexes),
  `complex` (the worst-case document) and `sync-standby` (`reference` with its writers behind an
  emulated 2 ms synchronous standby, `dev/bench/src/standby.rs`; by hand only, not in `bench.yml`)
  at a `ci` or `default` scale (or `--startup <copies>`, `just bench-startup`: time to `live` with each
  index declared `copies` times, `--skip-backfill`, fresh then restart, `startup.rs`; by hand only), and report the headline
  metrics: visible latency p50/p99 (commit → searchable, via the `updated_at`/`updatedAt` stamp on
  the dev root tables), drain throughput of a burst, backfill throughput, peak RSS + CPU (sampled
  with `ps`), plus flush p50/p99 from `/metrics` for attribution and, per phase, a table of each
  stage's busy seconds per wall second, by task (`attribution.md`, from `flusso.stage.duration`;
  issue #399). The latency probe polls every 5 ms and reports the midpoint between the last miss and
  the hit; latency points carry `extra.images.probe` so that change restarts their window. (2) **Docker-backed component
  benches** (`postgres`, `opensearch`, `engine`'s `pipeline` — attribution only; its backfill group
  is gone; plus `source-postgres`'s `cases`, `build_many` over batch sizes, hit ratios, key types and
  root shapes, run only by `just bench-components` and `just review-ab`, never by `bench.yml`, so the
  history has no `cases` series). `postgres`, `cases` and `pipeline` read `REVIEW_AB_PG_URL`/`REVIEW_AB_OS_URL`,
  which only `review-ab` sets (so `flusso-bench`'s `BENCH_PG_URL` never reaches them), and start
  testcontainers without them. **`just review-ab [base]`** (`review-ab.sh`, issue #340) is the
  reviewer's A/B: an exclusive `flock` on `~/.cache/flusso/review-ab/lock` for the whole run (a second run
  fails at once); containers of a killed run removed (`review-ab-housekeeping.py stale`: labelled `flusso.review-ab.owner` with this host and pid namespace,
  pid gone; `review-ab-housekeeping.py cache-state`: the cache must be new, empty, carry `.flusso-review-ab`, or be
  a pre-marker cache whose `stamp.json` has exactly a stamp's keys with nothing foreign beside it; anything else is
  refused and left untouched); the base in a warm
  tree checked out with a private index (no `git worktree`), reset to the base commit before each build,
  with its own target dir; the head's bench sources laid over it (`--own-benches` opts out and refuses a
  base whose benches predate `REVIEW_AB_PG_URL`); reused without cargo when `review-ab-stamp.py check`
  says it is current (commit, toolchain, `RUSTFLAGS`/`CARGO_PROFILE_*` + `bench-in-process.sh`'s hash + `CARGO_TARGET_<TRIPLE>_{RUSTFLAGS,LINKER}` + cargo config files,
  overlaid files, the executables' sha256, the benches asked for); `bench-in-process.sh` builds only those
  (`BENCH_SET=docker BENCH_ONLY=… BENCH_BUILD_ONLY=1`); named `flusso-review-<pid>-*` containers
  (`--pg 14,16,18`, an OpenSearch for `pipeline`) removed by a trap; CPU pinning (`review-ab-housekeeping.py cpus`: the bench
  under `taskset -c`, the containers `--cpuset-cpus` on a disjoint set, default core 0 left alone and the
  next 2·k split, k = min(4, (nproc − 1) / 2); `--bench-cpus`/`--server-cpus`/`--no-pin` or
  `REVIEW_AB_*_CPUS`/`REVIEW_AB_PIN=0`; off, and said so, without `taskset` or under 5 cores); per version,
  per bench, `--rounds` (3) of base then head (`base-<r>`/`head-<r>`); `bench-compare.py --repeats
  --breaches` flags a case past the limit and twice the control spread (the gap between
  each side's two fastest runs, which narrows with more rounds, pinned by `SpreadSimulation`), each
  flagged case is re-run alone (`--exact`, base then head) and `--confirm` decides with the gate's rule (`confirmed()`,
  shared with `bench-gate.sh`'s path); exit 1 only on a confirmed case. Never fetches. Keeps the last 5
  `target/review-ab/<timestamp>` dirs; `--clean` deletes the cache, `--clean-results` the results. Tests: `test_review_ab.py` (the script
  end to end over fake `cargo`/`docker`/bench executables in a throwaway repo). Two **modes** (issue #347) swap
  the measuring step and keep the rest: they run `source-postgres`'s `review` bench target (`benches/review.rs`,
  not a benchmark), which includes `cases.rs` as a `#[path]` module for its roots, laid over the base with it and
  built, stamped and provenance-checked like any bench. Only a mode builds or overlays it (`bench-in-process.sh`'s
  docker set lists it; review-ab passes it in `BENCH_ONLY` only in a mode), so `cases.rs` must use nothing old
  bases lack and plain `review-ab` keeps building any base it built before (guard: `test_review_modes.py`'s
  `HarnessSplit`, a proxy that checks names, not a compile of `cases.rs` against an old base); what `review.rs` needs is one `// review-ab requires: <text>` line each, looked up with
  `git grep` in the base's `source-postgres/src`, and a base without it is refused by name (from #357 on today).
  A mode refuses `--bench`, `--rounds`, `--limit`, `--own-benches`; `--own-review` lays nothing over the base,
  each side running its own `review.rs` over its own `cases.rs` (for a change to what a form calls: the
  backfill form pages `keys` since #386 stage 3, which bases before it lack; refused without the base's harness). **`just review-digest`** (`--digest`): each side builds every
  root index's documents by `build`, `build_many` under 16 keys and 16+, and a backfill (key pages of 1,024 →
  `build_many` by 256), rendered by `sink::to_json` with sorted keys; `review-ab-digest.py` prints a digest per
  index and side, `same`/`differ` (with the first ids and a diff), `forms disagree` (two forms built one id
  differently) and `no documents` (an index the harness declared with `#index` and nothing built), plus the
  harness's `#note`s (no missing ids on a root keyed by more than `id`, no backfill on a root without a
  single-column key). **`just review-plans`** (`--plans`): Postgres with `pg_stat_statements` (`track_planning`) and
  `auto_explain` (JSON, every statement); per `plan_cache_mode` (`auto`, `force_generic_plan`, set on the fresh
  database) each side runs each root at 1/10/100 keys, 2 × 10 `build_many` calls on one connection, counts
  summed over the root's statements (Postgres 18 merges `ARRAY[…]` lists into one entry), each case opened and
  closed by a `flusso-review-case` marker statement in the server log; `review-ab-plans.py` flags plans per call
  ≥ 0.5 on head only, a changed root scan, lost pruning (partitions left, `Subplans Removed` of the root's own
  Append), and `no plan` for a case with no logged plan on a side; a log with no plan for a whole side and
  `plan_cache_mode` exits 2. A mode exits 0/1
  (flagged)/2 (setup). Tests: `test_review_modes.py`. (3) **In-process benches**, no Docker, the PR gate: `engine`'s `engine` (both engines
  over the channel stream with mocks; `live_drain_held` holds each sink write until the ingest engine
  has begun its next build, so the sink always frees the batch, overlapping that build: the contended
  upper bound, not production's ratio; with an instant sink who freed it was a microsecond race worth
  about 2.4× locally, about 3× on CI's runner. It catches allocation volume and ingest work per batch,
  not sink-side cost below that work or gains from larger batches), `source-postgres`'s `pgoutput` (the decoder over the recorded
  `benches/fixtures/pgoutput.bin`; `tests/record_pgoutput.rs`, an `#[ignore]`d Docker test in the
  nextest `docker` group, re-records it), `source-postgres`'s `resolve` (`PgDocumentBuilder::resolve`
  for the changes that need no query — root table, unread table, update of unread columns, a path walked
  from the key — over
  `bench_offline_builder`, a pool that never connects; its group is `resolve_no_io`, apart from the
  Docker `postgres` bench's `resolve`; issue #332), `source-postgres`'s `documents` (`coerce_document` + the sink
  render over float-heavy documents via `bench_document_reader`, issue #366), `sink-opensearch`'s `render`, `script`'s `scripted` (public
  API, no feature; `bench-in-process.sh` skips a bench a PR base predates). `pgoutput`, `resolve`, `documents` and `render` sit behind each
  crate's `bench` cargo feature (like `fuzzing`), so CI's `cargo check --all-targets` passes
  `--features flusso-source-postgres/bench,flusso-sink-opensearch/bench`. `.github/workflows/bench.yml`:
  on a PR, `in-process` builds the in-process benches on the base (its own worktree **and its own
  target dir**: a shared one makes cargo run the base's binary for the PR, since a workspace member's
  fingerprint is keyed by its path inside the workspace) and the PR, then runs the base then the PR,
  same runner, both into one `CRITERION_HOME`; `bench-provenance.py` checks each side's recorded
  executables come from its own target dir; a base with no in-process bench (exit 78) only warns, any
  other base failure fails. A bench **identical on both sides** (issue #346; `bench_identity.py` is the
  one home: same executable sha256 and same run-time inputs declared in `.github/scripts/bench-inputs.tsv`,
  hashed into `<baseline>.identity.tsv`; ids tied to their executable by `--list` in `<baseline>.ids.tsv`)
  is not run (`BENCH_SKIP_SAME_AS`) and gets no verdict ("identical binaries, not compared"); every
  target present on both sides identical passes (a target the PR adds is listed one-sided); an id no
  executable lists is compared; one both list under a differing target but one side measured is exit 2; `test_bench_gate.py` scans the bench sources
  for undeclared run-time reads and pins the differing-binary output to `fixtures/gate-golden/`
  (`origin/main`'s, before #346). `bench-gate.sh`
  re-measures each bench past 10% slower (`bench-compare.py`) on its own, base then PR (`--exact`), and
  a breach is confirmed when the re-run or the mean of both passes is past 10%, cleared only when both
  are within it; on a `main` push, `scenarios` runs everything and stores the
  points via `github-action-benchmark` on the `benchmarks` data branch (folded into Pages at
  `/bench/`), then `bench-regression.py` compares each point to the median of the previous five and
  calls a regression only on two consecutive points past 25% (20% RSS) and past three times the
  window's median absolute deviation (a noisy series needs a larger move, issue #317), image-tag change restarts
  the window, one self-maintained issue. Around the rule (PR #331, flagging nothing differently): it prints
  each series' **detection power** (the smallest lasting step the rule catches within two runs 9 times out
  of 10, measured by injecting steps at each of the last 20 runs and running the rule; weakest first; series
  absent from the last 50 runs or too short left out and counted) to the job summary, puts only the count of
  series caught at 1.5× their threshold and the median step in the issue (`--skip-detection-power`, tests
  only, leaves it out), skips malformed entries with a warning, and exits 1 before
  `gh` on an unreadable or structurally wrong data file; an `extra` without `images` reads as no image, so
  runner facts never restart a window; `bench-runner-facts.py` stamps every point with the runner (CPU
  model, cores, no-flusso calibrations; written via temp file + rename), read by nothing yet, never
  failing the job. Tests: `python3 -m unittest discover -s .github/scripts -p
  'test_*.py'` (CI's `scripts` job) over the real history in `.github/scripts/fixtures/`, with a frozen
  copy of the rule that every change must flag identically to. `just load` is the soak script — not a benchmark.
- The `#[ignore]`d e2e tests live in `source-postgres`'s `integration`, `key_matching`, `keyset`, `filter_operands`,
  `config_coverage`, `publication`, `introspection`, `tls`, `wal_idle`, `continuity`, and `partitions` binaries
  (`keyset`, issue #386 stage 3: a backfill's key pages read every row once in the primary key index's
  order for every key type, hash- and range-partitioned twins included, at page sizes 1 to 1,024, with a
  type whose operators sit off the search path, as the cases roots' cursor did, and hold no lock between
  pages; `tls` boots a hostssl-only Postgres with a committed throwaway self-signed cert and
  proves the replication stream + SQL pool honor `sslmode`; `wal_idle`: the slot keeps
  advancing from keepalives/filtered commits while watched tables are idle, so unrelated
  writes don't pin WAL retention — issue #111 (the capture's half: the test confirms `Idle` itself; the
  engine's half, the tick confirming with nothing new landed, is `wal_release`'s
  `idle_after_a_burst_the_slot_reaches_the_current_wal_position`); `continuity`: `continuity()` is read-only and
  reports `Fresh` exactly while the slot is missing, `prepare()` creates it — issue #120;
  `partitions`: a range-partitioned root and child stream under the parent's name via
  `publish_via_partition_root`, resolve, and backfill through the parent — issue #143) plus
  `engine`'s `wal`, `wal_release`, `visibility`, `backfill`, `keyset_backfill` and `pipeline`
  binaries (`keyset_backfill`: writes run right after each full key page, behind, ahead of and inside the
  page just read, including key changes both ways across the cursor, soft-deletes, a root filter and
  re-parents, on a plain and a hash-partitioned root and across a stop mid-walk; the sink must end equal
  to a `build` of every row; `wal_release`, issue #204: with the engines running over recording sinks, the slot's
  `confirmed_flush_lsn` and `restart_lsn` pass the WAL written by changes that produce no document
  — an unread table under `FOR ALL TABLES`, unread-column updates, filtered/soft-deleted/orphaned
  rows, a large and a split mixed transaction, idle WAL after a burst — `confirmed_flush_lsn` within
  `CONFIRM_BOUND` (5 s, what flusso controls; also from the last sink flush for bursts and lone
  changes that produce documents, issue #205), `restart_lsn` within `RELEASE_BOUND` (45 s, also
  waits on Postgres's next `running_xacts` record), and the sinks got exactly the expected ops; a stalled sink
  pins the slot at its oldest unlanded position (`Progress`'s `Before` over every lane) until it catches
  up, a `backfill = false` sink does not; the full
  source→engine→sink e2e lives in `engine` — a leaf source crate must not dev-depend on
  the engine, or it can't be published before the engine and the layering is violated);
  `testcontainers` spins up Postgres (and, for `pipeline`, OpenSearch). `engine`'s `wal`
  drives a *recording* sink (asserts which op the engine emits); `engine`'s `pipeline`
  drives a **real OpenSearch sink** and reads the index back over HTTP, asserting the
  actual document after each change: live insert/update/delete across
  `uuid`/`int`/`bigint`/`text` keys, soft-delete tombstoning (boolean + timestamp markers,
  set→removed, cleared→restored), backfill (active rows seeded, soft-deleted skipped), and the
  two seed-marker contradictions (restart after the generation index was deleted → reseeded in
  place; restart after the slot was dropped + data changed → rebuilt into a new generation, stale
  rows gone, old generation dropped), and `deleted_and_reparented_children_leave_their_old_parent`
  (a hard-deleted / re-parented `has_many` child and a row three relations deep leave their old
  parent's document, traced through the WAL pre-image; tables under `REPLICA IDENTITY FULL`, which
  also proves a `FULL` root keeps a plain `_id` — issue #140), and
  `a_self_referencing_has_many_rebuilds_the_parent` (a root-table row inside its own `has_many` —
  insert, update, delete, re-parent, and a two-level grandchild — rebuilds its parent, because a
  root-table change resolves to its own id *and* walks every path back at the root — issue #167),
  and `a_root_filtered_tree_indexes_only_top_level_rows` (the guide's tree with a `parent_id
  is_null` root filter: only top-level rows are documents across child/grandchild changes,
  re-parenting, promotion, and demotion), and `binary_columns_and_constants_are_indexed_as_written` (issue #291: a `bytea` at the root, as
`bytea[]`, in every join kind and two levels deep reads back as base64 of its bytes on backfill and
after a live update; constants land as written), and `date_and_time_columns_are_indexed_and_queryable`
(issue #304: `date`/`timestamp`/`timestamptz` and their arrays land as ISO 8601 a default `date`
mapping takes, on backfill and live, under a non-UTC `TimeZone`, with a `range` and a sort over each;
`sink::Json` writes a naive timestamp with a `T`) and `a_tuned_timestamp_mapping_meets_the_iso_form`
(a space-only `format` rejects each document holding a value, `strict_date_optional_time` or
`ignore_malformed` takes it, an `indexed: false` join stores the `T` form), and
`a_numeric_keeps_every_digit_as_a_json_number` (issue #327: a `decimal` scalar, array element,
`sum`/`max` and `map` value hold every digit as a JSON number, past `rust_decimal`'s range too, an
exponent form as written, on backfill and live; asserted on `_source`'s raw text) and
`a_non_finite_number_reaches_the_sink_as_postgres_wrote_it` (`NaN`/`Infinity` stay the strings
Postgres wrote: a mapped number field is an item-level rejection `on_error` decides at the sink;
`ignore_malformed`, an unindexed join and a namesake's discarded value keep it), and `an_update_of_an_unread_column_writes_nothing` (issue #169: an update changing
  only a column no document reads leaves the row's and its parent's `_version` alone; `wal`'s
  `an_update_of_unread_columns_rebuilds_nothing` covers root, child, aggregate, each read-column
  kind, and the no-pre-image fallback; `on_pairs_rebuild_the_parent_from_either_side` covers `on:`
  pairs on `has_many`/`belongs_to`/a direct aggregate/a `via` hop, both sides of a pair, and the skip). `sink-opensearch`'s `map_keys` binary (issue #177)
  proves on a live OpenSearch that map keys take their `values` type under the strict root and
  inside a `nested` array, and that a value a plain field would refuse is an item-level rejection;
  its `an_unindexed_subtree_is_stored_but_not_searchable` (issue #170) proves an `indexed: false`
  join/`json` lands in `_source`, matches no query, and accepts values a mapped field would refuse,
  and that a `subfields: false` text field maps bare but stays searchable.
  `daemon`'s `fence` binary (issue #136) runs the real daemon over a real Postgres and OpenSearch
  and reads OpenSearch **once, unpolled**, after each fence: applied and searchable, a position
  behind unwatched WAL, a transaction split across batches, concurrent writers, a stalled sibling
  sink. `sink-opensearch`'s `reindex` binary also pins the visibility a write reports
  (`a_flush_reports_when_its_documents_become_searchable`).
  `sink-opensearch`'s and `dev/query-e2e`'s `tls` binaries (issues #239, #243) run OpenSearch over
  `https://` with its security plugin's demo certificate (an untrusted CA, basic auth): the sink
  with `tls_verify = true` and `flusso-query` (no verify-off switch) refuse it promptly, and both
  read back once given the demo CA as `ca_file`, verification on; the sink with `false` writes too.
  The refusing sink allows six bulk retries whose backoff outlasts the deadline, so a prompt flush
  failure proves a refused certificate is not retried. The sink's is one sequential test, since it
  changes process-wide state: `SSL_CERT_FILE` names the demo CA (an OS-trust-store client accepts,
  the pinned sink still refuses), then the checks run with no process-default rustls provider and
  again with `ring` installed. The refusal must name the certificate: the sink renders a transport error's whole
  source chain (`transport.rs`'s `request_error`).
  New e2e binaries must be added to the filtersets in `.config/nextest.toml` (both profiles) to get
  the docker/opensearch group caps and retries.
  It is the only test that catches a live change rebuilt as the *wrong* op (e.g. an update
  written as a tombstone because the WAL key decoder and the read-back decoder disagree on
  the `GenericValue` variant). **A key is parsed by one function on both sides** (issue #295,
  `src/text_value.rs`'s `typed_value`): the WAL decoder on each cell, and every read-back of a key
  (backfill cursor, the batched build's `doc_key`, reverse queries, the root-key lookup) selects it
  with `value::key_select` — `text_value::key_text`, a rendering **no session setting changes**
  (`'\x' || encode(col, 'hex')` for `bytea`, `float8send`/`float4send` bits for floats, else
  `format('%s', col)`: the output function, `::text` trims `char(n)`; dates follow the `DateStyle=ISO`
  / `TimeZone=UTC` sqlx sends at startup, which PgBouncer passes in every mode) beside the column
  itself, never decoded, whose row description gives the base type OID — and parses it in
  `value::decode_key` (`text_value::read_back`); a type outside `text_value::is_key_type` reads as
  null (refused at start). **No startup `options`** anywhere (PgBouncer refuses them; issue #295's
  second review): the replication session runs `text_value::REPLICATION_SETTINGS` (`DateStyle`,
  `TimeZone`, `bytea_output`, `extra_float_digits`) as `SET`s on a gated `pgwire-replication` session
  before `START_REPLICATION` (`capture.rs`'s `live`), a refusal stopping the start. The builder's pool keeps an
  after-connect `SET bytea_output = 'hex'` only as a backstop for a `bytea` the query can't reach (below). The temporal parser reads any year
  width by hand (chrono's `%Y` stops at four digits), so a year past 9999 is typed and keeps its
  `+10000-01-01` id. A domain column is retyped to its base OID when the
  stream keys its `Relation` (`stream.rs`'s `retype_domains`). A value the parser can't hold
  (`infinity`, numeric `NaN`) stays text on both sides and binds as `$n::text::<type>`
  (`sql::key_cast`): sqlx caches a statement by its text with its first parameter types, so a text
  value bound where a typed one was prepared fails in binary format. For the same cache reason a
  backfill's key page (`query::key_page_query`) is its own statement text per root table, key type
  and bound type (the first page has no bound), so no page decodes another root's keys with its row
  description. **A resolved document id is
  the root row's own key** (`resolve.rs`'s `reads_root_from_a_link` / `root_keys`): where a path
  ends on a link column's value (`row_link` of its first step) and the root key's equal values can
  render differently (`ColumnMeta::equal_is_identical`: `numeric`, floats, `citext`, a
  nondeterministic collation) or the link column's type differs from the key's, `resolve` reads the
  keys back from the root table, **one query per change** per index and link column (`RootLinks`;
  `root_keys_query` casts each link from its bound type to its link column's type), as the change is
  resolved: a batched read was tried and dropped, since resolving at commit slowed the engine's
  `live_drain` bench by 13% and moved when the database is read; every other key type costs no query.
  Whether a path reads its keys back is decided once per path and kept on its `PathReads`
  (`document/reads.rs`), tagged with the column-type cache's generation, which every
  `ColumnTypes::forget_table` bumps (issue #332; guards: `document::tests`'
  `a_path_walked_from_the_key_looks_up_no_column_once_known`,
  `a_changed_column_type_reaches_the_kept_root_link_answer`). **A
  resolved value is compared as the join compares its column** (`resolve.rs`'s `cast_types`): every
  reverse query casts a bound value to the type of the column it was read from (the hop's origin: a
  `belongs_to` target key, a junction's far key, the previous hop's link), so a `char(5)` link against
  a `text` key resolves what the build joins. Guards: `wal`'s
  `every_relation_position_keyed_by_*` (root, `has_many` + `on`, `belongs_to`, `many_to_many`,
  `via`, pre-image, backfill, ids equal to the backfill's; also under non-ISO `DateStyle`s,
  `bytea_output = escape`, `extra_float_digits = 0`, `±infinity`, BC and numeric `NaN` keys) and
  `*_rebuilds_the_roots_own_document` (citext, numeric, float8, a nondeterministic collation, `char(5)`
  ↔ `text`), `the_pipeline_runs_through_pgbouncer_in_{session,transaction}_mode` (PgBouncer 1.26),
  `a_burst_of_numeric_links_reads_one_root_key_back_per_change` / `a_burst_of_int_links_reads_no_root_key_back`
  (`pg_stat_statements`), the `*_past_9999` cases,
  `text_value::tests`, `pgoutput`'s `a_value_decodes_as_the_document_builder_reads_it_back`, and
  `document::tests`' `equal_keys_render_alike_except_for_numbers_and_loose_text`. `.config/nextest.toml` caps their concurrency and
  retries them — they're legitimately slow/flaky.
- **The file formats are frozen for the major** (issue #109): any `flusso.toml` or
  `*.schema.yml` a release in the major accepts must keep loading on every later one, and so
  must any `flusso.lock` from 0.16 on (backwards only — `deny_unknown_fields` stays; deprecate,
  don't remove). The lock is **deterministic TOML** (format 3; `libs/1-config/src/compiled.rs`, no
  `flusso_version` field — byte-stable across flusso upgrades; each port entry is written as its
  `type` plus its options with sorted keys). Format 2 (kernel-typed adapter settings, before
  0.16) and the pre-freeze MessagePack are rejected with a regenerate hint, version checked
  before the body (ADR 0005). Two guards in `libs/1-config/tests/`: `golden_lock.rs` byte-pins
  the serialized shape against a maximal fixture (`tests/fixtures/golden/`; re-bless with
  `FLUSSO_BLESS=1` after reviewing the diff) and `compat.rs` walks the immutable per-release
  corpus `tests/compat/` (see its README; never edit a snapshot — fix the change instead; `v0.15`
  keeps only its user-authored files, the lock guarantee starts at `v0.16`).
- **Generated artifacts are drift-guarded.** `libs/1-config/config.schema.json` (the complete
  `flusso.toml` editor schema) and `docs/src/reference/generated/*.md` (one option table per
  adapter) are rendered from the adapters' `#[derive(AdapterConfig)]` declarations by
  `flusso schema config` / `flusso schema docs`; a unit test in `apps/cli/src/commands/schema_cmd.rs`
  fails when the committed copies differ. `just schema-gen` refreshes both. The derive's error
  messages are pinned with trybuild in `libs/0-kernel/derive/tests/ui/`.
- **Every Postgres start goes through `dev/test-postgres`** (`flusso-test-postgres`, issue #342):
  `test_postgres::image()` for the run's image, `test_postgres::pinned(Pinned::…)` for a test that
  needs one version (it prints why), `test_postgres::Container` as the type. The version, the
  `FLUSSO_TEST_PG_TAG` override and the exceptions are owned by `docs/src/contribute/testing.md`
  ("Which Postgres"); `review-ab.sh` reads its default `--pg` from the crate's `DEFAULT_TAG`.
  `testcontainers-modules`' own `Postgres::default()` is `11-alpine`. Guard:
  `.github/scripts/check-postgres-image.py` (via `test_check_postgres_image.py`, CI's `scripts` job)
  rejects any import of `testcontainers_modules::postgres` outside that crate (nested, glob, renamed),
  `Postgres::default`, `with_tag`/`with_name`, and a `GenericImage` naming Postgres or a non-literal
  image. The crate is a **path-only dev-dependency** of the published crates (`cargo publish` strips
  it), so it stays outside the layering.
- `apps/cli/tests/agent_docs_paths.rs` asserts every repo path named in `plugin/**`,
  `.claude/commands/**` and `.claude/skills/**` exists — the moved-file guard for those pointer-heavy trees. Ordinary test,
  caught by the nextest step.
- **Env-var tests live in the adapters.** `libs/2-adapters/source-postgres/tests/config_env.rs`
  and `libs/2-adapters/sink-opensearch/tests/config_env.rs` mutate the override variables
  (`SOURCE_POSTGRES_CONNECTION_URL`, `<SINK>_OPENSEARCH_*`). nextest gives each test its own
  process; the Postgres one is a single sequential test and each OpenSearch test uses its own
  sink name, so plain `cargo test` is safe too. An intermittent failure there is an env race
  from a new test sharing a variable, not a regression.
- CI's `test` job runs, in order: `cargo fmt --all --check` → `cargo clippy --workspace` →
  `cargo check --workspace --all-targets --features flusso-source-postgres/bench,flusso-sink-opensearch/bench`
  (compiles benches + examples, which clippy and nextest skip — clippy omits `--all-targets`,
  nextest only builds test targets; the features unlock the three feature-gated in-process bench targets) → `cargo check -p
  flusso-cli --no-default-features` (the server-only image build) → `.github/scripts/check-ahash-seed.py`
  (`cargo tree --offline`: `ahash` keeps `runtime-rng` beside `compile-time-rng`, issue #355) → `cargo nextest run
  --profile ci --run-ignored all` → `cargo test --doc` → `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --document-private-items` (broken/ambiguous/redundant intra-doc links fail
  the build). Match these before assuming green. A `CLI on <os> (check)` job runs `cargo check -p
  flusso-cli --locked` on `windows-latest` and `macos-latest`, the release targets beside Linux
  (`dist-workspace.toml`): a cross check from Linux can't build `ring`, and code that compiles only on
  Unix (`#[cfg(unix)]` the signal and `/proc` items) would otherwise fail only at a release tag. One scope note: on PRs confined to the query
  release train (`sdk/query`, `sdk/query-derive`, `dev/query-e2e`) the nextest step swaps in a
  filterset that skips the Postgres/OpenSearch container suites those paths can't break (the
  query train's own `combined_search`, `cursor_paging`, `map_keys`, `tls`, `array_columns` + `read_time` live e2e still run); pushes to main always run everything.
  A second scope note (issue #402): a PR **into the default branch** whose every changed file (both
  sides of a rename; the whole PR as it would merge, `HEAD^1..HEAD` of the merge commit) is
  documentation is **docs-only** (nothing under `.github/` is): `ci.yml`'s `test`, `fuzz`,
  `designer-frontend`, `other-platforms` and `bench.yml`'s `in-process` run
  `.github/actions/change-class` (fetch-depth 2; `.github/scripts/change-class.sh`) first and skip
  their heavy steps, so each still reports green under its name with a "docs-only: skipped …"
  summary; `test` runs only the `DOC_TESTS` whose files changed and clippy + doctests + doc links of a
  crate whose compiled README changed (clippy's doc lints read it), and saves no build cache; `docs`
  and `scripts` run in full. The action runs the **base's** `change-class.sh`, which runs the base's
  `change_class.py` (`git show HEAD^1:…`), never the PR's copies; everything runs when either is
  missing or fails, when the classifier's output doesn't end with `docs_only` (written last), or when
  the merge commit's `HEAD^2` isn't the event's `pull_request.head.sha`. Only the action's bootstrap
  and the workflows are the PR's own. The rule, its patterns and `DOC_TESTS` live in
  `change_class.py` (manual: `docs/src/contribute/testing.md`); `test_change_class.py` tokenizes every
  Rust source's string literals and fails on one naming documentation that no `DOC_TESTS` entry covers
  or `NOT_READS` explains (it sees literals only: a path with no such literal is not seen), so a new
  `include_str!` of a page needs an entry. Any other event or base, or a diff that can't be taken,
  runs everything.
  A `docs` job builds the mdBook (`mdbook build docs`), link-checks it (`lychee --offline
  --include-fragments --root-dir docs/book 'docs/book/**/*.html'`), and runs
  `.github/scripts/check-manual-links.sh`. A separate `fuzz` job runs a 60-second
  `pgoutput_decode` smoke fuzz on nightly (see below); a separate workflow, `postgres-matrix.yml`
  (issue #348, **not** a required check), runs `-E 'rdeps(flusso-test-postgres)'` under the `ci`
  profile once per tested major other than the default (`TESTED_TAGS` in `dev/test-postgres`, their
  one home; `cargo run -q -p flusso-test-postgres -- matrix` prints the legs), nightly and on
  `workflow_dispatch` only, no `pull_request` trigger: for a PR, `gh workflow run
  postgres-matrix.yml --ref <branch>` (its checks attach to the head commit's checks page and the
  Actions tab, not to the PR's checks list); it shares its setup with
  `clippy + tests` through `.github/actions/rust-test-setup`, and a failing nightly maintains one issue
  via `.github/scripts/postgres-matrix.py` (unit-tested in the `scripts` job); the `query.rs` proptests need no special
  handling — they're ordinary tests caught by the nextest step. A `scripts` job runs the stdlib unit
  tests under `.github/scripts` (`python3 -m unittest discover -s .github/scripts -p 'test_*.py'`).
  Outside `ci.yml`, `audit.yml`'s
  `advisories (cargo deny)` job runs `cargo deny check advisories --all-features` weekly and on
  changes to a `Cargo.toml`/`Cargo.lock` — advisory only, never a required check — and
  `.github/dependabot.yml` opens grouped weekly `chore(deps)` PRs (cargo, npm, actions; skips
  flusso's own crates, `rust-toolchain.toml`, ESLint 10). A bot npm bump fails the dist-drift guard
  until the SPA is rebuilt and committed on its branch (`docs/src/contribute/dependencies.md`).
- **The designer (`apps/design`) has two test layers.** (1) A property/"fuzz" round-trip
  (`apps/design/tests/roundtrip.rs`, proptest): random valid `IndexSchema`s →
  `codegen → parse → convert` identity — an ordinary test, caught by the nextest step (plus the
  `api.rs` integration test). (2) A
  `designer-frontend` CI job: `npm ci`, a **Prettier** format check (`npm run format:check`,
  `.prettierrc` printWidth 120 / double quotes / semicolons / trailing-all; `just design-fmt [check]`;
  `eslint-config-prettier` is appended last so ESLint owns no formatting rules), **ESLint**
  (`npm run lint`, flat config, **type-aware** —
  typescript-eslint recommended+stylistic *type-checked* sets, plus react / react-hooks / jsx-a11y /
  react-refresh, all as errors; CI runs it `--max-warnings 0`; `just design-lint [fix]`. Pinned to
  ESLint 9 because eslint-plugin-react/jsx-a11y don't support 10 yet; the canvas's pointer-driven
  rows scope off the jsx-a11y interaction rules), an **i18n completeness check** (`npm run check:i18n` →
  `apps/design/frontend/scripts/check-i18n.mjs`: every `t("…")` key the UI uses exists in the
  `en` base catalog and every locale defines the same key set — so a feature can't ship UI without
  its translations), `npm test` (`node --test scripts/*.test.mjs`: pure `src/model/` functions, run with
  Node's type stripping, e.g. `model/recovery.ts`, the recovery notice's wording), then `npm run build` + a `git diff` guard that the committed
  `apps/design/dist/` matches a fresh Vite build (the embedded SPA must not drift). There is **no**
  browser e2e — the UI is validated by dogfooding; the `designer-frontend` build/lint guards plus the
  Rust round-trip are the automated net.
- **A designer save is crash-safe, and a rollback settles each file once** (`apps/design/src/commit/`;
  its module docs hold the table of what a kill leaves and what the next start does). A rollback
  writes its per-file decisions into the save's record (state folder) before changing a file, takes
  each step as one rename (a backup over its file; a created file to its temp name), flushes the
  project folders it changed, then writes that step's mark in the record (every project change is
  flushed before any record write: a power cut never keeps a mark without its rename), and marks the
  record `settled` before deleting the save's own files, so a kill then a user edit never re-settles a
  file (issue #302). Completion is recorded, never read off the disk: an unmarked step is decided
  again from what the disk holds, as a fresh rollback would, so a deleted backup blocks instead of
  reading as "restored" (review of #305), except that the plan records each step's file `(dev, ino)`
  (`state::FileId`, Unix only) and that file at the step's destination proves the rename ran: the
  step is marked done and what the user put at the name it left is kept and named. Guards: the commit
  kill matrices (`a_save_killed_after_any_step_*`, `a_recovery_killed_after_any_step_*`,
  `a_file_recovery_settled_keeps_what_the_user_wrote_since`, `a_rollback_resumed_twice_keeps_what_it_decided`,
  `a_save_file_gone_after_the_plan_never_reads_as_a_step_done`, each asserting the promise: unblocked
  means every untouched file holds its pre-save bytes or is told kept),
  `every_change_is_flushed_before_the_record_says_it_happened` (call order through the `Disk` seam's
  test log), `a_file_made_again_after_its_removal_ran_is_kept_and_named` /
  `a_file_given_the_saves_text_after_its_restore_ran_is_kept_and_named`,
  `a_save_an_older_designer_left_recovers` (fixtures in `commit/fixtures/`, written by `056b4292`), and
  `api`'s `every_kill_during_recovery_still_tells_the_save_once` / `a_file_recovery_settled_is_never_settled_again`
  / `a_file_in_the_way_of_a_removal_is_its_own_problem`.
- **The designer frontend is Tailwind v4 + shadcn/Radix, atomic-design.** UI primitives are
  shadcn components in `apps/design/frontend/src/components/ui/` (`button`/`input`/`select`/
  `checkbox`/`dialog`/`tooltip`/`popover`/`command`/…), tuned to the flusso palette; molecules like
  `Hint`, `Field`, `Block`/`Bridge`/`Drawer`, the `Select`/`Combobox` pickers (the `Combobox` is a
  searchable Popover+cmdk list with optional free entry, used where a column is chosen) — all in
  `widgets.tsx` — compose them. There is **one** stylesheet,
  `src/index.css` (the Tailwind entry) — no `styles.css`. It maps shadcn's tokens onto the flusso
  palette via `@theme inline` (so `--primary` = brand emerald, `--accent` = the panel-3 hover
  surface — the flusso `--accent`/`--border`/`--muted` vars are never shadowed), binds `dark:` to
  `html[data-theme=dark]` (the app's dark-first signal), exposes the rest of the flusso palette as
  `@theme` tokens (`string`/`accent2`/`slate`/`warn`/`kind-*`) plus bespoke sizes (`text-2xs`/
  `text-3xs`), and holds the remaining bespoke component + React Flow `--xy-*` theming under
  `@layer components` (utilities still win). **React Flow's own `style.css` is `@import`ed into a
  dedicated `reactflow` layer** (ordered `theme, base, reactflow, components, utilities`) from
  `index.css` — *not* JS-imported in `Canvas.tsx`, because a JS import is unlayered and would beat
  the `@layer components` overrides (e.g. the connection-handle dots would fall back to RF's gray
  border-centred defaults). Keep new UI on shadcn atoms + Tailwind utilities, in
  **rem not px**, and **custom values — especially colours — as `@theme` tokens, never arbitrary
  `[var(--x)]`/`[0.6875rem]`** (`text-string`, not `text-[var(--string)]`).
  Every name box (index, sink, field, canvas title) goes through `components/nameDraft.ts`'s
  `useNameDraft`: the typed name is a draft in the UI store with its own `settle`/`drop` (Save
  settles, Undo/Redo/Reset drop; their buttons `keepFocus`), applied through the store's
  `renameField`/`renameIndex`/`renameSink`, which take an identity (uid, row id, sink id) and read
  the current state (`model/renames.ts`), never a closure's snapshot. **A blur inside a pointer
  press applies the name when the press ends**, after the click's handlers ran (`model/press.ts`:
  the pure reducer and `createPressTracker` on injectable timers, both under `npm test` in
  `scripts/press.test.mjs`, as `model/renameChips.ts` is in `scripts/renameChips.test.mjs`; the
  document-level capture listeners that feed it live in `nameDraft.ts`), so
  nothing re-renders mid-press and no click is lost to a moved or re-created control; Tab, Enter,
  unmount and Save apply at once, and a control acting on the renamed thing calls
  `settleNameDraft()` first (Save, duplicate, remove, Code mode). Lists are keyed by identity,
  never by a name a box can change (sidebar `e.id`, sink cards and per-sink sections
  `sinkIds[name]`, the Inspector by index row id + uid). Layout still never moves when a name
  applies (reasons float and take no pointer events, `model/renameChips.ts` keeps chips in place,
  a node header is one non-wrapping line); closing the palette hands the focus back through the
  draft's `refocus`. No test runs a browser, so a change here needs a by-hand check that each
  control near a pending name still takes its click; the canvas keeps React Flow's
  `measured`/`selected` across a re-projection, or every node is hidden until re-measured (issue #284).
- **The toolchain is pinned in `rust-toolchain.toml`** (CI's `dtolnay/rust-toolchain@stable`
  installs stable, but rustup honors the pin and switches to it). This exists because
  `flusso-query-derive`'s trybuild UI tests (`sdk/query-derive/tests/ui/*.stderr`) compare
  against exact compiler diagnostics, whose wording drifts between releases — a floating
  `stable` breaks them on every rustc bump. To upgrade: bump the channel, then re-bless with
  `TRYBUILD=overwrite cargo test -p flusso-query-derive`.

### Running the dev stack

```sh
docker compose up -d                       # Postgres + OpenSearch + Dashboards + Prometheus + Grafana
cargo run -- check --config dev/flusso.toml   # validate config/schemas against the DB
cargo run -- run   --config dev/flusso.toml --public-address 127.0.0.1:9464   # backfill + follow; serve metrics/status
```

The compose stack adds **Prometheus** (`:9090`) and **Grafana** (`:3000`, anonymous admin,
pre-provisioned with a flusso dashboard). Prometheus scrapes flusso's `/metrics` on the
*host* via `host.docker.internal:9464`, so run `flusso run` with `--public-address 127.0.0.1:9464`.
Prometheus config and Grafana provisioning live under `dev/prometheus/` and `dev/grafana/`.

The `Dockerfile` is a **registry-ready, config-less** image (its default `runtime` target
bakes no config and no secrets; you mount a `flusso.toml`/`flusso.lock` and pass `--config`,
or bake a lock into a child image). It builds the CLI with `-p flusso-cli
--no-default-features`, which **drops the visual designer** (`flusso design`) — the `design`
Cargo feature (default-on for a host install) pulls in the `flusso-design` crate + embedded
SPA, none of which a server image needs. It also defines a `demo` target that extends that
runtime with the repo's dev config compiled into `/app/flusso.lock` — that target is what the
demo compose builds.

For a **self-contained demo** that runs flusso *in* the cluster too (no host toolchain),
layer the demo override on the base the Docker way:
`docker compose -f docker-compose.yml -f docker-compose.demo.yml up --build`. The override
(`docker-compose.demo.yml`) just *adds* a `flusso` service built from the `Dockerfile`'s
`demo` target (release binary + a baked `flusso.lock`), pointed at the in-network services via
`SOURCE_POSTGRES_CONNECTION_URL` / `PRIMARY_OPENSEARCH_URL`. It publishes `9464`, so the base Prometheus scrapes
it via the same `host.docker.internal:9464` it uses for a host-run flusso — one config, both
modes. Same project as the base, so it shares its volumes; don't run a host `cargo run` flusso
and the container at once (same replication slot).

`default-members = ["apps/cli"]`, so bare `cargo run` is the `flusso` binary. CLI
subcommands: `build` (compile config+schemas → portable `flusso.lock`, no DB, no secrets
baked in), `check` (validate + print typed mapping; `--offline` skips the DB), `run`
(cargo-style: when a `flusso.toml` is present — the default path, or `--config` — it
recompiles and **rewrites the `flusso.lock`** (skipped when byte-identical) then runs that,
so the committed lock stays current; with no config it loads the existing `--lock`;
`--locked` runs the lock as-is with no recompile; a lock-write failure is fatal;
`--skip-backfill` resumes live only — see
`resolve_config`/`plan_config` in `apps/cli/src/commands/run.rs`), `schema` (print an embedded
editor-assist JSON Schema: `schema config` or `schema index`; no DB), `script run` (build one
document — `--id`, the first row, or `--input` JSON with no DB — run its index's scripts, print input,
prints, and output or the failure with a caret; `--watch` re-runs on file changes). See `dev/README.md` for the walk-through. **Every flag also reads a
`FLUSSO_*` env var** (clap's `env` feature; the flag wins when both are set) — e.g.
`FLUSSO_CONFIG`, `FLUSSO_SLOT`, `FLUSSO_PUBLIC_ADDRESS` — so the binary configures cleanly from
the environment (Helm/compose). This is separate from the config's own reserved env vars
(`SOURCE_POSTGRES_CONNECTION_URL`, `<SINK>_OPENSEARCH_URL`: one `<ENTRY>_<TYPE>_<KEY>` rule) and
`{ env = "VAR" }` secret refs.

A **Helm chart** lives at `deploy/helm/flusso/` — a single-instance Deployment (one
replication slot → `replicas: 1`, enforced; `Recreate` strategy), config via ConfigMap
(`config.flussoToml` + `config.schemas`, `--config`-mounted) and secrets via `envFrom`,
plus a Service, optional Prometheus-Operator `ServiceMonitor`, and probes on the HTTP
surface. Postgres/OpenSearch are external, not deployed by the chart. See its `README.md`.

## Workspace lints are strict — they fail the build

`Cargo.toml [workspace.lints]` denies (not warns) a large set. Code that trips these
**will not compile**:

- `unwrap_used`, `expect_used` — no `.unwrap()`/`.expect()` outside test code. Return
  `Result`. Test modules opt out with `#![allow(clippy::unwrap_used)]` / `#[allow(...)]`.
- `print_stdout`, `print_stderr` — use `tracing`, never `println!`/`eprintln!`.
- `indexing_slicing` — no `slice[i]`; use `.get(i)`, iterators, etc.
- `warnings = "deny"`, `unsafe_code`, `missing_debug_implementations` (every public type
  must derive `Debug`), `unused_*`, `unreachable_*`.

Why `clippy` runs **without** `--all-targets`: test targets use `.unwrap()`, which the
deny rules would reject. A test crate that needs a normally-denied dependency only at test
time uses `#![cfg_attr(test, allow(unused_crate_dependencies))]` (see `libs/2-engine`).

## Architecture

### Crate layering

Crates live under `libs/`, `apps/`, and `sdk/`; the **numeric prefix is the dependency layer** —
a crate only depends on lower-numbered ones (`0-kernel` → `1-{ports/*, config}` →
`2-{adapters/*, engine, script}` → `3-daemon` → `apps`, `sdk`). The five seams are named in `CONTEXT.md`:
**kernel** (the shared vocabulary), **ports** (source, stream, sink: the contracts the engine
drives), **adapters** (one implementation of one port for one technology, at
`libs/2-adapters/<port>-<technology>`), **engine** (the generic loop), **daemon** (the
supervisor that assembles a deployment). Keep this acyclic when adding crates; `libs/README.md`
is the crate table.

Layer 0 is the `kernel` (`libs/0-kernel`, plus its proc-macro `libs/0-kernel/derive`): the
**cross-cutting vocabulary** every layer trades in — `GenericValue`, the newtypes, `IndexMapping`,
`IndexSchema`, `Field`/`Filter`, `FailurePolicy`, `Secret`, the `Options` tree, `PortEntry`, and
the `AdapterConfig` trait. It names no adapter and no file format. Everything that turns config
*files* into that vocabulary lives a layer up in the `config` crate (`libs/1-config`): the two
file *parsers* as modules (`toml`, `yaml`), the **assembled `Config`** (source/stream/sinks as
`PortEntry`s + `Index`), the conversion, the loader, and the lock. **Adapters own their
configuration** (ADR 0001): each adapter crate declares its config struct with
`#[derive(AdapterConfig)]` (`source_postgres::PostgresConfig`, `stream_channel::ChannelConfig`,
`sink_opensearch::OpensearchConfig`, `sink_stdout::StdoutConfig`), and the CLI's registry
(`apps/cli/src/adapters.rs`) is the one place that maps a `type` to it. So the config crate
never names an adapter, an adapter can never reach the assembled `Config` (it depends only on
the kernel and its port), and adding an adapter is its crate + one registry entry + the build
arm in `apps/cli/src/backends.rs`. See "Config layer" below.

### The pipeline (`libs/2-engine/src/ingest.rs` + `sink_engine.rs`)

```text
                 ┌─ lanes (users,primary) (orders,primary) ─▶ SinkEngine(primary) ─▶ write ×writes ─▶ ack ─┐
ChangeCapture ─▶ IngestEngine ─┤                                                                          ├─▶ Progress ─▶ confirm
  resolve · build once · publish└─ lanes (users,audit)   (orders,audit)   ─▶ SinkEngine(audit)   ─▶ write ×writes ─▶ ack ─┘
                 ◀─ request lane ─ Backfill { index, sink } ─────────────────────
```

Two generic engines, backend-neutral (ADR 0002/0003, issue #130). The **ingest engine**
(`IngestEngine`) owns the source side: capture → batch → resolve → dedup → `build_many` once →
publish one `LaneItem::Batch` of kernel `Envelope`s per index to that index's lane of every sink; it also drains the
**request lane** and serves `Request::Backfill { index, sink }` as snapshots. One **sink
engine** per sink (`SinkEngine`), reading all its sink's lanes: `stage` (ensure indexes, retire stale seeds, request its
backfill) then recv → `write` (up to `writes` at once) → ack. Everything they drive — `ChangeCapture`,
`DocumentBuilder`, `Stream` (lanes + request lane), `Sink` — is a **trait object**; the engine's
`Progress` (`libs/2-engine/src/progress.rs`, one per deployment, shared by both engines and the fence)
is the one concrete piece.
`FanOutSink` is gone; sinks are independent. Key invariants to preserve when editing the engines:

- **One lane per `(index, sink)` (ADR 0012 stage 7).** `stream::LaneKey { index, sink }`; the daemon
  builds every enabled index × every sink (`Backends::stream(config, &[LaneKey])`), `Stream::lanes(sink)`
  is a sink engine's set. Publish (`ingest/commit.rs`) groups a built batch by index (`group_by_index`, one
  `Arc<[Envelope]>` per index, the documents moved, each piece carrying the build's `Hold`): one item per
  `(index, sink)` lane, keyed by the batch's last position on every lane; a lane with no item is
  `Progress::skipped`, so a quiet index pins nothing; `changes` rides on each sink's first item and a sink
  with no item hears `Observer::on_changes_settled`. Snapshots and `SnapshotComplete { index }` go to
  `(index, s)` of each requesting sink; requests are one per index and coalesce by sink set. The sink
  engine (`writes.rs`'s `Run`) keeps per lane a consumer, its window, counts and barrier; admission is per
  lane (two lanes never share a document); `pick` takes live first, every fourth contested grant to
  backfill, lanes taking turns (`turn`); an ordered sink writes the live front with the lowest position;
  `SnapshotComplete` is a barrier on its own lane only; after a failed run only the failed lanes
  (`Blame::Lanes`) write their front alone; a reindex is still staged sink-wide between writes. `Fences`
  and caught-up are per lane (a write is caught up when nothing else of its lane waits). The fence takes
  `indexes` (`/wait?index=…`, repeatable, `UnknownIndex` → 404); `/status` has `sinks.<s>.lanes.<index>`;
  `flusso.lane.outstanding{index,sink}`; `flusso-query` fences on the indexes a read reads, cached per
  `(level, index)` (`fence.rs`'s `After`). Guards: `engine` `tests::lanes`
  (`a_batch_is_split_by_index_and_a_quiet_index_pins_nothing`,
  `a_stalled_index_holds_back_neither_its_siblings_writes_nor_its_progress`,
  `lanes_take_turns_so_a_backlog_on_one_index_does_not_starve_another`,
  `a_snapshot_complete_waits_only_for_its_own_lane`), the channel's
  `a_release_on_one_lane_leaves_the_others_alone`, the daemon's
  `a_stalled_sibling_index_blocks_no_fence_on_the_healthy_index`, `positions_report_each_lane`, the CLI's
  `a_wait_on_named_indexes_echoes_them` / `an_unknown_or_invalid_index_is_refused`, `flusso-query`'s
  `a_fenced_client_waits_once_per_level_and_index`,
  `a_combined_search_and_a_multi_search_wait_on_every_index_they_read`.
- **Ready items merge into one write (stage 7, #397's revisit).** `writes.rs`'s `pick` takes the first
  item as before, then every other admissible item of the same kind (live or backfill), across lanes
  and of one lane, up to `SinkEngine::with_write_documents` (the daemon passes `[batch] max_changes`)
  and `Sink::write_limit` (OpenSearch: `batch_size`), never splitting an item, never waiting; it keeps
  the no-hash front pick unless another item could fit (`could_join_the_front`). One `writes` slot per
  write; the engine calls `Sink::write_parts(&[&[Envelope]], caught_up)` (default: `write` per part,
  `caught_up` on the last, reports folded; OpenSearch overrides it with one `_bulk`). Each part is
  decided alone: a `stop` rejection keeps that part unacknowledged and blames its lane only, the others
  land and are acknowledged; an `Err` blames every part's lane. Caught up when some lane of the write
  has nothing else waiting. A barrier lane and an ordered sink write one item alone. Guards:
  `tests::merge`, and `tests::concurrent`'s seeded schedule run with merging off and on (its other
  tests keep one item per write: `Rig::new` sets `with_write_documents(1)`).

- **At-least-once via write-then-ack and `Progress` (ADR 0012 stage 6).** A position describes
  itself: `kernel::Position { seq, fence_before, confirm_before, confirm_through }`, the stamps the
  source's own (Postgres, `cdc/ack.rs`'s `Positions::emit`, under the one lock it takes per change:
  `fence_before` = commit LSN − 1, `confirm_before` = the **horizon** before it, `confirm_through` =
  the commit LSN if it is its commit's last change, else `confirm_before`), so the capture keeps
  only `next_seq`, the horizon and the confirmed LSN, no map. The ingest engine **routes** a live
  batch (`commit.rs`'s `publish`): `Progress::published(lane, oldest, last)` registers it on each lane
  **as that lane's own send succeeds** (one lane per call;
  `skipped(lane, last)` for a lane a batch has nothing for), so a full lane holds back only itself. A sink
  engine raises its lane's **marks** (`Progress::applied` / `searchable`, in `writes.rs`'s
  `write_returned`, before the ack): the last item written, and made searchable, with every item
  before it in lane order (`Fences`' in-order answers); lane order is registration order, so a mark
  covers every registered item up to it, and one that runs ahead covers an item registered later.
  Each lane answers `Before(oldest unlanded)` / `Through(the newest position routed to it)` / `Idle`
  (`source::cdc::Landed`), and several lanes (`landed()`, `over`) answer as the lane that covers least,
  so a lane that hasn't taken a batch yet holds the others back and a fence on a healthy lane isn't
  held back by a full sibling. Both engines take the one `Arc<Progress>` in their constructors
  (`IngestEngine::new`, `SinkEngine::new`): none is built without it. The ingest engine hands it to `ChangeCapture::confirm` after a commit when
  it changed and on the 1 s tick **always**, since the source's answer moves with idle WAL while
  the case stays put; `drain_lanes` the same. The source decides what an emitted-but-unrouted change
  means: `Through(p)` follows the horizon only while `p.seq + 1 == next_seq`, else `p.confirm_through`;
  `Idle` follows it only while nothing was ever emitted (`FIRST_SEQ`), else nothing moves. Confirmation
  is monotonic. **Locks:** the registrations sit under one lock that only the ingest engine (once per
  lane per batch, and per confirmation) and a fence reader take; a sink engine never does. A reader
  reads every lane under it, so no registration falls between two lanes' answers (marks only grow: a
  mark read early makes its lane's answer older, never wider). Each mark is one `AtomicU64` with one
  writer (its lane's sink engine), raised by a plain load and store. A registration calls the fence's
  listener, which wakes waiters only while one is open. Per run of `live_drain_held/sinks2/1`
  (20,000 lane items), counted in the bench: lane ledger locks 60,030 → about 40,000–46,500 (timing),
  plus 30,002 registry locks and 40,000 mark stores.
  The Postgres capture confirms within ~1 s of the last flush (issue #205): `Positions::confirm` wakes
  the stream loop (`confirmed_moved`), which hands `confirmed_lsn()` to the replication client at once
  instead of at the next replication event, and `WalChangeCapture::new` sets the client's status and
  idle wake-up intervals to 1 s (`STATUS_INTERVAL`, fixed, no option). `pgwire-replication` 0.5 sends
  a raised LSN within its status interval on its own (issue #215), but only once `update_applied_lsn`
  is called, which only the stream loop can do (`recv` takes `&mut`), so the wake-up stays. Idle WAL
  is released by the tick's confirmation (keepalives move the horizon only; `read_through` confirms
  nothing). A sink engine that dies before its write returns leaves the item unacked; the lane
  redelivers it to the restarted engine (idempotent rebuild, deterministic id). Positions are
  monotonic for the life of the source across reopened streams (`Positions::new(start_lsn,
  first_seq)` continues `next_seq`), so the `next_seq` test stays exact across an ingest restart.
  Guards: `progress::tests` (`every_interleaving_answers_as_the_model_does`, 300 seeded interleavings
  against a model; `a_stalled_lane_pins_confirmation_and_its_own_fence_only`,
  `a_skipped_lane_pins_nothing`, `a_landing_reported_before_its_registration_still_counts`,
  `a_mark_never_moves_back`, `a_healthy_lane_reaches_what_it_landed_while_a_sibling_lane_is_full`,
  `several_lanes_answer_their_least_and_an_unknown_lane_none` (successor of the channel's
  `watermark_is_the_minimum_over_lanes`), `concurrent_registration_and_landing_never_answer_past_a_lane`,
  threads against an oracle), `tests::lanes`' `a_healthy_lane_reaches_what_it_landed_while_a_sibling_lane_is_full`
  (engine) and the daemon's `tests::lanes` `a_fence_on_a_healthy_sink_passes_while_a_sibling_lane_is_full`, `confirms_no_position_before_its_flush`,
  `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, the channel adapter's
  `several_deliveries_can_be_outstanding_and_ack_independently`, the daemon's
  `failed_sink_engine_restarts_and_redelivers` / `failed_ingest_engine_restarts_and_resumes_the_stream`,
  the Postgres adapter's `ack::tests` (`a_change_emitted_but_not_routed_is_never_fenced_or_confirmed`,
  `the_tick_releases_idle_wal_while_routed_stays_put`, `a_fresh_process_never_confirms_its_first_unrouted_change`,
  `an_idle_stream_after_an_ingest_restart_still_releases_wal`, `a_reopened_stream_continues_the_numbering`,
  `a_position_is_stamped_from_its_commit_and_the_horizon_before_it`),
  `a_confirmation_reaches_the_client_without_a_replication_event` (`cdc::stream` unit, over the
  `Replication` seam), and the `wal_release` e2e (`the_slot_confirms_within_seconds_of_the_last_flush`,
  `CONFIRM_BOUND` 5 s; an empty batch is still published and lands, keepalives move the horizon, a
  `backfill = false` lane lands).
- **A fence only observes, per sink, at two levels (issue #136, ADR 0009).** A client reads the
  **source position** after its commit (`pg_current_wal_lsn()`) and waits on the daemon's `Fence`
  (`libs/3-daemon/src/fence.rs`; the CLI serves it as `GET /wait`) until a sink, or every sink, has
  every commit at or before it **applied** (its write and every earlier one returned) or **searchable** (a refresh covered
  it). Nothing is flushed or refreshed for a waiter, and a backfill is not covered. Three pieces,
  each load-bearing: (1) `ChangeCapture::reached(Landed) -> Option<SourcePosition>` translates how
  far a sink's lane landed (`Progress::lane(sink, Level::Applied | Searchable)`) into the source's
  terms. The Postgres `Positions` (`cdc/ack.rs`) reads it off the stamps plus a **horizon** (the LSN
  through which everything read is emitted: moved by the last change of a commit being emitted, and
  by a keepalive or a filtered commit while nothing is buffered; seeded from the slot's
  `confirmed_flush_lsn`). `Before(o)` answers `o.fence_before`, **strictly below the oldest
  outstanding change's commit**, because the changes of one transaction share its commit LSN: a
  transaction split across batches is neither confirmed nor fenced until its last change has landed.
  A fence claims everything short of that commit; `confirm` stops at `o.confirm_before`, the **end of
  the last covered commit**, because Postgres skips a transaction whose commit record *starts* before
  the confirmed LSN (never confirm into an unprocessed commit's record). (2) A sink
  *reports* visibility (`WriteReport.visibility`: `Immediate` / `Within(d)` / `Deferred`); the
  sink engine keeps the positions it received in position order (`sink_engine/fences.rs`, kept
  across restarts of its run), reports each item landed and how far it is searchable to `Progress`
  **before the ack**, and `Observer::on_position_applied` / `on_position_searchable` in position
  order (the daemon's `FenceObserver` only wakes waiters on them), with a timer arm for searchable. A **caught-up** write (nothing else
  queued or waiting) makes every write that returned before its grant searchable (the
  `Sink::write` contract; each returned write gets a sequence number), and before the ack that
  leaves the engine idle a position not yet searchable gets an **empty caught-up write**, since
  the last writes of a burst can return after the caught-up one. The OpenSearch sink reports
  `Within(refresh_interval)` for an unforced write (`Deferred` with `refresh_interval = "-1"`),
  remembers every index an unforced write touched, and refreshes them in the next caught-up write,
  one caught-up refresh at a time (`refreshing`: take, refresh, put back on failure), so a second
  caught-up write never answers `Immediate` over a refresh still running or failed (guards
  `a_caught_up_write_waits_for_an_earlier_refresh_still_running`,
  `a_caught_up_write_refreshes_what_a_failed_refresh_put_back`), skipping an index its own
  `?refresh=true` bulk covered: one it wrote, with one primary shard (read per generation in
  `ensure_index`, `IndexTarget.single_shard`; a bulk refreshes only the shards it touched) and marked
  before the bulk was sent (`refresh.rs`'s `Unrefreshed`, marks with sequence numbers; guards
  `a_caught_up_write_over_the_one_index_left_unrefreshed_sends_no_extra_refresh`, `refresh::tests`); its
  `ensure_index` refreshes a seeded generation once per start, because what an earlier run left
  unrefreshed is remembered nowhere (fences are unavailable until staging is done). An index
  mid-reindex is **not** covered: live writes go to the new generation while the alias serves the
  old one (#6). (3) The fence reads both levels off `Progress` per sink whenever a waiter checks
  (searchable never above applied), and keeps each level monotonic across a reopened source stream. Guards: the Postgres adapter's
  `a_split_transaction_is_not_confirmed_until_its_last_change`,
  `a_keepalive_during_a_flight_does_not_pin_the_resume_point`,
  `confirmation_stops_at_a_commit_boundary_where_a_fence_goes_further`, `reached_*`,
  `emitting_the_last_change_of_a_commit_moves_the_horizon`; the engine's
  `a_position_becomes_searchable_when_its_refresh_interval_passes`,
  `a_deferred_position_waits_for_a_caught_up_immediate_write`,
  `an_idle_engine_makes_its_last_writes_searchable`, `fences::tests`; the daemon's `fence::tests`
  (`searchable_trails_applied_until_the_engine_reports_it`,
  `a_stalled_sibling_blocks_an_all_sinks_fence_but_not_one_on_the_healthy_sink`,
  `idle_source_progress_wakes_a_waiter`); the CLI's `http::tests`; and the `fence` live e2e.
- **A change is emitted only once its transaction is visible (issue #343, ADR 0010).** Postgres
  flushes a commit record before new snapshots see the transaction, so the walsender can deliver a
  change every read would still miss. The contract is on the port (`ChangeCapture::live`: a change
  is yielded only once it can be read back); the engine is unchanged. The Postgres stream
  (`cdc/stream.rs`) keeps `Begin`'s xid and holds committed transactions in `pending`, in commit
  order; the front one is emitted only once its xid is `readable`. A snapshot read
  (`pg_current_snapshot()::text`, judged in Rust by `cdc/visibility.rs`'s `Snapshot::sees`, the
  32-bit xid **widened by its signed distance to `xmax`**, which is last-finished + 1, not the next
  xid) runs on **its own task** (`refresh`, one read at a time) while an xid is `unchecked`, so
  reads overlap the engine's build; the task lives as long as the stream and sleeps on
  `Refresh.wake` otherwise (the stream stores `wanted` then `notify_one`s, and `Notify` keeps the
  permit), because a task that exited on its own missed a commit arriving as it went idle (item 1
  of the #343 review; guard `a_commit_arriving_as_the_snapshot_task_stops_is_still_checked`). A
  read has its own one-connection pool (`visibility_pool`), times out after `READ_TIMEOUT` (5 s)
  and is retried for `READ_FAILURES_FOR` (30 s) before the stream ends with it. The stream keeps
  the newest snapshot (≤ 1 s, `SNAPSHOT_REUSE`) and checks each commit against it on arrival, so
  a lone change costs one read, idle none, a lagging burst a few. A read per check, inline or one
  task each, made the stream not-ready and halved `drain_ready`'s batches (256 flushes for a
  burst main did in 117); burst batches are cut by `max_delay`, not by the stream. While the front is held the loop `select!`s
  `confirmed_moved` (confirmations keep reaching the client, #205), `recv` up to the read-ahead
  bound (`with_read_ahead`, the CLI passes `max_changes × 16`) and the poll schedule
  (`poll_delay`: 0.5 ms for 20 ms, tokio rounds it to 1 ms, then doubling to 50 ms). A transaction
  touching no table an index reads (`required_tables`, by name) needs no check. **Fences and
  confirmation stay honest only because the hold lives in `pending`**: nothing there is registered
  and the horizon never moves past buffered changes; hold anywhere else and both guards need
  extending. A hold past 5 s warns (sessions with `wait_event = 'SyncRep'`; the inspection is
  bounded and best effort), every minute after, and stops the stream if the server now takes in
  flusso as a synchronous standby (`refuse_standby`/`standby_listing`: by name, case-insensitive,
  or by `*`, each with its own remedy; the walsender's `sync_state` as a cross-check, NULL without
  `pg_read_all_stats`). `WalChangeCapture::check_server` refuses a server in recovery or one taking
  in flusso, before `prepare` writes, in every `live` (on the SQL pool and on the gated
  replication session), in the CLI's `Backends::source`, so `run` exits, and in `flusso check`
  (`backends::source_server_problem`, printed under *Source server*, `server` in JSON).
  `ChangeCapture::readable_wait` → `Status::with_source` → `/status` `visibility_wait_seconds` and
  the `flusso.replication.visibility_wait` gauge (read live; dev dashboard panel and
  `FlussoVisibilityWait` alert). The `HoldClock` records the xid it times and outlives a stream:
  a reopened stream holding the same xid goes on from its first hold (the warning too), cleared
  once that xid is emitted and on a refusal. One held
  transaction stalls the whole stream; no opt-out. Guards: `engine`'s `visibility` e2e (every
  scenario holds commits on a standby that never answers: nothing written during an 8 s hold, the
  document within 1 s of the release; root/child/re-parent/delete/count, a later commit released
  first, the slot not confirmed past a held commit, restart, backfill and a split transaction
  during a hold, `COMMIT PREPARED`, an unread table not held, the refusal at start and while
  running); `daemon`'s `fence` `a_fence_never_covers_a_commit_read_before_it_was_visible`;
  `visibility::tests` (widening at epochs 0 and ≥ 1, the standby-name forms, the schedule) and
  `postgres_agrees_on_every_widened_xid` (Docker, against `pg_visible_in_snapshot`);
  `stream::tests`' `a_held_transaction_is_not_emitted_and_nothing_passes_it`,
  `an_earlier_held_transaction_holds_back_a_later_visible_one`,
  `a_confirmation_is_handed_over_during_a_hold`, `a_hold_reads_no_further_than_its_bound`,
  `a_hold_on_a_server_waiting_for_flusso_stops_the_stream`, the race test above,
  `a_snapshot_read_that_never_answers_ends_the_stream`,
  `a_snapshot_read_failing_for_a_moment_is_retried`,
  `a_stall_inspection_that_never_answers_leaves_the_hold_running`,
  `a_reopened_stream_times_the_same_transaction_from_its_first_hold`,
  `a_stall_refusal_clears_the_clock`; `capture::tests`' `a_standby_is_refused`,
  `a_refusal_clears_the_hold_clock`; `visibility::tests`'
  `the_refusal_names_a_remedy_that_works_for_its_form`; `flusso-cli`'s `source_server` e2e (`check`
  and `run` refuse `ANY 1 (*)` alike). The e2e helper `SyncRep::configure` waits until a probe
  commit is seen in `SyncRep` (`standbys_in_force`): whether a commit waits is a flag the
  checkpointer sets on reload, after sessions see the setting, and an unforced probe table
  creation hung on a session the reload signal hadn't reached.
- **Built once; grant order is delivery order.** A batch buffers `(Position, ChangeEvent)`s,
  resolves each to `DocumentId`s, dedups, then `build_many` assembles each touched document
  **once** for all sinks. A backfill (a *snapshot*) goes through the same build but is **never
  resolved** (issue #385): it walks each requested index's root keys in pages
  (`DocumentBuilder::keys(index, after, through, limit)`, `engine::KEY_PAGE` = 1,024, the engine
  holding the last key read; issue #386 stage 3) up to `through`, the index's greatest key read
  once when its walk starts (`DocumentBuilder::last_key`): a row past it got its key after the slot
  existed, so the live stream delivers it, and writers inserting greater keys can't keep a walk going
  (guards: `tests::snapshot`'s `a_walk_ends_at_the_greatest_key_its_index_had_when_it_started`,
  `keyset`'s `a_walk_ends_at_the_greatest_key_it_found_when_it_started`, `keyset_backfill`'s
  `rows_added_ahead_of_the_walk_end_it_where_it_began_and_are_built_once`). Each key is its own
  document of that index (a key with a null part addresses nothing), built once per snapshot. The
  Postgres page is `WHERE key OPERATOR(s.>) $1::<bound>::<key type> ORDER BY key USING OPERATOR(s.<)
  LIMIT n` with `(key OPERATOR(s.<=) $2…) AS within` in the select list (a second range condition in
  the `WHERE` made a generic plan sort every page), on the builder's pool, one statement, no
  transaction (`document/keys.rs`, `query::key_page_query`/`last_key_query`). The operators are the
  primary key index's own btree operators, read from its opclass and written schema-qualified, so the
  index serves the order whatever the search path (a `citext` in `ext` compared as `text` would skip
  rows); a role that may not use their schema gets the bare names and `USING <`, correct but sorting
  each page, warned once per index (guards: `keyset`'s
  `a_key_whose_operators_are_off_the_search_path_is_paged_once` and
  `…_is_walked_through_its_index`, by `pg_stat_user_tables`).
  No snapshot spans pages: `prepare` made the slot first, and a page names ids only, each document
  read when it is built. Only a root whose single-column catalog primary key is the declared one is
  paged; any other answers an empty page with a warning (#391). The page future lives in
  `ingest/snapshot.rs`'s `Walk`, not in the `select!`, so a live change winning it drops no page. Resolving it like a live change ran a reverse query per related path and row
  and rebuilt related indexes' parents in every batch a child row landed in. They are published
  *without* a position and *only* to the requesting lanes, ending with `LaneItem::SnapshotComplete`;
  up to `builds` batches build at once on the one ingest task (`[concurrency] builds` /
  `--builds` / `FLUSSO_BUILDS`, default 4 via `daemon::DEFAULT_BUILDS`, at most 32;
  `IngestEngine::with_builds`, default 1; ADR 0012 stage 4, `ingest/builds.rs`), **published in
  grant order** through a reorder queue of at most `2 × builds` slots (`SnapshotComplete` takes a
  slot of its own), so lane order is grant order and live positions only grow. A batch is granted
  only when no running build holds one of its documents (sorted 64-bit id hashes; nothing hashed at
  `builds = 1`), so a later build of a document starts after the earlier one returned, reads newer
  state, and lands later. Live goes first except every fourth contested grant (the counter moves
  only on contested grants); a second live build waits for a full batch. A build is polled once
  where it is granted and spawned only if that doesn't finish it (mocks never spawn); spawned ones
  are awaited through `Flights`. A failed build (or a `Document::Failed` under `stop`) stops
  grants; slots before it are published, nothing from it on, and the run awaits every running build
  before returning the error. The stop and the stream's end `settle`: the buffered live batch once
  its ids are free, then every build, in order. The builder's pool is `builds + 2`
  (`backends.rs`); `resolve` and key pages run beside the builds. The sink engine keeps that order
  **per document** while it writes several items at once (admission, below), so sinks need no
  versioning. Guards: `ingest::tests` (`random_schedules_keep_every_document_in_order`, seeded,
  `FLUSSO_BUILD_SCHEDULE_SEED` replays; `builds_run_at_once_up_to_the_setting`;
  `a_document_is_never_built_twice_at_once`; `every_fourth_contested_grant_goes_to_the_backfill`;
  `a_failed_build_publishes_nothing_after_it`;
  `a_document_failed_under_stop_publishes_nothing_after_it_and_skip_goes_on`;
  `a_stop_finishes_the_builds_in_flight_and_starts_no_other`;
  `one_build_at_a_time_builds_and_publishes_as_before`, recorded from the serial engine),
  `builds_a_repeatedly_touched_document_once_per_batch`, `snapshots_go_only_to_the_requesting_lane`,
  `tests::snapshot`'s `a_backfill_of_related_indexes_builds_each_document_once` /
  `a_backfill_beside_seeded_related_indexes_writes_them_nothing` /
  `a_backfill_of_one_of_two_indexes_on_a_table_builds_only_that_one` /
  `each_page_follows_the_last_key_the_walk_read`, `engine`'s `keyset_backfill` e2e, and `engine`'s `backfill` e2e
  (`pg_stat_statements`: no reverse lookup; every document once and equal to its `build`; two
  indexes on one table, a self-referencing `has_many`, `soft_delete`, a root filter, seeded
  siblings).
- **Backfill is each *sink's* decision, and requests are at-least-once.** `stage` asks the sink
  `is_seeded` per index and sends one `Backfill` request for the unseeded set (skipped under
  `--skip-backfill` or the sink's `backfill = false`, the stateless-sink opt-out); the ingest
  engine coalesces requests into one snapshot fanned to every requester — it drains what is
  already queued with no wait (every startup request is, since the daemon stages every sink
  first) and holds for stragglers only while some lane has not asked, for at most
  `BatchPolicy::max_delay` (a seeded sibling that never asks costs one `max_delay`; a straggler
  past it gets its own snapshot) — and acks a request only after its `SnapshotComplete` is
  published (a crash mid-snapshot redelivers it); the sink engine keeps an `outstanding` set of requested,
  not-yet-complete indexes so re-staging after its own restart (and a reindex control for the
  same index) doesn't ask twice. A seeded sibling sees nothing. The channel adapter therefore keeps
  a **ticketed in-flight ledger** per lane: several deliveries can be outstanding, each acked by
  ticket; a nacked or dropped-unacked delivery is marked due by its own ticket and redelivered alone,
  due ones in lane order, never the deliveries still in flight beside it, so a released delivery can
  follow later acked ones: the sink engine's admission keeps document order (issue #395; guards
  `releasing_one_delivery_redelivers_only_that_one`, `released_deliveries_come_back_in_lane_order`). (A single redelivery slot
  made the coalescing loop receive the same request forever — the OOM behind the first attempts.)
  Guards: `an_unseeded_sink_requests_a_snapshot_then_marks_seeded`, `a_seeded_sink_requests_nothing`,
  `backfill_false_makes_a_stateless_sink_live_only`,
  `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`,
  `a_backfill_with_every_lane_asking_starts_without_waiting`,
  `a_straggling_reindex_request_within_max_delay_joins_the_snapshot`, the daemon's
  `unseeded_sink_is_backfilled_without_touching_its_sibling`.
- **A sink's indexes are staged concurrently (issue #398).** `SinkEngine::each_index` runs one
  `Step` per index, `STAGE_CONCURRENCY` (8) at once, each index's own steps in order; the `Sink`
  port allows `ensure_index`/`is_seeded`/`reindex` at once for different indexes, never for one,
  never beside `write`/`mark_seeded`. After a failure no further index is started (a
  hanging cluster would cost a timeout per wave), the ones in flight finish, and
  `EngineError::Staging` names every failure seen; the daemon retries the whole stage. A reindex
  control stages the same way, each index once even when named twice. `validate_indexes` checks `VALIDATE_CONCURRENCY` (4) indexes at
  once, reported in index order. Guards: the engine's `tests::staging`, the daemon's
  `every_index_of_every_sink_is_staged_before_the_source_prepares`, `validation::tests`'
  `indexes_are_checked_concurrently_and_reported_in_order`, the OpenSearch `reindex` e2e's
  `many_indexes_staged_at_once_each_get_their_own_generation` (24 at once on a fresh cluster).
- **A live stream that ends still drains queued requests.** The ingest engine returns `Ok` only
  once the live stream ended *and* no snapshot is active *and* the request consumer is empty, so
  a sink that staged its backfill before the ingest engine started is still served.
- **A stop is a graceful drain, not an abort (issue #371).** `engine::Stop`, handed to the ingest
  engine (`with_stop`) and the supervisor by the daemon, is checked between the live stream's items
  (`next_live` polls it before the stream, `drain_ready` stops at it), never by aborting a task. Once
  requested: the phase is `stopping` (not ready; `DaemonControl::reindex` → `ControlError::Stopping`);
  nothing more is read and no request served; a snapshot in progress is dropped unacked (the sink
  re-requests it on the next start, a backfill being idempotent), and a sink engine handed the stop
  (`SinkEngine::with_stop`) acks snapshot batches still waiting on its lane unwritten (the writes in
  flight finish) and skips their index's `SnapshotComplete`, so it isn't marked seeded with documents
  missing (the empty caught-up write before the ack that leaves the engine idle makes a `Deferred`
  position searchable: the last live write ran with snapshot batches still queued, guard
  `a_deferred_sink_still_refreshes_when_skipped_snapshot_batches_follow_the_last_live_one`);
  the buffered batch is published as a
  normal commit; `engine::drain_lanes` (the same drain `supervise` runs when the stream ends) waits until
  every lane is acked, confirming what landed as it moves; then `ChangeCapture::close_live` (Postgres:
  `Positions::request_close` → the stream loop hands `confirmed_lsn` over, `Replication::close` waits until
  a status update carried it, then stops the worker: stopping first would skip the update, the worker
  checks its stop before a due status; with nothing confirmed past the stream's start position, which
  the slot already holds, it stops at once) and the engine polls the stream to its end. A close that
  couldn't hand the position over (no status update in 5 s, `Replication::close` → `false`) ends the
  stream with `the final position was not confirmed to the source`, as does a stream that doesn't end
  within `CLOSE_WAIT`: the run fails, so the stop exits 1, never 0, with the position unconfirmed
  (guards: `a_close_whose_final_position_never_reaches_the_server_ends_the_stream_with_an_error`,
  `a_stop_whose_final_position_never_reaches_the_source_fails_the_run`). A stalled sink holds
  the drain (confirmation follows the slowest lane); the caller's deadline bounds it. After it the supervisor
  waits on `Fence::settled` only while a `/wait` on `searchable` is open, so a stop nobody waits on
  doesn't sit out a `Within(refresh_interval)`. A stop during staging ends before `prepare`; an ingest
  engine that fails while stopping is not restarted. Guards: `engine` `tests::stop` (a stop raised mid-burst
  ends the batch there, `drain_ready`'s check; mid-build batch
  published and flushed by every sink, final confirm before close; nothing read after the stop; an
  abandoned snapshot re-served; queued snapshot batches skipped, their index left unseeded; a stalled
  sink; a stop before the run), `daemon` `tests::stop`
  (staging, `stopping` + refused reindex, searchable waiter vs none), `source-postgres`'s
  `a_close_hands_over_the_confirmed_position_then_ends_the_stream_without_a_change`, and the
  `flusso-cli` `shutdown` e2e (steady writes: everything at or before `confirmed_flush_lsn` indexed, a
  restart rewrites no `_version`; a paused OpenSearch: exit 1 at the deadline naming the sink, a
  restart converges; a fence position answered by the next process at both levels;
  `random_stops_under_writes_converge_after_a_restart`, seeded, `FLUSSO_STOP_SEED` replays: stops at
  random points of a backfill and live writes, then the index must equal the table).
- **…but a seed is only as good as the stream behind it (issue #120).** Run order is
  `source.continuity()` → stage every sink (`ensure_index` all, on `Fresh` `reindex` +
  `ensure_index` every still-`is_seeded` index, request backfill) → `source.prepare()` (the
  ingest engine's first act) → serve requests + live. `continuity` is **read-only** (Postgres:
  does the slot exist?); `prepare` creates the resume point. **The order is load-bearing twice**:
  rebuilds are staged *before* `prepare` creates the slot (a crash in between comes back `Fresh`
  and re-stages) and `prepare` runs *before* any snapshot (a write no key page sees is in the
  stream). The daemon enforces it by finishing every sink's first `stage` before
  spawning the ingest engine. Only *seeded* indexes are staged (#121). Under `--skip-backfill`
  staging warns only. Both trait methods are required. The sink-side half: the OpenSearch
  `ensure_index` retracts a seeded marker whose generation index is missing; while running,
  seeded writes go through the hash alias with `require_alias`, so a deleted generation is a
  write-wide `Err` and the restart re-stages it (#123). Its `reindex` no-ops while unseeded and
  `mark_seeded` drops every non-active generation (#121): refresh, `refresh_interval` restore and
  the holder/generation lookups run at once, then one atomic `_aliases` call moves both aliases
  and `remove_index`es the stale generations (refused: re-read, hash alias alone), then the marker,
  last (#354, #410). `ensure_index` creates a first generation with its aliases in the create
  request, a rebuild's bare (#403). `mark_seeded` is safe for several indexes at once beside other
  indexes' writes (one meta doc per index, disjoint aliases). Guards: `seeding` e2e (behind a
  fault proxy: request order, a crash after every request, `several_indexes_seed_at_once_*`;
  `docs/adr/0012-stage-notes/seed.md`). Guarded by the
  engine's `fresh_source_*`/`skip_backfill_with_a_fresh_source_*` unit tests, the daemon's
  `fresh_source_rebuilds_seeded_sinks_before_preparing`, the sink's
  `deleted_generation_is_recreated_and_reported_unseeded`,
  `generation_deleted_while_running_fails_the_flush_and_is_rebuilt`,
  `a_second_reindex_while_rebuilding_stages_nothing`, `mark_seeded_sweeps_an_orphan_generation` e2e, the Postgres `continuity` e2e,
  and the two `restart_*` cases in `engine`'s `pipeline` e2e.
- **Reindex is an operation on one sink, no restart.** `DaemonControl::reindex(index,
  Option<SinkName>)` sends `SinkControl::Reindex` to the targeted engines; once its writes in flight
  returned (no new write starts meanwhile) each stages `reindex` + `ensure_index` and sends a fresh
  `Backfill` request (guard `a_reindex_waits_for_the_writes_in_flight`). Untargeted sinks
  are untouched; "all" coalesces into one snapshot; a `backfill = false` sink ignores it. The
  `Envelope` a sink receives is the ingest engine's one build, shared: `Batch.envelopes` is an
  `Arc<[Envelope]>`, so publishing to every lane and the channel's redelivery ledger copy no
  documents, and nothing downstream mutates it — an emitting sink stamps its own `SinkName`
  as it writes (`sink` on the wire; the stdout sink takes its name at construction), `position` serializes as the opaque `seq` string, and `Envelope<D = GenericValue>` is
  generic over the document so a consumer deserializes an emitting sink's output as the same
  type. Guards:
  `reindex_control_stages_and_requests_a_snapshot_without_restarting`, the daemon's
  `reindex_operation_targets_one_sink`.
- **Built documents are held within a byte budget (ADR 0012 stage 5).** `[concurrency] max_built_mib`
  (`--max-built-mib` / `FLUSSO_MAX_BUILT_MIB`, 16..=1,048,576, default 512 via
  `daemon::DEFAULT_MAX_BUILT_MIB`; never in the lock) is an `engine::Budget` the daemon hands the ingest
  engine (`IngestEngine::with_budget`; without one nothing is counted) and `/status` (`memory`). A grant
  reserves an estimate (ids × a running average bytes per document, per kind) in `Builds::choose`: a
  candidate the budget can't fit is not admissible, so the gate's every-fourth counter doesn't move, and
  the loop waits on `Budget::released` (`ingest.rs`'s release arm; `settle` waits too). The build future
  walks its documents (`budget::document_bytes`, B-tree nodes not entries; `tests/heap_walk.rs` holds it
  to a counting allocator within 0.8–1.2) and the reservation settles to that, at least `FLOOR` (1 KiB).
  The `Arc<Charge>` rides every lane's piece as `stream::Batch.hold` (an opaque `Hold`, serde-skipped,
  equal to any other), so the bytes return when the last copy drops: every lane acknowledged and no
  ledger can redeliver. Rules: total within the limit, backfill within half, and a kind holding nothing
  is always granted one batch (so a batch larger than the budget builds alone); held stays within the
  limit plus one estimate per build in flight. `[stream] capacity` / `--queue-capacity` are deprecated
  (load, warn, ignored); `ChannelStream::from_config` gives lanes `LANE_BACKSTOP` (1,048,576) items,
  which the floor keeps from binding (`ChannelStream::new` keeps an explicit capacity for tests).
  Metrics `flusso.memory.built{work}` and `flusso.memory.refused{work}` (once per wait). Guards:
  `budget::tests`, `ingest::tests`' `random_schedules_under_a_tiny_budget_stay_within_it_and_finish`
  (held ≤ limit + one estimate per build at every write, every byte back at the end),
  `a_stalled_sink_paces_builds_and_they_resume_when_it_does`,
  `live_keeps_room_beside_a_backfill_holding_its_half`, `a_batch_bigger_than_the_budget_still_builds`,
  `a_batch_redelivered_after_a_failed_write_gives_its_bytes_back_once_written`; every `ingest::tests`
  deployment with a budget asserts it returns to zero (`Deployment::assert_released`).
- **A lone change on a quiet stream is committed at once (issue #135).** `BatchPolicy`
  (default 256 changes / 50 ms; `flusso.toml`'s `[batch]` table `max_changes`/`max_delay_ms`,
  laid over the defaults by the daemon; `--batch-max-changes`/`--batch-max-delay-ms` +
  `FLUSSO_BATCH_*` override at run time, never baked into the lock) controls batch grouping:
  after the live arm buffers a change, `IngestEngine::drain_ready` buffers every change the
  stream has ready (`now_or_never`) while the batch `is_open` (not full, deadline not passed), and
  the batch goes to the build gate the moment the stream would block, so a burst fills batches for
  at most `max_delay` while a trickle never waits it out. Grants are event-driven (a buffered change,
  a landed build, a budget release, the snapshot taking keys); there is no deadline arm: a batch past
  its deadline only stops taking changes, and is granted when the gate allows. `max_changes:
  1` reproduces a write per change. The coalescing wait for further requests is `max_delay`. Guards:
  `a_lone_change_on_a_quiet_stream_commits_without_waiting`,
  `max_delay_caps_a_batch_while_changes_keep_arriving` (both on a channel-fed `MockSource::fed`
  whose live stream stays open and pending).
- **Several writes per sink, one document at a time (ADR 0012 stage 2).** A sink engine writes up to
  `writes` lane items at once (`[concurrency] writes`, `--writes` / `FLUSSO_WRITES`, default 4
  `daemon::DEFAULT_WRITES`; `SinkEngine::with_writes`, default 1; `Config.concurrency` is
  `#[serde(skip)]`, **never in the lock**, so `run --locked` takes the flag, env or default). It takes
  an item from the lane only to use it (a free write slot, or a live item to find behind backfill)
  into a **window** of at most `LOOKAHEAD` (32) waiting items, so a sink holds at most `writes + 32`
  delivered items beside the lane's `capacity` (`sink_engine/writes.rs`; `wants_more`). A granted
  write is polled at once and one that completes on its first poll is handled there (one still running
  waits in `sink_engine/flights.rs`'s `Flights`, polled only when its own waker fired: `FuturesUnordered`
  woke the run after every all-pending pass, an empty wake-up per waiting write that failed the gate
  at `live_drain_held/sinks2/1`; guard `a_pass_that_leaves_every_future_pending_does_not_wake_the_run`); `grant` stops
  once that fails the run. While there is room the `select!` awaits the lane's `recv` (the
  wake-up for the next item), through `next_delivery`, which makes the boxed `recv` future only when
  polled (`select!` evaluates a disabled branch's expression too); between waits, `fill` takes what
  is queued only when `is_empty` says something is (the channel answers from atomics, its due count
  and the queue's capacity, never the ledger lock the ingest shares; an acked handle takes no lock
  on drop; guard `only_a_handle_dropped_unacked_is_redelivered`). With nothing waiting and no room the
  run waits only on the landing write, a control message and a due position (the lane, a stop and the
  idle tick can't matter there; guards `a_reindex_taken_before_a_failed_write_is_staged_after_the_restart`,
  `a_stop_during_the_only_write_still_skips_the_backfill_behind_it`). **Admission**: an item starts
  only when no earlier unfinished item (in flight, or waiting ahead of it) shares a document
  (`(index, id)` hashed to `u64`, only when a comparison is needed: nothing in flight and one kind
  waiting grants the front item unhashed; a collision only holds an item back). **Selection**: live
  (positioned) items first, every fourth *contested* grant (both kinds admissible) to the oldest
  admissible backfill item; a `SnapshotComplete` starts once it is the window's front and nothing is
  in flight, and nothing behind it starts until it is done; a reindex waits for the writes in flight.
  A write is **caught up** when the window and the lane have nothing else. A **failure** (a write
  `Err`, a `stop` rejection) starts nothing new: the writes in flight are awaited and acked when they
  succeeded, then the run ends with the first error and the lane redelivers only what is unacked;
  the engine remembers the failed run (`failed_run`, also set on a fresh engine, so every process
  writes its oldest unconfirmed item alone), and the next run is a **barrier**: it grants only the
  window's front item until a write lands, so a `stop` rejection or a write refused every time halts
  the sink across engine restarts (nothing behind it lands, the backoff grows, the lane fills); in
  the run that first meets it, disjoint writes started while it is in flight can land, confirmation
  staying before it; a new process whose oldest unconfirmed item was already written (a sibling
  lags, a confirmation was lost) lands it and opens the window, and disjoint items past the failing
  one, granted beside it or while it is in flight, can land before it fails (nothing lost).
  A lane closing ends the run once the window and the writes in flight are done. Guards: the
  engine's `tests::concurrent` (`a_document_is_never_written_twice_at_once_and_its_versions_never_go_down`,
  seeded, `FLUSSO_SCHEDULE_SEED` replays; `writes_run_at_once_up_to_the_setting`,
  `an_item_waits_for_an_earlier_one_sharing_a_document`,
  `a_later_write_finishing_first_moves_neither_confirmation_nor_the_fence`,
  `live_goes_first_except_every_fourth_contested_grant`, `a_snapshot_complete_is_a_barrier`,
  `a_reindex_waits_for_the_writes_in_flight`, `a_failed_write_lets_the_others_finish_and_comes_back_alone`,
  `a_stop_finishes_the_writes_in_flight_and_skips_waiting_backfill`,
  `an_idle_engine_makes_its_last_writes_searchable`,
  `after_a_failed_run_the_oldest_item_is_written_alone_until_it_lands`,
  `a_fresh_engine_writes_nothing_past_a_persistent_stop_rejection`), the daemon's
  `a_stop_rejection_halts_a_sink_writing_several_items_at_once`, `tests::busy`'s
  `a_busy_item_holds_back_only_the_items_sharing_its_documents`, the daemon's
  `writes_default_to_four_unless_the_config_sets_them`, the golden lock (its `flusso.toml` sets
  `[concurrency]`, the lock stays byte-identical), and the sinks' concurrency tests below.
- **Item-level rejections vs write-wide errors.** `Sink::write` returns a `WriteReport`:
  `Err` is write-wide (transport down, whole request refused) and stops *that sink engine*,
  which the daemon restarts with exponential backoff (1 s doubling to `max_restart_backoff`,
  default 60 s, reset to 1 s after a run that made progress: the sink engine's oldest unfinished
  lane item landed, `SinkEngine::advanced` (not any acknowledgement: a later item landing beside an
  older one that then fails is no progress; guards `only_the_oldest_unfinished_item_landing_counts_as_progress`,
  `a_write_landing_after_an_older_one_failed_is_no_progress`),
  or the ingest engine moved its source forward,
  `IngestEngine::advances`: `ChangeCapture::reached` past the furthest it reported (keepalives on a
  quiet stream count), or, only for a source that answers no `reached`, confirmed past its furthest
  confirmation (a reopened stream numbers a replayed change afresh, so a stream position moves on
  every replay), or a snapshot finished; publishing
  is not progress, since a restart publishes a replayed change or backfill page again; guards
  `tests::backoff`'s `*_restarts_after_one_second_again` / `*_doubles_its_backoff`) while the ingest engine and the other sinks keep running — its lane fills, so a
  stalled sink eventually paces ingest and pins WAL (per-sink lag/in-flight are the alarm). An
  `Ok` report lists documents the destination *applied the write but rejected* individually. The
  `FailurePolicies` (global `FailurePolicy` default + per-index overrides, from config `on_error`,
  resolved by **logical** index name) decide each when the write returns (`decide_rejections`): `Stop` errors the engine out with
  the batch unacked (redelivered after restart); `Skip` quarantines it
  (`Observer::on_document_quarantined`, with the sink) and acks so the poison isn't redelivered.
  A single `Stop` halts the whole batch before any quarantine event is emitted. The OpenSearch
  sink does *not* retry item-level rejections, and reports a missing target
  (`index_not_found_exception`) as write-wide, not as a rejection. An item refused as **busy**
  (429 or 503: a full write queue, a circuit breaker, an unavailable shard; issue #395) is neither:
  `send_bulk_chunk` resends those items alone (`bulk::read_bulk_response` lists them by position,
  `bulk::is_busy`) from the same `max_retries` budget and backoff as a request-wide failure, and
  items still busy after it fail the write (`Err`), so the item stays unacked, and admission holds
  back any later item sharing a document, until every document in it is written or rejected for
  what it holds; a busy item never reaches `on_error`, and a real rejection beside it is reported
  once. Resending a subset is safe only while a request writes each document once (ids deduped per
  batch, admission across writes): `transport.rs` `debug_assert!`s it. `Sink::write` keeps nothing
  between calls (the OpenSearch sink builds each write's actions locally; the stdout sink writes a
  whole write under its lock, guard `concurrent_writes_never_interleave_their_lines`), so a failed
  write leaves nothing for the redelivered item to duplicate (guards
  `a_batch_written_again_after_a_failed_write_is_sent_once`,
  `concurrent_writes_each_send_their_own_documents`,
  `a_caught_up_write_refreshes_what_earlier_writes_left`). An id OpenSearch validates before
  running `_bulk` (an upsert's past 512 UTF-8 bytes, or an empty one) would 400 the *whole* request,
  so `write` rejects it item-level without sending it (`bulk::refused_id`; issue #301; never truncate
  or hash it); a delete's over-long id is sent (OpenSearch answers `not_found`). Guarded by
  `skip_policy_*`/`stop_policy_*`/`per_index_stop_*`, `bulk::tests`' `refused_id` cases and
  `an_item_refused_with_{429,503}_is_transient_not_a_rejection`, `transport::tests`'
  `items_refused_as_busy_are_resent_until_written` / `items_still_busy_after_the_retries_fail_the_write`
  (a stub `_bulk`), the engine's `tests::busy::a_batch_is_not_acked_until_its_busy_documents_are_written`, the sink's
  `document_ids` e2e (seeded alias and unseeded generation, multi-byte ids at the limit), and
  `pipeline`'s `an_id_past_512_bytes_is_decided_by_on_error` (`skip` acks, `stop` leaves it unacked).
- **A document a script fails on is decided on the ingest side (issue #198).** A
  `DocumentBuilder` may return `Document::Failed { id, failure: BuildFailure }`; the ingest engine
  resolves the index's `FailurePolicies` (the same `on_error` the sink engines use, handed to it by
  the daemon via `IngestEngine::with_failure_policies`): `Stop` returns `EngineError::DocumentFailed`
  before anything in the batch is published (the change is retried after the restart), `Skip` reports
  `Observer::on_document_failed` and publishes nothing for that id, so every sink keeps its indexed
  version and the position still advances. Guards: `skip_policy_leaves_a_failed_document_out_and_keeps_going`,
  `stop_policy_fails_the_ingest_engine_on_a_failed_document`.
- **Observability is a trait, not baked in.** Both engines report to an `Observer`
  (`libs/2-engine/src/observer.rs`) — sync, cheap, no-op by default, set via `with_observer`;
  every sink-side event carries the `SinkName`, engine errors an `EngineId`. It depends only on
  the trait, never on metrics or a status backend. The daemon's `Status` phase falls back to
  `Starting` on an ingest error (the supervisor restarts it); only a clean
  `on_engine_stopped(Ingest)` is the final `Stopped`. `reports_lifecycle_and_progress_to_the_observer`
  guards the emit points. **Where time goes is one event** (issue #399): `Observer::on_stage(StageTiming)`
  names a `Stage` by what it is, per task: ingest `idle`, `page` (getting snapshot rows: key-page
  waits, decoding and buffering; `read` is kept for ADR 0012's reader), `resolve`, `build`, `publish`, `lane_wait` (ingest time
  with a `sink` label), `snapshot` (open/close); each sink `idle` (only while no write is in flight and
  nothing is taken: `writes.rs`'s `idle_since`, opened at run start, closed by every arm and by
  taking an item), `write` (one per `Sink::write` in flight, so its rate passes 1 at `writes` > 1,
  plus the idle caught-up write), `seed` (`mark_seeded`, per `index`),
  `reindex`; with `work` (live/backfill), `sink` and `index` where known. Reported **once per
  batch, lane item, idle stretch or operation, never per change or document**; an idle window
  closes on every `select!` arm before its work and is cut by a 1 s tick (the sink's `IDLE_TICK`
  arm, the ingest `confirm_tick`), so a quiet hour is a flat line; the ingest loop reads no clock
  of its own: `mark` (where the last arm's work ended) carries across iterations, `waited` closes
  the wait from it (`idle` with no snapshot, else the snapshot's `page`), and the snapshot arm's one
  clock read per step puts the wait for a key page and the buffering of its keys into `page`. A task's stages add up to its wall time less bookkeeping (acks, confirms, a stop);
  share is busy seconds per wall second, which passes 1 once a task runs a stage concurrently.
  `on_startup_step` per start step (`source`, `stage` per sink, `prepare`, `open_live`). The CLI
  records them as `flusso.stage.duration{stage,work,sink,index}`, `flusso.flush.documents`/
  `flusso.flush.size` (non-empty writes; bytes from `WriteReport.bytes`) and the
  `flusso.startup.duration` gauge (`live` once per process); `flusso-bench` prints an attribution
  table per phase, grouped by task with an `other` row (`dev/bench/src/attribution.rs`).
  `WriteReport`, `CommitStats`, `StageTiming`, `Stage` and `StartupStep` are `#[non_exhaustive]`
  (builders: `WriteReport::clean()…`, `CommitStats::new`). When an engine is restructured, keep
  reporting each stage at its new home, with the `index`/`sink` it now runs per. Guards:
  `tests::timings` (paused clock: each stage's time, a full lane (120 batches: the sink's window
  holds `writes` + 32 before the lane fills), a backfill's pages/snapshot/seed,
  idle cut every second and a reindex not idle), `telemetry::observer::tests` (the `/metrics`
  names), `attribution::tests`, and `test_bench_regression.py`'s `ProbeMarker`.

### The daemon (`libs/3-daemon/src/lib.rs`) — domain only

The daemon owns the **domain**: it is the *supervisor* that assembles one deployment from a
`Config` — but it does **not** name concrete backends. Backend construction is a seam: the
`Backends` trait (`backends.rs`: `validate`, `source` → `SourceParts` = capture + document
builder, `stream`, `sinks` → `SinkParts` = name + `Sink` + `SinkOptions`) is supplied to
`Daemon::new`, so the daemon depends only on `source`/`stream`/`sink`, never on
Postgres/OpenSearch. The CLI is the one place that implements it (see below). A second
source-neutral capability lives beside `validate_indexes`: `CaptureProvisioning`
(`libs/1-ports/source/src/provisioning.rs`) — given the tables an index reads
(`SourceSpec::all_tables`), a source reports coverage + a privilege verdict (`CoverageReport`)
and, when allowed, provisions the gap. Postgres backs it with a **publication**
(`libs/2-adapters/source-postgres/src/cdc/publication.rs`, `impl CaptureProvisioning for
WalChangeCapture`): `run` auto-creates/extends it in `prepare()` **before the slot** (pgoutput
decodes a change against the publication as of its WAL position; `live` repeats it only for a
caller that skipped `prepare`) when the role
is privileged enough and `manage_publication` isn't opted out, else warns with the exact SQL;
`check` inspects read-only and prints the same. A partitioned table (issue #143) streams under
each *partition's* name unless the publication sets `publish_via_partition_root`, and resolution
matches by name: a publication flusso creates always sets it, an existing one is `SET` (same
privilege rule) only when an index reads a partitioned table — reported as
`CoverageReport.misrouted` — and a change for a table no index reads warns once
(`PgDocumentBuilder::resolve`). The trait/report never name "publication", so
the daemon/CLI/printer stay backend-neutral. The same trait's read-only `inspect_pre_image`
reports a second prerequisite (issue #140, ADR 0007): reverse resolution traces a deleted or
re-parented child to its old parent through the change's **pre-image** (`ChangeEvent.before:
Option<RowImage>`, passed to `DocumentBuilder::resolve` and read only on the first hop, unioned
with the current-DB lookup), so each child table's replica identity must carry its link column
(`SourceSpec::pre_image_columns`: `has_one`/`has_many` `foreign_key`, junction `left_key`), and
every table an index reads should log its **whole** old row, for the #169 skip below (issue #202).
`SourceSpec::pre_images` lists every `all_tables` entry with its links (mostly none); the CLI hands
it to `run` and `check`. `PreImageReport` keeps two severities apart: `gaps` (a link column is
missing: correctness — a loud warning with the SQL, and the stream warns once per table) and
`partial` (only the rest of the row: the skip is lost — one info line per start); a `partial` entry
with `PreImageGap.kept` set is one the source leaves on purpose (`will_manage` is then false, its own
info line, `check` says "kept as is" and prints no SQL for it). Postgres answers
from `relreplident` (`cdc/replica_identity.rs`, `shortfall`); `check` prints a Replica identity
section with both groups (read-only); `run` **sets** `REPLICA IDENTITY FULL` on every member that
isn't `FULL` and isn't **kept** — a `USING INDEX` identity whose index carries the whole, non-empty
primary key and every link (issue #212; `Member::kept`, per member, so a carrying partition stays while
a sibling missing the link is set). The PK half holds with no link too: on an index without it the
decoder keys changes by the index's columns (wrong `_id`, deletes miss), and a dropped identity index
carries nothing, so both are set — in `prepare()`, **before the slot** (a change logged in between would decode with the
old identity), via `ensure_pre_image` when `manage_replica_identity` (default on; `false` alters
nothing) and the role owns the table — each relation in its own transaction under a 5 s
`lock_timeout`, so a timeout or an unowned table never stops the others or the start; a partitioned
table is set on the parent and every partition (a partition added later is caught at the next
start), but only when it misses a link, or alone on each member whose identity leaves out the
parent's primary key (issue #224: a partition's change is decoded under the parent's `Relation`,
which flags the parent's key, so it would read null): its updates carry no post-image, so `partial`
never lists it; an intermediate partitioned table (stores no rows, announced under the root) is
neither reported nor set. A member short of the primary key, on any table, is a `gaps` entry with
`PreImageGap.missing_key` set (`replica_identity::plan` decides for both `shortfall` and `apply`).
A change whose old-tuple key reads null is still emitted (the stream warns once per table and kind,
`stream.rs` `addressable`/`Unkeyed`; an update drops only its key-change delete), and
`PgDocumentBuilder::resolve` never turns a key with a null part (`RowKey::has_null`) into a document
id (a root keyed by other columns than its `primary_key`, i.e. a table without one, warns once), and
a hop whose link column (`link_on_row`) is in the key takes the parent from the key with no query,
so a table without a primary key — keyed by every column under `FULL`, where a null is row data —
resolves from its row; any other key with a null walks its hop from `before` alone (no query binds a
null). Only a junction can be such a relation table: any other relation table without a primary key fails
every build in `table_primary_key` (issue #225). Guards: `replica_identity::tests` (`shortfall`,
`a_partitioned_table_is_left_alone_only_while_every_member_logs_the_primary_key`,
`an_intermediate_partitioned_table_is_never_reported_or_set`), `stream::tests`'
`a_delete_without_its_primary_key_under_a_full_parent_carries_the_old_row`, `document::tests`'
`a_child_change_with_a_null_key_still_rebuilds_the_parent_its_old_row_names` and the `keyless`/
null-key cases, `wal`'s `a_keyless_junction_rebuilds_every_parent_its_rows_link` and
`a_change_on_a_keyless_counted_table_stops_the_engine`, `partitions`'
`a_partition_whose_identity_leaves_out_the_primary_key_is_set_to_full` and
`a_keyless_child_partition_under_a_full_parent_still_rebuilds_the_parent_it_left`, `publication`'s
`every_table_an_index_reads_is_set_to_full` (root + `belongs_to` target, opt-out, a lock timeout on
one table), `an_identity_index_carrying_the_link_is_kept_and_the_rest_are_set_to_full`, and
`a_table_the_role_does_not_own_is_reported_and_the_rest_are_set`, `wal`'s
`managed_replica_identity_skips_unread_columns_on_the_root_and_a_belongs_to_target` and
`a_kept_identity_index_still_traces_deleted_and_reparented_rows`. The WAL decoder keys a change by the table's **catalog
primary key** (looked up per `Relation` message in `cdc/stream.rs`), never by the identity flags,
which mark every column under `FULL`. **An update that changes no column a document reads rebuilds nothing**
(issue #169): an update with a pre-image also carries a post-image (`ChangeEvent::Upsert.after`,
built in `cdc/stream.rs` via `pgoutput::post_image`; an unchanged-TOAST `'u'` cell takes the old
identity column's value; a **partitioned** table's update carries none — its partitions log old
rows by their own identity while the `Relation` flags the root's, so placeholder nulls would read as
real), and `PgDocumentBuilder::resolve` emits the root row's own id only when `source::root_columns`
was touched and walks a path only when `source::relation_columns` of its last relation was
(`ColumnUse::touched_by` over a per-change `RowDiff`: a missing image or column, or a key change,
counts as touched; the sets are derived once per builder in `document/reads.rs`, beside each path's
`fields::PathAddress`, so resolving a change never searches the field tree: guard
`resolving_a_change_searches_no_field_tree`). Those functions
(`libs/1-ports/source/src/spec/columns.rs`, beside `row_link`, which `pre_image_columns` and the
resolver's `link_on_row` share, and `soft_delete_column`, which the document query's soft-delete
predicate uses too) are the one derivation of "which columns a path reads" — fields, filters,
`order_by`, aggregate input, link. A **raw filter anywhere in an index** makes every set of that
index "all" (raw SQL is pasted into a correlated subquery and can read `root.…`, a junction, an
enclosing level; guard: `wal`'s `a_raw_filter_reading_the_root_row_rebuilds_on_that_column`). A third source-neutral capability is
`SchemaIntrospection` (`libs/1-ports/source/src/introspection.rs`): where `Catalog` answers
"the type of *this* column" (plus "what is *this* table's primary key" and "do *these*
`transforms` take *this* column", two methods whose defaults mean "unknown, skip the check";
Postgres answers the second with a table-free `SELECT lower(NULL::<format_type>)`, so validation
takes no lock — issue #225; "do *these* two columns compare", `compares`, the same way with
`SELECT NULL::<link> = NULL::<key>`: every link is checked against the key it names (`belongs_to`
`column`, `foreign_key` against the parent's or last hop's key, both junction keys, each hop's
`foreign_key`, both sides of each `on:` pair; one simple-protocol statement per distinct type pair,
cached, at start only; a refusal other than "no such operator" is no verdict: the probe answers
`SourceError::Unchecked` with Postgres's message and validation reports `CheckSkipped`, a warning
naming what wasn't checked and why, for the transform probe too — issue #324, since `check` installs
no logger; guards: `config_coverage`'s `a_check_the_role_may_not_run_is_reported_as_skipped`, the
mock-catalog `a_check_the_store_refuses_to_run_is_reported_as_skipped`), and a pair with no
`=` operator is `LinkTypeMismatch`, since the document query fails to plan for any root row,
whatever the related rows hold — issue #313; pairs that compare across
types, at any value, never stop the engine: a link no key can equal matches no parent — issue #312,
`wal`'s `linked_by_a_value_no_key_equals!` cases; an `ids` `element_type` is checked against the
related key's type by `validation/element.rs`'s `fit` (verdicts measured on OpenSearch 2):
`IdsElementMismatch` when it holds no key, `IdsElementLossy` (a warning) when it holds only some or
changes them — issue #299; guards: `config_coverage`'s
`validation_reports_a_link_postgres_cannot_compare_with_its_key` and
`validation_checks_an_ids_element_type_against_the_related_key`; and "does *this* table exist": `validate_indexes` walks every
relation at any depth and looks up its table, key, junction keys, `via` hops, both sides of each
`on:` pair, value/null-check filter columns, `order_by` and a join's `primary_key` on the table
each lives on — issue #202; a missing table is a `SourceError::UnknownTable` diagnostic, reported
once per relation; `Diagnostic.path` is the field path; and on the root table the root `filters`
and the `soft_delete` marker's column, `when` and `field` (issue #211), and the root key (issue #225:
the table's single-column catalog key, `RootPrimaryKey` when none or composite; the declared
`primary_key` must exist and be that key, `RootPrimaryKeyMismatch` otherwise, since the batched
build matches the table's key on the declared column and writes only tombstones live, and the
backfill pages no such root (it writes nothing) — `wal`'s
`a_root_primary_key_other_than_the_tables_writes_no_document`; and of a type `Catalog::reads_key`
accepts, `UnsupportedKeyType` otherwise (issue #295: Postgres answers from the base type through
domains against `document::value::KEY_TYPES`, the types `decode_key` reads — not `json`/`jsonb`;
any other key reads back null, so the backfill indexes no row and no related change reaches a document — `config_coverage`'s
`a_root_key_the_builder_cannot_read_back_is_refused`); on a root without a single-column
key a missing declared column is `UnknownColumn` only when a relation reads it, else
`RootPrimaryKey`; anchored at the key name,
`table`/`primary_key`/`filters`/`soft_delete`, with `Diagnostic.anchor` = `Anchor::IndexKey` so the
designer tells a key from a same-named field; every relation table also needs a single-column
primary key; a `field` that reads no column — `source::soft_delete_column` is `None`,
which the document query silently skips — is an error that `validate_spec` reports with no
database, so `check --offline` shows it; so is `DuplicateFieldName` (issue #284), a warning: two
fields sharing a `FieldName` at one level still load (format freeze), but the OpenSearch mapping
keeps the last, the Postgres document holds the last one's value coerced through the first one's
declaration (`coerce_document`; a first constant builds its own value, a last one reaches the
first as text, or as `null` for a list or object, and a `binary` column as hex text), and an
`emit: false` namesake strips the name from every
document (the designer refuses to make one: `model/names.ts`'s `fieldNameProblem`); the message
is built from `source::duplicate_outcome` (`libs/1-ports/source/src/duplicates.rs`), which the live
`apps/cli/tests/duplicate_names.rs` matrix (every ordered pair of 38 field kinds through the real
builder and OpenSearch sink; `opensearch` nextest group) pins pair by pair, a pair it can't place
getting `Unknown` and a wording that claims nothing; a pair that never gives an indexed document
whatever the rows (issue #316: `source::never_indexed` — an `array: true` scalar column before an
`object` fails every build, a constant a structured or `binary` last one's mapping refuses even
with `ignore_malformed` is refused in every document) is `DuplicateFieldNameFailsToBuild` /
`DuplicateFieldNameRefused` instead, an error; the matrix asserts `never_indexed` for every pair
against a third, all-null row (scripts return nothing on it) and a re-run under
`index.mapping.ignore_malformed`; a finding
under a duplicated name carries its position (`Diagnostic.positions`, rendered by `dotted_path` as
`orders#2.total`; the designer gets them on `DiagnosticDto.positions` and the exact text from
`/api/preview`'s `schema_diagnostics`, shown only while the fields it describes are unchanged:
`model/issues.ts`'s `exactDuplicateIssues`); guards: the `validation::tests` mock-catalog
cases and `config_coverage`'s `validation_reports_a_typo_in_every_relation_position` /
`validation_reports_a_typo_in_every_root_position`, `validation_reports_what_no_document_build_gets_past`). Every `Diagnostic` carries a
`DiagnosticKind`, which fixes its `Severity`; a join `primary_key` naming no column is the
`IgnoredPrimaryKey` warning, since relations key on the catalog primary key) and `CaptureProvisioning` answers "is this table set coverable",
`introspect` *enumerates* the whole relational catalog (`RelationalCatalog` — every table's
columns/types/PK/FKs, each with a suggested `FlussoType`) so discovery-driven tooling can pick
from what's really there; `junction_candidates` (a free function, not a trait method) flags m2m
junctions. Postgres backs it over `pg_catalog`/`information_schema`; the visual designer
(`apps/design`) is its first consumer. **`run` validates at start** (issue #211):
`backends.rs`'s `build_documents` runs `validate_indexes` on the document builder's own connection
inside `Backends::source`, before anything is staged (`--locked` included; an unreachable database
still fails as `connecting to Postgres`), prints every diagnostic to stderr with `check`'s printer,
and refuses to start only on a kind that cannot work as written: `stops_the_start` matches
`DiagnosticKind` exhaustively (`UnknownTable`/`UnknownColumn`/`UnreadSoftDeleteMarker`/
`RelationPrimaryKey`/`TransformNotApplicable`/`RootPrimaryKeyMismatch`/`UnsupportedKeyType`/
`NotAnArrayColumn`/`LinkTypeMismatch`/`DuplicateFieldNameFailsToBuild`/`DuplicateFieldNameRefused`/`TimeOfDayAsDate`
stop; the last two only while `a_sink_refuses` — every sink is an OpenSearch sink `plain_opensearch` can prove
plain: an allow-list (no `pipeline`, only `PLAIN_SETTINGS` index settings, plus
`ignore_malformed` where it doesn't lift the refusal; every `OpensearchConfig` field is destructured
by name, so a new option must be decided) — any state it doesn't model means no stop
(`refused_only_by_a_mapping`);
type, numeric, nullability, ignored-primary-key, `UndeclaredArrayColumn`, `IdsElementMismatch` (an empty
`ids` array still indexes), `IdsElementLossy`, `DuplicateFieldName`, `RootPrimaryKey`
and `TimestampFormat` (issue #304: a timestamp `options.format` no pattern of which reads the ISO
form, `source::reads_iso_timestamps`, checked offline in `validate_spec`) and `CheckSkipped` findings print and start, as they built or ran in 0.17), so a new kind forces a decision there. A kind
that stops a deployment the previous release started is a breaking change: allowed in a minor or major release when it is the correct behaviour, marked breaking, with an entry on that release's upgrade page (`docs/src/operate/upgrade-to-*.md`) saying what stops and how to fix it. `DiagnosticKind` stays exhaustive for that match; `Diagnostic` and `Anchor` are
`#[non_exhaustive]` (`Diagnostic::new` builds one). `check` still fails on every `Error`. No opt-out
flag. The stopping kinds apply only where every document carries the value, mapped as declared:
the root and plain objects (`validation.rs`'s `carries`: a `Group`, emitted, no `options`); a pair
elsewhere is the plain `DuplicateFieldName`. `TimeOfDayAsDate` (issue #309): a `time`/`timetz`
column under a `timestamp`/`date` field there (`Checker.date_mapped`: no namesake, no `options`
at all); not a `min`/`max` or a field in a join (a parent with no related row indexes). Guards:
`backends::tests` (one per kind, plus `only_an_opensearch_sink_that_maps_the_value_as_written_refuses_it`),
`config_coverage`'s `an_enum_or_domain_column_is_a_type_mismatch_that_still_builds` and
`a_time_of_day_under_a_date_mapping_is_reported`, `apps/cli/tests/time_columns.rs` (live Postgres
+ OpenSearch: reported only where no document indexes; the workarounds index and leave `check`
silent), and the two `*_some_document_survives_starts_and_indexes` tests (in `duplicate_names.rs`
and `time_columns.rs`: the real `flusso run` binary, via `tests/common`, starts and indexes each
configuration the review found refused, and still refuses the root cases). The daemon
wires a `StatusObserver` (`observer.rs`) that updates a shared per-sink `Status` (`status.rs`),
runs the engines under `supervise.rs` (stage every sink engine → spawn them + the ingest engine
as independent tasks → restart a failed one with backoff while the others keep going; the
ingest engine ending cleanly ends the deployment after the lanes drain), exposes the reindex
*operation* through `DaemonControl`, and polls source capture lag out-of-band (`lag.rs` over
`ChangeCapture::lag`). It is **telemetry-agnostic** — it depends only on the
engine's `Observer` trait, not on any metrics backend — and owns **no transport**: no HTTP
server, no process signals, no metrics *recording* or *exporter*; those are the binary's.
`Daemon::start()` builds everything and returns a `RunningDaemon` exposing `status()` (an
`Arc<Status>` a transport can read), `control()` (a `DaemonControl` for `reindex`), and
`run(shutdown)`, which runs until the live stream ends, or the caller's `shutdown` future fires and the deployment has drained (the stop invariant above). A binary attaches its own metrics
observer via `Daemon::with_observer`; the engines drive a `FanOut` (`engine::FanOut`) of the
status observer plus any attached ones. So the daemon's public contract is *data*: the
backend-agnostic `Observer` events and the `Status` handle.

The CLI (`apps/cli`) is the **composition root**. It is the single crate that names concrete
backends: `backends.rs`'s `FlussoBackends` implements the daemon's `Backends` trait, resolving
the connection (in the running environment) and building the Postgres source + the configured
sinks (the source-type dispatch and the OpenSearch/stdout `match` live here, not in the daemon). Adding a backend = a new match arm here plus its crate; the daemon and engine are
untouched. The composition root is also where `Config` is translated into the backend-facing
subsets it needs: the Postgres source builder takes a `SourceSpec` (the enabled indexes +
their schemas, in `kernel` types — `source::SourceSpec`), never the whole `Config`.
The `Config`→`SourceSpec` translation lives here, in `backends.rs`'s `source_spec` helper (it
filters out disabled indexes); `check.rs` calls the same helper to drive `validate_indexes`,
which is `SourceSpec`-based. So the source crate has no dependency on `Config` at all (nor can
it — `Config` lives at layer 2 in `schema`, above the backends), and the OpenSearch sink already
takes only `IndexMapping`/`Mapping`. It is also the composition root for transport and telemetry (grouped under `apps/cli/src/` as
`telemetry/` and `http/`): it installs the
meter provider (`telemetry/metrics.rs` — one `SdkMeterProvider` feeding a Prometheus reader scraped
at `/metrics`, and an OTLP periodic push when the standard
`OTEL_EXPORTER_OTLP_*` env vars configure an endpoint, matching the trace export in
`telemetry/mod.rs`), defines the metrics and records them (`telemetry/observer.rs`'s `OtelObserver`,
attached via `with_observer`; metric names/labels/buckets live here because they're a presentation
choice), serves **two** HTTP surfaces (`http/mod.rs`): a **public** one (`/healthz` `/readyz`
`/status` `/metrics` `/wait`, unauthenticated; `/wait` long-polls the daemon's `Fence`, default
5 s, capped at 60 s: `200` reached, `408` timed out, `400`/`404` refused, `503` before the source
is live or once flusso is stopping; `/status` adds the process `pid`) and a **private** one (`/indexes`, later `/reindex`; HTTP
Basic auth in `http/auth.rs`, default `admin`/`flusso` with a loud startup warning), both reading
the daemon's `Status`, and owns the process signals. It binds **both** listeners up front (a bad
`--public-address`/`--private-address` fails fast; a taken one names the flusso there, from its
`/status` pid), then `Daemon::with_observer(otel).start()`
→ register the `in_flight` observable gauge (read from `Status` at collection) → serve both →
`run(stop)` → close. **The CLI owns the stop's signals, deadline and exit code (issue #371,
`shutdown.rs`); the daemon owns the drain** (see the engine invariant above). `SIGINT`/`SIGTERM`/`SIGHUP`,
or the parent exiting under `--exit-with-parent` (`parent_id()` polled, no `unsafe`; opt-in), resolves
the future handed to `RunningDaemon::run`; the whole stop (drain, then HTTP close, then telemetry flush)
runs under `shutdown::within` (biased to the finished drain) and `--shutdown-timeout-secs` (default 30;
`0` aborts without draining, exit 0): past it the log names the step and the sinks still holding changes
and the process exits 1. Signals are read with `signal-hook`'s origin iterator (`extended-siginfo`, on a
thread; tokio's signals don't give the sender): `shutdown::Repeats` forces `128 + n` on the same sender
(pid; the kernel and outside-namespace senders are their own) twice within 500 ms, or any three within
1 s; every other repeat logs `stop already requested …` (`Throttle`, once a second). One Ctrl-C under a
forwarding runner is two senders, one stop; `SIGHUP` is caught only when it wasn't ignored at start
(`/proc/self/status` `SigIgn`; not caught where `/proc` is missing). HTTP close and telemetry flush are `side_step`s: at most
5 s each, ending before the deadline, and giving up on one is logged but still exits 0 (not the data
path). `/wait` (`http::Waits`) stays open through the drain and answers `503` + `Retry-After` +
`reason: stopping` once it ends (`starting` before the source is live); `/status` carries `pid` and the
`stopping` phase. `main` builds its runtime by hand and `shutdown_timeout`s it (1 s). `flusso design`
shares the signals/second-signal helpers. Guards: `shutdown::tests` (paused time), the `http::tests`
`/wait`/`pid`/taken-address cases, and the `flusso-cli` `shutdown` e2e (real binary, OpenSearch group).
A taken address names the flusso there (its `/status` pid); a busy slot names its holder from
`pg_stat_activity` before the stream opens (`WalChangeCapture::ensure_slot_free`). The lock is written
to a temporary file beside the file its path resolves to (through a symlink), with that file's
permissions, and renamed (`compiled.rs`'s `write_bytes`), so a stop mid-start never truncates it; where
no file can be made beside it (EACCES/EROFS) or the rename can't land (EBUSY/EXDEV) it is written in
place, as before (`falls_back`; guards: `compiled::tests`). With no meter provider installed the global meter is a no-op
and the instruments cost nothing — which is why the daemon tests run with no setup. A view in
`telemetry/metrics.rs` overrides the flush-duration histogram buckets to seconds (OTel's defaults
assume milliseconds). The Postgres `ChangeCapture::lag` and slot-check share a small lazily-opened
admin pool (`WalChangeCapture::admin_pool`) so periodic lag probes reuse connections.

### Config layer — two-stage parse then convert, then adapter validation

`config::load(path)` is the front door (`libs/1-config`, layer 1): it reads `flusso.toml`,
resolves+parses every referenced `*.schema.yml`, and returns one `Config`. Downstream crates that
legitimately compose a deployment (the daemon, the CLI, the designer, the query derive) depend on
`config` and reach the vocabulary via its re-export of `kernel`. Each file *parser* (the `toml`
and `yaml` modules) works in two stages:

1. **Parse** — `serde` deserializes into permissive *entity* types that mirror the file
   1:1; unknown top-level fields are rejected. A port table (`[source]`, `[stream]`,
   `[sinks.<name>]`) parses into `kernel::PortEntry`: `type` + every other key as an
   uninterpreted `Options` tree (`kernel::options`, sorted keys, serde + `IntoDeserializer`).
2. **Convert** — entities are lifted into the model and rules the format can't express are
   applied (identifier validation, join/aggregate arity, declared-type placement, filter
   shapes). For `*.schema.yml` → `IndexSchema` this lives in the `yaml` module. For
   `flusso.toml` → the assembled `Config` the conversion is a *composition* step
   (`libs/1-config/src/deployment/conversion.rs`, the `From<ConfigToml>` impl); port entries
   pass through and an omitted `[stream]` becomes `channel` with defaults.
3. **Validate against the adapters** — in the composition root, not the config crate:
   `adapters::validate(&Config)` (`apps/cli/src/adapters.rs`, also the daemon's
   `Backends::validate`) deserializes every entry's options into its adapter's
   `#[derive(AdapterConfig)]` struct with `deny_unknown_fields`. `build`, `check` (incl.
   `--offline`), `run`, and the designer call it right after load, so a typo in a sink option
   fails before any network call or lock write. `load` alone therefore does **not** mean
   "fully validated". The same goes for **per-index sink options** (issue #168): an `[[index]]`
   entry's `[index.sinks.<name>]` tables are `Index.sinks: BTreeMap<SinkName, Options>`, keyed by
   sink **name**, and `adapters::index_sink_config` checks the name is a declared sink and
   deserializes the table into that adapter's per-index struct (declared with
   `#[adapter(index_options = T)]`, published as `AdapterDescription::index_schema`; OpenSearch's is
   `OpensearchIndexConfig { index_settings }`). Like `on_error`, they sit outside `IndexSchema`,
   so they never move the content hash (`index_settings_never_change_the_content_hash`).

**Adapter knobs that are flags** (`--slot`, `--publication`, `--manage-publication`,
`--manage-replica-identity`, `--pretty`,
`--queue-capacity`) are laid over the file's entries by `adapters::apply_overrides` before
validation (flag > env > file); `DaemonOptions` carries nothing adapter-specific. A deployment
with no sink gets a `stdout` entry there.

**Secrets are deferred, never resolved at parse/convert time.** A `{ env = "VAR" }`
reference is a `kernel::Secret` (serialized exactly as written: a string or `{ env = "VAR" }`)
and is read by the adapter's config type in the environment that *runs* the pipeline — so a
compiled `flusso.lock` carries no secret it wasn't given literally. **One override rule for
every adapter**: `kernel::override_var(entry, kind, field)` = `<ENTRY>_<TYPE>_<KEY>`
(`PRIMARY_OPENSEARCH_URL`, `SOURCE_POSTGRES_CONNECTION_URL`, nested keys joined with `_`),
applied to `Secret` fields; an explicit `{ env }` beats it. `DATABASE_URL` no longer exists.

**One declaration renders everything.** `AdapterConfig::description()` yields the adapter's
draft-07 schema (doc comments → descriptions, serde defaults → defaults), its example, and its
secret paths (found by the `x-flusso-secret` marker `Secret`'s `JsonSchema` plants), plus its
per-index options schema when it takes any (`index_schema`). The CLI
renders the complete `flusso.toml` editor schema (`flusso schema config`, incl. `index[].sinks`),
the Reference option tables (`flusso schema docs`; a per-index table as `<port>-<kind>-index.md`), and hands the descriptions to the designer (`GET /api/adapters`),
whose forms are schema-driven (`apps/design/frontend/src/components/AdapterForm.tsx`). Adding an
option to an adapter = edit the struct, run `just schema-gen`, commit.

### Schema YAML is type-first

Each field is `- <type>: <name>` — e.g. `keyword: email`, `has_many: orders`,
`count: orderCount`, `geo: location`. The type key's value is the field/document name;
siblings are whatever that type allows. There is no `- field: x` + `type:` form. Joins
split by relationship verb, which names where the key lives: `belongs_to` (this table's
`column` pointing at the target, defaulting to the field name), `has_one`/`has_many`
(the related table's `foreign_key`), `many_to_many` (`through` a junction). Aggregates
split by op (`count`/`sum`/`avg`/`min`/`max`, plus `ids` — a flat scalar array of the
related table's primary keys, typed by an explicit `element_type`; `ResolvedField.array`
flags it — and `exists`, a non-null `boolean`, appended last to `AggregateOp` so the derived
`Hash` keeps every earlier discriminant). What an aggregate over `through` counts (`distinct`,
`through.filters`) is owned by the Aggregates reference. An aggregate reaches a table several
hops away with `via` (issue #172): `Aggregate.via: Vec<ViaHop>` (`table`, `foreign_key` pointing
at the table before, `filters`), downward foreign keys only (a hop's `column`/`through` is parsed
only to be rejected), its own key then pointing at the last hop. `Relation::via()` exposes it;
everything that walks relations must cover it: the SQL ties the key to the last hop through
nested `IN` levels (`Builder::parent_match`), reverse resolution walks one `Step::Via` per hop
(`document/fields.rs`), `via_columns` is the hop's #169 read set, and `all_tables` /
`pre_image_columns` include each hop. Guards: `an_aggregate_via_hops_rebuilds_its_parent_from_every_hop`,
`a_self_reaching_via_rebuilds_every_ancestor_it_reaches` (a hop back into the root's own table),
`a_via_under_a_nested_relation_rebuilds_the_root_from_every_hop` (a table that is both a relation
and a hop) (all `wal`), `an_aggregate_via_hops_adds_one_step_per_hop`, the `*via*` SQL tests.
Any hop but a junction takes `on:` (issue #173): `ColumnPair { local, related }` lists on
`Join.on`, `Aggregate.on` (direct key only) and `ViaHop.on`, each one more equality ANDed with
the key (`local` on the table before the hop, `related` on the hop's table; rejected at load on
`many_to_many`, over `through`, half-written or duplicated). `Relation::on()` / `parent_on()`
(the pairs whose `local` is on the parent row). The SQL appends `AND related = parent.local`; across
`via` a hop with pairs is a row-value `IN` selecting the next level's `local` columns
(`ParentMatch::link`). Reverse resolution is unchanged (the key walk is a superset); the #169
sets carry both sides (`related` on the hop's table, `local` on the level before: `Collector::level`,
`via_columns(schema, hop, next_on)` via `source::via_hops`, `Step::Via { hop, next_on }`), and
`pre_image_columns` needs no `related` column (a row changing only those keeps its key). Guards:
`on_pairs_rebuild_the_parent_from_either_side` (`wal`), the `on_pairs_*` SQL tests,
`a_schema_without_on_keeps_its_content_hash`.
`sum`/`avg`/`min`/`max` fold a `kernel::Expression` (issue #194; grammar and storage in the
`expression.rs` module doc, semantics in the Aggregates reference): parsed by hand, **no regex**,
depth-capped. A whole input `ColumnName::try_new` accepts is that column (so every old
spelling still loads, Unicode lowercasing included); a plain column is the `Column` leaf,
serialized and hashed as the bare `ColumnName` (format freeze); an expression is stored in the
lock as its canonical text, which must nest no deeper than the tree is tall (a double negation
renders `- -a`) so every accepted tree re-parses from its lock. An expression's
`sum`/`min`/`max` takes only a numeric `value_type`, rejected at load.
`sql::arithmetic` renders every `/` as `(left / NULLIF(right, 0))` (issue #202): a zero divisor
nulls that row, which the aggregate skips, instead of failing the document query and stalling the
deployment; integer overflow stays a Postgres error.
`AggregateOp::input()` is the one accessor (`sql::arithmetic`, `relation_columns`,
`validate_indexes`, designer codegen); the designer checks input through `POST /api/expression`,
the same parser. Guards: `a_plain_column_aggregate_keeps_its_content_hash`, the
`expression::tests` proptests (`a_tree_up_to_the_cap_renders_to_text_that_parses_back`),
`every_chain_up_to_the_cap_renders_to_text_that_parses_back`,
`a_whole_column_name_keeps_the_column_name_rules`, `the_deepest_expression_survives_a_lock_round_trip`
(`golden_lock`), `an_expression_needs_a_numeric_value_type`,
`an_aggregate_expression_rebuilds_from_each_operand` (`wal`).
**The content hash is the physical index name**, so `Aggregate`
and `Through` hand-write `Hash` (destructuring `Self`) to skip `distinct`/`filters`/`via`/`on` when
unset (`ViaHop` and `Join` likewise for `on`, a hop's pairs under their own tag) — a schema that doesn't opt in keeps its index; `a_through_schema_keeps_its_content_hash`
and `a_schema_without_via_keeps_its_content_hash` pin it. `Column` and `Join` do the same for the two mapping opt-outs (issue #170):
`subfields: false` (`text`/`keyword`/`uuid` → `Column.subfields`, projected to
`Mapping.subfields`; the sink drops the auto subfields, keeping a `text` field's analyzer) and
`indexed: false` (`json` → `Column.indexed`, any join → `Join.indexed`, projected to
`Mapping.indexed`). An unindexed field keeps its `mapping_type` and resolved children (the
*stored* shape, so the derive still shape-checks a struct against it), but the OpenSearch sink
renders it `{"type": "object", "enabled": false}` and `collect_enum_sorts`/`collect_map_templates`
stop at it; descendants' mapping-only settings are accepted and ignored, `options` on the
unindexed field itself is rejected. Guards: `a_schema_that_never_opts_out_keeps_its_content_hash`,
`opting_out_of_subfields_or_indexing_changes_the_content_hash`. Any new optional field on a hashed kernel config type needs the same treatment. A `map:` field (`values:` = the shared leaf type) is a dynamic-key object over a
`json`/`jsonb` column — `FlussoType::Map { values }` → OS `object` with `dynamic: true`
injected into options (so runtime keys are accepted); the resolved `Mapping.map_values` is
the **full `Mapping` every key gets** (what a plain field of `values` resolves to, incl.
`FlussoType::default_options`, the identifier analyzer) and the only thing distinguishing a
`map` from a plain `object`/`json`. The OpenSearch sink maps each key with it through one root
`dynamic_templates` entry per map (`flusso_map:<path>`, `path_match: "<path>.*"`, the property
`build_property` renders, never a second copy of the rules; issue #177), plus one addition:
a keyword key with subfields also gets an exact `.keyword` (`add_exact_key_subfield`, issue #191),
so `path.*.keyword` names the exact values alone. `Column`'s
hand-written `Hash` adds `MAP_KEY_MAPPING_REVISION` (`2`: typed keys, then the `.keyword`; both
in 0.17.0) for a map column only, so map schemas
moved to a new physical index when keys became typed and nothing else did
(`a_schema_without_a_map_keeps_its_content_hash` / `a_map_schema_moves_off_its_dynamically_mapped_hash`). The same
pattern moves two more sets once (issue #291): `BINARY_ENCODING_REVISION` (a `binary` column or an
`ids` of `binary`; the document query renders a `bytea` inside its JSON as `\x…` hex text, which
`document/value.rs`'s `coerce_scalar` turns back into `GenericValue::Bytes` for `sink::Json` to
base64). That text is rendered **in the query, not by the session's `bytea_output`** (issue #298: a
transaction-mode pooler drops a session `SET`): `PgDocumentBuilder::byte_columns` finds every value
column whose base type is `bytea`, or an array whose element is (`ColumnMeta.element_name`: `typelem`
then every domain, so `blob[]` of a domain `blob` counts) (`fields::collect_value_columns`: column
fields at any depth, `min`/`max` over a plain column, an `ids` key) from the cached catalog lookup, and
the `Builder` wraps it in `sql::hex_text` (`E'\\x' || encode(…, 'hex')`) or `sql::hex_array`, whatever
the declared type (a `keyword` over `bytea` keeps its `\x…` text), so the bytes under `hex` are
unchanged and `escape` stores the same. A `bytea` nested in a composite, a range or an array of
composites is out of reach: the pool's after-connect `SET bytea_output = 'hex'` stays as the backstop
(a direct connection reads it as before; behind a transaction-mode pooler it follows the server, which
the manual's "Behind PgBouncer" states), and `CONSTANT_OBJECT_REVISION` in `Field`'s `Hash`
(a `constant` that `constant_is_stored_unindexed`: an object, or a list holding one or mixing scalar
kinds — string/number/boolean, nulls aside, nested lists flattened, and a fractional or out-of-range
number after a whole one, which the first element's `long` would refuse or cut — since one OpenSearch field takes
one kind and the strict root refuses keys in a childless `object`; built as written by `coerce_field`
from the schema, never from the query's untyped literal, and projected as an `indexed: false`
`object`, so the designer and the derive (no handle) follow). Guards:
`a_binary_schema_moves_off_its_hex_text_hash`,
`a_schema_without_binary_or_an_object_constant_keeps_its_content_hash`,
`a_constant_list_mixing_kinds_moves_its_hash` / `_is_stored_unindexed`,
`a_constant_list_of_one_kind_stays_indexed_and_keeps_its_hash`, `integration`'s
`a_database_set_to_escape_bytea_output_still_builds_the_bytes`, the `engine` `bytea_output` e2e. **The
column-type cache follows the table** (`document/mod.rs`'s `ColumnTypes`, `PgDocumentBuilder::column_types`):
the CLI hands it to `WalChangeCapture::with_column_types`, and the stream drops a table's entries on a
`Relation` message (Postgres sends one after a definition change) naming another type OID or modifier than a cached
column's (`ColumnTypes::forget_changed`; unchanged types keep them, so a stream start costs no lookup), and a failed document query
(`failed_query`) or reverse lookup (`failed_lookup`) drops those of every table its index reads, so a
type change never loops the ingest engine on a stale cast or `encode`. A reverse lookup keeps its
query error typed (`resolve::LookupError`, the `sqlx::Error` boxed) up to `resolve_change` /
`read_root_links`, whose loops hold the index, so `resolve`'s happy path is main's and no argument is
threaded through the hops (its no-query paths are `just bench`'s `resolve_no_io`). The cache entry
compares `(type OID, typmod)` with the `Relation` message's, so a resize (`varchar(5)` →
`varchar(10)`) refreshes it. **Statement texts carry every type a prepared statement depends on**
(issue #335): sqlx caches one statement per text on each pooled connection, with the parameter types of
its first use, and Postgres refuses one whose result type changed. So a bound key is spelled
`$n::<bound>::<key type>` (`sql::key_cast`, as `root_keys_query` does), and a selected key's typed
column is cast to the key type (`value::key_select`); no statement cache is cleared and nothing is
retried. **A key or filter operand is never cast to a size**: the key type is `ColumnMeta.key_type`
(`PgDocumentBuilder::key_type`), the base type's schema-qualified `typname` with no modifier
(`pg_catalog.bpchar`/`pg_catalog."bit"`, never `character`/`bit`, which mean `char(1)`/`bit(1)`),
because an explicit cast to `varchar(5)`/`numeric(10,2)`/`bit(5)`/`time(0)`/`interval year`/a domain
over one truncates, pads or rounds without an error: a key would match no row, and a filter value
(`filter_column_types` takes the same type, issue #338) would match a row holding another value and
turn `neq`/`not_in` against the row they should keep. Postgres still does the comparison; plans keep
their index scans. When the role has no USAGE on the base type's schema (a domain `app.dcit` over
`ext.citext`) it is the declared type's `typname` instead, as before (a cast to a domain checks its
constraints); `has_schema_privilege` is read by the same catalog query and cached with the type, the
backfill's key lookup follows the same rule, and a role refused `has_schema_privilege` itself (42501)
gets the lookup once more without the probe, every key cast to its declared type unless the base
type is in `pg_catalog`, which every role may use (`COLUMN_META_SQL_UNPROBED`, remembered per
builder). `text_value::is_key_type` parses the type's `regtype` name as quoted identifiers (`ext.citext`, `"a.b".citext`; not `"a.citext"`). **The batched
build serves exactly the ids a single `build` serves, by construction**: `query::documents_query`
reads the root rows as main did, one level in (`SELECT root.pk AS flusso_key, <document> FROM root
WHERE root.pk = ANY(<keys>) … OFFSET 0`, the keys written in the `query::KeyForm` decided per chunk
(`KeyForm::for_batch`): a partitioned root or fewer than 16 keys get `key_constructor`, an
`ARRAY[$n::<bound>::<key type>, …]` grouped by bound type, each group padded to a power of two by
repeating its last key (Postgres sees every key: a generic plan knows the length and is cached for a
batch of one — `a_small_batch_reuses_its_plan` — and run-time pruning works on a root partitioned by
its key — `a_batch_is_pruned_to_its_keys_partitions_under_a_generic_plan`; the position form unnests it
beside a `bigint[]` of positions, `0` for a pad, dropped before the join); any other batch gets
`key_arrays`, one `$n::<bound>[]::<key type>[]` parameter per kind (bound by `bind_array`), which
binds and plans a large batch cheaper but which Postgres sizes at ten elements and can't prune by; at
most six statement texts per kind over all batch sizes on an unpartitioned root, ten on a partitioned one,
whose every size up to the 512-key chunk is a padded constructor), the root the only relation in scope there,
so a raw filter's unqualified name resolves as in a single build — `integration`'s
`a_raw_filter_names_the_root_rows_columns_whatever_the_batched_query_names`, which reads the names off
the statement itself), and one level out joins it to the same keys unnested `WITH ORDINALITY` on the
same `=`, a `FULL JOIN` (Postgres runs it only as a hash or merge join: an inner join planned a nested
loop rescanning the materialized documents per key — `integration`'s
`a_batched_build_pairs_positions_without_a_nested_loop`), selecting the position (`doc_position`)
beside the document; `document/matching.rs`'s `match_rows` pairs by position alone (no Rust cast or
equality: Postgres's input/output forms are open-ended), an unanswered position is a tombstone, and a
row matched by several positions answers each under its own id. A batch whose keys pair by value
(`matching::pairs_by_key`: `ColumnMeta::equal_is_identical`, the one definition of a key whose `=`
is identity, and a key type whose cast keeps a requested key as it is —
`bool`/`int2/4/8`/`uuid`/`bytea`/`date`/`time`/`timestamp(tz)`/`text`/`varchar`, never `numeric`,
floats, `citext`, `char(n)` (pads) or `name` (cuts to 63 bytes) — and every key already of the kind
the column reads back as)
runs the inner level alone, selecting `value::key_select` (`query::documents_by_key_query`, main's own
query, no join, which costs a light document about a quarter of its build) and pairs in
`match_by_key`, which reports a row no requested key equals: the cached key type is stale (its
`Relation` message is still to come), so `build_many` forgets the index's types and reruns that chunk
by position, as it does when Postgres refuses the prepared by-key statement because its result type
changed (`0A000`, `text` → `name` changes the selected key's collation; `Stale::ResultType`), logging
the rerun at `debug` (`integration`'s `a_batch_by_key_with_a_stale_type_cache_still_finds_its_rows`,
`a_batch_survives_its_key_column_changing_from_text_to_name`); everything else, a key read before a type change included, pairs by position. Where the declared key isn't the root's whole catalog primary key
(`query::RootKey::Shared`: keyless, composite, a partitioned root keyed with its partition column, a
view) a key several rows share builds the row its `query::TieBreak` orders first, in both builds
(`DISTINCT ON` / `first_row`): the table's primary-key columns when it has one, else the text of the
root columns the document reads (`source::root_columns`, the declared key left out; rows equal there
build the same document, and with none any row answers, `TieBreak(None)`, no term rendered),
else, under a raw filter, `ROW(root.*)::text`, so the choice reads no column the document doesn't
(`a_key_several_rows_share_builds_the_same_row_both_ways`,
`choosing_a_shared_keys_row_reads_no_column_the_document_does_not_read`: a view column that raises). The
builder's and the backfill's pools set `jit = off` after connect (`document::without_jit`, a refusal
only warns: `a_server_that_refuses_the_jit_setting_still_connects`, Postgres 10; the lag pool doesn't;
`the_builders_sessions_run_without_jit`, `partitions`'
`a_batched_build_on_a_partitioned_root_is_not_jit_compiled`); the manual tells a transaction-mode
pool's users to set it on the role. The result
columns are a `bigint` and the document whatever the key's type; a key Postgres refuses to cast fails
the whole statement, as on main. Guard: `source-postgres`'s `key_matching` (a generated differential,
4,560 pairs: 120 requested keys of every kind and edge form, keys past 63 bytes included, against tables of 19 key types holding
every stored spelling they accept, `build` vs `build_many` alone, beside every stored row and among 17 keys of
the requested kind (so both `KeyForm`s), each table also as a hash-partitioned twin, on Postgres 16 with a
non-UTC database zone). A query Postgres refuses as a schema mismatch (`42883`/`42804`/`42703`/`42P01`), a
document query or a reverse lookup, names the index and `flusso check` (`query_failed`); it is a real
mismatch, which validation reports, not a stale cache. Guard: the `engine` `type_change` e2e (root key,
links, junction and `via` keys, a recreated table, a migration in one transaction, a concurrent reader
on several pooled connections, resizes of `varchar`/`char`/`numeric` keys, a `numeric` key read before
its scale changed, keys moving between `int`/`numeric` and `timestamp`/`timestamptz` inside one
migration, a text key becoming `timestamptz` from offset-less values, a domain over a type in a schema
the role may not use (builder, capture and backfill as that role), a role refused `has_schema_privilege`;
no restart, no tombstone), `matching::tests`, `document::tests`'
`no_key_cast_carries_a_type_modifier` and `no_filter_operand_cast_carries_a_type_modifier`, `source-postgres`'s
`filter_operands` e2e (every sized type under every value operator, and a `varchar`/`numeric` value in
every place a filter sits, built single and batched; a role refused `has_schema_privilege`), `stream::tests`'
`a_relation_message_with_another_type_modifier_forgets_its_tables_column_types`. No query per build. The cache is nested by schema, table and
column, looked up by borrowed names, and hands out `Arc<ColumnMeta>`, so a hit allocates nothing; each
`forget_table` bumps its generation, which answers derived from it (the per-path root-link decision)
check. Guards: `stream::tests`'
`a_relation_message_with_{other,the_cached}_types_*`, `bytea_output`'s `a_column_changing_type_mid_run_*`;
the same e2e
(direct, and through PgBouncer in transaction mode resetting each server connection, PG 18 for `min`/`max` over `bytea`,
`hex` and `escape` databases, backfill and live) and `query::tests`' `a_bytea_*`.
`values` must be a leaf kind (text/keyword/number/date); the conversion rejects others. An `enum:` field takes an optional `variants:` list (its own
`EnumBody`/`Field::Enum` entity) — the variants in rank order; empty/omitted = a
plain keyword. It lands on `Column.enum_order` (**not** on `FlussoType::Enum`,
which stays a unit variant so `value_type: enum` keeps working and the lock
round-trips), projected to `Mapping.enum_order`. The OpenSearch sink prebakes the
rank into a `.sort` keyword subfield via a per-field `mapping` char-filter
normalizer (variant → zero-padded rank), so an ordered enum sorts by declared
order with no script; out-of-set values sort after. Parsing lives in
`libs/1-config/src/yaml/entities/field.rs`; the core model is `kernel::FieldSource`
(`Join.kind: JoinKind`, with reverse resolution per kind in
`libs/2-adapters/source-postgres/src/document/resolve.rs`).

**Array columns (issue #276).** `array: true` on a scalar/`enum`/`custom`/`json` column field is
`Column.array` (hashed only when set), projected to `ResolvedField.array`; the mapping stays the
element type. The Postgres builder renders `transforms` per string element over `to_jsonb(column)`
(`sql::element_transformed`, one path for `[]` and `json`/`jsonb`) and leaves an array `default`
(a list) to the decoder. `value::coerce_document` reads the row's raw `document` JSON bytes in one
pass (a schema-guided `DeserializeSeed`: `Level`/`FieldSeed`/`Joined`, no intermediate `Value` tree)
through a per-index `DocumentPlan` the builder builds once (the first field declared under each name:
a key resolves in one lookup in any order, and a namesake reads through its first declaration even
when `jsonb` sorted the keys of a >50-field document), and is **fallible**
(`DocumentError::Failed(BuildFailure)`, with the field path; `Malformed` is a query error). **Only a
declared `decimal` reads differently from a `serde_json::Value` walk** (issue #327): a column, array
element, `sum`/`min`/`max`, `ids` key or `map` value of `decimal` reads its raw number text
(`&RawValue`) into `GenericValue::BigDecimal` (the exact text, appended last to the enum), never
through `f64`; a declared `float` reads its raw text too (`single`), untyped numbers go through
`kernel::untyped_number` (issue #366), every other value reads as `main` did, `NaN`/`Infinity` stay strings, and reading goes on past a failure so a malformed document
is `Malformed` wherever it is and the failure reported is the first in declaration order; a raw value
read again may open no more levels than the document had left (`Untyped.remaining`), and skipped
values are checked (`Skip`). Guard: `value/differential.rs`, 7605 cases (every field kind but
`decimal`, aggregates, joins nested in joins, groups, namesakes, wide documents, depths at every level,
the spaced layout `jsonb` writes, fields built from schema YAML) whose sink bytes were recorded from
`main`'s reader and render into `differential.expected` (re-bless with `FLUSSO_BLESS=1` only for a
deliberate change). An array column's null
takes the default, a lone value is wrapped, null elements are dropped, and an object/array element of a non-`json` field is a `BuildFailure` that `build`/
`build_many` return as `Document::Failed` (so `on_error` decides). `Catalog::accepts_transforms`
takes `element: bool` (Postgres probes the `[]` base type, `text` for `json`/`jsonb`);
`NotAnArrayColumn`/`UndeclaredArrayColumn` are the catalog checks; they read `ColumnInfo.base_type`
(a domain resolved through `typbasetype`, which the element transform probe uses too), while the
declared type is still checked against the domain's own name. The designer's `array_default` /
`json_array_default` value slots refuse what the config conversion refuses, and `api::render_op`
refuses a schema whose file would not load, so a save never writes one (a `FileOp` carries its
content only as `schema`, written into `base`; there is no raw-text op). Query side: `check_type`'s
scalar arm also accepts a custom non-`Vec` type through const asserts (`OpaqueProbe::LEAF_ARRAY` +
`accepts_kind`), and `#[derive(FlussoValue)]` on a newtype over `Vec<E>` emits only
`FlussoValueMeta` (`E`'s kinds, `ARRAY`), no `FlussoValue<K>`. So `ARRAY` has **two
producers**, told apart by kinds (`OpaqueProbe::LEAF_ARRAY` = `ARRAY` with no `Nested`/`Object`
kind): a value array newtype goes only at an array leaf or a `json` array, a fragment array newtype
only at a `nested` field (root: `array_value_check`/`json_array_check`/`array_newtype_check`;
fragment: `array_shape_check`). **A `FlussoFragment`/`FlussoValue` type reads a JSON array exactly
when `ARRAY` is set** (issue #289; there is no separate "reads an array" fact): each derive refuses,
where the newtype is declared, every other newtype that would read one. A `FlussoRoot` reads an
object; `#[derive(FlussoMap)]` (`map.rs`'s `reads_an_object`) refuses every shape that certainly reads
no object (unit, a tuple of several, a newtype over a `Vec`/`Option`/primitive at macro time, over a
value, array newtype or derived sub-document through `OpaqueProbe`), and `from`/`try_from` on it, as on a fragment
(`serde_attr::converts_from`; trusted on a `FlussoValue`, like a field's `with`). A
`#[serde(flatten)]` field must read an object, on both derives, `opaque` or not
(`fragment::flattened_field`: macro error on a `Vec`/primitive, `!ARRAY && AT_OBJECT` asserts). The
forwarding `FlussoValue<K>` impl of an untagged value newtype carries both its bounds higher-ranked
(`for<'__flusso>`), so a newtype over an array newtype needs no `Serialize`. The value derive asserts `E` reads no array and is
no sub-document/object, refuses `Option<Vec<_>>`, and its untagged forwarding path forwards `ARRAY`
and refuses a sub-document inner type (a value's `__flusso_check` walks into nothing); the fragment
derive (`fragment.rs`'s `newtype_array`) refuses `Option<…>` of an array, a `Vec` of values, maps,
`Value` or arrays, and a value array newtype inside. Those asserts are spanned on the element type.
A `map` takes no `Vec` (`check::array_shape_ok`), as on a root. A fragment's shape check, and its
`opaque` check, let any element through at a `json` array, as a root does; a root does not walk a
non-`Vec` type at a `json` array, which `json_array_check` already placed. Guards:
`an_array_column_changes_the_content_hash_only_while_it_is_set`, the `value::tests`/`query::tests`
array cases, `validation::tests`' `an_array_*`, `config_coverage`'s
`array_columns_clean_up_their_value_and_are_checked_against_the_catalog`, the `array_columns` UI
pass test + `array_*`/`json_array_*`/`value_newtype_over_an_array_or_a_fragment`/
`fragment_newtype_reads_another_array`/`flatten_not_a_struct_or_map`/`map_type_reads_no_object`/
`value_newtype_without_serialize` compile-fail tests, `ui_pass/{opaque_at_a_json_array,
flatten_struct_or_map,map_type_shapes,refused_newtypes_fixed}.rs` (the last follows every refusal's
suggested fix and reads),
and `dev/query-e2e`'s `array_columns` (a derived root read back from real hits).

### Script fields (issue #198)

`script:` beside a leaf type key is `FieldSource::Script(Script { source, ty, nullable, array,
enum_order, subfields })`; `emit: false` on any field is `Field.emit` (input-only). Both hash only when
used (`Field`/`Script` hashing: `emit` hashed only when off), so a schema without them keeps its index.
The YAML entity is `Field { kind: FieldKind, emit }` (`emit` is lifted off the mapping before the body
deserializes). The projection maps a script as its declared leaf (`array` → `ResolvedField.array`) and
**drops `emit: false` fields from the mapping** — the Postgres builder still fetches them (it walks
`IndexSchema.fields`), and renders a script field as a `NULL` placeholder. The `script` crate
(`libs/2-script`, layer 2: kernel + source port only; Rhai with `sync`/`decimal`/`no_time`/`no_module`, default features on: their `ahash/runtime-rng` keeps the build reproducible, issue #355; CI's `check-ahash-seed.py` guards the features, `check-reproducible-build.sh` the bytes, by hand)
compiles every script once (`Scripts::compile`, all errors with line/column/excerpt; the source is
wrapped as `fn __flusso(root) { … }` so `this` can be bound — columns on line 1 are shifted back) and
`ScriptedBuilder` decorates any `DocumentBuilder`: per object level, scripts run against `this` (a
fresh copy per call) and `root` (read-only, shared), all before any output is written, so no script
sees another's output; then outputs are written and `emit: false` fields stripped. A failure becomes
`Document::Failed` (see the engine invariant). A declared `decimal` enters Rhai as the nearest
`Decimal` (about 28 significant digits, at most 28 after the point); exponent text as the `Decimal` nearest the `f64`
it names (`kernel::f64_to_nearest_decimal`, issue #366), a zero unsigned; past the range it is a string of the exact text (`value.rs`'s `declared_decimal`,
`#[cold]`: it slows `to_dynamic` otherwise). A `decimal` result is written exactly (`BigDecimal`, a
zero unsigned), a string returned to it fails with `main`'s message, and a decimal's `to_float()` is
the sandbox's own, parsed from its text, placed in the standard package's module (`sandbox.rs`; a
second module or the global namespace slows every call). Guard: `tests/decimal_from_main.rs`, 330
inputs recorded from `main`, plus doubles known independently. Sandbox: strict variables, failing on a missing
property, no `eval`, fixed limits (`sandbox.rs`). The CLI wraps the Postgres builder in
`backends.rs`, compiles scripts beside `adapters::validate` in `build`/`check`/`run`
(`compile_scripts`), records `flusso.script.failures{index,field}` and a rate-limited `error!` log
(`telemetry/script_log.rs`, array positions folded to `[]`); the daemon's `Status` carries
`script_failures` per index. The designer compiles scripts in `/api/preview` (`script_errors`) and runs
them on the sample (`/api/sample` takes an `id`). **Dates are typed (issue #364):** `value::to_dynamic` hands a `Date`/`Time`/`Timestamp`/`TimestampTz`
over as the custom Rhai types of `dates.rs` (`date`, `time`, `timestamp` with an optional zone,
`duration`), registered by `dates/library.rs` into the sandbox's one module (a second module slows
every call). A `Timestamp` keeps its wall clock plus the zone's offset there, so parts never touch the
zone database (jiff's bundled `jiff-tzdb`, used only for zones; chrono does the rest; jiff's range
ends near ±9999, so an instant past it is clamped to the edge, where the offset holds, and a leap
second is folded: a named zone reaches every year chrono does, `a_named_zone_reaches_as_far_as_a_fixed_offset`); calendar units and rounding move the wall clock and
`Zone::resolve` turns it back (gap: forward by its length; overlap: the earlier). Every comparison
between a date kind and another kind (string, number, bool, char, container, another date kind;
`()` for ordering only) is registered to **fail** with what to write (`library.rs`'s `mismatches`: one
function per operator, kind and side on `Dynamic`, which Rhai tries only after the exact signature
misses; an explicit matrix of ~600 functions slowed `scripted/flat` ~4% on CI),
since Rhai answers a mismatched comparison with a silent `false`; `switch` can't be hooked (a custom
type has no hash, so every arm is skipped) and is documented instead. `sort`/`sort_desc` with no
comparator are replaced by a copy of Rhai's that also sorts one date kind (the standard one refuses a
custom type; `tests/sort_parity.rs` runs every other element type through both and must agree, so a
Rhai bump that changes the original fails there), and a map's `to_json` writes each date as its text. `to_debug` is quoted, so a
container prints as strings did. Rhai names the found type as Rust spells it in some errors;
`dates::script_names` renames it in every runtime failure but a `throw` (`ErrorRuntime`: the script's
own text). The upgrade page's table of script changes is read by `every_replacement_on_the_upgrade_page_runs`,
which runs each "Write instead" snippet: keep every code span in that column complete. `months_until` counts as Postgres's `age()`
(fixture: `tests/fixtures/months_from_postgres.tsv`, 408 pairs from Postgres 16). No `now()`. A `map` of dates is typed for the script only (`view.rs`, from the schema;
the document keeps the text), and `flusso script run --input` types its JSON by the schema
(`input::document`). Guards: `tests/dates.rs` (every function and edge rule, plus proptests).
The manual's script guide
(`docs/src/author/scripts/`, dates in `dates.md`) includes its recipes by mdBook anchor from
`libs/2-script/tests/recipes/recipes.schema.yml`, which `tests/recipes.rs` runs and asserts: change a
recipe there, not in the page.

### Query side — `flusso-query` + the derive

`sdk/query` (crate `flusso-query`) is a backend-neutral OpenSearch/Elasticsearch query
client. `sdk/query-derive` (`flusso-query-derive`, re-exported behind the `derive`
feature) is a proc-macro that, **at compile time and with no DB**, discovers `flusso.toml`,
resolves the named index mapping, validates the struct against it, and generates a typed
query surface. `dev/search-api` is a working axum consumer. This is a deep subsystem — the
proc-macro internals (scope tagging, `FlussoValue<K>` kind markers, nested/object handles)
are documented in the `flusso-query-derive` memory note; read that before changing the
derive.

**Exactly one type references the schema: the root (issue #98).** `#[derive(FlussoRoot)]`
(which replaced `FlussoDocument` outright — no alias) binds to an index and
generates the **whole** handle tree — `doc.rs::codegen` walks the resolved mapping and
emits one namespace per container level into a generated `flusso_<root>_query` module
(`flusso_user_query::Orders`, `flusso_user_query::OrdersItems`) — never the caller's namespace, so a user
type named after a level can't collide.
An **object** flattens into its enclosing scope so its namespace chains from the parent as
`&self` methods (`User::account().tier()`); a **nested** array introduces its own scope so
its namespace is a named type with associated fns (`flusso_user_query::Orders::total()`) implementing
`FlussoScope` (the renamed `FlussoDocument` *trait*; `FlussoIndex` → `FlussoRoot`; both old
names removed). This
retired `path = "…"`, `resolve::Scope`/`scope_at`, and the "object under nested"
restriction.
Everything below the root is `#[derive(FlussoFragment)]`: **location-free**, so one
declaration serves several paths/indexes, validated by each root that embeds it. The two
derives can't see each other's tokens, so they meet as **const data**: the root bakes the
resolved level (`spec.rs` → `&[FieldSpec]` in `sdk/query/src/check.rs`) and emits
`const _: () = Frag::__flusso_check(children(LEVEL, "…"))` spanned on its field;
`fragment.rs` emits that const fn with one baked-message assertion per field, recursing
into sub-fragments. `FlussoValueMeta` carries a custom type's kinds/variants as consts (a
const fn can't name `FlussoValue<K>` for a `K` it only has as a value) — which is also how
enum-variant coverage is checked. Constraints: const `panic!` takes a literal (no
interpolation) and const eval **can't warn**, so a Rust variant the schema doesn't declare
is a hard error while a subset is silently fine. Embedding is checked by default;
`#[flusso(opaque)]` opts a plain struct out of the walk into its type only; the field still has to
exist, with the schema's nullability and array shape, and a type with `FlussoValueMeta` keeps its
kind check at a leaf (issue #256). A fragment does this as const assertions
(`fragment::opaque_check`, `OpaqueProbe::{HAS_META, KINDS}`): at a leaf a fragment rejects a type
without `FlussoValueMeta`, `opaque` or not, since it can't check the bound a root checks (issue
#265). A hand-written `FlussoValue` implements `FlussoValueMeta` by hand and calls
`flusso_query::embeddable!(T)`, which emits the hidden `__flusso_check` the fragment calls
(`check.rs`; `ui_pass/hand_written_value_in_fragment.rs`). `#[serde(flatten)]` and
`#[serde(transparent)]` newtypes are supported (both checked against the enclosing level). A
fragment newtype over a bare `Vec<Struct>` (or over another such newtype) sets
`FlussoValueMeta::ARRAY` (`fragment::newtype_array`), so it is placed where the `Vec` would be: at
a `nested` field, rejected at an `object`; the root asserts it in `doc::array_newtype_check`, a
fragment in `array_shape_check`, both through `OpaqueProbe::ARRAY` so `opaque` keeps it
(`ui_pass/newtype_over_vec_at_nested.rs`). A map type, or a `Vec` of maps, at an `object`/`nested`
level with fields is a "declare a struct" error on both derives; a `Vec` of maps goes only at a
`json` field (`ui/array_of_maps_at_a_level.rs`).

Dynamic-key `map` fields (issue #28) get typed handles too: `handles/map.rs` emits one
handle per value kind — `TextMap`/`KeywordMap`/`NumberMap`/`DateMap` — where
`.key(runtime_str)` returns the **plain-field handle** (`Text`/`Keyword`/`Number`/`Date`) of
the declared kind, since the sink maps every key like a plain field (#177): runtime keys,
compile-time value type. `TextMap`/`KeywordMap` carry the `WithSubfields`/`NoSubfields`
type-state (the derive stamps it via `string_handle`, same rule as a plain field) and hand it
to their keys. **The map handle mirrors its values kind across every key** (issue #186): the
plain handle's operators, same names/args, each a builder (`MapMatch`/`MapTerm`/`MapRange`/
`MapPrefix`/`MapWildcard`/`MapRegexp`/`MapFuzzy`, one `CrossKey` body + `cross_key_builder!`)
with `prefer(key, weight)` / `only_preferred()` over `KeyFields`. The analyzed family is a
`multi_match` over `path.*` (which also reaches each key's subfields: an exact-value boost, by
design; `match_phrase_prefix` renders `lenient: true` since a phrase prefix 400s on keyword
subfields); everything else is a `query_string` over one pattern, the value escaped/quoted (a
bare term starting `AND`/`OR`/`NOT` gets `\` on its first letter, a `wildcard` copies a user `\x`
pair through; `only_preferred()` with no `prefer` renders `match_none`, since empty `fields` falls
back to every field). A
field pattern can't exclude a key's subfields, so the exact family (`eq`/`any_of`/`prefix`/
`wildcard`/`regexp`/`fuzzy`) on `KeywordMap<_, WithSubfields>` delegates to its `keyword()` view
over the exact `path.*.keyword` the sink adds to keyword keys (issue #191); on `NoSubfields` it
renders over `path.*`. With subfields the map mirrors the plain accessors as **views**
(`TextMap::keyword()`/`keyword_lowercase()`, `KeywordMap::keyword()`/`text()`/`keyword_lowercase()`
→ a map handle over `path.*.<subfield>`). `search` was
removed (`matches` replaces it). `contains(part)` (string maps, subfields only) is a `MapContains`: a
`query_string` of the escaped `*part*` over `path.*.keyword_lowercase`, a case/accent-folded
substring match across keys; `has_key`/`exists` are presence checks. Guarded by the `*map*` unit
tests in `sdk/query/src/tests.rs`, the `map_cross_key_gated` UI test, the `map_exact_family`
pass tests (both type-states: `tests/ui_pass/`, `tests/ui_no_subfields/`), and `map_cross_key_queries_match_the_per_key_or` (every family vs the per-key `or`) +
`map_cross_key_adversarial_values_match_the_per_key_or` (operator words, backslashes, quotes,
empty values; with and without subfields) in the `map_keys` live e2e. The
doc-side type is `HashMap<String, V>` or `BTreeMap<String, V>` (blanket `FlussoMap<K>` impls
for any `V: FlussoValue<K>`), or a `#[derive(FlussoMap)]` struct of **any shape that reads an object** — newtype or
named fields (e.g. a translations type with a `fallback` beside the language keys) with a
**required** `#[flusso(keyword|text)]` value-kind tag (no default — untagged is a compile
error); the
derive's `check_type` map arm hard-checks a `HashMap`/`BTreeMap` value type and defers a
`FlussoMap<kind>` bound otherwise. What a `FlussoMap` type holds is checked against its tag where it
is declared (issue #294, `map.rs`): a newtype's `HashMap<String, V>` (`V` may be one `Option` level: a map value can be `null`) gets a `FlussoValue<tag>` bound
on `V`, any other wrapped type (and a named struct's flattened field) must carry the tag in
`OpaqueProbe::MAP_VALUES` (so `Box<…>`/a plain struct is refused), a named field must be `Option<V>`
with the same bound, `deny_unknown_fields` and a field's `alias` are refused. A `FlussoFragment`
newtype over a map (through `Option`), and an untagged `FlussoValue` newtype over a map type, carry
`MAP_VALUES` (the map's value kinds; `[Object]` for `serde_json::Value` values, so only `json` takes
it) and a forwarding `FlussoMap<K>` impl, so they go exactly where the map goes. A map key the
derive knows by name reads no arbitrary string (a number, `bool`, `char`, `Uuid`, a tuple) is refused
(`doc::non_string_key`, run by every derive on every field); any other key type is trusted. **Refuse by
name only what can't be a user's own string type** (the owner's rule: a struct that compiled on
`main` and read real documents keeps compiling; `ui_pass/map_keys_of_your_own.rs` and
`ui_pass/tagged_newtypes_over_named_types.rs` guard it, since the boundary test can't see
over-refusal). A `FlussoFragment` newtype over a leaf value is refused
(`fragment::newtype_over_a_value`): it is a `FlussoValue`. A tagged `FlussoValue` newtype
(`value::tagged_inner`) looks through the std wrappers serde reads transparently, refuses a map, a std
collection, a slice/tuple/array, a primitive other than `String`/`char` and `NonZero*` at macro time,
and a flusso type that reads an object through const data; any other type (named `Date`, `Decimal`,
a type from another crate, an alias) is trusted. The `FlussoMap` derive also emits `FlussoValueMeta` (its
`MAP_VALUES` carries the declared value kind) + a no-op `__flusso_check`, which is what lets
the type sit inside a `FlussoFragment`; a hand-written `impl FlussoMap<K>` alone works only at a
root — in a fragment it fails to compile, with the E0277 note pointing at the derive (or at a hand
`FlussoValueMeta` + `embeddable!`). Both
sides also enforce the value kind symmetrically (fragment `map_kind_ok` assert; the root's
`embed_checks` mirrors it, so a map wrapper on a plain object/nested is rejected everywhere),
and an opaque `json` field (object, no children) accepts any map-shaped type on both sides.
`handle_fn` dispatches on `Mapping.map_values` (`Text`→`TextMap`, `Keyword`→`KeywordMap`,
`Date`→`DateMap`, the numerics→`NumberMap`). Phase 2 (per-key analyzers for per-language
stemming) is deferred.

**The boundary is pinned (issues #294, #306).** What the derive checks and trusts, and the two
places a root and a fragment differ, is one section, owned by `docs/src/query/document-structs.md`
("What the derive trusts"); don't restate it here. `sdk/query-derive/tests/boundary.rs` places two
shape lists (hand-written `named_shapes`, and `generated_shapes`: inner type × wrapper × newtype
derive; a shape whose declarations alone are refused is not placed) at the fields of the `boundary`
fixture (every leaf kind, `custom` `ip`, the aggregates, an `enum` with variants, maps, `json`, every
join verb, an unindexed join, `subfields: false`; required kinds bare, nullable ones in `Option`;
generated shapes skip the `NAMED_ONLY` kinds, a time-budget cut) on a root, then in a fragment for
shapes that compile at some root field other than `custom` (at `custom` alone otherwise), ~12,100
cases (`ct`/`kns`/`bt`/`mm` share another field's mapping and are not placed). Cold it takes ~232 s
here and ~550 s on CI; nextest marks it SLOW at 300 s and kills it at 900 s
(`slow-timeout = { period = "300s", terminate-after = 3 }`), so keep CI under ~600 s when widening. It compiles them in ≤60-module files
in its own crate and target dir (`target/tests/flusso-boundary/`, so the `ui` harness never waits on
its lock; nextest group `boundary`, one at a time): the first round runs `cargo check`, later rounds
`cargo build`; each file is named after a hash of its text and of every input (derive, `flusso-query`,
`config`, `kernel`, fixtures, `Cargo.lock`, toolchain), and its results are cached under that name, so
a rerun on an unchanged derive compiles nothing. It reads sample documents (null, missing keys, empty
and many elements, a `null` map value, each date form, and an extreme one: numbers at each kind's edge,
a value outside `variants:`, `jsonb` values of another type) into what builds, and fails on a
compile-then-fail case unless, for every document it fails on, a `TRUSTED` rule matches that document
and its error; or on a (shape, field) whose root and fragment outcomes differ unless `DISAGREE` lists
it (`*` = every field); outcomes are recorded in `tests/boundary.outcomes` (re-record with
`FLUSSO_BLESS=1`), and each rule's phrase must appear in the manual. The outcome is only `refused`,
not which check refused, so three refusals that another check backs up are pinned by their `ui`
`.stderr` files instead (see the test's module doc). The manual's map example is included by anchor
from `tests/ui_pass/docs_map_examples.rs`, and the plugin's is checked equal to it.

**Ordered enums (issue #87).** A keyword mapping with `Mapping.enum_order` set gets the
`Enum<S, Sub>` handle (`handles/string.rs`) instead of `Keyword`: same value ops (delegated;
`.keyword()` exposes the full keyword surface), but `.asc()`/`.desc()` sort on the prebaked
`{field}.sort` subfield — a plain, nesting-aware field sort, **no script** — so the enum
sorts by declared order. `handle_fn` branches the `Keyword` arm on `enum_order.is_some()`;
`check_type` is unchanged (still keyword-kind). A bare enum stays `Keyword`.

**Sorting a `map` by key, with language fallback (issue #58).** `Type::field().sort_key("it").or("en")`
reads as "sort by `it`, else `en`" and returns a `MapKeySort` (`handles/sort.rs`) that implements
`Sortable`, so it flows through the **normal** `SortBuilder::by(handle, dir)` (same
`Into<MaybeOrderBy>` optionality as any field sort) — single key is just `sort_key("it")` with no
`.or`. It renders a `_script` sort whose painless source walks the keys in order and sorts by the
first one a doc has — true fallback, not lexicographic tiers. String maps sort case- and
accent-insensitively on each key's `.keyword_lowercase` (`MapSortValueKind::Folded`, no script
lowercasing; so `TextMap::sort_key` exists only with subfields), a `KeywordMap<_, NoSubfields>` on
the bare key with `toLowerCase` (`Exact`); numeric/date maps on the bare key (epoch millis
for dates). Nesting-aware via the scope path, like a field sort. `missing_first`/`missing_last`/`missing(v)`
(on the produced `Sort`, or via an `OrderBy` through `by`) **redirect into the script's
`params.missing`** with a direction-correct sentinel (`Sort` carries a `script_kind` flag + a `dedup_id`
distinct from the render key, so several `_script` map sorts dedup by field path and coexist).
**A single key sorts like a plain field** (the `MapKey` marker is gone, #177): with typed keys a
text key with subfields sorts on its real `.keyword_lowercase`, a keyword/number/date key on its
doc-valued path, and a text key without subfields is not `Sortable` — the `NoSubfields` type-state
already forbids it, as for a plain field. Guarded by the `*_map_sort_key_*`/`map_sort_*` tests, the
`map_key_not_sortable` UI test (in the `ui_no_subfields` harness), and the `map_keys` live e2e in
`dev/query-e2e`.

The query surface is **builder-based** (issue #19): each leaf operator returns a small
per-query builder (`handles/string.rs`/`scalar.rs`/`geo.rs`/`nested.rs`) carrying that
query's options plus the universal `boost`/`name` (the `Common` carrier + `common_opts!`
macro in `handles/mod.rs`), implementing `AsQuery<S>` so it composes and renders the DSL
shorthand when no option is set; `and`/`or`/`not`/`to_value` are **provided methods on
`AsQuery`** (inherent `Query` methods shadow them, so existing behavior is unchanged — but
`.or()` on a *builder* needs `use flusso_query::AsQuery`). Compound/scoring queries live in
`handles/compound.rs` (`constant_score`/`dis_max`/`function_score`/`boosting`), standalone
ones in `handles/extra.rs` (`ids`/`query_string`/`simple_query_string`/`combined_fields`/
`script`/`script_score`/`distance_feature`/`rank_feature`/`more_like_this`); `Sort` is a
builder (`sort.rs`); search-level controls + the `Highlight` builder live on `Search`
(`search.rs`). The `uuid` feature makes `uuid::Uuid` a `keyword` value (no `#[flusso(skip)]`). `Box<str>` and `Cow<'_, str>` are
values wherever `String` is (`FlussoValue`/`FlussoValueMeta` impls beside `String`'s), `Arc<str>`/`Rc<str>`
behind the `rc` feature (serde's `rc`). A fragment checks a std type it knows by name (a primitive, a
generic std wrapper) through `OpaqueProbe`, with no walk into it; any other type without flusso metadata
needs the walk, so it can't stand even at a `custom` field (stable Rust has no const fn pointer or const
trait call to skip it).
Every scalar handle is typed by **kind**, not by one fixed Rust type: value ops take `impl
FlussoValue<kind::…>`. Numerics are **split per type** — `Number<kind::Byte|Short|Integer|Long|
Float|Double|Decimal, S>` — and `FlussoValue` is impl'd by **lossless widening**, so `eq(5)` works
on `long`/`double`/`decimal` but a float on an integer field (or `i64` on a `short`) is a compile
error. `decimal` vs `double` is carried by `Mapping.decimal` (0-core), since both map to OS
`double`. `Bool` is kind-based too (`kind::Bool`). `FlussoValue<K>` has a `serde::Serialize`
supertrait. A `#[derive(FlussoValue)]` **newtype inherits its inner type's kinds** (blanket
forward impl) — `struct Money(Decimal)` is a decimal value with no tag; an enum requires an
explicit `#[flusso(keyword|text)]`, no default (numeric/date tags don't exist). An enum may
add `exhaustive` (`#[flusso(keyword, exhaustive)]`, issue #100): a partial projection is
normally legal, but the marker makes every embedding demand the schema's **whole** declared
`variants:` set (rides `FlussoValueMeta::EXHAUSTIVE` as const data; an untagged newtype
forwards it; enum-only — on a newtype/`FlussoMap` it's a macro error, and at a field with no
declared variants it's a const error, so a schema edit can't silently disarm it).
An `indexed: false` field gets **no handle** (`emit_level` skips it) but still deserializes and is
shape-checked; a `subfields: false` field is stamped `NoSubfields` like a sink with
`auto_subfields` off (`ui/unindexed_has_no_handle.rs`, `ui/subfields_false_has_no_keyword.rs`, the
`stored` fixture index in `derive.rs`). A `#[flusso(scope = "…")]` that renames nothing is a compile
error (issue #244, `doc.rs`'s `misplaced_scope`, `fragment.rs`): it is allowed only on a root field
whose key names an indexed `nested` or `object` level (`has_namespace`, the same test `emit_level`
uses), a `skip` field included; leaf, `map`, `json`, unindexed, flattened and fragment fields are
rejected (`ui/scope_on_*.rs`, `ui/scope_in_fragment.rs`, `a_skipped_field_still_renames_its_level`). The same holds for the
other attributes (issue #251): `opaque` only where a custom type's check would run (`misplaced_attrs`,
shared by both derives, plus the root's mapping-aware `opaque_without_a_shape_check`; the placements
that do something are pinned by `ui_pass/opaque_placements.rs`), no `rename` on a `#[serde(flatten)]`
field, a `#[flusso(rename)]` only naming one of the field's `#[serde(alias)]`es (issue #262: the derive
reads serde's key itself, so any other is redundant or a key serde never reads; `doc::unseen_rename`), no
`scope_mod` on a root that generates no module, and a fragment reads every container
`#[flusso(…)]` (`ui/opaque_*.rs`, `ui/rename_on_flatten_field.rs`, `ui/rename_contradicts_serde.rs`,
`ui/rename_serde_does_not_see.rs`,
`ui/scope_mod_without_levels.rs`, `ui/fragment_location_in_second_attribute.rs`). A flattened
`serde_json::Value` or map is serde's catch-all and is checked on neither derive (both go through
`fragment::embedded_type`; `ui_pass/flattened_catch_all.rs`).
Issue #256 closed the rest: a `#[flusso(…)]` the derive never reads is an error (a `FlussoValue`
newtype's field or enum variant, a `FlussoMap` field, the field of a `#[serde(transparent)]`
fragment/root: `value::unread_attr`, `doc::newtype_field`; a transparent struct is read as the one
field serde reads, `serde_attr::passed_over_by_transparent` mirroring serde's rule, tuple or named,
issue #262); every key is given once
(`lib.rs`'s `once`; `keyword` beside `text` is a conflict); on a root `keyword`/`text` field
`opaque` on a type with no variants to check is a const error through `flusso_query::OpaqueProbe`
(inherent consts exist only with `FlussoValueMeta`; its `OpaqueWithoutMeta` fallback lets a
hand-written `FlussoValue` keep `opaque` on a root). Serde attributes are read in one place,
`serde_attr.rs`, which steps over every item it doesn't read (`bound = ""` before `rename_all`).
Built-in leaf types are matched by **name** (a macro can't resolve paths, and path matching would
reject re-exports like `sqlx::types::Uuid`), except where no built-in could sit: a flattened field
and a root `object`/`nested` field take a `Date`/`Uuid`/… as the user's own type
(`fragment::checked_type`'s `structured`), but a flattened `GeoPoint` stays flusso's (unchecked).
A field's key is its `#[serde(rename)]`, plain or the `deserialize` name. A raw-identifier field
(`r#type`) reads the unraw key; a schema key that is a keyword in any edition gets a
raw-identifier handle (`doc::KEYWORDS`, `doc::generated_ident`; `reserved` fixture,
`ui_pass/raw_identifiers.rs`), and one no identifier spells (`_`) is an error
(`ui/unnamable_schema_key.rs`), as are two keys giving one handle on a level or one scope type in the
module (`doc::claim_handle`, `Namespaces::claim_scope`; `colliding` fixture, issue #262). A map type
at an `object` with fields is rejected on both derives; a map of `serde_json::Value` fits a `json`
field on both (`check::has_fields`/`is_json`, `ui/map_at_structured_object.rs`,
`ui_pass/fragment_json_maps.rs`).
`Text`/`Keyword` expose `.keyword()`/`.keyword_lowercase()`/`.text()` subfield accessors
(runtime methods, not derive codegen — keeps the field method returning the shared handle
type for `multi_match`/composition). Issue #19 acceptance test: `sdk/query-derive/tests/
derive.rs::acceptance_realistic_projection_needs_no_escape_hatch`.

**Sorting + nesting-aware path metadata (issue #49).** `FlussoScope` carries only
`const PATH: &[Segment]` (the container chain from the index root, in `sdk/query/src/path.rs`);
the index identity + `query`/`get` live on the root-only `FlussoRoot: FlussoScope` supertrait,
so a fragment physically can't `.query()`. Since #98 the derive emits `FlussoScope` for the root
and for **each generated nested namespace** (a fragment gets neither — it has no location), and
`FlussoRoot` only for the root; the `Root` scope marker stays (the shared root/flattened-object
scope, so combined search and object handles keep composing) and impls `FlussoScope` with
`PATH = &[]`. Sorting goes through a `Sortable` trait
(`asc`/`desc`, impl'd for `Keyword`/`Text`/`Number`/`Date`/`Bool`, **not** `Geo`/`Object`/map) whose
sorts are nesting-aware: `Sort::field::<S>` reads `nested_boundaries(S::PATH)` and renders the
recursive `nested:{path, nested:{…}}` chain (mode defaulted from direction), so a bare
`flusso_user_query::Orders::placed_at().desc()` is correct at top level; `NestedProjection` (inner_hits) strips that
wrapper. `SortBuilder` (`by`/`near`/`score`/`score_if`/`raw`/`tiebreak`/`or_default`/`build`,
deduping by key) collapses request→`sort` mapping; `OrderBy`/`MaybeOrderBy` carry a direction +
optionality (a request's `Option<dir>` self-skips). `Search`/`MultiSearch`/`NestedProjection` take
plural `.sorts(..)`.

**Index prefix (issue #24).** A deployment-wide `prefix` (top-level `Config` field;
`--index-prefix`/`FLUSSO_INDEX_PREFIX` override it, flag > env > config) is prepended to
**every** name the OpenSearch sink owns — the hash alias `{prefix}{logical}_{hash}`, its
generations, the `{prefix}{logical}` convenience alias, and the `{prefix}flusso_meta` index —
so several deployments (dev/staging/nightly) can share one cluster. Write side: resolved +
validated in `commands/run.rs` (`kernel::validate_index_prefix`), threaded via
`backends.rs` into `OpensearchSink::with_index_prefix`; the sink prefixes at the single
chokepoint (`hash_alias`/`convenience_alias`/`meta_index` helpers in `lib.rs`). Read side is
**runtime, not baked**: the derive still emits the unprefixed `INDEX`/`SCHEMA_HASH`, and
`flusso-query`'s `Client::index_prefix` prepends the prefix to each request path (and strips
it from `_index` in combined-search decode), so one compiled consumer serves every env. The
generation naming functions (`generations.rs`) are prefix-agnostic — they operate on whatever
hash-alias string they're handed.

Combined-search decode (`sdk/query/src/multi.rs`, `decode_response`) also **normalizes the
generation suffix**: a hit's `_index` is the concrete `{logical}_{hash}_{n}` behind the hash
alias (OpenSearch reports the backing index, never the alias the query used), so the `_{n}` is
collapsed back to a union variant's `{logical}_{hash}` before dispatch — anchored on the
union's known targets, not a blind trailing-`_{digits}` trim, because the eight-hex hash can
itself be all digits. Without this, every hit from a real deployment missed dispatch (issue
#67). Single-index search and `_msearch` don't dispatch by `_index`, so they were unaffected.
Guarded by the `multi_decode_*` unit tests and the `combined_search` live e2e (`dev/query-e2e/tests/`
— an unpublished crate, so the published `flusso-query` carries no flusso lib deps and the query
release train can't be dragged into a release by a libs bump).

**Page cursors (issue #156, ADR 0008).** Offset paging is unchanged; `Search::cursor(Option<PageCursor>)`
/ `MultiSearch::cursor(..)` return a typestate (`CursorSearch`/`MultiCursorSearch`) over `search_after`
(`sdk/query/src/cursor.rs`). **Live by default** (no server state, never expires);
`.point_in_time()` opts into a **pinned walk** over an OpenSearch PIT (OpenSearch only, ES is #157),
and the token — not the builder — decides the mode on resume. Gotchas: OpenSearch has no
`_shard_doc`, so the sort is made total with `_id` (union: `_index`, `_id`) and an unsorted walk leads
with `_score`; a live union resume rewrites each `_index` sort value to the generation its hash alias
serves now (`GET {path}/_alias`), or a reindex mid-walk repeats every tie; a PIT search POSTs to bare
`/_search` and continues with the *returned* `pit_id`; the token's fingerprint is a hand-rolled
FNV-1a over the body (minus delivery keys) and the *prefixed* path, pinned by
`the_fingerprint_is_pinned`; a gone PIT is `CursorExpired`, matched on the exception type, not the 404.
`Walk<I>` (`sdk/query/src/walk.rs`, issue #158) streams a whole walk (`pages`/`hits`/`id_stream`
consume the cursor search): a hand-written `futures_core::Stream` over a boxed page fetch, fetching the
next page once the current one is consumed; its `Drop` closes a still-pinned point-in-time by **spawning** on
`tokio::runtime::Handle::try_current()` (`Drop` can't await), while `into_cursor` keeps it open.
Guarded by `tests::cursor`, `tests::walk`, and the `cursor_paging` live e2e (helpers shared in
`tests/common/`).

**Fenced reads (issue #136, ADR 0009).** Reads still go straight to OpenSearch; the one thing the
client asks flusso is to wait. `Client::flusso(url)` sets the daemon's public address
(`fence_sink`, `fence_timeout` tune it) and `Client::after(position)` returns a **clone** carrying
the position: the hook is in the transport (`Client::fence_before` in `sdk/query/src/fence.rs`,
called by `get_one` → applied, and by `search_at`/`count_at`/`open_pit`/`search_pit`/`msearch_raw`
→ searchable), so every terminal is covered with no builder change, and a fenced client waits once
per level (an `AtomicU8` its clones share; only a fence that passed marks it). A new read path must
call `fence_before`. A fence never falls back to a stale read. It **rides out a restart** (issue
#377): `fence_timeout` (default 20 s) is one deadline across attempts and the fence returns by it.
`fence::run` is the loop, separate from the HTTP `ask`, so its tests run on paused time. Each request
asks `/wait` for the time left less a margin (`timeout_ms` rounded up, never 0) and may take at most
the time left, connect included. Unavailable (`503`/`502`/`504`/`429`, transport failure) pauses
the larger of the backoff (100 ms doubling to 2 s) and `Retry-After`, plus up to half again of
jitter (one past the deadline fails at once); a `408` with time left is asked again at once only if
flusso held it about as long as asked, else paced the same. The status decides before the body is
read. `400`/`404` are `FenceRefused`; a refused certificate (typed `rustls::Error` check, as the
sink's), a redirect error and a missing `.flusso(url)` are `FenceMisconfigured`, both at once. Out
of time, the last answer decides: `FenceTimeout` or `FenceUnavailable { cause: UnavailableCause, reason }`.
Nothing is spawned, so dropping the read stops it. Guards: `tests::fence` (stub daemon, incl. TLS),
`tests::fence_pacing` (paused time: pacing, deadline, jitter spread over 500 clients), and
`apps/cli`'s `fence_restart` e2e (the real binary stopped and restarted on one address, both
levels; `flusso-query` is a path-only dev-dependency there).
`position::CURRENT_SQL` is the statement; the `diesel`/`diesel-async` features add
`position::{diesel,diesel_async}::current` (backend types only, no driver). Guarded by
`tests::fence` (a local TCP stub standing in for both servers).

**Read-time fields (issue #381, ADR 0011).** A root names a context type
(`#[flusso(index = …, context = Viewing)]`; `FlussoRoot::Context`, `()` for a plain root) and declares
`#[flusso(rules = f)]` / `#[flusso(computed = f)]` fields, decided when a document is read
(`sdk/query/src/rules/`). A rule field is ordered `Condition → outcome` arms (`Rules<T>`); the one
list is decided twice and the two must agree: in Rust on `_source` for the value (`Plan::value`), and
as plain queries for the filter, sort and counts (`RuleField<T>`; `Plan::select`), never a script.
Invariants to keep when editing:
- **A test exists only if Rust decides it as the index does.** Conditions are built from rule handles
  (`RuleKeyword`/`RuleBool`/`RuleNumber<K>`/`RuleDate`), which the derive emits behind `Root::doc()`
  (`read_time.rs`'s `rule_handles`) for keyword, boolean, whole-number, `double`, `decimal` and date leaves
  and the plain objects holding them, and only when the mapping has no option outside
  `RULE_SAFE_OPTIONS`. A new handle or test needs both halves (`Test::render` and `Test::holds` in
  `condition.rs`) and a case in the live fuzz. A document value of another type than its field is
  read as the index coerced it (`condition.rs`'s `scalar`: a fraction in a whole-number field
  dropped towards zero, a number or boolean spelled as text, `""` as `false`).
- **Rust decides the form that is sent.** `positive` pushes negations down and flips a negated range
  on a field the schema declares always present (`Presence::TOTAL`); `Plan` stores each arm as sent
  (`when`, `unless`) and `Plan::value` refuses a document no filter would select (`UNDECIDED`).
  "No value" is one thing on both sides: a fixed `null` and a missing stored field are selected
  together by `eq(None)` and fail every ordered comparison (`RuleField::one_of` / `compares`).
- **The value order** (sort, counts, and ordered comparison through a stored keyword) is
  `Rules::order`, else the replaced enum's `variants:` followed by the other fixed values the rules
  name (`RuleField::ordered`): a stored enum rarely declares what only the rules yield.
- **Dates compare at the millisecond** (`date.rs`): a timestamp to the second is one instant, a bare
  date its whole day, with `gt`/`lte` measured from the end of the stretch. The plain `Date` handle
  still lets OpenSearch read the whole second (issue #382).
- **The handle's type carries presence** (`Required`/`Optional`/`Array`), which decides what an arm
  may yield it as (`IntoOutcome`: `T`, `Option<T>`, `Vec<T>`; `assume_present()` is the opt-out).
- **A rule sort is the score**: `Sort::ranked` carries `function_score` functions, `Search::ranked`
  wraps the finished query (outside a `filter_nested` projection) and packs adjacent rule sorts;
  `Sort::to_value` of a rule sort is the invalid stand-in, so a combined search or a nested
  projection refuses it.
- **What survives compilation is refused before sending.** A rule that cannot be evaluated renders as
  the `flusso_invalid_rule` stand-in clause; `rules::refuse_invalid` stops it in `search_at`,
  `count_at`, `search_pit` and `msearch` (`append_lines`) as `Error::ReadTimeField`. A new transport
  that sends a query body must call it. A rule list that cannot be built fails every terminal of
  its root before anything is sent (`Search::runnable`, `Computation::check`).
- **Every read path computes.** `Computation` (rules first, all from the stored document; computed
  fields after, in declaration order) is applied in `Search::send`, `CursorSearch::send_from`,
  `msearch::decode_slot` and `Client::get_one_with`, before the document decodes. A new read path
  must apply it. A combined search cannot, so its derive requires `CombinedSearchVariant`
  (`Context = ()`; issue #383).
- The derive names the handle of a stored root field `stored_<name>()` when the root itself takes
  the plain name: a read-time field replaces it, or (on a root with a context) the key generates
  `doc`/`query`/`get` (`ReadTimeNames`).
Guards: `rules::tests` (incl. `pushing_negations_down_never_changes_what_a_condition_decides`, and
`reading`: every read path over a local stub server, computed, and refused with nothing sent),
`rules::date::tests`, the derive's `tests/read_time.rs` (`every_readable_kind_has_a_rule_handle`
over the `boundary` fixture) and the `rule_*` / `read_time_*` / `context_root_*` UI cases
(`rule_handle_missing_for_a_kind`: every kind with no handle), and the `read_time` live e2e
(values, filters, sorts, walks and counts against a plain-Rust oracle; random conditions decided by
the index and by Rust, over coerced values too: `store_loose_values`).

## Keeping this file current

This file is a living index — keep it accurate as part of normal work, no separate ask
needed. When a change makes something here wrong or missing, update it in the **same**
change: a crate added/moved/renamed (fix the layering + "Where things live" table), a
command or test workflow that changes, a new engine invariant or guard test, a lint policy
change, or a config/schema format change. Don't let it drift; don't pad it with detail that
belongs in the linked docs.

## Keeping the designer current (every feature aligns it)

The visual designer (`apps/design`) is part of the product surface, not an optional extra:
a feature isn't done until the designer can author it **and** its UI is fully translated.
When a change adds or alters something a user authors — a `*.schema.yml`/`flusso.toml` key,
a field type tag/sibling, an enum token, a source/sink capability — align the designer in the
**same** change: model/codegen/preview (`apps/design/`), the canvas/inspector controls
(`apps/design/frontend/`), and the introspection/source-steer if the source informs it. The one
exception is an **adapter option**: the designer's source/stream/sink forms render from the
adapters' `#[derive(AdapterConfig)]` descriptions (`AdapterForm.tsx`; per-index sink options from
`index_schema`, in each index row's drawer), so a new option or a new adapter needs no designer
edit beyond `just schema-gen`.
And any user-facing string goes through `t("ns.key")` with the key added to **every** locale
catalog in `apps/design/frontend/src/locales/` (English `en.ts` is the base; translate the rest).
Two CI guards in the `designer-frontend` job enforce this and will fail the build otherwise: the
**i18n check** (`npm run check:i18n`, key parity across locales) and the **dist-drift** guard
(committed `apps/design/dist/` must match a fresh build — so rebuild + commit the SPA). Locally:
`just design-i18n`, then rebuild the SPA. The `/implement` flow has an explicit step for this.

## Where things live (jump here first)

| To work on… | Go to |
| --- | --- |
| The sync loops / batching / ack ordering | `libs/2-engine/src/` — `ingest.rs` (`IngestEngine`: the run loop; `ingest/live.rs` batching and dedup, `ingest/builds.rs` the build gate and the grant-ordered slots, `ingest/commit.rs` granting and publishing, `ingest/snapshot.rs` request coalescing and the key walk, `ingest/confirm.rs` confirming `Progress::landed`; `progress.rs` the per-lane `Progress`), `sink_engine.rs` (`SinkEngine`: `stage`, reindex staging) with `sink_engine/writes.rs` (the run: window, admission, live-first selection, barriers, concurrent writes, failure policies) and `sink_engine/fences.rs` (applied/searchable in position order), `sink_engine/flights.rs` (the writes in flight, polled when woken), `policy.rs` (`BatchPolicy`/`FailurePolicies`), `tests.rs` (drives both engines over a real `ChannelStream`) |
| Pipeline observability trait (`Observer`, `BuildStats`/`CommitStats`, `EngineId`, `FanOut`) | `libs/2-engine/src/observer.rs` |
| Script fields (Rhai): compile, run, strip input-only fields, the `DocumentBuilder` decorator, the sandbox, value conversion, typed dates | `libs/2-script/src/` — `scripts.rs` (`Scripts`: plan + apply), `builder.rs` (`ScriptedBuilder`), `sandbox.rs`, `value.rs`, `dates.rs` + `dates/library.rs` (the date types + their Rhai registration), `view.rs` (a `map` of dates typed for scripts), `input.rs` (JSON typed by the schema); `benches/scripted.rs`; CLI `commands/script.rs` (`flusso script run`), `telemetry/script_log.rs` |
| Daemon (domain): supervisor, `DaemonControl`, per-sink `Status`, `StatusObserver`, lag poll, the fence | `libs/3-daemon/src/` — `lib.rs` (`Daemon`/`RunningDaemon`/`DaemonOptions`), `supervise.rs` (stage → spawn → restart with backoff; `DaemonControl::reindex`), `backends.rs` (`Backends` trait + `SourceParts`/`SinkParts` seam), `observer.rs`, `status.rs`, `lag.rs`, `fence.rs` (`Fence`/`FenceRequest`/`FenceOutcome`/`FenceObserver`) |
| Read-after-visible (issue #343): holding a change until its transaction is visible, the stall warning, the synchronous-standby refusal | `libs/2-adapters/source-postgres/src/cdc/stream.rs` (the hold in `pending`, the snapshot read on its own task), `cdc/visibility.rs` (`Snapshot`, `widen`, `poll_delay`, `refuse_standby`, `standby_listing`, `PgVisibility`), `cdc/capture.rs` (`check_server`, `with_read_ahead`, `readable_wait`); `libs/3-daemon/src/status.rs` (`with_source`); CLI `telemetry/metrics.rs` (`register_visibility_wait_gauge`); e2e `libs/2-engine/tests/visibility.rs`; scenario `dev/bench/src/standby.rs` (`sync-standby`) |
| Read-your-writes (fences), end to end | source: `libs/1-ports/source/src/cdc/capture.rs` (`SourcePosition`, `reached`, `on_advance`), `libs/2-adapters/source-postgres/src/cdc/ack.rs` (`Positions`: stamps + horizon, no map) and `stream.rs` (who moves the horizon); sink: `libs/1-ports/sink/src/sink.rs` (`Visibility`), `libs/2-adapters/sink-opensearch/src/sink_impl.rs` (`settle_visibility`); engine: `libs/2-engine/src/progress.rs` (`Progress`: what each lane landed), `sink_engine/writes.rs` (`write_returned` reports to it), `sink_engine/fences.rs` (the searchable queue); daemon: `libs/3-daemon/src/fence.rs`; CLI: `apps/cli/src/http/mod.rs` (`wait`); client: `sdk/query/src/fence.rs`, `position.rs`; e2e: `libs/3-daemon/tests/fence.rs` |
| Adapter registry (the one place that names adapters): `type` → config struct, `validate`, descriptions, flag overrides | `apps/cli/src/adapters.rs` |
| Adapter assembly (build the running source/sinks): the `Backends` impl | `apps/cli/src/backends.rs` (`FlussoBackends` — Postgres source + OpenSearch/stdout sinks) |
| Transport + telemetry (binary): exporters, metrics recording, HTTP surfaces, auth, signals | `apps/cli/src/` — `telemetry/mod.rs` (traces), `telemetry/metrics.rs` (meter provider + `in_flight` gauge), `telemetry/observer.rs` (`OtelObserver`), `http/mod.rs` (public + private routers + `serve`), `http/auth.rs` (Basic auth), `commands/run.rs` (orchestration + signals) |
| Config loading + the assembled `Config`/`Index` with `PortEntry` ports (layer 1) | `libs/1-config/src/` — `lib.rs` (`load`), `loader.rs`, `compiled.rs` (`flusso.lock`), `deployment/` (the `Config` family + `From<ConfigToml>` conversion + `resolve_mappings`) |
| Kernel vocabulary (the shared types — layer 0) | `libs/0-kernel/src/` — `config/` (`IndexSchema`, `FailurePolicy`, `Secret`, …), `common/` (newtypes), `GenericValue`, `envelope.rs` (`Envelope`/`Op`/`Position` — the stream message), `options.rs` (`Options`/`OptionValue`), `port_entry.rs`, `adapter.rs` (`AdapterConfig`/`AdapterDescription`/`Port`/`override_var`); the derive in `libs/0-kernel/derive/` |
| Adapter config types (one per adapter, `#[derive(AdapterConfig)]`) | `libs/2-adapters/source-postgres/src/config.rs` (`PostgresConfig`, `Connection`, `SslMode`, `Tls`), `stream-channel/src/config.rs`, `sink-opensearch/src/config.rs` (+ `TextAnalysis`, the per-index `OpensearchIndexConfig`; `settings.rs` holds `IndexSettings`: normalization, owned-key rejection, layering), `sink-stdout/src/config.rs` |
| `flusso.toml` parsing (entities only; conversion is beside `Config`) | `libs/1-config/src/toml/` (`entities/`) |
| `*.schema.yml` parsing / field syntax | `libs/1-config/src/yaml/entities/field.rs`, `conversion.rs` |
| Postgres WAL capture / backfill / doc building / publication management | `libs/2-adapters/source-postgres/src/` — `cdc/` (incl. `publication.rs`, `replica_identity.rs`), `document/` (`resolve.rs`: reverse resolution + the first-hop pre-image union; `keys.rs`: a backfill's key pages) |
| Source trait abstractions (`ChangeCapture` + `Continuity`, `ChangeEvent` + `RowImage` pre-image, `DocumentBuilder`, `SourceSpec` + `all_tables`/`pre_image_columns`/`pre_images`, `validate_indexes`, `CaptureProvisioning`/`CoverageReport`/`PreImageReport`, `SchemaIntrospection`/`RelationalCatalog`) | `libs/1-ports/source/src/` (`provisioning.rs` for coverage; `introspection.rs` for catalog enumeration + `junction_candidates`) |
| Visual schema designer (web app: introspect → edit → preview → write files) | `apps/design/` (`flusso-design`) — `server.rs` (axum + JSON API: project/catalog/test-connection/**parse**/**expression**/**value**/preview/validate/**sample**/diff/save; `value.rs` reads a typed inspector value as its saved file will in its slot (`constant`/`default`/`option`; a `default` takes only a scalar), refusing a number it cannot hold exactly or nesting past `MAX_DEPTH` (32, so preview/diff/save stay under serde_json's 128 levels), and returns the file's spelling for the *Saved as* note; the frontend's `model/exact.ts` keeps integers past 2^53 exact in every parse/clone, and `model/pending.ts` holds values still being read, one per target, flushed before Save/Code/undo (Save waits a bounded time and refuses, naming the field, on a read that failed or got no answer) and dropped on Reset; a reply is applied through the setter of the render it was typed in, which addresses the field by `uid` (`edit.updateField`), and the inspector is keyed by that uid), `codegen.rs` (model → canonical `*.schema.yml`/`flusso.toml`: a new file's form), `api.rs`'s `plan` (every save check, before any write: which on-disk index each saved index is comes from `SaveRequest.identity`, else by name, never from the request's word; a file the review leaves out is not checked; after any save every index of the on-disk `flusso.toml` has a file of its own with its own schema), the **stale-page check** (issue #280: `load_project` returns `Project.digests`, SHA-256 per project-relative file the model came from (`../x` outside the folder), `null` = absent; the page sends them as `SaveRequest.digests` — **required**, and `SaveRequest` denies unknown fields: `read_save_request` answers a page of another designer version with a 400 `BadRequest { reload }` — and merges `SaveResponse.digests` after its own save; `flusso.toml` is compared **first** (`stale_file`), before `plan` reads the disk: the review is that entry alone (`stale_config`), the save a `Stale` 409; then `plan` marks `OpDiff.stale` per file (a file the page never loaded counts as expected absent) and `save_with` refuses, under the lock, an applied entry that is marked; the expected digests go into `commit` as `Expect::Holds` (`Write.expect`/`Remove.expect`): `Journal::plan` takes them as the journal's `before`, and `run` re-checks each file (`Disk::holds`, not a counted step; `Disk::editing_at_check` is the test hook) right before the rename that replaces or removes it, so a lock-free writer meanwhile stops the commit with `Changed`, `undo_reached` undoes the renames done (keeping any file written since, `Disk::with_edit` is the test hook), and the API answers the same 409 naming every changed file — only a write between that check and the rename can still be lost; "overwrite anyway" sends the review's `StaleFile.on_disk`; completed recoveries are kept per process (`COMPLETED`) and reported on a diff/save whose digests show the page hasn't seen them (`unseen_recoveries`, compared before the save writes), and the page shows each `RecoveredSave.id` once; "overwrite anyway" recomputes the review with the override (`api.diff`'s `overwrite`), so Write shows every file it writes; guards: `tests/api.rs`'s `a_page_cannot_save_over_a_file_another_page_saved` and siblings, `a_config_another_page_restructured_is_the_refusal_not_a_failed_check`, `a_request_from_another_designer_version_is_told_to_reload`, `api::tests`' `a_file_written_*` and `an_editor_writing_during_a_save_gets_a_save_or_the_refusal`, and `commit::tests`' `an_editor_write_after_any_step_is_kept_and_nothing_is_left`), `commit/` (the all-or-nothing commit, kill- and power-loss-safe, issue #274; invariant in `mod.rs`'s doc: the project is, or after recovery becomes, exactly before or after a save: under `lock.rs`'s advisory lock (`.flusso-save.lock`, std `File::try_lock`, held for the whole commit and every recovery; never followed: a link or non-file there is `LockError::NotAFile`), `journal.rs` writes `.flusso-save.journal` (format 2 JSON: every destination and removal **relative to the config's folder**, with SHA-256 of each file before and after; temp/backup names are derived from the path, a token and the position, never stored). Order: a **record** in the user's state folder (`state.rs`: `<token>.json` = journal bytes' digest + canonical config folder + `committed`, under `$XDG_STATE_HOME/flusso/design-saves/` via `etcetera`, 0700/0600, `create_new` through a hard-linked temp, never followed, non-blocking reads, a folder or record open to others or another owner's is not used; `State::for_saving` falls back to an unrecorded save, logged once; pruned at start past 180 days, or with its folder missing once past 7 days (`GONE_AGE`); a temp a killed private write left goes once its pid is dead (an hour where that can't be checked); a unit test sees only its thread's `state::use_test_dir`, the integration tests call `api::use_state_dir`), then the journal, flushed temp files, hard-link backups, renames with `flusso.toml` last, the **commit point = marking the record committed** (one rename, then the state folder flushed), rename to `.flusso-save.done`, cleanup, record removed last; journal/staging/temps are written and read with `O_NOFOLLOW` (`disk.rs`'s `no_follow`). **A journal is acted on only when a record of this folder names its exact bytes**, so once, in its own folder: `recover.rs` decides roll back vs finish from the record, never the file name; a file holding a **third text** (neither its before- nor after-text, or deleted) is another program's write and is kept, never blocks (`recover::Kept`, `RecoveredSave.kept`): `finish` cleans up around it, a rollback restores the rest (`Backups::Keep` in recovery keeps and names its before-text backup, `Backups::Drop` in the save's own undo drops it, a backup holding the kept bytes always goes), so the result is before or after plus that write, never a mix; only an unreadable file, a missing backup or a failed step blocks; `Journal::steps` then accepts only what a save writes (plain names, leading `..` for a `plan`-accepted outside file, no save-file name, no link, config only as the last write, removals inside, `dirs` ancestors of a destination, ≤ `MAX_ENTRIES` paths of ≤ `fspath::MAX_PATH` bytes); anything else (no record, another folder's, no state folder, too large, malformed, not a save, `.journal` + `.done` both) is never acted on: `recover.rs` blocks with a read-only `Report` that looks only inside the config folder (`Inside`; an outside path is text, `Holds::Outside`) and gives `Report::route` back/forward as literal `HandStep`s (each file's `Fix`, every implied temp/backup on disk whatever it holds, created folders if empty, the journal last) plus "delete only the journal"; a blocked *recorded* save carries its `Direction`, per `Problem` a `fix` from a copy verified by digest at report time (never a backup while finishing: the after-text is nowhere else) and, when every problem is a missing backup, `keep` steps; `commit::tests` follow every such step literally and assert the promised state. Every recovery action is digest-gated (an empty temp is deleted only while rolling back and its file unreplaced; a `dirs` folder only when empty); `fspath::on_disk` (shared with `plan`, which caps request paths at `MAX_PATH`) bisects for the deepest existing ancestor, so resolution is O(log n) canonicalizations; the backups are deleted only once the commit mark is flushed; `api.rs`'s `recover_project` runs it at `flusso design` start, on `/api/project` (handlers run it under `spawn_blocking`), and before a diff/save that finds a journal: a rollback lets the diff/save go on with the notice, a completion is reported except the process's own save whose cleanup it left (`OWN_CLEANUP`), and the pages that loaded before it are caught by the digest check below; a blocked one is a 409 too (carrying `SaveRecovery`, with `BlockedSave.report`; the frontend's `components/Recovery.tsx` shows the notice or, when blocked, the files and the report instead of the project); stray `*.flusso-save.*` files with no journal are only reported; `disk.rs`'s `Disk` counts steps so `commit::tests` fails, or stops dead after, each one, for the commit and for recovery itself, and `a_process_aborted_mid_save_is_recovered` re-runs the test binary as a child that aborts mid-save), `keep/` (a save, `/api/diff` and the preview's YAML over an existing file: its text is patched only where the canonical renderings of its model and the saved model differ, so comments and untouched formatting stay — `yaml.rs` a line-based block scanner: the comment block directly above a key/item is its own, a blank-line-separated one is nobody's and stays put, a list's trailing comments are the list's; `toml_doc.rs` over `toml_edit`, the same rules for headers via markers in their prefixes; list items, `[[index]]` entries and sinks pair by identity — `pairing.rs`, from the uids/index ids/sink ids the client sends as `FileOp.identity`/`SaveRequest.identity`/`PreviewRequest.identity` (before = the model the base reads as, after = the saved model; a sink rename is applied to the TOML in place, `[index.sinks]` included, before the patch; an identity with an empty, repeated or unknown id is ignored), else by name, never by slot; a `|+` scalar owns the blank lines after it, so the patcher drops blank lines it would put after one (`KEPT_OPEN`); a comment dropped while its owner may still exist counts in `OpDiff.dropped`; the result is re-parsed and must equal the model, else the canonical form with `OpDiff.notice`; the Code-mode text is the save `base` — `FileOp.base`, the design store's `texts`, under undo with `doc`), `preview.rs` (mapping + document tree), `assets.rs` (embedded SPA); CLI `design` subcommand in `apps/cli/src/commands/design.rs`; frontend under `apps/design/frontend/` (React Flow node-graph canvas — `model/` projects the `IndexSchema` tree ↔ nodes/edges + path-addressed edits (every field carries a client-only `uid` from `ids.ts`: a container's is its node id, which collapse state + drag positions key on, and inspector edits address a field by it; stripped from every request), plus `complete.ts` (incomplete-field checks) and `prune.ts` (drops incomplete pieces from the **live preview** payload only, so a mid-build blank name doesn't 400 the strict backend, and lists them; that copy is lossy, so Code mode seeds from the unpruned schema and refuses to open while anything would be pruned), `selection.ts` (the selection carries the selected field's uid; the store re-points its path after every doc change and clears it once the field is gone), `rowIds.ts` (option rows keyed by a row id, not their editable key), `components/` the canvas/nodes/inspector/catalog-browser), built to `apps/design/dist/`; property round-trip in `apps/design/tests/roundtrip.rs` (also: random edits saved into a hand-written, commented file keep every comment with the field it names). The **sample document** preview builds a real doc from one live row via `PgDocumentBuilder::sample_document` (postgres crate — keeps sqlx/`RowKey` there; reuses the `build` path + `sink::to_json`) |
| `Sink` trait (`write`, safe for concurrent calls; seeding; `reindex`), `WriteReport`, `SinkOptions`, JSON render | `libs/1-ports/sink/src/` |
| OpenSearch sink (bulk, mappings, seeding; alias-over-generations + reindex) | `libs/2-adapters/sink-opensearch/src/` — `lib.rs` (the `OpensearchSink` type + ctor), `sink_impl.rs` (the `Sink` impl), `transport.rs` (HTTP plumbing + index CRUD), `generations.rs` (aliases, meta doc, generation naming), `mapping.rs` (index body/analysis), `bulk.rs` (wire format + chunking) |
| Stream port (`Stream`, lanes, `LaneItem`/`Request`, `Producer`/`Consumer`/`AckHandle`) / in-process channel adapter (ticketed in-flight ledger) | `libs/1-ports/stream/src/` (`items.rs`, `queue.rs`), `libs/2-adapters/stream-channel/src/lib.rs` |
| Editor schema + Reference table generation (`flusso schema config\|docs`, drift test) | `apps/cli/src/commands/schema_cmd.rs`; artifacts at `libs/1-config/config.schema.json`, `docs/src/reference/generated/` |
| CLI subcommands (`build`/`run`/`check`/`schema`/`indexes`/`reindex`) | `apps/cli/src/` — `main.rs` dispatches; `commands/` holds one module per command (`build.rs`, `run.rs` → composition root: installs telemetry, serves the HTTP surfaces, drives `Daemon::start`/`run`, owns signals; `check.rs`, `schema_cmd.rs`, the `indexes`/`reindex` HTTP-client `admin.rs`, shared `print.rs`); `telemetry/` and `http/` hold the transport, `backends.rs` the backend assembly |
| On-demand reindex (alias-over-generations, per sink, no restart) | sink: `libs/2-adapters/sink-opensearch/src/sink_impl.rs` (`reindex`/`ensure_index`/`mark_seeded`) + `generations.rs` (generation helpers); engine `SinkEngine::reindex` (staged between batches + a `Backfill` request); daemon `DaemonControl::reindex` (`supervise.rs`); CLI `http/mod.rs` (`POST /reindex?index&sink`), `commands/admin.rs` (client, `--sink`). Deferred write-side zero-downtime follow-on: issue #6 |
| Query client (`flusso-query`) | `sdk/query/src/` (`cursor.rs`: page cursors; `walk.rs`: streaming a walk; `fence.rs`: fenced reads; `position.rs`: reading the source position; `rules/`: read-time fields — `condition.rs` the two-sided tests, `date.rs`, `handles.rs` the rule handles, `list.rs` `Rules`/`Plan`, `field.rs` `RuleField`, `computation.rs`) |
| `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]` proc-macros | `sdk/query-derive/src/` — `lib.rs` (entry points + `Attrs`), `doc.rs` (field parsing/validation + the recursive handle tree + `embed_checks`), `read_time.rs` (what `context`/`rules`/`computed` generate, and which stored fields get a rule handle), `fragment.rs` (the location-free shape check), `spec.rs` (baking a level into `&[FieldSpec]`), `resolve.rs` (finding `flusso.toml`); the const-check vocabulary is `sdk/query/src/check.rs`. Plus the `flusso-query-derive` memory note |
| Runnable example (stack, seed, consumer) | `dev/` (`flusso.toml`, `postgres/init/`, `search-api/`) |
| The Postgres image tests and benches start (default tag, `FLUSSO_TEST_PG_TAG`, `Pinned` exceptions, `Container`) + its guard | `dev/test-postgres/` (`flusso-test-postgres`, `publish = false`, path-only dev-dependency), `.github/scripts/check-postgres-image.py` + `test_check_postgres_image.py`, `docs/src/contribute/testing.md` ("Which Postgres") |
| Benchmarks: scenario harness, in-process + Docker benches, CI gate + history | `dev/bench/` (`flusso-bench`: `src/main.rs` phases, `scenario.rs` mixes, `scale.rs` presets, `startup.rs` time to `live`, `flusso.rs` child process, `scenarios/`), `libs/2-engine/benches/{engine,pipeline}.rs`, `libs/2-adapters/source-postgres/benches/{pgoutput,resolve,postgres,cases,review}.rs` + `tests/record_pgoutput.rs`, `libs/2-adapters/sink-opensearch/benches/{render,opensearch}.rs`, `.github/workflows/bench.yml`, `.github/scripts/bench-*.{py,sh}`, `.github/scripts/review-ab*.{sh,py}` (`just review-ab`, `review-digest`, `review-plans`; the two review modes' harness is `benches/review.rs`, over `cases.rs`'s roots), `docs/src/contribute/benchmarks.md`, ADR 0006 |
| Registry image / containerized demo | `Dockerfile` (`runtime` target = config-less registry image; `demo` target = + baked dev lock), `docker-compose.demo.yml` (override adding the `flusso` service, built from the `demo` target), `.dockerignore`; user-facing shipping recipes in `docs/src/deploy/docker.md` |
| Kubernetes deploy (Helm chart) | `deploy/helm/flusso/` — `Chart.yaml`, `values.yaml`, `templates/`, `README.md` |
| Agent-facing docs (the Claude plugin + internal commands) | `plugin/` — `ARCHITECTURE.md` is the contract (one corpus/three consumers, who owns which meaning, the self-containment rule), `skills/*/SKILL.md` the knowledge corpus (`flusso-query` discloses `migration.md`/`options.md`/`maps.md`/`read-time.md`), `commands/` thin workflow entries, `agents/flusso-expert.md`, `hooks/`; `.claude/commands/{implement,new-issue}.md` the internal spine; `.claude/skills/coordinator/SKILL.md` how a round of issues is driven through agents (order of work, the two independent reviews, merge and issue rules). Guarded by `apps/cli/tests/agent_docs_paths.rs` |
| Domain glossary + architecture decisions | `CONTEXT.md` (the vocabulary: kernel / ports / adapters / engine / daemon, stream / lane / envelope / position / source position / fence, operation vs primitive vs transport) and `docs/adr/` (0001 adapter-owned config, 0004 the rename, 0005 lock format 3, 0006 benchmarks, 0007 reverse resolution reads the pre-image, 0008 page cursors are keyset with an opt-in point-in-time, 0009 a fence waits on the source's own position, per sink, at two observed levels: accepted; 0010 a change is emitted once its transaction is visible: accepted, issue #343; 0011 read-time fields are rules decided twice, with no script: accepted, issue #381; 0012 each index runs on its own task and order is kept per document: accepted, issue #386, landing in stages, so the pipeline above describes `main` until each stage updates it; 0002/0003 the engine split: `proposed` until phase 2 of #130 lands) |

## Conventions

- **Code is self-descriptive; comments are the exception, not the rule.** The code itself —
  names, types, structure — must carry the meaning. Do **not** write comments that narrate or
  restate what the code already says. The only inline comments that belong in production code
  are: (1) doc comments (`///`/`//!`) on items/modules — always keep and write these for the
  published API surface; and (2) comments that explain genuinely *non-obvious* behavior the code
  cannot express — a correctness invariant or ordering that would otherwise look like a bug,
  cancel-safety/concurrency rationale, why an unused/`_`-bound value or a deliberate no-op must
  stay, an upstream-bug workaround, or a "must happen before X" constraint. If a comment could be
  deleted by renaming a variable or extracting a function, do that instead of commenting. When
  reviewing or editing, strip narration; keep only the genuine gotchas.
  - **NEVER add useless comments when writing new code.** Useless means any of: a comment that
    describes the change you just made ("added X", "now does Y") — that belongs in the commit, not
    the code; a comment that restates code that already describes itself; a comment that
    re-describes a function or type that already carries an explicit doc comment.
  - **DO comment, briefly, in exactly these cases:** a public-API function gets a short `///`
    saying what it does for the caller; an externally-exported struct used as public API gets a
    `///`, and so do its fields. That is the bar for "when to comment" — public surface, said
    once, concisely.
  - **Module docs (`//!`) are the one place to be detailed.** Every module gets a `//!` header
    that explains what the module does and how it fits in, and includes runnable examples
    (```rust``` blocks — they're doctests, so keep them compiling). This is where depth belongs;
    spend the words here instead of scattering them across inline comments.
  - **Tone, everywhere — write for an ADHD reader.** Lead with the point, keep sentences short and
    concrete, prefer scannable structure (a one-line summary first, then specifics) over dense
    prose. Applies to `///`, `//!`, and the rare inline gotcha alike.
  - **Prose docs have their own tone guide.** The above governs code comments. The mdBook
    chapters (`docs/src/`) and per-crate READMEs follow `docs/STYLE.md` — impersonal/conversational
    voice, expert body with newcomer callouts, terse + scannable, `flusso` always lowercase. Read it
    before writing or editing a doc page.
- Domain newtypes (validated identifiers, URLs) use the `nutype` crate (`try_new`) — see
  `libs/0-kernel/src/common/`. `GenericValue` is the **typed canonical value vocabulary** that
  crosses layers — the middle type a source maps *into* and a sink maps *out of*. It's
  fine-grained (numerics split by width; `Date`/`Time`/`Timestamp`/`TimestampTz`; `Uuid`;
  `Bytes`) so no type is erased in transit, and its serde is **derived/format-agnostic** (a
  queue may encode it however it likes; it round-trips losslessly). A sink converts it to its
  own representation at its boundary (`sink::Json` is the JSON translation the OpenSearch and
  stdout sinks write, a `Serialize` view where e.g. `bytea`→base64 lives, a `BigDecimal` is written as
  its exact number text through serde_json's raw value and a `Decimal` as its nearest `f64`, as
  always; `sink::to_json` is the same as a `Value` tree, numbers as `f64`, a `BigDecimal` past
  `f64`'s range as its text) — core picks no wire format. serde_json's
  `arbitrary_precision` stays off: it is global, allocates per number, and breaks numbers under
  serde's buffered (`flatten`/`untagged`) deserialization.
- **Numbers are read and written exactly (issue #366).** serde_json's default float parser is not
  correctly rounded (about one `float8` in three read one ulp off), so `float_roundtrip` is on where
  flusso reads numbers it keeps: `source-postgres`, the CLI (`script run --input`) and `flusso-query`
  (`_source`, cursor sort values; always on, so it unifies into the consumer's build: a deliberate choice,
  cost stated in its README). A declared `float` reads its raw text (`value.rs`'s `single`) as Postgres's
  `::float4` would: up to 9 significant digits or 18+ parsed as `f32` (trailing zeros not counted: `jsonb` pads a large double with them), 10–17 (a `float8`'s shortest text)
  through the correct `f64` then `as f32` (a correct `f64` then `as f32` on a `float4`'s text rounds twice:
  `7.038531e-26`); past `f32` it is infinite (written `null`, as 0.17). A number with no declared type is
  `kernel::untyped_number`, the one rule for the reader and `--input`: the exact `Decimal` of its shortest
  digits, below 1 with more digits than `Decimal` holds the `BigDecimal` of them (the sink writes them as
  they are, a script reads the nearest `Decimal`), past ±7.9e28 0.17's string (a number there would be
  refused by a dynamically mapped `float` key), so a script sees one kind; a whole number past `i64` is the
  exact `Decimal`. `f64` ↔ `Decimal` goes through `kernel::f64_to_decimal`/`f64_to_nearest_decimal`/
  `decimal_to_f64` (shortest text / correctly rounded, with an exact `m / 10^s` fast path), never
  `Decimal::try_from(f64)` or `to_f64`, which are lossy. Guards: `value::tests`'
  `a_double_is_read_as_the_value_postgres_wrote`, `a_float_is_read_as_the_single_postgres_wrote`,
  `a_float_reads_as_postgres_casts_to_float4`, `every_float_postgres_writes_is_read_as_itself`, the
  `kernel::common::float` proptests, `script`'s `tests/untyped_numbers.rs` and
  `a_decimal_with_an_exponent_keeps_the_doubles_digits`, `flusso-query`'s
  `a_double_is_read_as_opensearch_wrote_it` (fails without the feature only when the crate is built alone),
  and `pipeline`'s `a_float8_is_indexed_as_the_double_postgres_holds` and
  `an_untyped_number_lands_where_0_17_s_did` (a fresh index and one 0.17's form mapped).
- Sources/sinks are `#[async_trait]` trait objects; mock them in tests as the engine tests do.
- **Every `reqwest` client flusso builds pins its trust roots.** reqwest 0.13 verifies against the
  OS trust store by default; the OpenSearch sink, `flusso-query` and the CLI's admin client each
  call `tls_certs_only` with the Mozilla bundle from `webpki-root-certs` (`mozilla_roots`), as 0.12's
  `rustls-tls` did. A new client that skips it silently changes which certificates it trusts. A
  private CA is *added* to that set, never swapped for the OS store: the sink's `ca_file` option,
  `Client::ca_file`/`ca_pem`, the admin client's `--ca-file` (each read at build time, so a bad file
  fails the start). The sink's bulk retry skips a refused certificate (`transport::is_certificate_error`,
  typed: the `rustls::Error` inside nested `io::Error`s, opened with `get_ref`). The
  OTLP exporter builds its own client and reads the OS store.
- **The whole workspace publishes to crates.io** (so `cargo install flusso-cli` works), under a
  `flusso-*` package namespace. Every crate is published **except** `dev/search-api`
  (`flusso-dev-search-api`), `dev/query-e2e` (`flusso-query-e2e`), `dev/bench` (`flusso-bench`), and
  `dev/test-postgres` (`flusso-test-postgres`) — all `publish = false` (a runnable example, a live-e2e
  guard, the benchmark harness, and the test Postgres image, not shipping code). The
  catch: a crate's published **package name** (`flusso-engine`, `flusso-kernel`, …) differs
  from the **extern name** code uses (`engine`, `kernel`, `source_postgres`, …). Two mechanisms keep that split
  so the rename needs **no source change**: each lib sets `[lib] name = "<extern>"`, and each
  `[workspace.dependencies]` entry keeps its short key plus `package = "flusso-…"` + `version`.
  So `use kernel::…`, `package(flusso-source-postgres)` in `.config/nextest.toml`, and the
  fuzz crate's `package = "flusso-source-postgres"` path-dep all coexist. Shared listing metadata
  (license, repo, authors, keywords, readme) lives in `[workspace.package]`; crates inherit it
  with `.workspace = true`, and set their own `description` + `categories`. **Releases are three
  independent trains** (issue #110; the `version_group`s in `release-plz.toml`): **libs** (every
  `libs/*` crate, one version), **cli** (`flusso-cli` + `flusso-design` — the SPA ships in the
  binary), **query** (`flusso-query` + `flusso-query-derive`, the derive exact-pinned `=x.y.z`
  serde-style). Each crate owns its `version` (no shared `[workspace.package] version`), and the
  internal `[workspace.dependencies]` reqs are **minor-precision** (`"0.15"`) — that precision is
  load-bearing: release-plz rewrites a dependent's req only when the new version stops matching,
  so a libs *patch* releases nothing but libs, while a libs 0.x *minor* (breaking) legitimately
  cascades a bump into the dependent trains. Never tighten those reqs to patch precision. A libs
  fix reaches the docker/dist binaries only via a cli-train release: land a `fix(cli): adopt …`
  commit appending to `apps/cli/ADOPTIONS.md` (the binaries build in-tree from the tag, so they
  always carry main-tip libs). **Publish order within a train is still bottom-up** (a dep must be
  on crates.io before its dependents): `flusso-kernel` → `flusso-kernel-derive` → the ports
  (`flusso-source`/`flusso-stream`/`flusso-sink`) → `flusso-config` → the adapters →
  `flusso-engine` → `flusso-daemon`, then apps on top (`flusso-design` → `flusso-cli`;
  `flusso-query-derive` → `flusso-query`). The pre-0.16 names (`flusso-schema-core`, …) stay on
  crates.io at their last release; `scripts/tombstone-crates.sh` publishes a final pointer README
  under each (manual, needs a token; ADR 0004).
- **Agent-facing docs follow `writing-for-agents`, not `docs/STYLE.md`.** `plugin/**`,
  `.claude/commands/**` and `.claude/skills/**` are consumed by an agent, so `plugin/ARCHITECTURE.md` is the standard: each
  meaning has exactly one home and everything else points at it. **This file owns the definition of
  done** (the designer + i18n + dist rule, the editor JSON schemas, the CI order, the engine
  invariants); a skill or command restating any of it is the defect, so point here instead. The
  internal commands point at the `mattpocock-skills` collection (`/grill-with-docs`, `/code-review`), which
  `.claude/settings.json` declares so any checkout has it. `apps/cli/tests/agent_docs_paths.rs`
  fails the build on a dangling repo path in either tree.
- `dev/` is a runnable example, not shipping code; the two editor-assist schemas live **inside
  the `config` crate** (so they ship in the published `.crate`): `config::CONFIG_SCHEMA`
  (`libs/1-config/config.schema.json`, **generated** by `flusso schema config` from the adapters'
  descriptions, drift-guarded by a CLI test) and `config::INDEX_SCHEMA`
  (`libs/1-config/index.schema.yml`, hand-curated), each embedded via a crate-local
  `include_str!` and emitted by `flusso schema config|index`. They sit in-crate (not bare at the
  repo root) because `cargo package` only bundles files under the crate dir. On each release the
  `.github/workflows/pages.yml` workflow publishes copies to GitHub Pages under an immutable
  per-version path (`https://alias2k.github.io/flusso/schemas/v<version>/{index.schema.yml,config.schema.json}`,
  plus `v<minor>` and `latest` aliases), triggered by the `flusso-config-v*` release tags (the
  pre-0.16 `flusso-schema-{index-yaml,config-toml}-v*` tags are still read at their old paths);
  editor `# yaml-language-server: $schema=…` modelines point at that versioned URL, while in-repo
  files (`dev/*.schema.yml`, the test fixtures) use a relative path to the in-crate files.
  `libs/1-config/tests/schema_drift.rs` guards the index schema's enumerable sets — field type
  tags, field siblings, enum tokens — against the parser (reading the embedded const), so adding
  a tag/sibling/variant fails CI until the schema matches. It does **not** check descriptions,
  defaults, the permissive `field` union, or the identifier `pattern`s (which can't model the
  newtypes' trim/lowercase sanitization). The config schema needs no such test: it is generated.
