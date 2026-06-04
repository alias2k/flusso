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
    mapping: { type: keyword }
    transforms: [lowercase, trim]

  # Pull each user's orders in as a nested array.
  - field: orders
    mapping: { type: nested }
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      order_by: [{ column: created_at, direction: desc }]
      limit: 5
    fields: [id, total, status]

  - field: orderCount
    mapping: { type: integer }
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
```

A change to a user — *or* to one of their orders — rebuilds the whole `users`
document and re-emits it. flusso resolves which documents a changed row affects,
reassembles each, and writes it to the configured sink(s) by a deterministic id.

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

# 2. Start flusso: it creates the replication slot if needed, backfills
#    OpenSearch, then follows live changes. Logs go to stderr.
cargo run -- --config dev/config.toml

# 3. In another terminal, make changes and watch them stream through.
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
curl -s http://localhost:9200/users/_search?pretty
```

See [`dev/README.md`](dev/README.md) for the full walk-through (resetting state,
inspecting the slot/publication, OpenSearch Dashboards at
http://localhost:5601).

The binary takes the source connection from the config; the replication slot and
publication names, queue capacity, backfill skip, and output formatting are CLI
flags:

```sh
flusso --help
flusso --config config.toml --slot flusso --publication flusso
flusso --config config.toml --skip-backfill   # resume live capture only
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
2. **Convert** — `TryFrom` lifts those entities into `schema-core` types and
   applies the rules the format can't express on its own: identifier validation,
   join and aggregate arity, filter value shapes, and environment-variable
   resolution for secrets.

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
