# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

flusso keeps OpenSearch in sync with Postgres from declarative config. You describe a
search document in YAML (`*.schema.yml`); flusso derives the index mapping, seeds it,
then follows Postgres logical replication so the index stays current. Read `README.md`
for the full picture. The user manual is an mdBook under `docs/`, published to GitHub Pages at the site root
(`alias2k.github.io/flusso/`; the versioned editor schemas sit beside it under `/schemas/`).
`docs/src/SUMMARY.md` is the page tree: seven parts (Start here, Author, Deploy, Operate,
Query, Reference, Contribute) under one rule, **one fact, one home**. Every `*.schema.yml`
key, `flusso.toml` key, adapter option, env var, CLI flag, Helm value, metric, and HTTP
endpoint is documented once, on a page under `docs/src/reference/`, and the how-to parts link
there instead of restating it. Adapter option tables are **generated** (`just schema-gen` →
`docs/src/reference/generated/`) and included by the adapter pages; edit the doc comment on the
adapter's config struct, never the table. Docker recipes are `docs/src/deploy/docker.md`, the query
manual is the twelve chapters under `docs/src/query/`, the designer is
`docs/src/author/design-visually.md` + `designer-reference.md`, and the human-facing
architecture tour is `docs/src/contribute/` (this file stays the agent-facing index). Page
types, templates, and the ownership rule are in `docs/STYLE.md`. Every crate has its own
`README.md` — its crates.io/docs.rs landing, wired via `#![doc = include_str!("../README.md")]`
(the `sdk/query` README is a landing too, kept separate from its `//!`) — and `libs/README.md`
maps the crate layering. When you change a doc's content, update the Reference page or README
that owns it; when you move a page, add an `[output.html.redirect]` entry in `docs/book.toml`.
CI's `docs` job builds the book, runs `lychee --offline --include-fragments` on it, and asserts
every `alias2k.github.io/flusso/<path>` URL in the repo's markdown and Rust resolves to a built
page (`.github/scripts/check-manual-links.sh`).

## Commands

