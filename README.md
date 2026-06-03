# flusso

Keep Postgres tables in sync with OpenSearch, driven by declarative config.

You describe what a search document should look like — its columns, the related
tables that fold into it, the field mappings — in YAML. The tool builds the
index from that description and keeps it current as the source changes.

> **Status:** early development. The configuration layer is complete and tested.
> The sync engine, sources, and sinks are not yet implemented.

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

Loading a config resolves and validates both layers in one call:

```rust
let config = schema::load("config.toml")?;
```

## Layout

Crates live under `libs/` and `apps/`. The numeric prefix is the dependency
layer — a crate only depends on lower-numbered ones.

| Crate | Path | Role |
| --- | --- | --- |
| `schema` | `libs/0-schema` | Entry point. `load()` reads a config and its schemas into one validated `Config`. |
| `schema-core` | `libs/0-schema/0-core` | The validated domain model — the types every other crate produces and consumes. |
| `schema-config-toml` | `libs/0-schema/1-config-toml` | Parses `config.toml` and converts it into core types. |
| `schema-index-yaml` | `libs/0-schema/1-index-yaml` | Parses `*.schema.yml` and converts it into core types. |
| `engine` | `libs/2-engine` | The sync engine. Not yet implemented. |
| `core` | `libs/0-core` | Shared primitives. Not yet implemented. |
| `flusso-cli` | `apps/cli` | Command-line binary. Not yet implemented. |

## Parsing

Each format crate works in two stages, which keeps the on-disk format separate
from the model the rest of the system relies on:

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

## Editor support

`schemas/config.schema.json` and `schemas/index.schema.yml` are JSON Schemas for
the two formats. Point an editor at them for completion and inline validation;
the bundled example schemas already reference them through a
`yaml-language-server` modeline.
