# flusso

> [!IMPORTANT]
> ## 🤖 Generative AI disclosure
>
> **Generative AI was used in this project to produce boilerplate and
> documentation.** Every single line of code has been manually reviewed and
> revised by a human software developer who can be blamed accordingly.

**Keep OpenSearch in sync with Postgres, driven by declarative config.**

You write a bit of YAML describing what a search document should look like.
flusso builds the index, seeds it from your existing rows, then tails Postgres'
logical replication stream so the index stays current — no cron job, no nightly
reindex, no hand-rolled `for row in rows: es.index(...)` script you'll regret at
2am.

In short: you describe the *what*, flusso handles the *keep-it-in-sync*.

## Contents

- [Quickstart](#quickstart) — running in about five commands
- [What it does](#what-it-does) — the two files you write
- [How the pipeline works](#how-the-pipeline-works) — the bit that does the work
- [The CLI](#the-cli) — `build`, `check`, `run`
- [`just` commands](#just-commands) — the shortcuts you'll actually use
- [Requirements](#requirements) — what Postgres and OpenSearch need first
- [Deploying it](#deploying-it) — Docker image + Helm chart
- [Docs](#docs) — everything else, linked
- [Project layout](#project-layout) — where the code lives
- [Testing & development](#testing--development)

## Quickstart

The [`dev/`](dev/) directory is a complete, runnable example — a docker-compose
stack (Postgres wired for logical replication, OpenSearch, Dashboards,
Prometheus, Grafana), seeded data, and a matching config. With
[`just`](https://just.systems) installed (`cargo install just --locked`):

```sh
just up        # bring the whole stack up and wait for it to be healthy
just check     # validate the config + schemas against the database
just run       # backfill OpenSearch, then follow live changes (serves /status + /metrics)
```

Then, in another terminal, poke it and watch changes stream through:

```sh
just psql                                            # make some changes
curl -s localhost:9200/users/_search?pretty          # see them land in OpenSearch
just status                                          # live pipeline status
```

That's it. Run `just` on its own to see every recipe. The full walk-through —
resetting state, inspecting the slot, OpenSearch Dashboards on `:5601` — lives in
[`dev/README.md`](dev/README.md).

> No `just`? Every recipe is just a thin wrapper; the raw `cargo run -- …` and
> `docker compose …` commands are right there in the [`justfile`](justfile).

## What it does

A deployment is two kinds of files. That's the whole mental model.

**`flusso.toml`** — one per deployment. Where the data comes from, where it goes,
and which indexes to build.

```toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }   # secrets stay as env refs, never baked in

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[sinks.audit]          # sinks fan out — write the same docs to more than one place
type = "stdout"
pretty = true

[[index]]
name = "users"
schema = "users.schema.yml"
```

**`*.schema.yml`** — one per index. What a single search document looks like:
its table, its fields, and the related tables that fold in.

```yaml
table: users
primary_key: id

soft_delete:
  column: deleted          # users.deleted = true → tombstone instead of upsert

fields:
  - keyword: email
    transforms: [lowercase, trim]

  # Fold each user's recent orders in as a nested array.
  - has_many: orders
    table: orders
    foreign_key: user_id
    primary_key: id
    order_by: [{ column: created_at, direction: desc }]
    limit: 5
    fields:
      - double: total
      - keyword: status

  # ...or just count them. A count is always a long.
  - count: orderCount
    table: orders
    foreign_key: user_id
```

Every field declares its **type** from a fixed set ([`SCHEMA.md`](SCHEMA.md) lists
them all) that bridges a Postgres column and an OpenSearch mapping. So a schema is
self-describing: flusso derives the full index mapping — and validates your config
— without ever touching a database.

The neat part: change a user *or one of their orders* and flusso rebuilds the
whole `users` document and re-emits it. It figures out which documents a changed
row affects, reassembles each, and writes it by a deterministic id. You don't tell
it what to update; it works it out.

## How the pipeline works

The engine wires pluggable edges together and runs the loop:

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

A **capture** task drains the source's change stream into a bounded queue (full
queue → capture blocks; back-pressure for free). A **worker** pulls changes,
works out which document ids they touch, builds each document, and writes it.
Writes are **batched** — N changes, or whatever shows up in a short window, flush
together as one bulk round-trip.

Delivery is **at-least-once** — not exactly-once, because exactly-once is mostly a
story distributed systems tell at conferences. A change's ack is confirmed only
*after* the flush that made its document durable, so the replication slot advances
exactly when the data has landed. Crash before the flush? The batch is redelivered
on restart and re-applied idempotently — same id, same result, no duplicates.

Before going live, the engine runs an optional **backfill**: it asks each sink
whether an index is already seeded and snapshots the tables for the ones that
aren't. Whether a backfill is needed is the *destination's* call, not the source's.

Queue, source, sink, and document builder are all trait objects — so WAL-vs-polling,
stdout-vs-OpenSearch, in-process-channel-vs-durable-broker are all swappable
without touching the engine loop. (Today the menu is short: Postgres in,
OpenSearch out. The seams are there for when it isn't.)

## The CLI

Three subcommands, and **every flag also reads a `FLUSSO_*` env var** (the flag
wins when both are set) — handy for containers.

- **`flusso build`** — compile a config and its schemas into one portable
  `flusso.lock`. No database, no secrets baked in (`{ env = "VAR" }` refs are
  carried through and resolved wherever it runs). Ship one file instead of a tree
  of YAML.
- **`flusso run`** — stream changes through the engine. Bare `run` loads
  `flusso.lock`; `--config` compiles from source and runs that. Credentials are
  resolved here, in the running environment.
- **`flusso check`** — validate the config and print the fully-typed mapping, with
  no database. Drop `--offline` and it also confirms the declared types match the
  live database and grumbles about any that don't.

```sh
flusso --help
flusso build  --config flusso.toml -o flusso.lock   # build the portable artifact
flusso check  --config flusso.toml                  # validate (+ check vs database)
flusso check  --config flusso.toml --offline        # validate without a database
flusso run                                          # run the compiled flusso.lock
flusso run    --config flusso.toml                  # compile from source and run
flusso run    --skip-backfill                       # resume live capture only
```

Logging honors `RUST_LOG` (default `info`); `FLUSSO_LOG_FORMAT=json` for structured
logs. Set the standard `OTEL_EXPORTER_OTLP_ENDPOINT` and traces export there too.
Every environment variable flusso reads — secrets, the `FLUSSO_*` flags, telemetry
— is collected in one place: [`CONFIG.md`](CONFIG.md).

## `just` commands

Common workflows are wrapped in the [`justfile`](justfile). Run `just` for the
full menu; the greatest hits:

| Recipe | Does |
| --- | --- |
| `just up` / `just down` / `just reset` | Start / stop / wipe-and-restart the dev stack |
| `just check` / `just check-offline` | Validate config + schemas (with / without a DB) |
| `just run` / `just run-live` | Backfill + follow / resume live only |
| `just build-lock` | Compile a portable `flusso.lock` |
| `just demo` | Run flusso *inside* the cluster — no host toolchain |
| `just status` / `just metrics` / `just eta` | Live status / raw metrics / backlog drain ETA |
| `just psql` / `just grafana` | psql shell / open the Grafana dashboard |
| `just test` / `just test-all` / `just doc` | Fast tests / + Postgres e2e / doctests |
| `just lint` / `just fmt` / `just ci` | Lint / format / the full local CI gate |

## Requirements

flusso doesn't own Postgres or OpenSearch — it's a guest in both. A few things
have to be true *before* it can run. The [`dev/`](dev/) stack sets all of this up
for you; here's what you'd replicate against your own infrastructure. Full
per-source/per-sink options are in [`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md).

**Postgres (the source):**

- **PG 14+** with **`wal_level = logical`** (a restart-required setting), and
  `max_wal_senders` / `max_replication_slots` high enough for flusso plus any
  other consumers.
- **A publication that already exists** and covers *every* table any index reads
  — root tables *and* every table a join or aggregate pulls from. flusso won't
  create it (which tables to stream is your schema decision):
  `CREATE PUBLICATION flusso FOR TABLE …`.
- **A replication slot** — but this one flusso creates for you on first connect.
  Heads-up: Postgres hoards WAL until flusso confirms it, so a flusso that's down
  for a long time means WAL piling up on the server. Drop the slot when you retire
  a deployment, unless you're a fan of disk-full pages.
- **Row identity on every replicated table** — a primary key (usual case) or an
  explicit `REPLICA IDENTITY`. Keyless tables can't be addressed, so flusso skips
  them in backfill and errors on a live change it can't key.
- **A role with `REPLICATION` + `SELECT`** on the published tables, via the usual
  `postgres://user:pass@host:port/db` URL.

**OpenSearch (the sink):**

- **OpenSearch 2.x** (also speaks Elasticsearch 7.x on the query side via
  [`flusso-search`](CLIENT.md)).
- **A reachable HTTP(S) endpoint** as the sink `url`. Optional HTTP basic auth
  (`username` / `password`); `tls_verify` defaults to `true` — flip it off only
  for self-signed dev clusters.
- **A user that can manage and write flusso's indexes** plus the small hidden
  `flusso_meta` index where it records "this one's seeded". flusso owns the index
  lifecycle: it derives a strict typed mapping per schema and names each index from
  a hash of that schema (`users_<hash>`), so a structural change rolls onto a fresh
  index and re-seeds instead of fighting a mismatched one. The plain logical name
  (`users`) is kept as an alias on the current index, so you can always query it
  without knowing the hash.

## Deploying it

- **Container image** — the [`Dockerfile`](Dockerfile) builds a registry-ready,
  config-less image (you mount a config or bake your own `flusso.lock`). It also
  has a `demo` target with the dev config baked in, which is what `just demo`
  runs.
- **Kubernetes** — the [Helm chart](deploy/helm/flusso/) deploys flusso as a
  single instance (it consumes one replication slot, so it's *firmly* a party of
  one) with config via ConfigMap, secrets via env, a Service, and an optional
  Prometheus `ServiceMonitor`. See [its README](deploy/helm/flusso/README.md).

## Docs

| Doc | What's in it |
| --- | --- |
| [`SCHEMA.md`](SCHEMA.md) | Every config + schema key: field types, joins, aggregates, filters, validation rules |
| [`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md) | Every source and sink option — batch sizes, retries, analysis modes |
| [`CONFIG.md`](CONFIG.md) | Every environment variable in one place — secrets, `FLUSSO_*` flags, logging & telemetry |
| [`CLIENT.md`](CLIENT.md) | `flusso-search`, the typed query-side client and its `#[derive(FlussoDocument)]` |
| [`dev/README.md`](dev/README.md) | The dev stack walk-through |
| [`deploy/helm/flusso/README.md`](deploy/helm/flusso/README.md) | The Helm chart |
| [`CLAUDE.md`](CLAUDE.md) | Architecture + contributor notes (also where the AI takes its instructions) |

## Project layout

Crates live under `libs/` and `apps/`. The **numeric prefix is the dependency
layer** — a crate only depends on lower-numbered ones.

| Crate | Path | Role |
| --- | --- | --- |
| `schema` | `libs/0-schema` | Config entry point. `load()` reads a config + its schemas into one validated `Config`. |
| `schema-core` | `libs/0-schema/0-core` | The validated domain model every other crate produces and consumes. |
| `schema-config-toml` | `libs/0-schema/1-config-toml` | Parses `flusso.toml` → core types. |
| `schema-index-yaml` | `libs/0-schema/1-index-yaml` | Parses `*.schema.yml` → core types. |
| `queue-core` | `libs/1-queue/0-core` | The work-queue abstraction, generic over the payload. |
| `queue-channel` | `libs/1-queue/1-channel` | In-process queue over a bounded `tokio` mpsc channel. |
| `sources-core` | `libs/1-sources/0-core` | Source abstractions: `cdc` (*what changed?*) and `document` (*what to build?*). |
| `sources-postgres` | `libs/1-sources/1-postgres` | Postgres source: WAL capture, backfill, document building. |
| `sinks-core` | `libs/1-sinks/0-core` | The `Sink` trait, JSON rendering, and a fan-out sink. |
| `sinks-stdout` | `libs/1-sinks/1-stdout` | Writes each operation to stdout (NDJSON or pretty). |
| `sinks-opensearch` | `libs/1-sinks/2-opensearch` | OpenSearch sink: bulk API, typed mappings, hashed index names + latest-alias, seeding markers. |
| `engine` | `libs/2-engine` | The sync engine — capture → queue → build → sink, batched, at-least-once. |
| `daemon` | `libs/3-daemon` | Wires a `Config` into a running pipeline and exposes live `Status`. |
| `flusso-cli` | `apps/cli` | The `flusso` binary: transport, telemetry, signals. |

Each config-format crate works in two stages — **parse** (serde → permissive
entity types, unknown fields rejected) then **convert** (lift into `schema-core`,
apply the rules the format can't express). Secrets are *not* resolved here, so a
compiled config never carries a secret it wasn't literally given.

## Testing & development

Tests run with [`cargo-nextest`](https://nexte.st)
(`cargo install cargo-nextest --locked`):

```sh
just test        # fast: unit + parse/convert, no external deps
just test-all    # + Postgres e2e (needs Docker; spins up containers via testcontainers)
just doc         # doctests — nextest doesn't run these
just ci          # the full local gate: lint → e2e → doctests
```

The e2e tests are `#[ignore]`d by default and legitimately slow/flaky, so
`.config/nextest.toml` caps their concurrency and retries them. A few crates also
carry [Criterion](https://bheisler.github.io/criterion.rs/) benchmarks
(`cargo bench`) — the engine, the OpenSearch sink, and the Postgres source.

Editor support: point your editor at `schemas/config.schema.json` and
`schemas/index.schema.yml` for completion and inline validation — the bundled
example schemas already reference them via a `yaml-language-server` modeline.

Want to hack on it? [`CLAUDE.md`](CLAUDE.md) has the architecture tour and the
house rules (the lints are strict — no `unwrap`, no `println!`, and yes, they fail
the build).
