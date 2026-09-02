# flusso

flusso keeps an OpenSearch index in sync with Postgres from two declarative files. No cron job, no nightly reindex, no hand-rolled sync script.

## The whole model in one table

| File | One per | Holds |
| --- | --- | --- |
| **`flusso.toml`** | deployment | where rows come from, where documents go, which indexes to build |
| **`*.schema.yml`** | index | what one search document looks like: its root table, its fields, the related tables that fold in |

Every field declares a **type** from a fixed set that bridges a Postgres column and an OpenSearch mapping. A schema is therefore self-describing: flusso derives the full index mapping and validates a deployment **without a database**.

At run time flusso seeds each index from existing rows, then follows Postgres logical replication. Change a user *or one of their orders* and the whole `users` document is rebuilt and re-emitted. The schema says *what*; flusso works out *which* documents a changed row touches.

```yaml
version: 1
table: users
primary_key: id
fields:
  - keyword: email
    required: true
  - has_many: orders          # fold a related table in as a nested array
    table: orders
    foreign_key: user_id
    primary_key: id
    fields:
      - decimal: total
        required: true
  - count: orderCount         # or roll it up
    table: orders
    foreign_key: user_id
```

## Pick your path

| You want to… | Start at |
| --- | --- |
| understand what it does before committing | [How flusso works](how-it-works.md) |
| see it run in ten minutes | [Quickstart](quickstart.md) |
| write a `*.schema.yml` | [Your first schema](../author/first-schema.md) |
| point it at your infrastructure and ship it | [Write flusso.toml](../deploy/flusso-toml.md) |
| run it in production | [Watch it run](../operate/watch-it-run.md) |
| query the index from Rust | [Query overview](../query/overview.md) |
| look up one key, flag, or metric | [Reference](../reference/config-toml.md) |
| change flusso itself | [Architecture](../contribute/architecture.md) |

> ℹ️ **Info** — Every key, flag, environment variable, metric, and endpoint has exactly one home in the **Reference** part. The other parts link there instead of repeating it, so a reference table is never stale relative to a how-to.

The source is on [GitHub](https://github.com/alias2k/flusso); the binary installs with `cargo install flusso-cli` or ships as the `alias2k/flusso` container image.
