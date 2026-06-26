# flusso

flusso keeps an OpenSearch index in sync with Postgres, driven by declarative config — no cron job, no nightly reindex, no hand-rolled reindex script.

## The whole model: two files

A deployment is two kinds of file. That's the mental model.

| File | One per | Holds | Reference |
| --- | --- | --- | --- |
| **`flusso.toml`** | deployment | where data comes from, where it goes, which indexes to build | [Configuring a deployment](guides/configuration.md) |
| **`*.schema.yml`** | index | what one search document looks like — its table, fields, and the related tables that fold in | [Authoring schemas](guides/schema-authoring.md) |

Every field declares its **type** from a fixed set that bridges a Postgres column and an OpenSearch mapping. A schema is therefore self-describing: flusso derives the full index mapping — and validates the config — without touching a database.

Change a user *or one of their orders*, and flusso rebuilds the whole `users` document and re-emits it. It works out which documents a changed row affects, reassembles each, and writes it by a deterministic id — no instructions about *what* to update.

## This manual

- **[Getting started](getting-started.md)** — run flusso in three commands, and what Postgres and OpenSearch need first.
- **[Authoring schemas](guides/schema-authoring.md)** — the `*.schema.yml` format: field types, objects, maps, joins, aggregates, geo, filters, soft-delete, validation.
- **[Configuring a deployment](guides/configuration.md)** — the `flusso.toml` format, every source and sink option, secrets and `{ env = "VAR" }` references, the `FLUSSO_*` flag overrides, the index prefix, and logging/telemetry.
- **[Querying](guides/querying.md)** — the typed query-side client, `flusso-query`, and its `#[derive(FlussoDocument)]`.
- **[Deploying](guides/deploying.md)** — Docker recipes: smallest image, baking or compiling a `flusso.lock`, scoped `.dockerignore`.

The source, the contributor architecture tour, the Helm chart, and the runnable `dev/` example all live in the [repository on GitHub](https://github.com/alias2k/flusso).
