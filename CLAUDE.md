# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

flusso keeps OpenSearch in sync with Postgres from declarative config. You describe a
search document in YAML (`*.schema.yml`); flusso derives the index mapping, seeds it,
then follows Postgres logical replication so the index stays current. Read `README.md`
for the full picture. The user manual is an mdBook under `docs/` (published to GitHub
Pages); its chapters are the canonical docs: `docs/src/guides/schema-authoring.md` for
every `*.schema.yml` key, `docs/src/guides/configuration.md` for every `flusso.toml` key,
source/sink option, and environment variable (secrets, the `FLUSSO_*` flag overrides,
logging/telemetry — all centralized there), and `docs/src/guides/deploying.md` for the
Docker shipping recipes (bake/compile a `flusso.lock`, scoped per-Dockerfile
`.dockerignore`). The query side is `apps/query/README.md` (the `flusso-query` crate;
`docs/src/guides/querying.md` `{{#include}}`s it). Every crate has its own `README.md`
— its crates.io/docs.rs landing, wired via `#![doc = include_str!("../README.md")]`
(except the `apps/query` README, kept separate from its `//!` because it's the full query
manual) — and `libs/README.md` maps the crate layering. When you change a doc's content,
update the chapter/README that owns it, not a now-deleted root `.md`.

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
cargo +nightly fuzz run pgoutput_decode    # fuzz the WAL decoder (from libs/1-sources/1-postgres)
```

- **Fuzzing** needs nightly + `cargo install cargo-fuzz --locked`. The one target,
  `pgoutput_decode`, lives in `libs/1-sources/1-postgres/fuzz/` (its own workspace, kept out
  of the strict `[workspace.lints]`) and drives the otherwise crate-private pgoutput decoder
  via the `sources-postgres` `fuzzing` feature (`fuzz_pgoutput_decode`). Contract: the
  decoder must never panic on arbitrary bytes — an `Err` is the correct outcome. Run from the
  crate dir; a crash lands in `fuzz/artifacts/`.
- The `#[ignore]`d e2e tests live in `sources-postgres`'s `integration` and
  `config_coverage` binaries plus `engine`'s `wal` and `pipeline` binaries (the full
  source→engine→sink e2e lives in `engine` — a leaf source crate must not dev-depend on
  the engine, or it can't be published before the engine and the layering is violated);
  `testcontainers` spins up Postgres (and, for `pipeline`, OpenSearch). `engine`'s `wal`
  drives a *recording* sink (asserts which op the engine emits); `engine`'s `pipeline`
  drives a **real OpenSearch sink** and reads the index back over HTTP, asserting the
  actual document after each change: live insert/update/delete across
  `uuid`/`int`/`bigint`/`text` keys, soft-delete tombstoning (boolean + timestamp markers,
  set→removed, cleared→restored), and backfill (active rows seeded, soft-deleted skipped).
  It is the only test that catches a live change rebuilt as the *wrong* op (e.g. an update
  written as a tombstone because the WAL key decoder and the read-back decoder disagree on
  the `GenericValue` variant; see `cdc/pgoutput.rs::typed_value` vs
  `document/value.rs::decode_column`). `.config/nextest.toml` caps their concurrency and
  retries them — they're legitimately slow/flaky.
- **The `schema` crate's config env-var tests must run single-threaded**: the
  `flusso.toml` parse/convert tests (`libs/2-schema/tests/config_toml.rs`) mutate process-wide
  env (`DATABASE_URL`, `<SINK>_OPENSEARCH_URL`). nextest gives each test its own process so it's
  fine there; under plain `cargo test` use `cargo test -p schema -- --test-threads=1`.
  Intermittent `MissingConnectionUrl` / wrong-override failures are this race, not a regression.
- CI's `test` job runs, in order: `cargo fmt --all --check` → `cargo clippy --workspace` →
  `cargo check --workspace --all-targets` (compiles benches + examples, which clippy and nextest
  skip — clippy omits `--all-targets`, nextest only builds test targets) → `cargo nextest run
  --profile ci --run-ignored all` → `cargo test --doc` → `RUSTDOCFLAGS="-D warnings" cargo doc
  --workspace --no-deps --document-private-items` (broken/ambiguous/redundant intra-doc links fail
  the build). Match these before assuming green. A separate `fuzz` job runs a 60-second
  `pgoutput_decode` smoke fuzz on nightly (see below); the `query.rs` proptests need no special
  handling — they're ordinary tests caught by the nextest step.
