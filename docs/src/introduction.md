# flusso

**Keep OpenSearch in sync with Postgres, driven by declarative config.**

You write a bit of YAML describing what a search document should look like. flusso
builds the index, seeds it from your existing rows, then tails Postgres' logical
replication stream so the index stays current — no cron job, no nightly reindex, no
hand-rolled `for row in rows: es.index(...)` script you'll regret at 2am.

In short: you describe the *what*, flusso handles the *keep-it-in-sync*.

## The whole mental model: two files

A deployment is two kinds of files.

- **`flusso.toml`** — one per deployment. Where the data comes from, where it goes,
  and which indexes to build. See **[Configuring a deployment](guides/configuration.md)**.
- **`*.schema.yml`** — one per index. What a single search document looks like: its
  table, its fields, and the related tables that fold in. See
  **[Authoring schemas](guides/schema-authoring.md)**.

Every field declares its **type** from a fixed set that bridges a Postgres column and
an OpenSearch mapping, so a schema is self-describing: flusso derives the full index
mapping — and validates your config — without ever touching a database.

The neat part: change a user *or one of their orders* and flusso rebuilds the whole
`users` document and re-emits it. It works out which documents a changed row affects,
reassembles each, and writes it by a deterministic id. You don't tell it what to
update; it works it out.

## This manual

- **[Getting started](getting-started.md)** — run flusso in about five commands, and
  what Postgres and OpenSearch need first.
- **[Authoring schemas](guides/schema-authoring.md)** — the `*.schema.yml` format:
  field types, objects, maps, joins, aggregates, geo, filters, soft-delete,
  validation.
- **[Configuring a deployment](guides/configuration.md)** — the `flusso.toml` format,
  every source and sink option, secrets and `{ env = "VAR" }` references, the
  `FLUSSO_*` flag overrides, the index prefix, and logging/telemetry.
- **[Querying](guides/querying.md)** — the typed query-side client, `flusso-query`,
  and its `#[derive(FlussoDocument)]`.
- **[Deploying](guides/deploying.md)** — Docker recipes: smallest image, baking or
  compiling a `flusso.lock`, scoped `.dockerignore`.

The source, the contributor architecture tour, the Helm chart, and the runnable
`dev/` example all live in the
[repository on GitHub](https://github.com/alias2k/flusso).
