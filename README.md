# flusso

> [!IMPORTANT]
> ## 🤖 Generative AI disclosure
>
> **Generative AI was used in this project to produce boilerplate and
> documentation.** Every single line of code has been manually reviewed and
> revised by a human software developer.

Keep OpenSearch in sync with Postgres, driven by declarative config.

You describe what a search document should look like — its columns, the related
tables that fold into it, the field mappings — in YAML. flusso builds the index
from that description, seeds it from the existing rows, then follows Postgres'
logical replication stream so the index stays current as the source changes.

## How it works

A deployment is described by two kinds of files.

- **`config.toml`** — one per deployment. Declares the Postgres source, the
  sinks to write to, and the list of indexes to build.
- **`*.schema.yml`** — one per index. Describes a single search document: the
  root table, its fields, and how related tables join or aggregate into it.

```toml
# config.toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

# Sinks fan out: write the same documents to more than one destination.
[sinks.audit]
type = "stdout"
pretty = true

[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true
```

```yaml
# users.schema.yml
version: 1
table: users
primary_key: id

# Setting users.deleted = true emits a tombstone (delete) instead of an upsert.
soft_delete:
  column: deleted

fields:
  - id
  - field: email
    type: keyword
    transforms: [lowercase, trim]

  # Pull each user's orders in as a nested array. A join is structural — its
  # `nested` shape is implied, so it declares no `type`. Its `primary_key` is
  # the related table's, and its `fields` are projected from each related row.
  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      primary_key: id
      order_by: [{ column: created_at, direction: desc }]
      limit: 5
      fields:
        - id
        - field: total
          type: double
        - field: status
          type: keyword

  # A `count` is always a `long`, so it needs no `type`.
  - field: orderCount
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
```

Every scalar field declares its **type** from a fixed set
([`SCHEMA.md`](SCHEMA.md) lists them) that bridges a Postgres column type and an
OpenSearch mapping. That makes a schema self-describing: flusso derives the full
index mapping — and validates a config — without a database.

A change to a user — *or* to one of their orders — rebuilds the whole `users`
document and re-emits it. flusso resolves which documents a changed row affects,
reassembles each, and writes it to the configured sink(s) by a deterministic id.