- **The designer (`apps/design`) has three test layers.** (1) A property/"fuzz" round-trip
  (`apps/design/tests/roundtrip.rs`, proptest): random valid `IndexSchema`s →
  `codegen → parse → convert` identity — an ordinary test, caught by the nextest step. (2) A
  `designer-frontend` CI job: `npm ci`, **ESLint** (`npm run lint`, flat config, **type-aware** —
  typescript-eslint recommended+stylistic *type-checked* sets, plus react / react-hooks / jsx-a11y /
  react-refresh, all as errors; CI runs it `--max-warnings 0`; `just design-lint [fix]`. Pinned to
  ESLint 9 because eslint-plugin-react/jsx-a11y don't support 10 yet; the canvas's pointer-driven
  rows scope off the jsx-a11y interaction rules), an **i18n completeness check** (`npm run check:i18n` →
  `apps/design/frontend/scripts/check-i18n.mjs`: every `t("…")` key the UI uses exists in the
  `en` base catalog and every locale defines the same key set — so a feature can't ship UI without
  its translations), then `npm run build` + a `git diff` guard that the committed
  `apps/design/dist/` matches a fresh Vite build (the embedded SPA must not drift). (3) A
  `designer-e2e` CI job: spins a seeded Postgres, builds the binary, then a **Playwright** suite
  (`apps/design/frontend/e2e/`) drives the *real* served UI (load/add/edit/delete/collapse) and runs
  the **UI-save → `flusso check`** pipeline. Locally: `just design-e2e` (needs Docker + downloads
  Chromium). The browser e2e is the only net that catches *rendered/interaction* regressions —
  builds and `curl` checks are blind to them. **Gotcha:** `e2e/server.mjs` runs the prebuilt
  `target/debug/flusso` which *embeds* the SPA (rust-embed) — it does **not** rebuild — so a bare
  `npm run test:e2e` serves a stale UI. Always `cargo build -p flusso-cli` first (this is why
  `just design-e2e` builds the binary before Playwright).
- **The designer frontend is Tailwind v4 + shadcn/Radix, atomic-design.** UI primitives are
  shadcn components in `apps/design/frontend/src/components/ui/` (`button`/`input`/`select`/
  `checkbox`/`dialog`/`tooltip`/…), tuned to the flusso palette; molecules like `Hint`, `Field`,
  `Block`/`Bridge`/`Drawer` (in `widgets.tsx`) compose them. There is **one** stylesheet,
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
  `flusso-query-derive`'s trybuild UI tests (`apps/query-derive/tests/ui/*.stderr`) compare
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
`DATABASE_URL` / `PRIMARY_OPENSEARCH_URL`. It publishes `9464`, so the base Prometheus scrapes
it via the same `host.docker.internal:9464` it uses for a host-run flusso — one config, both
modes. Same project as the base, so it shares its volumes; don't run a host `cargo run` flusso
and the container at once (same replication slot).

