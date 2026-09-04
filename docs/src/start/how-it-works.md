# How flusso works

flusso turns a declarative schema into an OpenSearch index it owns end to end: it derives the mapping, seeds it, then follows the Postgres WAL so every change rebuilds the documents it touches.

## The model

Two files describe a deployment. `flusso.toml` names one source database, one or more sinks, and the indexes to build. Each index has a `*.schema.yml` describing one search document: a root table, typed fields, and related tables folded in as objects, nested arrays, or rollups.

```text
flusso.toml ──▶ [source]  Postgres
            ├─▶ [sinks]   OpenSearch (+ optional stdout)
            └─▶ [[index]] users  ──▶ users.schema.yml
                [[index]] orders ──▶ orders.schema.yml
```

Because every leaf declares its type, the OpenSearch mapping is a pure function of the schema. `flusso check --offline` prints it with no database, and the query-side derive resolves the same mapping at compile time.

## Documents are assembled, not copied

A document is not a row. It is the root row plus whatever the schema folds in: a `belongs_to`/`has_one` becomes an object, a `has_many`/`many_to_many` a nested array, a `count`/`sum`/`avg`/`min`/`max`/`ids` a scalar rollup. flusso generates one server-side SQL query per document, so assembling a user with fifty orders is one round-trip, not fifty-one.

The reverse direction is what makes sync possible. When a row changes, flusso resolves it *back* to the document ids it affects: a changed order row maps to its user's document through the join's key. Every affected document is rebuilt from the current rows and written by a deterministic id. Nothing is patched in place.

## The pipeline

```text
ChangeCapture ─▶ ingest engine ─▶ lane per sink ─▶ sink engine ─▶ apply ─▶ flush ─▶ ack
                 resolve · build once · publish                                     └▶ watermark ─▶ slot
```

An **ingest engine** follows Postgres logical replication, resolves each change to the document ids it affects, deduplicates them, builds each document **once** per batch, and publishes the batch onto one lane per sink. A **sink engine** per sink pulls from its lane, applies the documents, flushes them as one bulk request, and acknowledges. Sinks are independent: each seeds itself, a reindex targets one sink, and one sink's outage stops only that sink.

Delivery is **at-least-once**. Each lane's acknowledgements feed a watermark, and the replication slot advances only to the lowest position **every** sink has flushed. A crash before a flush redelivers that sink's batch; the rebuild is idempotent, so redelivery is harmless.

## flusso owns the index

The sink creates each index from the derived mapping with `dynamic: strict`. Its addressable name is `{logical}_{hash}`, where the hash comes from the schema, and that name is an alias over a numbered *generation* index. A structural schema change moves the hash, so the new shape lands in a fresh index and is re-seeded instead of colliding with the old mapping. The bare logical name (`users`) is kept as a convenience alias for humans and dashboards.

A [reindex](../operate/reindex.md) builds the next generation behind the same alias while the current one keeps serving reads, then swaps atomically.

## Seeding and continuity

On start, flusso asks the sink whether each index is already seeded. Seeded state lives in a small `flusso_meta` index. Unseeded indexes are backfilled from a snapshot of their root tables through the same pipeline, then marked seeded.

A seed is only as good as the stream behind it. If the replication slot is missing at start (the database was replaced, or someone dropped the slot), every change since the last seed is unobservable, so flusso rebuilds every seeded index into a fresh generation before backfilling. The rebuild is staged *before* the slot is created, so a crash in between comes back to the same decision. Dropping the slot is therefore the supported way to force a full rebuild. See [Recover from a dropped slot](../operate/dropped-slot.md).

## When the destination says no

A flush-wide failure (OpenSearch unreachable, the whole request refused) always stops the run; the batch is redelivered on restart. An **item-level rejection** (one document with a mapping conflict) is decided by the `on_error` policy: `stop` halts the run, `skip` quarantines the document, counts it, and lets the batch through. See [Handle rejected documents](../operate/rejected-documents.md).

## Where this shows up

- The two files: [Write flusso.toml](../deploy/flusso-toml.md), [Your first schema](../author/first-schema.md)
- The pipeline and its invariants in code: [The pipeline](../contribute/pipeline.md)
- Index naming, aliases, generations: [Sink: OpenSearch](../reference/sink-opensearch.md)
- The nouns used above: [Glossary](../reference/glossary.md)
