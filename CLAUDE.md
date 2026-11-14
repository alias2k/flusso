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
manual is the fifteen chapters under `docs/src/query/`, the designer is
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
just bench                                 # in-process benches (engine loop, pgoutput decode, sink render); no Docker
just bench-components                      # Docker-backed Criterion benches (postgres, opensearch, engine pipeline)
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
  (or `BENCH_PG_URL`/`BENCH_OS_URL`) through `reference` (the dev store, three indexes) and
  `complex` (the worst-case document) at a `ci` or `default` scale, and report the headline
  metrics: visible latency p50/p99 (commit → searchable, via the `updated_at`/`updatedAt` stamp on
  the dev root tables), drain throughput of a burst, backfill throughput, peak RSS + CPU (sampled
  with `ps`), plus flush p50/p99 from `/metrics` for attribution. (2) **Docker-backed component
  benches** (`postgres`, `opensearch`, `engine`'s `pipeline` — attribution only; its backfill group
  is gone). (3) **In-process benches**, no Docker, the PR gate: `engine`'s `engine` (both engines
  over the channel stream with mocks), `source-postgres`'s `pgoutput` (the decoder over the recorded
  `benches/fixtures/pgoutput.bin`; `tests/record_pgoutput.rs`, an `#[ignore]`d Docker test in the
  nextest `docker` group, re-records it), `sink-opensearch`'s `render`, `script`'s `scripted` (public
  API, no feature; `bench-in-process.sh` skips it on a PR base that predates the crate). `pgoutput` and `render` sit behind each
  crate's `bench` cargo feature (like `fuzzing`), so CI's `cargo check --all-targets` passes
  `--features flusso-source-postgres/bench,flusso-sink-opensearch/bench`. `.github/workflows/bench.yml`:
  on a PR, `in-process` runs the in-process benches on the base (its own worktree **and its own
  target dir**: a shared one makes cargo run the base's binary for the PR, since a workspace member's
  fingerprint is keyed by its path inside the workspace) then the PR, same runner, both into one
  `CRITERION_HOME`; `bench-provenance.py` checks each side ran executables from its own target dir; a
  base with no in-process bench (exit 78) only warns, any other base failure fails. `bench-gate.sh`
  re-measures each bench past 10% slower (`bench-compare.py`) on its own, base then PR (`--exact`), and
  a breach is confirmed when the re-run or the mean of both passes is past 10%, cleared only when both
  are within it; on a `main` push, `scenarios` runs everything and stores the
  points via `github-action-benchmark` on the `benchmarks` data branch (folded into Pages at
  `/bench/`), then `bench-regression.py` compares each point to the median of the previous five and
  calls a regression only on two consecutive points past 25% (20% RSS), image-tag change restarts
  the window, one self-maintained issue. `just load` is the soak script — not a benchmark.
