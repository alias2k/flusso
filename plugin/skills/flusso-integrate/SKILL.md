---
name: flusso-integrate
description: Integrate flusso into a project or migrate an existing search setup to it — stand up flusso.toml, point it at Postgres + OpenSearch, define a first index, validate, and run. Use when setting up flusso for the first time, wiring it into a deployment, or migrating from a hand-rolled indexer.
---

# Integrating flusso

flusso keeps OpenSearch in sync with Postgres from declarative config: you describe a search document in YAML, flusso derives the index mapping, seeds it, then follows Postgres logical replication so the index stays current. No imperative setup.

For the field/schema syntax itself, lean on the **flusso-schema** skill. This skill is the integration path around it.

## Prerequisites (check these first — they cause the most failures)

1. **Postgres `wal_level = logical`** — required for change capture. Restart needed after setting it.
2. **A Postgres publication exists** — it decides which tables stream. flusso creates the *replication slot* automatically but **not** the publication. Default name `flusso`:
   ```sql
   CREATE PUBLICATION flusso FOR ALL TABLES;   -- or FOR TABLE a, b, c;
   ```
3. **OpenSearch reachable** — flusso owns the index (mapping, analyzers, subfields); it creates indexes with `dynamic: strict`.
4. **`flusso` binary available** — `cargo run -- <cmd>` from the repo, or an installed binary / container image.

## Path: from zero to syncing

```
1. Write flusso.toml         — source + sink(s) + one [[index]]
2. Write <index>.schema.yml  — root table + fields (see flusso-schema)
3. flusso check              — validate config + schemas (add --offline if no DB)
4. flusso run                — backfill, then follow live
```

### 1. `flusso.toml`

```toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }   # or a literal postgresql://… URL

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[[index]]
name = "users"
schema = "users.schema.yml"   # resolved relative to this file
enabled = true
```

Define multiple `[sinks.<name>]` and flusso **fans out** — every document lands in each. No sinks at all → it falls back to a stdout sink. A `stdout` sink (`type = "stdout"`, optional `pretty = true`) alongside OpenSearch is the fastest way to *see* what documents look like while integrating.

Once `flusso.toml` exists, **offer** to wire editor validation for it — a `.taplo.toml` rule pointing at the published config schema (see **flusso-schema**) — but only add it if the user agrees; don't create `.taplo.toml` unprompted.

### 2. The index schema

One `*.schema.yml` per `[[index]]`. Use the **flusso-schema** skill for the field syntax. Minimal:

```yaml
# yaml-language-server: $schema=https://alias2k.github.io/flusso/schemas/v0.3/index.schema.yml
version: 1
table: users
primary_key: id
fields:
  - integer: id
  - keyword: email
    required: true
  - text: name
```

### 3. Validate (no DB writes)

```sh
flusso check --config flusso.toml            # validates + prints the typed mapping
flusso check --config flusso.toml --offline  # format/rules only, skip the DB
```

`check` against a live DB also confirms declared types/nullability against the real columns. Fix every error here before running.

### 4. Run

```sh
flusso run --config flusso.toml                          # backfill unseeded indexes, then follow
flusso run --config flusso.toml --public-address 127.0.0.1:9464   # also serve /metrics /status /healthz
flusso run --config flusso.toml --skip-backfill          # resume live capture only
```

flusso decides backfill per index: on start it ensures every mapping, asks each sink whether the index is seeded, snapshots root tables for the unseeded ones, then follows the WAL. At-least-once: the slot only advances past a change once its documents are durable.

## Ship a portable artifact (recommended for deploy)

```sh
flusso build --config flusso.toml -o flusso.lock   # compile config + every schema inlined; no DB, no secrets baked
flusso run                                          # loads flusso.lock by default; resolves secrets from its own env
```

The lock carries `{ env = … }` refs as references, so the same artifact runs in any environment that supplies the secrets (`DATABASE_URL`, `<SINK>_OPENSEARCH_URL`).

## Operational surface

`flusso run --public-address` serves unauthenticated `/healthz` `/readyz` `/status` `/metrics`. A private surface (`--private-address`, HTTP Basic auth) serves `/indexes` and `/reindex`. The `flusso indexes` / `flusso reindex` subcommands are clients for it.

## Migrating from a hand-rolled indexer

**A migration reproduces the existing document, it does not redesign it.** The target shape is whatever the project already indexes/queries today — preserve it field-for-field.

1. **Find the existing document definition first** (the mapping, the indexed struct, the serializer) and treat it as the spec. Map each existing search document to **one** `*.schema.yml`: root table + the related tables it folds in via joins/aggregates.
2. **Carry every existing field across — including the `id` / primary key.** Do not drop, rename, or omit a field that the current implementation indexes; if the old document has `id`, the schema declares `- <type>: id` (and `primary_key: id`) and the struct keeps its `id` field. Dropping fields silently changes the document contract and breaks consumers. If a field genuinely can't be mapped, surface it and ask — don't quietly leave it out.
3. **Edit the existing code in place; do not create a parallel new struct/module.** Convert the project's current document type to a `#[derive(FlussoDocument)]` projection (see flusso-query), keeping its name, fields, and `serde` renames. A second "v2" struct alongside the original is wrong unless the user explicitly asks for one.
4. Let flusso **own the index**: drop your bespoke mapping; flusso derives a fully-typed one (`dynamic: strict`) with tuned analyzers + subfields. Read `docs/src/guides/configuration.md` "Index analysis & subfields" so you query the right subfield.
5. `flusso check` to confirm the derived mapping matches your data, then run a backfill into a fresh cluster/index alongside the old one, and cut the read path over once seeded.
6. Retire your CDC/cron glue — logical replication replaces it.

## Before you call it done

1. `wal_level = logical` and the publication exist.
2. `flusso check --config flusso.toml` passes against the real DB.
3. A `flusso run` backfill completes and live changes propagate (watch `/status` or a `stdout` sink).