`default-members = ["apps/cli"]`, so bare `cargo run` is the `flusso` binary. CLI
subcommands: `build` (compile config+schemas → portable `flusso.lock`, no DB, no secrets
baked in), `check` (validate + print typed mapping; `--offline` skips the DB), `run`
(cargo-style: when a `flusso.toml` is present — the default path, or `--config` — it
recompiles and **rewrites the `flusso.lock`** then runs that, so the committed lock stays
current; with no config it loads the existing `--lock`; `--locked` runs the lock as-is with
no recompile; a lock-write failure is fatal; `--skip-backfill` resumes live only — see
`resolve_config`/`plan_config` in `apps/cli/src/commands/run.rs`), `schema` (print an embedded
editor-assist JSON Schema: `schema config` or `schema index`; no DB). See `dev/README.md` for the walk-through. **Every flag also reads a
`FLUSSO_*` env var** (clap's `env` feature; the flag wins when both are set) — e.g.
`FLUSSO_CONFIG`, `FLUSSO_SLOT`, `FLUSSO_PUBLIC_ADDRESS` — so the binary configures cleanly from
the environment (Helm/compose). This is separate from the config's own reserved env vars
(`DATABASE_URL`, `<SINK>_OPENSEARCH_URL`) and `{ env = "VAR" }` secret refs.

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

Crates live under `libs/` and `apps/`; the **numeric prefix is the dependency layer** — a
crate only depends on lower-numbered ones (`0-core` → `1-{queue,sources,sinks}` →
`2-{engine,schema}` → `3-daemon` → `apps`). Within a layer, `0-core` holds the abstraction/domain
types and higher numbers are concrete backends. Keep this acyclic when adding crates.

Layer 0 is a single crate, `schema-core` (`libs/0-core`): the **cross-cutting vocabulary**
every layer trades in — `GenericValue`, the newtypes, `IndexMapping`, `IndexSchema`,
`Field`/`Filter`, `FailurePolicy`, the per-sink configs. Everything that turns config *files*
into that vocabulary lives a layer up, in the `2-schema` group (crate name `schema`): the two
file *parsers* (`schema-config-toml`/`schema-index-yaml`, nested at
`libs/2-schema/1-{config-toml,index-yaml}`, which produce neutral entities), the **assembled
`Config`** (plus `Index`/`Source`/the `Sink` enum), the `Config`→entity conversion, and the
loader. Keeping the parsers out of layer 0 is deliberate — it's the one place a layer-1
backend *could* otherwise reach config-loading machinery; with them at layer 2 the backends
depend only on the layer-0 vocabulary and *cannot* see the assembled `Config` or the file
parsers. See "Config layer" below.

### The pipeline (`libs/2-engine/src/pipeline.rs`)

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

The engine is the only orchestrator. Everything it drives — `ChangeCapture` (source),
`DocumentBuilder`, `Sink`, and the queue — is a **trait object**, so backends swap without
touching the loop. Key invariants to preserve when editing the engine:

- **At-least-once via flush-then-confirm.** Source acks for a batch are confirmed *only
  after* the `Sink::flush` that made its documents durable, so the replication slot
  advances past a change exactly when its documents have landed. A crash before flush
  redelivers the whole batch; it's re-applied idempotently (documents rebuilt from the
  current row, written by deterministic id). The `confirms_no_ack_before_its_flush` test
  guards this.
- **Two-step resolve → build, deduped per batch.** A batch buffers changes, resolves each
  to affected `DocumentId`s, dedups them, then `build_many` assembles each touched
  document **once** even if several changes hit it. Reordering writes within a batch is
  safe (keyed, idempotent). `builds_a_repeatedly_touched_document_once_per_batch` guards
  this.
- **Backfill is the *sink's* decision.** On start the engine `ensure_index`es every
  mapping, then (unless `--skip-backfill`) asks each sink `is_seeded`; unseeded indexes get
  their root tables snapshotted through the same queue→resolve→build→sink path, scoped so a
  seeded index sharing a table isn't rewritten, then `mark_seeded`.
- `BatchPolicy` (default 256 changes / 50ms) controls flush grouping; `max_changes: 1`
  reproduces flush-per-change.
- **Item-level rejections vs flush-wide errors.** `Sink::flush` returns a `FlushReport`:
  `Err` is flush-wide (transport down, whole request refused) and always stops the run;
  an `Ok` report instead lists documents the destination *applied the batch but rejected*
  individually (a mapping conflict, a malformed value). The `FailurePolicies` (a global
  `FailurePolicy` default + per-index overrides, from config `on_error`, resolved by **logical**
  index name) decide each rejection in `commit`: `Stop` halts (batch left unconfirmed →
  redelivered); `Skip` quarantines it (`Observer::on_document_quarantined` → metric/status/log)
  and acks the batch so the slot advances and the poison isn't redelivered. A single `Stop`
  rejection halts the whole batch, decided before any quarantine event is emitted. The
  OpenSearch sink does *not* retry item-level rejections (re-sending re-rejects); it maps each
  back to its logical index. Guarded by `skip_policy_*`/`stop_policy_*`/`per_index_stop_*` tests.
- **Observability is a trait, not baked in.** The engine reports lifecycle/progress to an
  `Observer` (`libs/2-engine/src/observer.rs`) — sync, cheap, no-op by default, set via
  `with_observer`. It depends only on the trait, never on metrics or a status backend. The
  `daemon` crate is the one consumer, fanning events to both the `metrics` facade and a live
  status surface. `reports_lifecycle_and_progress_to_the_observer` guards the emit points.

### The daemon (`libs/3-daemon/src/lib.rs`) — domain only

