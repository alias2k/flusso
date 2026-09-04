---
name: flusso-integrate
description: Stand flusso up in a project, or migrate an existing search setup onto it. Use for a first-time setup, wiring flusso into a deployment, or replacing a hand-rolled indexer.
---

# Integrating flusso

flusso keeps OpenSearch in sync with Postgres from declarative config: describe a search document in
YAML, flusso derives the index mapping, seeds it, then follows logical replication so the index stays
current. No imperative setup.

This skill is the integration path. **flusso-schema** owns the field syntax and editor wiring,
**flusso-postgres** owns what the database must provide, **flusso-opensearch** owns what the sink
produces. Don't re-derive any of those here.

## Prerequisites

1. **`wal_level = logical`** on the Postgres server. Needs a restart. This is the one prerequisite
   that blocks everything.
2. **OpenSearch reachable.** flusso creates the index with `dynamic: strict` and owns its mapping.
3. **The `flusso` binary**, or `cargo run --` from a checkout, or the container image.

flusso creates the replication slot itself on first connect, and by default it also
**creates or extends the publication** for the tables your schemas read. When the source role lacks
the privileges, it prints the exact SQL and carries on rather than failing. See **flusso-postgres**
for the privilege floor and what to do when the role can't manage the publication.

## Zero to syncing

```
1. Write flusso.toml         — source + sink(s) + one [[index]]
2. Write <index>.schema.yml  — root table + fields (flusso-schema)
3. flusso check              — validate config + schemas (--offline if no DB)
4. flusso run                — backfill, then follow live
```

> **Shortcut for steps 1-2:** once `[source]` is set, `flusso design --config flusso.toml` opens a
> database-aware web UI that authors both files. Pick tables and columns from the live DB, preview
> the document, save, then resume at step 3.

### 1. `flusso.toml`

```toml
[source]
type = "postgres"
connection_url = { env = "PG_URL" }   # or a literal postgresql://… URL; SOURCE_POSTGRES_CONNECTION_URL overrides either

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[[index]]
name = "users"
schema = "users.schema.yml"   # resolved relative to this file
enabled = true
```

TLS negotiation, including managed providers and mTLS, is in **flusso-postgres**. Sink keys are in
**flusso-opensearch**.

Define multiple `[sinks.<name>]` and flusso **fans out**, so every document lands in each. With no
sinks it falls back to a stdout sink. A `stdout` sink (`type = "stdout"`, optional `pretty = true`)
alongside OpenSearch is the fastest way to *see* documents while integrating.

Once `flusso.toml` exists, **offer** to wire editor validation for it (the `.taplo.toml` rule in
**flusso-schema**) and add it only if the user agrees.

### 2. The index schema

One `*.schema.yml` per `[[index]]`. Field syntax is **flusso-schema**; the minimum is a `version`, a
root `table`, a `primary_key`, and `fields`.

### 3. Validate

```sh
flusso check --config flusso.toml            # validates + prints the typed mapping
flusso check --config flusso.toml --offline  # format and rules only, skip the DB
```

Against a live DB this also confirms each declared type and nullability against the real columns,
and prints the publication coverage report. Fix every error here before running.

### 4. Run

```sh
flusso run --config flusso.toml                                   # backfill unseeded, then follow
flusso run --config flusso.toml --public-address 127.0.0.1:9464   # also serve /metrics /status
flusso run --config flusso.toml --skip-backfill                   # resume live capture only
```

flusso decides backfill per index: it ensures every mapping, asks each sink whether the index is
seeded, snapshots root tables for the unseeded ones, then follows the WAL. At-least-once, so the slot
advances past a change only once its documents are durable.

## Ship a portable artifact

```sh
flusso build --config flusso.toml -o flusso.lock   # config + every schema inlined; no secrets baked
flusso run                                          # loads flusso.lock by default
```

The lock carries `{ env = … }` as references, so one artifact runs in any environment that supplies
the secrets.

It is deterministic, generated-only TOML, so commit it and the diffs stay reviewable. A rebuild from
unchanged inputs is byte-identical, and `flusso run` skips the rewrite when nothing changed. The file
formats are **frozen for the major**: a config, schema, or lock that one release accepts keeps
loading on every later release in that major.

## Operational surface

`--public-address` serves unauthenticated `/healthz`, `/readyz`, `/status`, `/metrics`. A private
surface (`--private-address`, HTTP Basic) serves `/indexes` and `/reindex`; `flusso indexes` and
`flusso reindex` are its clients.

## Migrating from a hand-rolled indexer

**A migration reproduces the existing document, it does not redesign it.** The target shape is
whatever the project indexes and queries today.

1. **Find the existing document definition first** — the mapping, the indexed struct, the serializer.
   That is the spec. Map each existing search document to **one** `*.schema.yml`.
2. **Carry every field across, above all the `id`.** Dropping a field silently changes the document
   contract and breaks consumers. If one genuinely can't be mapped, surface it and ask.
3. **Edit the existing code in place.** The read-side procedure, including why a parallel "v2" struct
   is wrong, is `${CLAUDE_PLUGIN_ROOT}/skills/flusso-query/migration.md`.
4. **Let flusso own the index.** Drop the bespoke mapping. Read **flusso-opensearch** for which
   subfield to query, since the derived mapping adds analyzers and subfields yours may not have had.
5. `flusso check`, then backfill into a fresh index alongside the old one and cut the read path over
   once seeded.
6. Retire the CDC or cron glue. Logical replication replaces it.

## Before you call it done

1. `wal_level = logical`, and `flusso check` reports full publication coverage.
2. `flusso check --config flusso.toml` passes against the real DB.
3. A `flusso run` backfill completes and a live change propagates (watch `/status` or a stdout sink).