Common workflows are wrapped in a `justfile` — run `just` to list them (e.g. `just up`,
`just run`, `just check`, `just test`, `just lint`, `just bench`, `just status`). It needs
[`just`](https://just.systems) (`cargo install just --locked`). The raw commands it wraps
are below.

Tests run with [`cargo-nextest`](https://nexte.st) (`cargo install cargo-nextest --locked`).

```sh
cargo nextest run                          # fast tests: unit + parse/convert, no deps
cargo nextest run --run-ignored all        # + Postgres e2e (needs a running Docker daemon)
cargo nextest run -E 'test(name_substr)'   # a single test by name
cargo test --doc                           # doctests — nextest does NOT run these
cargo clippy --workspace                   # lint (NOT --all-targets; see below)
cargo bench                                # Criterion benches (engine, opensearch, postgres)
cargo +nightly fuzz run pgoutput_decode    # fuzz the WAL decoder (from libs/2-adapters/source-postgres)
```

- **Fuzzing** needs nightly + `cargo install cargo-fuzz --locked`. The one target,
  `pgoutput_decode`, lives in `libs/2-adapters/source-postgres/fuzz/` (its own workspace, kept out
  of the strict `[workspace.lints]`) and drives the otherwise crate-private pgoutput decoder
  via the `source-postgres` `fuzzing` feature (`fuzz_pgoutput_decode`). Contract: the
  decoder must never panic on arbitrary bytes — an `Err` is the correct outcome. Run from the
  crate dir; a crash lands in `fuzz/artifacts/`.
- The `#[ignore]`d e2e tests live in `source-postgres`'s `integration`,
  `config_coverage`, `publication`, `introspection`, `tls`, `wal_idle`, and `continuity` binaries
  (`tls` boots a hostssl-only PG 16 with a committed throwaway self-signed cert and
  proves the replication stream + SQL pool honor `sslmode`; `wal_idle`: the slot keeps
  advancing from keepalives/filtered commits while watched tables are idle, so unrelated
  writes don't pin WAL retention — issue #111; `continuity`: `continuity()` is read-only and
  reports `Fresh` exactly while the slot is missing, `prepare()` creates it — issue #120) plus
  `engine`'s `wal` and `pipeline`
  binaries (the full
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
  rows gone, old generation dropped). New e2e binaries must be added to the filtersets in
  `.config/nextest.toml` (both profiles) to get the docker/opensearch group caps and retries.
  It is the only test that catches a live change rebuilt as the *wrong* op (e.g. an update
  written as a tombstone because the WAL key decoder and the read-back decoder disagree on
  the `GenericValue` variant; see `cdc/pgoutput.rs::typed_value` vs
  `document/value.rs::decode_column`). `.config/nextest.toml` caps their concurrency and
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
  `cargo check --workspace --all-targets` (compiles benches + examples, which clippy and nextest
  skip — clippy omits `--all-targets`, nextest only builds test targets) → `cargo check -p
  flusso-cli --no-default-features` (the server-only image build) → `cargo nextest run
  --profile ci --run-ignored all` → `cargo test --doc` → `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --document-private-items` (broken/ambiguous/redundant intra-doc links fail
  the build). Match these before assuming green. One scope note: on PRs confined to the query
  release train (`sdk/query`, `sdk/query-derive`, `dev/query-e2e`) the nextest step swaps in a
  filterset that skips the Postgres/OpenSearch container suites those paths can't break (the
  query train's own `combined_search` live e2e still runs); pushes to main always run everything.
  A `docs` job builds the mdBook (`mdbook build docs`), link-checks it (`lychee --offline
  --include-fragments --root-dir docs/book 'docs/book/**/*.html'`), and runs
  `.github/scripts/check-manual-links.sh`. A separate `fuzz` job runs a 60-second
  `pgoutput_decode` smoke fuzz on nightly (see below); the `query.rs` proptests need no special
  handling — they're ordinary tests caught by the nextest step.
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
  its translations), then `npm run build` + a `git diff` guard that the committed
  `apps/design/dist/` matches a fresh Vite build (the embedded SPA must not drift). There is **no**
  browser e2e — the UI is validated by dogfooding; the `designer-frontend` build/lint guards plus the
  Rust round-trip are the automated net.
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
editor-assist JSON Schema: `schema config` or `schema index`; no DB). See `dev/README.md` for the walk-through. **Every flag also reads a
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
`2-{adapters/*, engine}` → `3-daemon` → `apps`, `sdk`). The five seams are named in `CONTEXT.md`:
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
  advances past a change exactly when *every* sink has landed it. A sink engine that dies before
  its flush leaves the item unacked; the lane redelivers it to the restarted engine (idempotent
  rebuild, deterministic id). Guards: `confirms_no_position_before_its_flush`,
  `redelivers_the_unacked_batch_to_a_restarted_sink_engine`, the channel adapter's
  `watermark_is_the_minimum_over_lanes`/`several_deliveries_can_be_outstanding_and_ack_independently`,
  the daemon's `failed_sink_engine_restarts_and_redelivers`.
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
  engine coalesces concurrent requests for the same index into one snapshot fanned to every
  requester and acks a request only after its `SnapshotComplete` is published (a crash
  mid-snapshot redelivers it). A seeded sibling sees nothing. The channel adapter therefore keeps
  a **ticketed in-flight ledger** per lane: several deliveries can be outstanding, each acked by
  ticket; a nacked or dropped-unacked delivery is redelivered in order. (A single redelivery slot
  made the coalescing loop receive the same request forever — the OOM behind the first attempts.)
  Guards: `an_unseeded_sink_requests_a_snapshot_then_marks_seeded`, `a_seeded_sink_requests_nothing`,
  `backfill_false_makes_a_stateless_sink_live_only`,
  `concurrent_requests_for_the_same_index_coalesce_into_one_snapshot`, the daemon's
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
  `ensure_index` retracts a seeded marker whose generation index is missing. Guarded by the
  engine's `fresh_source_*`/`skip_backfill_with_a_fresh_source_*` unit tests, the daemon's
  `fresh_source_rebuilds_seeded_sinks_before_preparing`, the sink's
  `deleted_generation_is_recreated_and_reported_unseeded` e2e, the Postgres `continuity` e2e,
  and the two `restart_*` cases in `engine`'s `pipeline` e2e.