The daemon owns the **domain**: it assembles the pipeline from a `Config` — but it does
**not** name concrete backends. Backend construction is a seam: the `Backends` trait
(`backends.rs`, returning `SourceParts` = capture + document builder, and the `Sink`) is
supplied to `Daemon::new`, so the daemon depends only on `sources-core`/`sinks-core`, never
on Postgres/OpenSearch. The CLI is the one place that implements it (see below). A second
source-neutral capability lives beside `validate_indexes`: `CaptureProvisioning`
(`libs/1-sources/0-core/src/provisioning.rs`) — given the tables an index reads
(`SourceSpec::all_tables`), a source reports coverage + a privilege verdict (`CoverageReport`)
and, when allowed, provisions the gap. Postgres backs it with a **publication**
(`libs/1-sources/1-postgres/src/cdc/publication.rs`, `impl CaptureProvisioning for
WalChangeCapture`): `run` auto-creates/extends it on `live` (after `ensure_slot`) when the role
is privileged enough and `manage_publication` isn't opted out, else warns with the exact SQL;
`check` inspects read-only and prints the same. The trait/report never name "publication", so
the daemon/CLI/printer stay backend-neutral. A third source-neutral capability is
`SchemaIntrospection` (`libs/1-sources/0-core/src/introspection.rs`): where `Catalog` answers
"the type of *this* column" and `CaptureProvisioning` answers "is this table set coverable",
`introspect` *enumerates* the whole relational catalog (`RelationalCatalog` — every table's
columns/types/PK/FKs, each with a suggested `FlussoType`) so discovery-driven tooling can pick
from what's really there; `junction_candidates` (a free function, not a trait method) flags m2m
junctions. Postgres backs it over `pg_catalog`/`information_schema`; the visual designer
(`apps/design`) is its first consumer. The daemon
wires a `StatusObserver` (`observer.rs`) that updates a
shared `Status` (`status.rs`), runs the engine, and polls source capture lag out-of-band
(`lag.rs` over `ChangeCapture::lag`). It is **telemetry-agnostic** — it depends only on the
engine's `Observer` trait, not on any metrics backend — and owns **no transport**: no HTTP
server, no process signals, no metrics *recording* or *exporter*; those are the binary's.
`Daemon::start()` builds everything and returns a `RunningDaemon` exposing `status()` (an
`Arc<Status>` a transport can read) and `run(shutdown)`, which runs until the stream ends, an
error stops it, or the caller's `shutdown` future fires. A binary attaches its own metrics
observer via `Daemon::with_observer`; the engine drives a `FanOut` (`engine::FanOut`) of the
status observer plus any attached ones. So the daemon's public contract is *data*: the
backend-agnostic `Observer` events and the `Status` handle.

The CLI (`apps/cli`) is the **composition root**. It is the single crate that names concrete
backends: `backends.rs`'s `FlussoBackends` implements the daemon's `Backends` trait, resolving
the connection (in the running environment) and building the Postgres source + the configured
sinks (the source-type dispatch and the OpenSearch/stdout/fan-out `match` live here, not in the
daemon). Adding a backend = a new match arm here plus its crate; the daemon and engine are
untouched. The composition root is also where `Config` is translated into the backend-facing
subsets it needs: the Postgres source builder takes a `SourceSpec` (the enabled indexes +
their schemas, in `schema-core` types — `sources_core::SourceSpec`), never the whole `Config`.
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

### Config layer — two-stage parse then convert

`schema::load(path)` is the front door (in `libs/2-schema`, layer 2): it reads `flusso.toml`,
resolves+parses every referenced `*.schema.yml`, and returns one validated `Config`. Downstream
crates that legitimately compose a deployment (the daemon, the CLI) depend on `schema` and reach
the vocabulary via its re-export of `schema-core`. Each file *parser* (`schema-config-toml`,
`schema-index-yaml`, both in the `2-schema` group) works in two stages:

1. **Parse** — `serde` deserializes into permissive *entity* types that mirror the file
   1:1; unknown fields are rejected. This is all the parser crates do.
2. **Convert** — entities are lifted into the model and rules the format can't express are
   applied (identifier validation, join/aggregate arity, declared-type placement, filter
   shapes). For `*.schema.yml` → `IndexSchema` (a `schema-core` vocabulary type) this lives in
   `schema-index-yaml`. For `flusso.toml` → the assembled `Config` the conversion is a
   *composition* step, so it lives in the `schema` crate (`libs/2-schema/src/deployment/conversion.rs`,
   the `From<ConfigToml>` impl), next to `Config` — the toml parser stays free of `Config`.