- The `#[ignore]`d e2e tests live in `source-postgres`'s `integration`,
  `config_coverage`, `publication`, `introspection`, `tls`, `wal_idle`, `continuity`, and `partitions` binaries
  (`tls` boots a hostssl-only PG 16 with a committed throwaway self-signed cert and
  proves the replication stream + SQL pool honor `sslmode`; `wal_idle`: the slot keeps
  advancing from keepalives/filtered commits while watched tables are idle, so unrelated
  writes don't pin WAL retention — issue #111; `continuity`: `continuity()` is read-only and
  reports `Fresh` exactly while the slot is missing, `prepare()` creates it — issue #120;
  `partitions`: a range-partitioned root and child stream under the parent's name via
  `publish_via_partition_root`, resolve, and backfill through the parent — issue #143) plus
  `engine`'s `wal`, `wal_release`, and `pipeline`
  binaries (`wal_release`, issue #204: with the engines running over recording sinks, the slot's
  `confirmed_flush_lsn` and `restart_lsn` pass the WAL written by changes that produce no document
  — an unread table under `FOR ALL TABLES`, unread-column updates, filtered/soft-deleted/orphaned
  rows, a large and a split mixed transaction, idle WAL after a burst — `confirmed_flush_lsn` within
  `CONFIRM_BOUND` (5 s, what flusso controls; also from the last sink flush for bursts and lone
  changes that produce documents, issue #205), `restart_lsn` within `RELEASE_BOUND` (45 s, also
  waits on Postgres's next `running_xacts` record), and the sinks got exactly the expected ops; a stalled sink
  pins the slot at its last flushed position (the watermark's minimum over lanes) until it catches
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
after a live update; constants land as written), and `an_update_of_an_unread_column_writes_nothing` (issue #169: an update changing
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
  sink. `sink-opensearch`'s `reindex` binary also pins the visibility a flush reports
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
  before `START_REPLICATION` (`capture.rs`'s `live`), a refusal stopping the start; the builder keeps
  its after-connect `SET bytea_output` for document bodies only. The temporal parser reads any year
  width by hand (chrono's `%Y` stops at four digits), so a year past 9999 is typed and keeps its
  `+10000-01-01` id. A domain column is retyped to its base OID when the
  stream keys its `Relation` (`stream.rs`'s `retype_domains`). A value the parser can't hold
  (`infinity`, numeric `NaN`) stays text on both sides and binds as `$n::text::<type>`
  (`sql::key_cast`): sqlx caches a statement by its text with its first parameter types, so a text
  value bound where a typed one was prepared fails in binary format. The backfill reads each table
  through **its own cursor name** (`cdc/backfill.rs`) for the same cache reason (a shared `FETCH`
  decoded a later table's key with the first table's row description). **A resolved document id is
  the root row's own key** (`resolve.rs`'s `reads_root_from_a_link` / `root_keys`): where a path
  ends on a link column's value (`row_link` of its first step) and the root key's equal values can
  render differently (`ColumnMeta::equal_is_identical`: `numeric`, floats, `citext`, a
  nondeterministic collation) or the link column's type differs from the key's, `resolve` reads the
  keys back from the root table, **one query per change** per index and link column (`RootLinks`;
  `root_keys_query` casts each link from its bound type to its link column's type), as the change is
  resolved: a batched read was tried and dropped, since resolving at commit slowed the engine's
  `live_drain` bench by 13% and moved when the database is read; every other key type costs no query. **A
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
- `apps/cli/tests/agent_docs_paths.rs` asserts every repo path named in `plugin/**` and
  `.claude/commands/**` exists — the moved-file guard for those pointer-heavy trees. Ordinary test,
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
  nextest only builds test targets; the features unlock the two in-process bench targets) → `cargo check -p
  flusso-cli --no-default-features` (the server-only image build) → `cargo nextest run
  --profile ci --run-ignored all` → `cargo test --doc` → `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --document-private-items` (broken/ambiguous/redundant intra-doc links fail
  the build). Match these before assuming green. One scope note: on PRs confined to the query
  release train (`sdk/query`, `sdk/query-derive`, `dev/query-e2e`) the nextest step swaps in a
  filterset that skips the Postgres/OpenSearch container suites those paths can't break (the
  query train's own `combined_search`, `cursor_paging`, `map_keys`, `tls` + `array_columns` live e2e still run); pushes to main always run everything.
  A `docs` job builds the mdBook (`mdbook build docs`), link-checks it (`lychee --offline
  --include-fragments --root-dir docs/book 'docs/book/**/*.html'`), and runs
  `.github/scripts/check-manual-links.sh`. A separate `fuzz` job runs a 60-second
  `pgoutput_decode` smoke fuzz on nightly (see below); the `query.rs` proptests need no special
  handling — they're ordinary tests caught by the nextest step. Outside `ci.yml`, `audit.yml`'s
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
                 ┌─ lane ─▶ SinkEngine(primary) ─▶ apply ─▶ flush ─▶ ack ─┐
ChangeCapture ─▶ IngestEngine ─┤                                             ├─▶ watermark ─▶ confirm
  resolve · build once · publish└─ lane ─▶ SinkEngine(audit)   ─▶ apply ─▶ flush ─▶ ack ─┘
                 ◀─ request lane ─ Backfill { sink, indexes } ─────────────────────
```

Two generic engines, backend-neutral (ADR 0002/0003, issue #130). The **ingest engine**
(`IngestEngine`) owns the source side: capture → batch → resolve → dedup → `build_many` once →
publish one `LaneItem::Batch` of kernel `Envelope`s to every sink's lane; it also drains the
**request lane** and serves `Request::Backfill { sink, indexes }` as snapshots. One **sink
engine** per sink (`SinkEngine`): `stage` (ensure indexes, retire stale seeds, request its
backfill) then recv → `apply` → `flush` → ack. Everything they drive — `ChangeCapture`,
`DocumentBuilder`, `Stream` (lanes + request lane + watermark), `Sink` — is a **trait object**.
`FanOutSink` is gone; sinks are independent. Key invariants to preserve when editing the engines:

- **At-least-once via flush-then-ack and the watermark.** A sink engine acks a lane item *only
  after* the `Sink::flush` that made its documents durable; acking a positioned batch records
  that position for the lane, `Stream::watermark()` is the **minimum** over lanes, and the ingest
  engine hands it to `ChangeCapture::confirm` after each commit (and on a 1 s tick), so the slot
  advances past a change exactly when *every* sink has landed it. The Postgres capture confirms
  within ~1 s of the last flush (issue #205): `Positions::confirm` wakes the stream loop
  (`confirmed_moved`), which hands `confirmed_lsn()` to the replication client at once instead of
  at the next replication event, and `WalChangeCapture::new` sets the client's status and idle
  wake-up intervals to 1 s (`STATUS_INTERVAL`, fixed, no option). `Positions` stays the one place
  that computes what may be confirmed; only the moment of hand-over is new. `pgwire-replication`
  0.5 sends a raised LSN within its status interval on its own (issue #215), but only once
  `update_applied_lsn` is called, which only the stream loop can do (`recv` takes `&mut`), so the
  wake-up stays: without it a confirm waits for the next replication event (measured +1 s quiet,
  up to +1 s under a trickle). Guards:
  `a_confirmation_reaches_the_client_without_a_replication_event` (`cdc::stream` unit, over the
  `Replication` seam) and `wal_release`'s `the_slot_confirms_within_seconds_of_the_last_flush` (`CONFIRM_BOUND`, 5 s). A sink engine that dies before
  its flush leaves the item unacked; the lane redelivers it to the restarted engine (idempotent
  rebuild, deterministic id). Positions are monotonic for the life of the source across
  reopened streams (`Positions::new(start_lsn, first_seq)` continues from the previous map), so
  the watermark the lanes hold across an ingest-engine restart never names a new-stream change.
  Guards: `confirms_no_position_before_its_flush`,
  `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, the channel adapter's
  `watermark_is_the_minimum_over_lanes`/`several_deliveries_can_be_outstanding_and_ack_independently`,
  the daemon's `failed_sink_engine_restarts_and_redelivers` /
  `failed_ingest_engine_restarts_and_resumes_the_stream`, the Postgres adapter's
  `a_reopened_stream_continues_the_numbering`, and the `wal_release` e2e (an empty batch is still
  published and acked, keepalives move the horizon, a `backfill = false` lane acks).
- **A fence only observes, per sink, at two levels (issue #136, ADR 0009).** A client reads the
  **source position** after its commit (`pg_current_wal_lsn()`) and waits on the daemon's `Fence`
  (`libs/3-daemon/src/fence.rs`; the CLI serves it as `GET /wait`) until a sink, or every sink, has
  every commit at or before it **applied** (flush returned) or **searchable** (a refresh covered
  it). Nothing is flushed or refreshed for a waiter, and a backfill is not covered. Three pieces,
  each load-bearing: (1) `ChangeCapture::reached(Option<Position>) -> Option<SourcePosition>`
  translates a stream position into the source's terms. The Postgres `Positions` map
  (`cdc/ack.rs`) answers it from its entries plus a **horizon** (the LSN through which everything
  read is registered: moved by the last change of a commit being emitted, and by a keepalive or a
  filtered commit while nothing is buffered; seeded from the slot's `confirmed_flush_lsn`). The
  answer is **strictly below the next outstanding change's LSN**, because the changes of one
  transaction share its commit LSN: a transaction split across batches is neither confirmed nor
  fenced until its last change is in. A fence claims everything short of that commit (`next - 1`);
  `confirm` stops at the **end of the last covered commit**, because Postgres skips a transaction
  whose commit record *starts* before the confirmed LSN (never confirm into an unprocessed
  commit's record). (2) A sink
  *reports* visibility (`FlushReport.visibility`: `Immediate` / `Within(d)` / `Deferred`); the
  sink engine reports `Observer::on_position_applied` **before the ack** (the source forgets a
  position once every sink confirmed it, so the fence must hear first) and
  `on_position_searchable` from an in-order queue with a timer arm. The OpenSearch sink reports
  `Within(refresh_interval)` for an unforced flush and, with `refresh_interval = "-1"`,
  `Deferred` plus a refresh of the indexes it wrote on the next caught-up flush; its
  `ensure_index` refreshes a seeded generation once per start, because what an earlier run left
  unrefreshed is remembered nowhere (fences are unavailable until staging is done). An index
  mid-reindex is **not** covered: live writes go to the new generation while the alias serves the
  old one (#6). (3) The fence
  translates a position when it is applied and keeps that answer until it is searchable, and
  keeps each level monotonic across a reopened source stream. Guards: the Postgres adapter's
  `a_split_transaction_is_not_confirmed_until_its_last_change`,
  `a_keepalive_during_a_flight_does_not_pin_the_resume_point`,
  `confirmation_stops_at_a_commit_boundary_where_a_fence_goes_further`, `reached_*`,
  `emitting_the_last_change_of_a_commit_moves_the_horizon`; the engine's
  `a_position_becomes_searchable_when_its_refresh_interval_passes`,
  `a_deferred_position_waits_for_the_next_immediate_flush`; the daemon's `fence::tests`
  (`searchable_trails_applied_until_the_engine_reports_it`,
  `a_stalled_sibling_blocks_an_all_sinks_fence_but_not_one_on_the_healthy_sink`,
  `idle_source_progress_wakes_a_waiter`); the CLI's `http::tests`; and the `fence` live e2e.
- **Built once; build order is delivery order.** A batch buffers `(Position, ChangeEvent)`s,
  resolves each to `DocumentId`s, dedups, then `build_many` assembles each touched document
  **once** for all sinks. Snapshot rows go through the same path but are published *without* a
  position and *only* to the requesting lanes, ending with `LaneItem::SnapshotComplete`; snapshot
  and live builds are serialized on the one ingest task, so on any lane a later message is the
  newer state and sinks need no versioning. Guards:
  `builds_a_repeatedly_touched_document_once_per_batch`, `snapshots_go_only_to_the_requesting_lane`.
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
  ticket; a nacked or dropped-unacked delivery is redelivered in order. (A single redelivery slot
  made the coalescing loop receive the same request forever — the OOM behind the first attempts.)
  Guards: `an_unseeded_sink_requests_a_snapshot_then_marks_seeded`, `a_seeded_sink_requests_nothing`,
  `backfill_false_makes_a_stateless_sink_live_only`,
  `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`,
  `a_backfill_with_every_lane_asking_starts_without_waiting`,
  `a_straggling_reindex_request_within_max_delay_joins_the_snapshot`, the daemon's
  `unseeded_sink_is_backfilled_without_touching_its_sibling`.
- **A live stream that ends still drains queued requests.** The ingest engine returns `Ok` only
  once the live stream ended *and* no snapshot is active *and* the request consumer is empty, so
  a sink that staged its backfill before the ingest engine started is still served.
- **…but a seed is only as good as the stream behind it (issue #120).** Run order is
  `source.continuity()` → stage every sink (`ensure_index` all, on `Fresh` `reindex` +
  `ensure_index` every still-`is_seeded` index, request backfill) → `source.prepare()` (the
  ingest engine's first act) → serve requests + live. `continuity` is **read-only** (Postgres:
  does the slot exist?); `prepare` creates the resume point. **The order is load-bearing twice**:
  rebuilds are staged *before* `prepare` creates the slot (a crash in between comes back `Fresh`
  and re-stages) and `prepare` runs *before* any snapshot (a write between snapshot and first
  live read is covered). The daemon enforces it by finishing every sink's first `stage` before
  spawning the ingest engine. Only *seeded* indexes are staged (#121). Under `--skip-backfill`
  staging warns only. Both trait methods are required. The sink-side half: the OpenSearch
  `ensure_index` retracts a seeded marker whose generation index is missing; while running,
  seeded writes go through the hash alias with `require_alias`, so a deleted generation is a
  flush-wide `Err` and the restart re-stages it (#123). Its `reindex` no-ops while unseeded and
  `mark_seeded` drops every non-active generation (#121). Guarded by the
  engine's `fresh_source_*`/`skip_backfill_with_a_fresh_source_*` unit tests, the daemon's
  `fresh_source_rebuilds_seeded_sinks_before_preparing`, the sink's
  `deleted_generation_is_recreated_and_reported_unseeded`,
  `generation_deleted_while_running_fails_the_flush_and_is_rebuilt`,
  `a_second_reindex_while_rebuilding_stages_nothing`, `mark_seeded_sweeps_an_orphan_generation` e2e, the Postgres `continuity` e2e,
  and the two `restart_*` cases in `engine`'s `pipeline` e2e.
- **Reindex is an operation on one sink, no restart.** `DaemonControl::reindex(index,
  Option<SinkName>)` sends `SinkControl::Reindex` to the targeted engines; between two batches
  each stages `reindex` + `ensure_index` and sends a fresh `Backfill` request. Untargeted sinks
  are untouched; "all" coalesces into one snapshot; a `backfill = false` sink ignores it. The
  `Envelope` a sink receives is the ingest engine's one build, shared: `Batch.envelopes` is an
  `Arc<[Envelope]>`, so publishing to every lane and the channel's redelivery ledger copy no
  documents, and nothing downstream mutates it — an emitting sink stamps its own `SinkName`
  as it writes (`sink` on the wire; the stdout sink takes its name at construction), `position` serializes as the opaque `seq` string, and `Envelope<D = GenericValue>` is
  generic over the document so a consumer deserializes an emitting sink's output as the same
  type. Guards:
  `reindex_control_stages_and_requests_a_snapshot_without_restarting`, the daemon's
  `reindex_operation_targets_one_sink`.
- **A lone change on a quiet stream is committed at once (issue #135).** `BatchPolicy`
  (default 256 changes / 50 ms; `flusso.toml`'s `[batch]` table `max_changes`/`max_delay_ms`,
  laid over the defaults by the daemon; `--batch-max-changes`/`--batch-max-delay-ms` +
  `FLUSSO_BATCH_*` override at run time, never baked into the lock) controls batch grouping:
  after the live arm buffers a change, `IngestEngine::drain_ready` buffers every change the
  stream has ready (`now_or_never`) while the batch `is_open` (not full, deadline not passed), and
  the engine commits the moment the stream would block, so a burst fills batches for at most
  `max_delay` while a trickle never waits it out. The `sleep_until` deadline arm stays. `max_changes:
  1` reproduces flush-per-change. The coalescing wait for further requests is `max_delay`. Guards:
  `a_lone_change_on_a_quiet_stream_commits_without_waiting`,
  `max_delay_caps_a_batch_while_changes_keep_arriving` (both on a channel-fed `MockSource::fed`
  whose live stream stays open and pending).
- **Item-level rejections vs flush-wide errors.** `Sink::flush` returns a `FlushReport`:
  `Err` is flush-wide (transport down, whole request refused) and stops *that sink engine*,
  which the daemon restarts with exponential backoff (1 s doubling to `max_restart_backoff`,
  default 60 s) while the ingest engine and the other sinks keep running — its lane fills, so a
  stalled sink eventually paces ingest and pins WAL (per-sink lag/in-flight are the alarm). An
  `Ok` report lists documents the destination *applied the batch but rejected* individually. The
  `FailurePolicies` (global `FailurePolicy` default + per-index overrides, from config `on_error`,
  resolved by **logical** index name) decide each in `commit`: `Stop` errors the engine out with
  the batch unacked (redelivered after restart); `Skip` quarantines it
  (`Observer::on_document_quarantined`, with the sink) and acks so the poison isn't redelivered.
  A single `Stop` halts the whole batch before any quarantine event is emitted. The OpenSearch
  sink does *not* retry item-level rejections, and reports a missing target
  (`index_not_found_exception`) as flush-wide, not as a rejection. An id OpenSearch validates before
  running `_bulk` (an upsert's past 512 UTF-8 bytes, or an empty one) would 400 the *whole* request,
  so `flush` rejects it item-level without sending it (`bulk::refused_id`; issue #301; never truncate
  or hash it); a delete's over-long id is sent (OpenSearch answers `not_found`). Guarded by
  `skip_policy_*`/`stop_policy_*`/`per_index_stop_*`, `bulk::tests`' `refused_id` cases, the sink's
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
  guards the emit points.

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
counts as touched; the sets are derived once per builder in `document/reads.rs`). Those functions
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
takes no lock — issue #225; and "does *this* table exist": `validate_indexes` walks every
relation at any depth and looks up its table, key, junction keys, `via` hops, both sides of each
`on:` pair, value/null-check filter columns, `order_by` and a join's `primary_key` on the table
each lives on — issue #202; a missing table is a `SourceError::UnknownTable` diagnostic, reported
once per relation; `Diagnostic.path` is the field path; and on the root table the root `filters`
and the `soft_delete` marker's column, `when` and `field` (issue #211), and the root key (issue #225:
the table's single-column catalog key, `RootPrimaryKey` when none or composite; the declared
`primary_key` must exist and be that key, `RootPrimaryKeyMismatch` otherwise, since the batched
build matches the table's key on the declared column and writes only tombstones — `wal`'s
`a_root_primary_key_other_than_the_tables_writes_no_document`; and of a type `Catalog::reads_key`
accepts, `UnsupportedKeyType` otherwise (issue #295: Postgres answers from the base type through
domains against `document::value::KEY_TYPES`, the types `decode_key` reads — not `json`/`jsonb`;
any other key reads back null, so every build is a tombstone — `config_coverage`'s
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
getting `Unknown` and a wording that claims nothing; a finding
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
`NotAnArrayColumn` stop;
type, numeric, nullability, ignored-primary-key, `UndeclaredArrayColumn`, `DuplicateFieldName` and `RootPrimaryKey`
findings print and start, as they built or ran in 0.17), so a new kind forces a decision there; stop
only on a state no working 0.17 deployment can be in (or on a key new since, like `array`). `DiagnosticKind` stays exhaustive for that match; `Diagnostic` and `Anchor` are
`#[non_exhaustive]` (`Diagnostic::new` builds one). `check` still fails on every `Error`. No opt-out
flag. Guards: `backends::tests` (one per kind) and `config_coverage`'s
`an_enum_or_domain_column_is_a_type_mismatch_that_still_builds`. The daemon
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
`run(shutdown)`, which runs until the live stream ends or the caller's `shutdown` future fires. A binary attaches its own metrics
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
is live) and a **private** one (`/indexes`, later `/reindex`; HTTP
Basic auth in `http/auth.rs`, default `admin`/`flusso` with a loud startup warning), both reading
the daemon's `Status`, and owns SIGINT/SIGTERM. It binds **both** listeners up front (a bad
`--public-address`/`--private-address` fails fast), then `Daemon::with_observer(otel).start()`
→ register the `in_flight` observable gauge (read from `Status` at collection) → serve both →
`run(shutdown_signal())` → drain. With no meter provider installed the global meter is a no-op
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
`document/value.rs`'s `coerce_scalar` turns back into `GenericValue::Bytes` for `sink::to_json` to
base64; `PgDocumentBuilder::connect` sets `bytea_output = 'hex'` on every pooled connection, since a
database set to `escape` would render escape text) and `CONSTANT_OBJECT_REVISION` in `Field`'s `Hash`
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
`a_database_set_to_escape_bytea_output_still_builds_the_bytes`.
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
(a list) to the decoder. `value::coerce_document` is **fallible** (`Result<_, BuildFailure>`, with
the field path): an array column's null takes the default, a lone value is wrapped, null elements are
dropped, and an object/array element of a non-`json` field is a `BuildFailure` that `build`/
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
(`libs/2-script`, layer 2: kernel + source port only; Rhai with `sync`/`decimal`/`no_time`/`no_module`)
compiles every script once (`Scripts::compile`, all errors with line/column/excerpt; the source is
wrapped as `fn __flusso(root) { … }` so `this` can be bound — columns on line 1 are shifted back) and
`ScriptedBuilder` decorates any `DocumentBuilder`: per object level, scripts run against `this` (a
fresh copy per call) and `root` (read-only, shared), all before any output is written, so no script
sees another's output; then outputs are written and `emit: false` fields stripped. A failure becomes
`Document::Failed` (see the engine invariant). Sandbox: strict variables, failing on a missing
property, no `eval`, fixed limits (`sandbox.rs`). The CLI wraps the Postgres builder in
`backends.rs`, compiles scripts beside `adapters::validate` in `build`/`check`/`run`
(`compile_scripts`), records `flusso.script.failures{index,field}` and a rate-limited `error!` log
(`telemetry/script_log.rs`, array positions folded to `[]`); the daemon's `Status` carries
`script_failures` per index. The designer compiles scripts in `/api/preview` (`script_errors`) and runs
them on the sample (`/api/sample` takes an `id`). The manual's script guide
(`docs/src/author/scripts/`) includes its recipes by mdBook anchor from
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
`FlussoMap<kind>` bound otherwise. The `FlussoMap` derive also emits `FlussoValueMeta` (its
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
(`search.rs`). The `uuid` feature makes `uuid::Uuid` a `keyword` value (no `#[flusso(skip)]`).
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
per level (an `AtomicU8` its clones share). A new read path must call `fence_before`. A fence
never falls back to a stale read: `Error::FenceTimeout`/`FenceUnavailable`/`FenceRefused`.
`position::CURRENT_SQL` is the statement; the `diesel`/`diesel-async` features add
`position::{diesel,diesel_async}::current` (backend types only, no driver). Guarded by
`tests::fence` (a local TCP stub standing in for both servers).

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
| The sync loops / batching / ack ordering | `libs/2-engine/src/` — `ingest.rs` (`IngestEngine`: batching, dedup, build once, publish, request coalescing, snapshots, watermark confirm), `sink_engine.rs` (`SinkEngine`: `stage`, recv → apply → flush → ack, failure policies, `SinkControl::Reindex`), `policy.rs` (`BatchPolicy`/`FailurePolicies`), `tests.rs` (drives both engines over a real `ChannelStream`) |
| Pipeline observability trait (`Observer`, `BuildStats`/`CommitStats`, `EngineId`, `FanOut`) | `libs/2-engine/src/observer.rs` |
| Script fields (Rhai): compile, run, strip input-only fields, the `DocumentBuilder` decorator, the sandbox, value conversion | `libs/2-script/src/` — `scripts.rs` (`Scripts`: plan + apply), `builder.rs` (`ScriptedBuilder`), `sandbox.rs`, `value.rs`; `benches/scripted.rs`; CLI `commands/script.rs` (`flusso script run`), `telemetry/script_log.rs` |
| Daemon (domain): supervisor, `DaemonControl`, per-sink `Status`, `StatusObserver`, lag poll, the fence | `libs/3-daemon/src/` — `lib.rs` (`Daemon`/`RunningDaemon`/`DaemonOptions`), `supervise.rs` (stage → spawn → restart with backoff; `DaemonControl::reindex`), `backends.rs` (`Backends` trait + `SourceParts`/`SinkParts` seam), `observer.rs`, `status.rs`, `lag.rs`, `fence.rs` (`Fence`/`FenceRequest`/`FenceOutcome`/`FenceObserver`) |
| Read-your-writes (fences), end to end | source: `libs/1-ports/source/src/cdc/capture.rs` (`SourcePosition`, `reached`, `on_advance`), `libs/2-adapters/source-postgres/src/cdc/ack.rs` (`Positions`: entries + horizon) and `stream.rs` (who moves the horizon); sink: `libs/1-ports/sink/src/sink.rs` (`Visibility`), `libs/2-adapters/sink-opensearch/src/sink_impl.rs` (`settle_visibility`); engine: `libs/2-engine/src/sink_engine.rs` (`report_positions`, the searchable queue); daemon: `libs/3-daemon/src/fence.rs`; CLI: `apps/cli/src/http/mod.rs` (`wait`); client: `sdk/query/src/fence.rs`, `position.rs`; e2e: `libs/3-daemon/tests/fence.rs` |
| Adapter registry (the one place that names adapters): `type` → config struct, `validate`, descriptions, flag overrides | `apps/cli/src/adapters.rs` |
| Adapter assembly (build the running source/sinks): the `Backends` impl | `apps/cli/src/backends.rs` (`FlussoBackends` — Postgres source + OpenSearch/stdout sinks) |
| Transport + telemetry (binary): exporters, metrics recording, HTTP surfaces, auth, signals | `apps/cli/src/` — `telemetry/mod.rs` (traces), `telemetry/metrics.rs` (meter provider + `in_flight` gauge), `telemetry/observer.rs` (`OtelObserver`), `http/mod.rs` (public + private routers + `serve`), `http/auth.rs` (Basic auth), `commands/run.rs` (orchestration + signals) |
| Config loading + the assembled `Config`/`Index` with `PortEntry` ports (layer 1) | `libs/1-config/src/` — `lib.rs` (`load`), `loader.rs`, `compiled.rs` (`flusso.lock`), `deployment/` (the `Config` family + `From<ConfigToml>` conversion + `resolve_mappings`) |
| Kernel vocabulary (the shared types — layer 0) | `libs/0-kernel/src/` — `config/` (`IndexSchema`, `FailurePolicy`, `Secret`, …), `common/` (newtypes), `GenericValue`, `envelope.rs` (`Envelope`/`Op`/`Position` — the stream message), `options.rs` (`Options`/`OptionValue`), `port_entry.rs`, `adapter.rs` (`AdapterConfig`/`AdapterDescription`/`Port`/`override_var`); the derive in `libs/0-kernel/derive/` |
| Adapter config types (one per adapter, `#[derive(AdapterConfig)]`) | `libs/2-adapters/source-postgres/src/config.rs` (`PostgresConfig`, `Connection`, `SslMode`, `Tls`), `stream-channel/src/config.rs`, `sink-opensearch/src/config.rs` (+ `TextAnalysis`, the per-index `OpensearchIndexConfig`; `settings.rs` holds `IndexSettings`: normalization, owned-key rejection, layering), `sink-stdout/src/config.rs` |
| `flusso.toml` parsing (entities only; conversion is beside `Config`) | `libs/1-config/src/toml/` (`entities/`) |
| `*.schema.yml` parsing / field syntax | `libs/1-config/src/yaml/entities/field.rs`, `conversion.rs` |
| Postgres WAL capture / backfill / doc building / publication management | `libs/2-adapters/source-postgres/src/` — `cdc/` (incl. `publication.rs`, `replica_identity.rs`), `document/` (`resolve.rs`: reverse resolution + the first-hop pre-image union) |
| Source trait abstractions (`ChangeCapture` + `Continuity`, `ChangeEvent` + `RowImage` pre-image, `DocumentBuilder`, `SourceSpec` + `all_tables`/`pre_image_columns`/`pre_images`, `validate_indexes`, `CaptureProvisioning`/`CoverageReport`/`PreImageReport`, `SchemaIntrospection`/`RelationalCatalog`) | `libs/1-ports/source/src/` (`provisioning.rs` for coverage; `introspection.rs` for catalog enumeration + `junction_candidates`) |
| Visual schema designer (web app: introspect → edit → preview → write files) | `apps/design/` (`flusso-design`) — `server.rs` (axum + JSON API: project/catalog/test-connection/**parse**/**expression**/**value**/preview/validate/**sample**/diff/save; `value.rs` reads a typed inspector value as its saved file will in its slot (`constant`/`default`/`option`; a `default` takes only a scalar), refusing a number it cannot hold exactly or nesting past `MAX_DEPTH` (32, so preview/diff/save stay under serde_json's 128 levels), and returns the file's spelling for the *Saved as* note; the frontend's `model/exact.ts` keeps integers past 2^53 exact in every parse/clone, and `model/pending.ts` holds values still being read, one per target, flushed before Save/Code/undo (Save waits a bounded time and refuses, naming the field, on a read that failed or got no answer) and dropped on Reset; a reply is applied through the setter of the render it was typed in, which addresses the field by `uid` (`edit.updateField`), and the inspector is keyed by that uid), `codegen.rs` (model → canonical `*.schema.yml`/`flusso.toml`: a new file's form), `api.rs`'s `plan` (every save check, before any write: which on-disk index each saved index is comes from `SaveRequest.identity`, else by name, never from the request's word; a file the review leaves out is not checked; after any save every index of the on-disk `flusso.toml` has a file of its own with its own schema), the **stale-page check** (issue #280: `load_project` returns `Project.digests`, SHA-256 per project-relative file the model came from (`../x` outside the folder), `null` = absent; the page sends them as `SaveRequest.digests` — **required**, and `SaveRequest` denies unknown fields: `read_save_request` answers a page of another designer version with a 400 `BadRequest { reload }` — and merges `SaveResponse.digests` after its own save; `flusso.toml` is compared **first** (`stale_file`), before `plan` reads the disk: the review is that entry alone (`stale_config`), the save a `Stale` 409; then `plan` marks `OpDiff.stale` per file (a file the page never loaded counts as expected absent) and `save_with` refuses, under the lock, an applied entry that is marked; the expected digests go into `commit` as `Expect::Holds` (`Write.expect`/`Remove.expect`): `Journal::plan` takes them as the journal's `before`, and `run` re-checks each file (`Disk::holds`, not a counted step; `Disk::editing_at_check` is the test hook) right before the rename that replaces or removes it, so a lock-free writer meanwhile stops the commit with `Changed`, `undo_reached` undoes the renames done (keeping any file written since, `Disk::with_edit` is the test hook), and the API answers the same 409 naming every changed file — only a write between that check and the rename can still be lost; "overwrite anyway" sends the review's `StaleFile.on_disk`; completed recoveries are kept per process (`COMPLETED`) and reported on a diff/save whose digests show the page hasn't seen them (`unseen_recoveries`, compared before the save writes), and the page shows each `RecoveredSave.id` once; "overwrite anyway" recomputes the review with the override (`api.diff`'s `overwrite`), so Write shows every file it writes; guards: `tests/api.rs`'s `a_page_cannot_save_over_a_file_another_page_saved` and siblings, `a_config_another_page_restructured_is_the_refusal_not_a_failed_check`, `a_request_from_another_designer_version_is_told_to_reload`, `api::tests`' `a_file_written_*` and `an_editor_writing_during_a_save_gets_a_save_or_the_refusal`, and `commit::tests`' `an_editor_write_after_any_step_is_kept_and_nothing_is_left`), `commit/` (the all-or-nothing commit, kill- and power-loss-safe, issue #274; invariant in `mod.rs`'s doc: the project is, or after recovery becomes, exactly before or after a save: under `lock.rs`'s advisory lock (`.flusso-save.lock`, std `File::try_lock`, held for the whole commit and every recovery; never followed: a link or non-file there is `LockError::NotAFile`), `journal.rs` writes `.flusso-save.journal` (format 2 JSON: every destination and removal **relative to the config's folder**, with SHA-256 of each file before and after; temp/backup names are derived from the path, a token and the position, never stored). Order: a **record** in the user's state folder (`state.rs`: `<token>.json` = journal bytes' digest + canonical config folder + `committed`, under `$XDG_STATE_HOME/flusso/design-saves/` via `etcetera`, 0700/0600, `create_new` through a hard-linked temp, never followed, non-blocking reads, a folder or record open to others or another owner's is not used; `State::for_saving` falls back to an unrecorded save, logged once; pruned at start past 180 days, or with its folder missing once past 7 days (`GONE_AGE`); a temp a killed private write left goes once its pid is dead (an hour where that can't be checked); a unit test sees only its thread's `state::use_test_dir`, the integration tests call `api::use_state_dir`), then the journal, flushed temp files, hard-link backups, renames with `flusso.toml` last, the **commit point = marking the record committed** (one rename, then the state folder flushed), rename to `.flusso-save.done`, cleanup, record removed last; journal/staging/temps are written and read with `O_NOFOLLOW` (`disk.rs`'s `no_follow`). **A journal is acted on only when a record of this folder names its exact bytes**, so once, in its own folder: `recover.rs` decides roll back vs finish from the record, never the file name; a file holding a **third text** (neither its before- nor after-text, or deleted) is another program's write and is kept, never blocks (`recover::Kept`, `RecoveredSave.kept`): `finish` cleans up around it, a rollback restores the rest (`Backups::Keep` in recovery keeps and names its before-text backup, `Backups::Drop` in the save's own undo drops it, a backup holding the kept bytes always goes), so the result is before or after plus that write, never a mix; only an unreadable file, a missing backup or a failed step blocks; `Journal::steps` then accepts only what a save writes (plain names, leading `..` for a `plan`-accepted outside file, no save-file name, no link, config only as the last write, removals inside, `dirs` ancestors of a destination, ≤ `MAX_ENTRIES` paths of ≤ `fspath::MAX_PATH` bytes); anything else (no record, another folder's, no state folder, too large, malformed, not a save, `.journal` + `.done` both) is never acted on: `recover.rs` blocks with a read-only `Report` that looks only inside the config folder (`Inside`; an outside path is text, `Holds::Outside`) and gives `Report::route` back/forward as literal `HandStep`s (each file's `Fix`, every implied temp/backup on disk whatever it holds, created folders if empty, the journal last) plus "delete only the journal"; a blocked *recorded* save carries its `Direction`, per `Problem` a `fix` from a copy verified by digest at report time (never a backup while finishing: the after-text is nowhere else) and, when every problem is a missing backup, `keep` steps; `commit::tests` follow every such step literally and assert the promised state. Every recovery action is digest-gated (an empty temp is deleted only while rolling back and its file unreplaced; a `dirs` folder only when empty); `fspath::on_disk` (shared with `plan`, which caps request paths at `MAX_PATH`) bisects for the deepest existing ancestor, so resolution is O(log n) canonicalizations; the backups are deleted only once the commit mark is flushed; `api.rs`'s `recover_project` runs it at `flusso design` start, on `/api/project` (handlers run it under `spawn_blocking`), and before a diff/save that finds a journal: a rollback lets the diff/save go on with the notice, a completion is reported except the process's own save whose cleanup it left (`OWN_CLEANUP`), and the pages that loaded before it are caught by the digest check below; a blocked one is a 409 too (carrying `SaveRecovery`, with `BlockedSave.report`; the frontend's `components/Recovery.tsx` shows the notice or, when blocked, the files and the report instead of the project); stray `*.flusso-save.*` files with no journal are only reported; `disk.rs`'s `Disk` counts steps so `commit::tests` fails, or stops dead after, each one, for the commit and for recovery itself, and `a_process_aborted_mid_save_is_recovered` re-runs the test binary as a child that aborts mid-save), `keep/` (a save, `/api/diff` and the preview's YAML over an existing file: its text is patched only where the canonical renderings of its model and the saved model differ, so comments and untouched formatting stay — `yaml.rs` a line-based block scanner: the comment block directly above a key/item is its own, a blank-line-separated one is nobody's and stays put, a list's trailing comments are the list's; `toml_doc.rs` over `toml_edit`, the same rules for headers via markers in their prefixes; list items, `[[index]]` entries and sinks pair by identity — `pairing.rs`, from the uids/index ids/sink ids the client sends as `FileOp.identity`/`SaveRequest.identity`/`PreviewRequest.identity` (before = the model the base reads as, after = the saved model; a sink rename is applied to the TOML in place, `[index.sinks]` included, before the patch; an identity with an empty, repeated or unknown id is ignored), else by name, never by slot; a `|+` scalar owns the blank lines after it, so the patcher drops blank lines it would put after one (`KEPT_OPEN`); a comment dropped while its owner may still exist counts in `OpDiff.dropped`; the result is re-parsed and must equal the model, else the canonical form with `OpDiff.notice`; the Code-mode text is the save `base` — `FileOp.base`, the design store's `texts`, under undo with `doc`), `preview.rs` (mapping + document tree), `assets.rs` (embedded SPA); CLI `design` subcommand in `apps/cli/src/commands/design.rs`; frontend under `apps/design/frontend/` (React Flow node-graph canvas — `model/` projects the `IndexSchema` tree ↔ nodes/edges + path-addressed edits (every field carries a client-only `uid` from `ids.ts`: a container's is its node id, which collapse state + drag positions key on, and inspector edits address a field by it; stripped from every request), plus `complete.ts` (incomplete-field checks) and `prune.ts` (drops incomplete pieces from the **live preview** payload only, so a mid-build blank name doesn't 400 the strict backend, and lists them; that copy is lossy, so Code mode seeds from the unpruned schema and refuses to open while anything would be pruned), `selection.ts` (the selection carries the selected field's uid; the store re-points its path after every doc change and clears it once the field is gone), `rowIds.ts` (option rows keyed by a row id, not their editable key), `components/` the canvas/nodes/inspector/catalog-browser), built to `apps/design/dist/`; property round-trip in `apps/design/tests/roundtrip.rs` (also: random edits saved into a hand-written, commented file keep every comment with the field it names). The **sample document** preview builds a real doc from one live row via `PgDocumentBuilder::sample_document` (postgres crate — keeps sqlx/`RowKey` there; reuses the `build` path + `sink::to_json`) |
| `Sink` trait (`apply`/`flush`/seeding/`reindex`), `SinkOptions`, JSON render | `libs/1-ports/sink/src/` |
| OpenSearch sink (bulk, mappings, seeding; alias-over-generations + reindex) | `libs/2-adapters/sink-opensearch/src/` — `lib.rs` (the `OpensearchSink` type + ctor), `sink_impl.rs` (the `Sink` impl), `transport.rs` (HTTP plumbing + index CRUD), `generations.rs` (aliases, meta doc, generation naming), `mapping.rs` (index body/analysis), `bulk.rs` (wire format + chunking) |
| Stream port (`Stream`, lanes, `LaneItem`/`Request`, `Producer`/`Consumer`/`AckHandle`) / in-process channel adapter (ticketed in-flight ledger, watermark) | `libs/1-ports/stream/src/` (`items.rs`, `queue.rs`), `libs/2-adapters/stream-channel/src/lib.rs` |
| Editor schema + Reference table generation (`flusso schema config\|docs`, drift test) | `apps/cli/src/commands/schema_cmd.rs`; artifacts at `libs/1-config/config.schema.json`, `docs/src/reference/generated/` |
| CLI subcommands (`build`/`run`/`check`/`schema`/`indexes`/`reindex`) | `apps/cli/src/` — `main.rs` dispatches; `commands/` holds one module per command (`build.rs`, `run.rs` → composition root: installs telemetry, serves the HTTP surfaces, drives `Daemon::start`/`run`, owns signals; `check.rs`, `schema_cmd.rs`, the `indexes`/`reindex` HTTP-client `admin.rs`, shared `print.rs`); `telemetry/` and `http/` hold the transport, `backends.rs` the backend assembly |
| On-demand reindex (alias-over-generations, per sink, no restart) | sink: `libs/2-adapters/sink-opensearch/src/sink_impl.rs` (`reindex`/`ensure_index`/`mark_seeded`) + `generations.rs` (generation helpers); engine `SinkEngine::reindex` (staged between batches + a `Backfill` request); daemon `DaemonControl::reindex` (`supervise.rs`); CLI `http/mod.rs` (`POST /reindex?index&sink`), `commands/admin.rs` (client, `--sink`). Deferred write-side zero-downtime follow-on: issue #6 |
| Query client (`flusso-query`) | `sdk/query/src/` (`cursor.rs`: page cursors; `walk.rs`: streaming a walk; `fence.rs`: fenced reads; `position.rs`: reading the source position) |
| `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]` proc-macros | `sdk/query-derive/src/` — `lib.rs` (entry points + `Attrs`), `doc.rs` (field parsing/validation + the recursive handle tree + `embed_checks`), `fragment.rs` (the location-free shape check), `spec.rs` (baking a level into `&[FieldSpec]`), `resolve.rs` (finding `flusso.toml`); the const-check vocabulary is `sdk/query/src/check.rs`. Plus the `flusso-query-derive` memory note |
| Runnable example (stack, seed, consumer) | `dev/` (`flusso.toml`, `postgres/init/`, `search-api/`) |
| Benchmarks: scenario harness, in-process + Docker benches, CI gate + history | `dev/bench/` (`flusso-bench`: `src/main.rs` phases, `scenario.rs` mixes, `scale.rs` presets, `flusso.rs` child process, `scenarios/`), `libs/2-engine/benches/{engine,pipeline}.rs`, `libs/2-adapters/source-postgres/benches/{pgoutput,postgres}.rs` + `tests/record_pgoutput.rs`, `libs/2-adapters/sink-opensearch/benches/{render,opensearch}.rs`, `.github/workflows/bench.yml`, `.github/scripts/bench-*.{py,sh}`, `docs/src/contribute/benchmarks.md`, ADR 0006 |
| Registry image / containerized demo | `Dockerfile` (`runtime` target = config-less registry image; `demo` target = + baked dev lock), `docker-compose.demo.yml` (override adding the `flusso` service, built from the `demo` target), `.dockerignore`; user-facing shipping recipes in `docs/src/deploy/docker.md` |
| Kubernetes deploy (Helm chart) | `deploy/helm/flusso/` — `Chart.yaml`, `values.yaml`, `templates/`, `README.md` |
| Agent-facing docs (the Claude plugin + internal commands) | `plugin/` — `ARCHITECTURE.md` is the contract (one corpus/three consumers, who owns which meaning, the self-containment rule), `skills/*/SKILL.md` the knowledge corpus (`flusso-query` discloses `migration.md`/`options.md`/`maps.md`), `commands/` thin workflow entries, `agents/flusso-expert.md`, `hooks/`; `.claude/commands/{implement,new-issue}.md` the internal spine. Guarded by `apps/cli/tests/agent_docs_paths.rs` |
| Domain glossary + architecture decisions | `CONTEXT.md` (the vocabulary: kernel / ports / adapters / engine / daemon, stream / lane / envelope / position / source position / fence, operation vs primitive vs transport) and `docs/adr/` (0001 adapter-owned config, 0004 the rename, 0005 lock format 3, 0006 benchmarks, 0007 reverse resolution reads the pre-image, 0008 page cursors are keyset with an opt-in point-in-time, 0009 a fence waits on the source's own position, per sink, at two observed levels: accepted; 0002/0003 the engine split: `proposed` until phase 2 of #130 lands) |

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
  own representation at its boundary (`sink::to_json` is the OpenSearch JSON translation,
  where e.g. `bytea`→base64 lives) — core picks no wire format.
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
  (`flusso-dev-search-api`), `dev/query-e2e` (`flusso-query-e2e`), and `dev/bench` (`flusso-bench`) —
  all `publish = false` (a runnable example, a live-e2e guard, and the benchmark harness, not shipping code). The
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
- **Agent-facing docs follow `writing-for-agents`, not `docs/STYLE.md`.** `plugin/**` and
  `.claude/commands/**` are consumed by an agent, so `plugin/ARCHITECTURE.md` is the standard: each
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