- **Reindex is an operation on one sink, no restart.** `DaemonControl::reindex(index,
  Option<SinkName>)` sends `SinkControl::Reindex` to the targeted engines; between two batches
  each stages `reindex` + `ensure_index` and sends a fresh `Backfill` request. Untargeted sinks
  are untouched; "all" coalesces into one snapshot. Guards:
  `reindex_control_stages_and_requests_a_snapshot_without_restarting`, the daemon's
  `reindex_operation_targets_one_sink`.
- `BatchPolicy` (default 256 changes / 50ms) controls batch grouping; `max_changes: 1`
  reproduces flush-per-change. The coalescing wait for further requests is `max_delay`.
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
  sink does *not* retry item-level rejections. Guarded by `skip_policy_*`/`stop_policy_*`/
  `per_index_stop_*`.
- **Observability is a trait, not baked in.** Both engines report to an `Observer`
  (`libs/2-engine/src/observer.rs`) — sync, cheap, no-op by default, set via `with_observer`;
  every sink-side event carries the `SinkName`, engine errors an `EngineId`. It depends only on
  the trait, never on metrics or a status backend. `reports_lifecycle_and_progress_to_the_observer`
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
WalChangeCapture`): `run` auto-creates/extends it on `live` (after `ensure_slot`) when the role
is privileged enough and `manage_publication` isn't opted out, else warns with the exact SQL;
`check` inspects read-only and prints the same. The trait/report never name "publication", so
the daemon/CLI/printer stay backend-neutral. A third source-neutral capability is
`SchemaIntrospection` (`libs/1-ports/source/src/introspection.rs`): where `Catalog` answers
"the type of *this* column" and `CaptureProvisioning` answers "is this table set coverable",
`introspect` *enumerates* the whole relational catalog (`RelationalCatalog` — every table's
columns/types/PK/FKs, each with a suggested `FlussoType`) so discovery-driven tooling can pick
from what's really there; `junction_candidates` (a free function, not a trait method) flags m2m
junctions. Postgres backs it over `pg_catalog`/`information_schema`; the visual designer
(`apps/design`) is its first consumer. The daemon
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
`/status` `/metrics`, unauthenticated) and a **private** one (`/indexes`, later `/reindex`; HTTP
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
   "fully validated".

**Adapter knobs that are flags** (`--slot`, `--publication`, `--manage-publication`, `--pretty`,
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
secret paths (found by the `x-flusso-secret` marker `Secret`'s `JsonSchema` plants). The CLI
renders the complete `flusso.toml` editor schema (`flusso schema config`), the Reference option
tables (`flusso schema docs`), and hands the descriptions to the designer (`GET /api/adapters`),
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
flags it). A `map:` field (`values:` = the shared leaf type) is a dynamic-key object over a
`json`/`jsonb` column — `FlussoType::Map { values }` → OS `object` with `dynamic: true`
injected into options (so runtime keys stay searchable); the resolved `Mapping.map_values`
carries the value kind, which is the only thing distinguishing a `map` from a plain
`object`/`json`. `values` must be a leaf kind (text/keyword/number/date); the conversion
rejects others. An `enum:` field takes an optional `variants:` list (its own
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
`#[flusso(opaque)]` opts a plain struct out. `#[serde(flatten)]` and
`#[serde(transparent)]` newtypes are supported (both checked against the enclosing level).