For the full configuration format — every key, join/aggregate/filter shape, and
validation rule — see [`SCHEMA.md`](SCHEMA.md). For the supported source and sink
types and their options, see [`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md).

## The pipeline

The engine wires pluggable edges together and runs:

```text
ChangeCapture ─▶ queue ─▶ resolve ─▶ build ─▶ Sink ─▶ flush ─▶ ack
```

A **capture** task drains the source's change stream into a bounded in-process
queue (back-pressure: capture blocks when the queue is full). A **worker** pulls
changes, resolves the affected document ids, assembles each document, and writes
it to the sink. Writes are **batched** — up to N changes, or whatever arrives
within a short delay window, flush together as one bulk round-trip.

Delivery is **at-least-once**. The source ack for a change is confirmed only
*after* the flush that made its documents durable, so the replication slot
advances past a change exactly when its documents have landed downstream. A
crash before the flush leaves the batch unconfirmed; it's redelivered on restart
and re-applied idempotently (documents are rebuilt from the current row and
written by deterministic id).

Before live capture, the engine runs an optional **backfill**: it asks each sink
whether an index is already seeded and, for those that aren't, snapshots the
root tables through the same path before following live changes. "Is a backfill
needed?" is the destination's call, not the source's.

The queue, source, sink, and document builder are all trait objects, so the
backend choices (WAL vs polling, stdout vs OpenSearch, in-process channel vs a
durable broker) are swappable without touching the engine loop.

## Running it

The `dev/` directory has a complete, runnable example: a docker-compose stack
(Postgres configured for logical replication + OpenSearch + Dashboards), seeded
data, and a matching config and schema.

```sh
# 1. Bring up Postgres + OpenSearch (init scripts seed data + publication).
docker compose up -d
docker compose ps                         # wait for both to be "healthy"

# 2. Validate the config and schemas first — that every table, column, and key
#    the schemas name resolves against the database. Prints the resolved mapping.
cargo run -- check --config dev/config.toml

# 3. Start flusso: it creates the replication slot if needed, backfills
#    OpenSearch, then follows live changes. Logs go to stderr.
cargo run -- run --config dev/config.toml

# 4. In another terminal, make changes and watch them stream through.
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
curl -s http://localhost:9200/users/_search?pretty
```

See [`dev/README.md`](dev/README.md) for the full walk-through (resetting state,
inspecting the slot/publication, OpenSearch Dashboards at
http://localhost:5601).

The CLI has three subcommands:

- `flusso compile` reads a config and the schemas it references and writes the
  whole validated configuration to a single portable binary artifact
  (`flusso.bin` by default). No database is needed — schemas are self-describing
  — and no secret is baked in: `{ env = "VAR" }` references are carried through
  and resolved wherever the artifact runs. This is what lets a deployment ship
  one file instead of a tree of YAML.
- `flusso run` streams changes through the engine. With no `--config` it loads
  the compiled artifact (`--artifact`, default `flusso.bin`); with `--config` it
  compiles the source afresh and runs that. The connection and credentials are
  resolved here, in the running environment.
- `flusso check` validates the config and prints the fully-typed mapping — each
  field's type and nullability — derived from the schema with no database.
  Unless `--offline`, it also connects and confirms the declared types and
  nullability agree with the live database, reporting any disagreement.

```sh
flusso --help
flusso compile --config config.toml -o flusso.bin  # build the portable artifact
flusso check   --config config.toml                # validate (+ check vs database)
flusso check   --config config.toml --offline      # validate without a database
flusso run                                          # run the compiled flusso.bin
flusso run --config config.toml --slot flusso       # compile from source and run
flusso run --skip-backfill                          # resume live capture only
```

Logging honors `RUST_LOG` (default `info`). Set `FLUSSO_LOG_FORMAT=json` for
structured logs. When an OTLP endpoint is configured via the standard
OpenTelemetry env vars (`OTEL_EXPORTER_OTLP_ENDPOINT`), spans are also exported
to that collector.

## Layout

Crates live under `libs/` and `apps/`. The numeric prefix is the dependency
layer — a crate only depends on lower-numbered ones.

| Crate | Path | Role |
| --- | --- | --- |
| `schema` | `libs/0-schema` | Config entry point. `load()` reads a config and its schemas into one validated `Config`. |
| `schema-core` | `libs/0-schema/0-core` | The validated domain model — the types every other crate produces and consumes. |
| `schema-config-toml` | `libs/0-schema/1-config-toml` | Parses `config.toml` and converts it into core types. |
| `schema-index-yaml` | `libs/0-schema/1-index-yaml` | Parses `*.schema.yml` and converts it into core types. |
| `queue-core` | `libs/1-queue/0-core` | The work-queue abstraction (`Producer` / `Consumer` / `AckHandle`), generic over the payload. |
| `queue-channel` | `libs/1-queue/1-channel` | In-process queue over a bounded `tokio` mpsc channel (back-pressure; ack is a no-op). |
| `sources-core` | `libs/1-sources/0-core` | Source abstractions: `cdc` (*what changed?*) and `document` (*what to build?*). |
| `sources-postgres` | `libs/1-sources/1-postgres` | Postgres source: WAL logical-replication capture, backfill snapshots, and the document builder. |
| `sinks-core` | `libs/1-sinks/0-core` | The `Sink` trait, JSON rendering, and a fan-out sink that writes to several at once. |
| `sinks-stdout` | `libs/1-sinks/1-stdout` | Writes each operation to stdout as a JSON envelope (NDJSON or pretty). |
| `sinks-opensearch` | `libs/1-sinks/2-opensearch` | OpenSearch sink: bulk API, explicit typed mappings, hashed physical index names, seeding markers. |
| `engine` | `libs/2-engine` | The sync engine — drives capture → queue → build → sink with batching and at-least-once acks. |
| `flusso-cli` | `apps/cli` | The `flusso` command-line binary that wires a config into a running pipeline. |

## Parsing

Each config-format crate works in two stages, which keeps the on-disk format
separate from the model the rest of the system relies on:

1. **Parse** — `serde` deserializes the file into permissive *entity* types that
   mirror the format one-to-one. Unknown fields are rejected.
2. **Convert** — the entities are lifted into `schema-core` types and the rules
   the format can't express on its own are applied: identifier validation, join
   and aggregate arity, declared-type placement, and filter value shapes. Secrets
   are **not** resolved here — a `{ env = "VAR" }` reference becomes a deferred
   `Secret` and is read in the environment that runs the pipeline, so a compiled
   config carries no secret it wasn't given literally.

## Testing

Tests run with [`cargo-nextest`](https://nexte.st). Install it with
`cargo install cargo-nextest --locked` (or a prebuilt binary from the site).

```sh
# Fast tests — unit + parsing/conversion. No external dependencies.
cargo nextest run

# Everything, including the Postgres e2e tests. These are #[ignore]d by
# default and need a running Docker daemon (they spin up Postgres via
# testcontainers).
cargo nextest run --run-ignored all

# Doctests — nextest does not run these, so run them separately.
cargo test --doc
```

Configuration lives in `.config/nextest.toml`. The container-backed e2e tests
(`sources-postgres`'s `integration`, `wal`, and `config_coverage` binaries) are
grouped so only a few of their Postgres containers come up at once, and they
retry transient failures. CI uses the `ci` profile
(`cargo nextest run --profile ci --run-ignored all`).

Several crates carry [Criterion](https://bheisler.github.io/criterion.rs/)
benchmarks (`cargo bench`) — the engine pipeline, the OpenSearch sink, and the
Postgres source.

## Editor support

`schemas/config.schema.json` and `schemas/index.schema.yml` are JSON Schemas for
the two formats. Point an editor at them for completion and inline validation;
the bundled example schemas already reference them through a
`yaml-language-server` modeline.

For the prose reference — every key and join/aggregate/filter shape, with
examples and validation rules — see [`SCHEMA.md`](SCHEMA.md), and
[`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md) for the source and sink types.