**Secrets are deferred, never resolved at parse/convert time.** A `{ env = "VAR" }`
reference becomes a `Secret` and is read in the environment that *runs* the pipeline — so a
compiled `flusso.lock` carries no secret it wasn't given literally.

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
rejects others. Parsing lives in
`libs/2-schema/1-index-yaml/src/entities/field.rs`; the core model is `schema_core::FieldSource`
(`Join.kind: JoinKind`, with reverse resolution per kind in
`libs/1-sources/1-postgres/src/document/resolve.rs`).

### Query side — `flusso-query` + the derive

`apps/query` (crate `flusso-query`) is a backend-neutral OpenSearch/Elasticsearch query
client. `apps/query-derive` (`flusso-query-derive`, re-exported as `flusso_query::FlussoDocument`
behind the `derive` feature) is a proc-macro that, **at compile time and with no DB**,
discovers `flusso.toml`, resolves the named index mapping, validates the struct against it,
and generates a typed query surface. `dev/search-api` is a working axum consumer. This is a
deep subsystem — the proc-macro internals (scope tagging, `FlussoValue<K>` kind markers,
nested/object handles) are documented in the `flusso-query-derive` memory note; read that
before changing the derive.

Dynamic-key `map` fields (issue #28) get typed handles too: `handles/map.rs` emits one
handle per value kind — `TextMap`/`KeywordMap`/`NumberMap`/`DateMap` — where
`.key(runtime_str)` returns a fully-typed leaf handle (`Text`/`Keyword`/`Number`/`Date`)
of the declared kind: runtime keys, compile-time value type. `TextMap::search(q)` builds a
`MapSearch` (a `best_fields` `multi_match` over `prefer`'d keys plus a `path.*` fallback) for
cross-key search with per-key preference; `has_key`/`exists` are presence checks. The
doc-side type is `HashMap<String, V>` (a blanket `FlussoMap<K>` impl for any
`V: FlussoValue<K>`), or a `#[derive(FlussoMap)]` newtype wrapper; the derive's `check_type`
map arm hard-checks a `HashMap` value type and defers a `FlussoMap<kind>` bound otherwise.
`handle_fn` dispatches on `Mapping.map_values` (`Text`→`TextMap`, `Keyword`→`KeywordMap`,
`Date`→`DateMap`, the numerics→`NumberMap`). Phase 2 (`dynamic_templates` per-key
analyzers for per-language stemming) is deferred.

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
forward impl) — `struct Money(Decimal)` is a decimal value with no tag; only enums need an
explicit `#[flusso(keyword|text)]` (numeric/date tags don't exist).
`Text`/`Keyword` expose `.keyword()`/`.keyword_lowercase()`/`.text()` subfield accessors
(runtime methods, not derive codegen — keeps the field method returning the shared handle
type for `multi_match`/composition). Issue #19 acceptance test: `apps/query-derive/tests/
derive.rs::acceptance_realistic_projection_needs_no_escape_hatch`.

**Sorting + nesting-aware path metadata (issue #49).** `FlussoDocument` now carries only
`const PATH: &[Segment]` (the container chain from the index root, in `apps/query/src/path.rs`);
the index identity + `query`/`get` moved to a root-only `FlussoIndex: FlussoDocument` supertrait,
so a child projection physically can't `.query()`. The derive emits `FlussoDocument` for **every**
struct (root + each nested element) and `FlussoIndex` only for the root; the `Root` scope marker
stays (the shared root/flattened-object scope, so combined search and object handles keep
composing) and impls `FlussoDocument` with `PATH = &[]`. Sorting goes through a `Sortable` trait
(`asc`/`desc`, impl'd for `Keyword`/`Text`/`Number`/`Date`/`Bool`, **not** `Geo`/`Object`/map) whose
sorts are nesting-aware: `Sort::field::<S>` reads `nested_boundaries(S::PATH)` and renders the
recursive `nested:{path, nested:{…}}` chain (mode defaulted from direction), so a bare
`Order::placed_at().desc()` is correct at top level; `NestedProjection` (inner_hits) strips that
wrapper. `SortBuilder` (`by`/`near`/`score`/`score_if`/`raw`/`tiebreak`/`or_default`/`build`,
deduping by key) collapses request→`sort` mapping; `OrderBy`/`MaybeOrderBy` carry a direction +
optionality (a request's `Option<dir>` self-skips). `Search`/`MultiSearch`/`NestedProjection` take
plural `.sorts(..)`.

**Index prefix (issue #24).** A deployment-wide `prefix` (top-level `Config` field;
`--index-prefix`/`FLUSSO_INDEX_PREFIX` override it, flag > env > config) is prepended to
**every** name the OpenSearch sink owns — the hash alias `{prefix}{logical}_{hash}`, its
generations, the `{prefix}{logical}` convenience alias, and the `{prefix}flusso_meta` index —
so several deployments (dev/staging/nightly) can share one cluster. Write side: resolved +
validated in `commands/run.rs` (`schema_core::validate_index_prefix`), threaded via
`backends.rs` into `OpensearchSink::with_index_prefix`; the sink prefixes at the single
chokepoint (`hash_alias`/`convenience_alias`/`meta_index` helpers in `lib.rs`). Read side is
**runtime, not baked**: the derive still emits the unprefixed `INDEX`/`SCHEMA_HASH`, and
`flusso-query`'s `Client::index_prefix` prepends the prefix to each request path (and strips
it from `_index` in combined-search decode), so one compiled consumer serves every env. The
generation naming functions (`generations.rs`) are prefix-agnostic — they operate on whatever
hash-alias string they're handed.

Combined-search decode (`apps/query/src/multi.rs`, `decode_response`) also **normalizes the
generation suffix**: a hit's `_index` is the concrete `{logical}_{hash}_{n}` behind the hash
alias (OpenSearch reports the backing index, never the alias the query used), so the `_{n}` is
collapsed back to a union variant's `{logical}_{hash}` before dispatch — anchored on the
union's known targets, not a blind trailing-`_{digits}` trim, because the eight-hex hash can
itself be all digits. Without this, every hit from a real deployment missed dispatch (issue
#67). Single-index search and `_msearch` don't dispatch by `_index`, so they were unaffected.
Guarded by the `multi_decode_*` unit tests and the `combined_search` live e2e (`apps/query/tests/`).

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
a field type tag/sibling, an enum token, a sink option, a source/sink capability — align the
designer in the **same** change: model/codegen/preview (`apps/design/`), the canvas/inspector
controls (`apps/design/frontend/`), and the introspection/source-steer if the source informs it.
And any user-facing string goes through `t("ns.key")` with the key added to **every** locale
catalog in `apps/design/frontend/src/locales/` (English `en.ts` is the base; translate the rest).
Two CI guards in the `designer-frontend` job enforce this and will fail the build otherwise: the
**i18n check** (`npm run check:i18n`, key parity across locales) and the **dist-drift** guard
(committed `apps/design/dist/` must match a fresh build — so rebuild + commit the SPA). Locally:
`just design-i18n`, then rebuild the SPA. The `/implement` flow has an explicit step for this.

## Where things live (jump here first)

| To work on… | Go to |
| --- | --- |
| The sync loop / batching / ack ordering | `libs/2-engine/src/` — `lib.rs` (the `Engine` builder + public API), `pipeline.rs` (the `Pipeline` run machinery: `run_inner`/`backfill`/`pump`/`work`/`commit`/`CaptureGuard`), `policy.rs` (`BatchPolicy`/`FailurePolicies`), `tests.rs` |
| Pipeline observability trait (`Observer`, `BatchStats`, `FanOut`) | `libs/2-engine/src/observer.rs` |
| Daemon (domain): pipeline wiring, `Status`, `StatusObserver`, lag poll | `libs/3-daemon/src/` — `lib.rs` (`Daemon`/`RunningDaemon`/`DaemonOptions`), `backends.rs` (`Backends` trait + `SourceParts` seam), `observer.rs`, `status.rs`, `lag.rs` |
| Backend assembly (which concrete source/sink): the `Backends` impl | `apps/cli/src/backends.rs` (`FlussoBackends` — Postgres source + OpenSearch/stdout sinks) |
| Transport + telemetry (binary): exporters, metrics recording, HTTP surfaces, auth, signals | `apps/cli/src/` — `telemetry/mod.rs` (traces), `telemetry/metrics.rs` (meter provider + `in_flight` gauge), `telemetry/observer.rs` (`OtelObserver`), `http/mod.rs` (public + private routers + `serve`), `http/auth.rs` (Basic auth), `commands/run.rs` (orchestration + signals) |
| Config loading + the assembled `Config`/`Index`/`Source`/`Sink` (layer 2) | `libs/2-schema/src/` — `lib.rs` (`load`), `loader.rs`, `compiled.rs` (`flusso.lock`), `deployment/` (the `Config` family + `From<ConfigToml>` conversion + `resolve_mappings`) |
| Validated domain model / vocabulary (the shared types — the sole layer-0 crate) | `libs/0-core/src/` — `config/` (`IndexSchema`, `FailurePolicy`, per-sink configs, …), `common/` (newtypes), `GenericValue` |
| `flusso.toml` parsing (entities only; conversion is in the `schema` loader) | `libs/2-schema/1-config-toml/src/` (`entities/`) |
| `*.schema.yml` parsing / field syntax | `libs/2-schema/1-index-yaml/src/entities/field.rs`, `conversion.rs` |
| Postgres WAL capture / backfill / doc building / publication management | `libs/1-sources/1-postgres/src/` — `cdc/` (incl. `publication.rs`), `document/` |
| Source trait abstractions (`ChangeCapture`, `DocumentBuilder`, `SourceSpec` + `all_tables`, `validate_indexes`, `CaptureProvisioning`/`CoverageReport`, `SchemaIntrospection`/`RelationalCatalog`) | `libs/1-sources/0-core/src/` (`provisioning.rs` for coverage; `introspection.rs` for catalog enumeration + `junction_candidates`) |
| Visual schema designer (web app: introspect → edit → preview → write files) | `apps/design/` (`flusso-design`) — `server.rs` (axum + JSON API: project/catalog/test-connection/preview/validate/**sample**/diff/save), `codegen.rs` (model → `*.schema.yml`/`flusso.toml`), `preview.rs` (mapping + document tree), `assets.rs` (embedded SPA); CLI `design` subcommand in `apps/cli/src/commands/design.rs`; frontend under `apps/design/frontend/` (React Flow node-graph canvas — `model/` projects the `IndexSchema` tree ↔ nodes/edges + path-addressed edits, `components/` the canvas/nodes/inspector/catalog-browser, `e2e/` the Playwright suite + save→check pipeline), built to `apps/design/dist/`; property round-trip in `apps/design/tests/roundtrip.rs`. The **sample document** preview builds a real doc from one live row via `PgDocumentBuilder::sample_document` (postgres crate — keeps sqlx/`RowKey` there; reuses the `build` path + `sinks_core::to_json`) |
| `Sink` trait, JSON render, fan-out | `libs/1-sinks/0-core/src/` |
| OpenSearch sink (bulk, mappings, seeding; alias-over-generations + reindex) | `libs/1-sinks/2-opensearch/src/` — `lib.rs` (the `OpensearchSink` type + ctor), `sink.rs` (the `Sink` impl), `transport.rs` (HTTP plumbing + index CRUD), `generations.rs` (aliases, meta doc, generation naming), `mapping.rs` (index body/analysis), `bulk.rs` (wire format + chunking) |
| Queue abstraction / in-process channel | `libs/1-queue/0-core/src/`, `libs/1-queue/1-channel/src/lib.rs` |
| CLI subcommands (`build`/`run`/`check`/`schema`/`indexes`/`reindex`) | `apps/cli/src/` — `main.rs` dispatches; `commands/` holds one module per command (`build.rs`, `run.rs` → composition root: installs telemetry, serves the HTTP surfaces, drives the `Daemon::start`/`run` **restart loop**, owns signals; `check.rs`, `schema_cmd.rs`, the `indexes`/`reindex` HTTP-client `admin.rs`, shared `print.rs`); `telemetry/` and `http/` hold the transport, `backends.rs` the backend assembly |
| On-demand reindex (alias-over-generations + restart trigger) | sink: `libs/1-sinks/2-opensearch/src/sink.rs` (`reindex`/`ensure_index`/`mark_seeded`) + `generations.rs` (generation helpers); engine `CaptureGuard` + daemon `LagGuard` (clean cancel) + `Daemon::with_status`; CLI `commands/run.rs` (restart loop), `http/mod.rs` (`POST /reindex`), `commands/admin.rs` (client). Deferred write-side zero-downtime follow-on: issue #6 |
| Query client (`flusso-query`) | `apps/query/src/` |
| `#[derive(FlussoDocument)]` proc-macro | `apps/query-derive/src/` (+ the `flusso-query-derive` memory note) |
| Runnable example (stack, seed, consumer) | `dev/` (`flusso.toml`, `postgres/init/`, `search-api/`) |
| Registry image / containerized demo | `Dockerfile` (`runtime` target = config-less registry image; `demo` target = + baked dev lock), `docker-compose.demo.yml` (override adding the `flusso` service, built from the `demo` target), `.dockerignore`; user-facing shipping recipes in `docs/src/guides/deploying.md` |
| Kubernetes deploy (Helm chart) | `deploy/helm/flusso/` — `Chart.yaml`, `values.yaml`, `templates/`, `README.md` |

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
  `libs/0-core/src/common/`. `GenericValue` is the **typed canonical value vocabulary** that
  crosses layers — the middle type a source maps *into* and a sink maps *out of*. It's
  fine-grained (numerics split by width; `Date`/`Time`/`Timestamp`/`TimestampTz`; `Uuid`;
  `Bytes`) so no type is erased in transit, and its serde is **derived/format-agnostic** (a
  queue may encode it however it likes; it round-trips losslessly). A sink converts it to its
  own representation at its boundary (`sinks_core::to_json` is the OpenSearch JSON translation,
  where e.g. `bytea`→base64 lives) — core picks no wire format.
- Sources/sinks are `#[async_trait]` trait objects; mock them in tests as the engine tests do.
- **The whole workspace publishes to crates.io** (so `cargo install flusso-cli` works), under a
  `flusso-*` package namespace. Every crate is published **except** `dev/search-api`
  (`flusso-dev-search-api`, `publish = false` — a runnable example, not shipping code). The
  catch: a crate's published **package name** (`flusso-engine`, `flusso-schema-core`, …) differs
  from the **extern name** code uses (`engine`, `schema_core`, …). Two mechanisms keep that split
  so the rename needs **no source change**: each lib sets `[lib] name = "<extern>"`, and each
  `[workspace.dependencies]` entry keeps its short key plus `package = "flusso-…"` + `version`.
  So `use schema_core::…`, `package(flusso-sources-postgres)` in `.config/nextest.toml`, and the
  fuzz crate's `package = "flusso-sources-postgres"` path-dep all coexist. Shared listing metadata
  (license, repo, authors, keywords, readme) lives in `[workspace.package]`; crates inherit it
  with `.workspace = true`, and set their own `description` + `categories`. **Publish order is
  bottom-up** (a dep must be on crates.io before its dependents): `flusso-schema-core` → parsers →
  `flusso-schema` → `flusso-engine`/sinks/sources/queue → `flusso-daemon` → `flusso-design`
  (depends on `flusso-schema` + `flusso-sources-postgres`) → `flusso-query-derive` →
  `flusso-query` → `flusso-cli`.
- `dev/` is a runnable example, not shipping code; the hand-curated JSON Schemas for editor
  completion live **inside the parser crate that owns each** (so they ship in the published
  `.crate`): `schema_config_toml::CONFIG_SCHEMA`
  (`libs/2-schema/1-config-toml/config.schema.json`) and `schema_index_yaml::INDEX_SCHEMA`
  (`libs/2-schema/1-index-yaml/index.schema.yml`), each embedded via a crate-local
  `include_str!`, both re-exported from `schema` and emitted by `flusso schema config|index`.
  They sit in-crate (not bare at the repo root) because `cargo package` only bundles files under the
  crate dir — an out-of-crate `include_str!` would break the published crate. On each release the
  `.github/workflows/pages.yml` workflow publishes copies of these files to GitHub Pages under an
  immutable per-version path
  (`https://alias2k.github.io/flusso/schemas/v<version>/{index.schema.yml,config.schema.json}`, plus
  `v<minor>` and `latest` aliases), triggered by the schema crates' release tags
  (`flusso-schema-{index-yaml,config-toml}-v*`, which move together since all crates share a version);
  editor `# yaml-language-server: $schema=…` modelines point at that versioned URL, while in-repo files
  (`dev/*.schema.yml`, the parser test fixtures) use a relative path to the in-crate `schemas/`.
  `libs/2-schema/tests/schema_drift.rs` guards their enumerable sets — field type tags, field
  siblings, enum tokens, sink fields — against the parsers (reading the embedded consts), so
  adding a tag/sibling/variant fails CI until the schema matches. It does **not** check
  descriptions, defaults, the permissive `field` union, or the identifier `pattern`s (which
  can't model the newtypes' trim/lowercase sanitization).
