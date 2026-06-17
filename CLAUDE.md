# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

flusso keeps OpenSearch in sync with Postgres from declarative config. You describe a
search document in YAML (`*.schema.yml`); flusso derives the index mapping, seeds it,
then follows Postgres logical replication so the index stays current. Read `README.md`
for the full picture, `SCHEMA.md` for every config/schema key, `SOURCES_AND_SINKS.md`
for source/sink options, `CONFIG.md` for every environment variable (secrets, the
`FLUSSO_*` flag overrides, logging/telemetry — centralized there, not in the other docs),
and `CLIENT.md` for the query-side `flusso-query` crate.

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
- The `#[ignore]`d e2e tests live in `sources-postgres`'s `integration`, `wal`, and
  `config_coverage` binaries; `testcontainers` spins up Postgres. `.config/nextest.toml`
  caps their concurrency and retries them — they're legitimately slow/flaky.
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
or bake a lock into a child image). It also defines a `demo` target that extends that runtime
with the repo's dev config compiled into `/app/flusso.lock` — that target is what the demo
compose builds.

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
baked in), `check` (validate + print typed mapping; `--offline` skips the DB), `run` (loads
`flusso.lock` by default, or `--config` to compile-and-run; `--skip-backfill` resumes live
only), `schema` (print an embedded editor-assist JSON Schema: `schema config` or `schema
index`; no DB). See `dev/README.md` for the walk-through. **Every flag also reads a
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
on Postgres/OpenSearch. The CLI is the one place that implements it (see below). The daemon
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
split by op (`count`/`sum`/`avg`/`min`/`max`). Parsing lives in
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

## Keeping this file current

This file is a living index — keep it accurate as part of normal work, no separate ask
needed. When a change makes something here wrong or missing, update it in the **same**
change: a crate added/moved/renamed (fix the layering + "Where things live" table), a
command or test workflow that changes, a new engine invariant or guard test, a lint policy
change, or a config/schema format change. Don't let it drift; don't pad it with detail that
belongs in the linked docs.

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
| Postgres WAL capture / backfill / doc building | `libs/1-sources/1-postgres/src/` — `cdc/`, `document/` |
| Source trait abstractions (`ChangeCapture`, `DocumentBuilder`, `SourceSpec`, `validate_indexes`) | `libs/1-sources/0-core/src/` |
| `Sink` trait, JSON render, fan-out | `libs/1-sinks/0-core/src/` |
| OpenSearch sink (bulk, mappings, seeding; alias-over-generations + reindex) | `libs/1-sinks/2-opensearch/src/` — `lib.rs` (the `OpensearchSink` type + ctor), `sink.rs` (the `Sink` impl), `transport.rs` (HTTP plumbing + index CRUD), `generations.rs` (aliases, meta doc, generation naming), `mapping.rs` (index body/analysis), `bulk.rs` (wire format + chunking) |
| Queue abstraction / in-process channel | `libs/1-queue/0-core/src/`, `libs/1-queue/1-channel/src/lib.rs` |
| CLI subcommands (`build`/`run`/`check`/`schema`/`indexes`/`reindex`) | `apps/cli/src/` — `main.rs` dispatches; `commands/` holds one module per command (`build.rs`, `run.rs` → composition root: installs telemetry, serves the HTTP surfaces, drives the `Daemon::start`/`run` **restart loop**, owns signals; `check.rs`, `schema_cmd.rs`, the `indexes`/`reindex` HTTP-client `admin.rs`, shared `print.rs`); `telemetry/` and `http/` hold the transport, `backends.rs` the backend assembly |
| On-demand reindex (alias-over-generations + restart trigger) | sink: `libs/1-sinks/2-opensearch/src/sink.rs` (`reindex`/`ensure_index`/`mark_seeded`) + `generations.rs` (generation helpers); engine `CaptureGuard` + daemon `LagGuard` (clean cancel) + `Daemon::with_status`; CLI `commands/run.rs` (restart loop), `http/mod.rs` (`POST /reindex`), `commands/admin.rs` (client). Deferred write-side zero-downtime follow-on: `ROADMAP.md` |
| Query client (`flusso-query`) | `apps/query/src/` |
| `#[derive(FlussoDocument)]` proc-macro | `apps/query-derive/src/` (+ the `flusso-query-derive` memory note) |
| Runnable example (stack, seed, consumer) | `dev/` (`flusso.toml`, `postgres/init/`, `search-api/`) |
| Registry image / containerized demo | `Dockerfile` (`runtime` target = config-less registry image; `demo` target = + baked dev lock), `docker-compose.demo.yml` (override adding the `flusso` service, built from the `demo` target), `.dockerignore` |
| Kubernetes deploy (Helm chart) | `deploy/helm/flusso/` — `Chart.yaml`, `values.yaml`, `templates/`, `README.md` |

## Conventions

- Domain newtypes (validated identifiers, URLs) use the `nutype` crate (`try_new`) — see
  `libs/0-core/src/common/`. `GenericValue` is the value enum that crosses layers.
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
  `flusso-schema` → `flusso-engine`/sinks/sources/queue → `flusso-daemon` → `flusso-query-derive` →
  `flusso-query` → `flusso-cli`.
- `dev/` is a runnable example, not shipping code; the hand-curated JSON Schemas for editor
  completion live **inside the parser crate that owns each** (so they ship in the published
  `.crate`): `schema_config_toml::CONFIG_SCHEMA`
  (`libs/2-schema/1-config-toml/schemas/config.schema.json`) and `schema_index_yaml::INDEX_SCHEMA`
  (`libs/2-schema/1-index-yaml/schemas/index.schema.yml`), each embedded via a crate-local
  `include_str!`, both re-exported from `schema` and emitted by `flusso schema config|index`.
  They sit in-crate (not repo root) because `cargo package` only bundles files under the crate
  dir — an out-of-crate `include_str!` would break the published crate. Editor `# yaml-language-server:
  $schema=…` modelines and registry refs point at these in-crate paths.
  `libs/2-schema/tests/schema_drift.rs` guards their enumerable sets — field type tags, field
  siblings, enum tokens, sink fields — against the parsers (reading the embedded consts), so
  adding a tag/sibling/variant fails CI until the schema matches. It does **not** check
  descriptions, defaults, the permissive `field` union, or the identifier `pattern`s (which
  can't model the newtypes' trim/lowercase sanitization).