Dynamic-key `map` fields (issue #28) get typed handles too: `handles/map.rs` emits one
handle per value kind — `TextMap`/`KeywordMap`/`NumberMap`/`DateMap` — where
`.key(runtime_str)` returns a fully-typed leaf handle (`Text`/`Keyword`/`Number`/`Date`)
of the declared kind: runtime keys, compile-time value type. `TextMap::search(q)` builds a
`MapSearch` (a `best_fields` `multi_match` over `prefer`'d keys plus a `path.*` fallback) for
cross-key search with per-key preference; `has_key`/`exists` are presence checks. The
doc-side type is `HashMap<String, V>` or `BTreeMap<String, V>` (blanket `FlussoMap<K>` impls
for any `V: FlussoValue<K>`), or a `#[derive(FlussoMap)]` struct of **any shape** — newtype or
named fields (e.g. a translations type with a `fallback` beside the language keys) with a
**required** `#[flusso(keyword|text)]` value-kind tag (no default — untagged is a compile
error); the
derive's `check_type` map arm hard-checks a `HashMap`/`BTreeMap` value type and defers a
`FlussoMap<kind>` bound otherwise. The `FlussoMap` derive also emits `FlussoValueMeta` (its
`MAP_VALUES` carries the declared value kind) + a no-op `__flusso_check`, which is what lets
the type sit inside a `FlussoFragment`; a hand-written `impl FlussoMap<K>` works only at a
root — in a fragment it fails to compile, with the E0277 note pointing at the derive. Both
sides also enforce the value kind symmetrically (fragment `map_kind_ok` assert; the root's
`embed_checks` mirrors it, so a map wrapper on a plain object/nested is rejected everywhere),
and an opaque `json` field (object, no children) accepts any map-shaped type on both sides.
`handle_fn` dispatches on `Mapping.map_values` (`Text`→`TextMap`, `Keyword`→`KeywordMap`,
`Date`→`DateMap`, the numerics→`NumberMap`). Phase 2 (`dynamic_templates` per-key
analyzers for per-language stemming) is deferred.

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
first one a doc has — true fallback, not lexicographic tiers. String maps sort case-insensitively
(`toLowerCase`) on the dynamic `.keyword` subfield; numeric/date maps on the bare key (epoch millis
for dates). Nesting-aware via the scope path, like a field sort. `missing_first`/`missing_last`/`missing(v)`
(on the produced `Sort`, or via an `OrderBy` through `by`) **redirect into the script's
`params.missing`** with a direction-correct sentinel (`Sort` carries a `script_kind` flag + a `dedup_id`
distinct from the render key, so several `_script` map sorts dedup by field path and coexist).
**The single-key footgun is closed:** `TextMap::key`/`KeywordMap::key` return `MapKey`-marked leaves
(`string.rs`) that are **not** `Sortable` — a plain `.asc()` used to compile and 400 on the
nonexistent `name.it.keyword_lowercase` (the `Keyword` `Sortable` impl was split into
`WithSubfields`+`NoSubfields` to exclude `MapKey`). Number/date map keys stay directly sortable
(their bare path is doc-valued and runtime-correct). Guarded by the `*_map_sort_key_*`/`map_sort_*`
tests and the `map_key_not_sortable` UI test.

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
adapters' `#[derive(AdapterConfig)]` descriptions (`AdapterForm.tsx`), so a new option or a new
adapter needs no designer edit beyond `just schema-gen`.
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
| Daemon (domain): supervisor, `DaemonControl`, per-sink `Status`, `StatusObserver`, lag poll | `libs/3-daemon/src/` — `lib.rs` (`Daemon`/`RunningDaemon`/`DaemonOptions`), `supervise.rs` (stage → spawn → restart with backoff; `DaemonControl::reindex`), `backends.rs` (`Backends` trait + `SourceParts`/`SinkParts` seam), `observer.rs`, `status.rs`, `lag.rs` |
| Adapter registry (the one place that names adapters): `type` → config struct, `validate`, descriptions, flag overrides | `apps/cli/src/adapters.rs` |
| Adapter assembly (build the running source/sinks): the `Backends` impl | `apps/cli/src/backends.rs` (`FlussoBackends` — Postgres source + OpenSearch/stdout sinks) |
| Transport + telemetry (binary): exporters, metrics recording, HTTP surfaces, auth, signals | `apps/cli/src/` — `telemetry/mod.rs` (traces), `telemetry/metrics.rs` (meter provider + `in_flight` gauge), `telemetry/observer.rs` (`OtelObserver`), `http/mod.rs` (public + private routers + `serve`), `http/auth.rs` (Basic auth), `commands/run.rs` (orchestration + signals) |
| Config loading + the assembled `Config`/`Index` with `PortEntry` ports (layer 1) | `libs/1-config/src/` — `lib.rs` (`load`), `loader.rs`, `compiled.rs` (`flusso.lock`), `deployment/` (the `Config` family + `From<ConfigToml>` conversion + `resolve_mappings`) |
| Kernel vocabulary (the shared types — layer 0) | `libs/0-kernel/src/` — `config/` (`IndexSchema`, `FailurePolicy`, `Secret`, …), `common/` (newtypes), `GenericValue`, `envelope.rs` (`Envelope`/`Op`/`Position` — the stream message), `options.rs` (`Options`/`OptionValue`), `port_entry.rs`, `adapter.rs` (`AdapterConfig`/`AdapterDescription`/`Port`/`override_var`); the derive in `libs/0-kernel/derive/` |
| Adapter config types (one per adapter, `#[derive(AdapterConfig)]`) | `libs/2-adapters/source-postgres/src/config.rs` (`PostgresConfig`, `Connection`, `SslMode`, `Tls`), `stream-channel/src/config.rs`, `sink-opensearch/src/config.rs` (+ `TextAnalysis`), `sink-stdout/src/config.rs` |
| `flusso.toml` parsing (entities only; conversion is beside `Config`) | `libs/1-config/src/toml/` (`entities/`) |
| `*.schema.yml` parsing / field syntax | `libs/1-config/src/yaml/entities/field.rs`, `conversion.rs` |
| Postgres WAL capture / backfill / doc building / publication management | `libs/2-adapters/source-postgres/src/` — `cdc/` (incl. `publication.rs`), `document/` |
| Source trait abstractions (`ChangeCapture` + `Continuity`, `DocumentBuilder`, `SourceSpec` + `all_tables`, `validate_indexes`, `CaptureProvisioning`/`CoverageReport`, `SchemaIntrospection`/`RelationalCatalog`) | `libs/1-ports/source/src/` (`provisioning.rs` for coverage; `introspection.rs` for catalog enumeration + `junction_candidates`) |
| Visual schema designer (web app: introspect → edit → preview → write files) | `apps/design/` (`flusso-design`) — `server.rs` (axum + JSON API: project/catalog/test-connection/**parse**/preview/validate/**sample**/diff/save), `codegen.rs` (model → `*.schema.yml`/`flusso.toml`), `preview.rs` (mapping + document tree), `assets.rs` (embedded SPA); CLI `design` subcommand in `apps/cli/src/commands/design.rs`; frontend under `apps/design/frontend/` (React Flow node-graph canvas — `model/` projects the `IndexSchema` tree ↔ nodes/edges + path-addressed edits, plus `complete.ts` (incomplete-field checks) and `prune.ts` (drops incomplete pieces from the **live preview** payload only, so a mid-build blank name doesn't 400 the strict backend), `components/` the canvas/nodes/inspector/catalog-browser), built to `apps/design/dist/`; property round-trip in `apps/design/tests/roundtrip.rs`. The **sample document** preview builds a real doc from one live row via `PgDocumentBuilder::sample_document` (postgres crate — keeps sqlx/`RowKey` there; reuses the `build` path + `sink::to_json`) |
| `Sink` trait (`apply`/`flush`/seeding/`reindex`), `SinkOptions`, JSON render | `libs/1-ports/sink/src/` |
| OpenSearch sink (bulk, mappings, seeding; alias-over-generations + reindex) | `libs/2-adapters/sink-opensearch/src/` — `lib.rs` (the `OpensearchSink` type + ctor), `sink_impl.rs` (the `Sink` impl), `transport.rs` (HTTP plumbing + index CRUD), `generations.rs` (aliases, meta doc, generation naming), `mapping.rs` (index body/analysis), `bulk.rs` (wire format + chunking) |
| Stream port (`Stream`, lanes, `LaneItem`/`Request`, `Producer`/`Consumer`/`AckHandle`) / in-process channel adapter (ticketed in-flight ledger, watermark) | `libs/1-ports/stream/src/` (`items.rs`, `queue.rs`), `libs/2-adapters/stream-channel/src/lib.rs` |
| Editor schema + Reference table generation (`flusso schema config\|docs`, drift test) | `apps/cli/src/commands/schema_cmd.rs`; artifacts at `libs/1-config/config.schema.json`, `docs/src/reference/generated/` |
| CLI subcommands (`build`/`run`/`check`/`schema`/`indexes`/`reindex`) | `apps/cli/src/` — `main.rs` dispatches; `commands/` holds one module per command (`build.rs`, `run.rs` → composition root: installs telemetry, serves the HTTP surfaces, drives `Daemon::start`/`run`, owns signals; `check.rs`, `schema_cmd.rs`, the `indexes`/`reindex` HTTP-client `admin.rs`, shared `print.rs`); `telemetry/` and `http/` hold the transport, `backends.rs` the backend assembly |
| On-demand reindex (alias-over-generations, per sink, no restart) | sink: `libs/2-adapters/sink-opensearch/src/sink_impl.rs` (`reindex`/`ensure_index`/`mark_seeded`) + `generations.rs` (generation helpers); engine `SinkEngine::reindex` (staged between batches + a `Backfill` request); daemon `DaemonControl::reindex` (`supervise.rs`); CLI `http/mod.rs` (`POST /reindex?index&sink`), `commands/admin.rs` (client, `--sink`). Deferred write-side zero-downtime follow-on: issue #6 |
| Query client (`flusso-query`) | `sdk/query/src/` |
| `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]` proc-macros | `sdk/query-derive/src/` — `lib.rs` (entry points + `Attrs`), `doc.rs` (field parsing/validation + the recursive handle tree + `embed_checks`), `fragment.rs` (the location-free shape check), `spec.rs` (baking a level into `&[FieldSpec]`), `resolve.rs` (finding `flusso.toml`); the const-check vocabulary is `sdk/query/src/check.rs`. Plus the `flusso-query-derive` memory note |
| Runnable example (stack, seed, consumer) | `dev/` (`flusso.toml`, `postgres/init/`, `search-api/`) |
| Registry image / containerized demo | `Dockerfile` (`runtime` target = config-less registry image; `demo` target = + baked dev lock), `docker-compose.demo.yml` (override adding the `flusso` service, built from the `demo` target), `.dockerignore`; user-facing shipping recipes in `docs/src/deploy/docker.md` |
| Kubernetes deploy (Helm chart) | `deploy/helm/flusso/` — `Chart.yaml`, `values.yaml`, `templates/`, `README.md` |
| Agent-facing docs (the Claude plugin + internal commands) | `plugin/` — `ARCHITECTURE.md` is the contract (one corpus/three consumers, who owns which meaning, the self-containment rule), `skills/*/SKILL.md` the knowledge corpus (`flusso-query` discloses `migration.md`/`options.md`/`maps.md`), `commands/` thin workflow entries, `agents/flusso-expert.md`, `hooks/`; `.claude/commands/{implement,new-issue}.md` the internal spine. Guarded by `apps/cli/tests/agent_docs_paths.rs` |
| Domain glossary + architecture decisions | `CONTEXT.md` (the vocabulary: kernel / ports / adapters / engine / daemon, stream / lane / envelope / position, operation vs primitive vs transport) and `docs/adr/` (0001 adapter-owned config, 0004 the rename, 0005 lock format 3: accepted; 0002/0003 the engine split: `proposed` until phase 2 of #130 lands) |

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
- **The whole workspace publishes to crates.io** (so `cargo install flusso-cli` works), under a
  `flusso-*` package namespace. Every crate is published **except** `dev/search-api`
  (`flusso-dev-search-api`) and `dev/query-e2e` (`flusso-query-e2e`) — both `publish = false`
  (a runnable example and a live-e2e guard, not shipping code). The
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
