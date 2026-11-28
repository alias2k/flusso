# Glossary

The project's nouns, each with a pointer to the page that owns it.

| Term | Means | Home |
| --- | --- | --- |
| **backfill** | Seeding an unseeded index by reading its root keys, a page at a time, and building each through the normal pipeline beside live capture. | [How flusso works](../start/how-it-works.md#seeding-and-continuity) |
| **applied** | A fence level: the sink's write for a change, and every earlier one, has returned, so a fetch by id sees it. | [HTTP endpoints](http.md#get-waitpositionposition) |
| **batch** | One index's changes grouped into one build: everything the stream has ready, committed the moment it would block, capped at `max_changes` / `max_delay_ms`; one build takes at most `max_changes` documents. A sink may merge several ready batches (of several indexes) into one write. | [batch](config-toml.md#batch) |
| **capture gap** | A table an index reads whose changes don't all reach flusso (missing from the publication, or a replica identity short of its key or a parent link), and that `run` won't fix by itself. An error of `check`. | [Checks and findings](checks.md#capture-gaps) |
| **computed field** | A read-time field given by a Rust function over the document and the context. Value only. | [Read-time fields](../query/read-time-fields.md#computed-fields) |
| **context** | The request-time values a root's read-time fields are decided from (the clock, the caller), in one consumer-defined type. | [Read-time fields](../query/read-time-fields.md#the-context) |
| **convenience alias** | The bare logical name (`users`) kept as an alias on the current generation, for humans and dashboards. Best-effort. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **continuity** | Whether the source's resume point (the slot) survived from the last run. `Resumed` or `Fresh`. | [Recover from a dropped slot](../operate/dropped-slot.md) |
| **detached lane** | A lane flusso stopped writing and took out of the slot's minimum, recorded in the sink's meta index: its index stays readable, frozen at the detach. A rebuild or a reattach brings it back. | [Bring back a detached index](../operate/detached-index.md) |
| **document** | One search record, assembled from a root row plus its joins and aggregates, written by a deterministic id. | [How flusso works](../start/how-it-works.md#documents-are-assembled-not-copied) |
| **fence** | A wait until a sink has reached a source position, so a client can read its own write. It only observes. | [Read your own writes](../operate/read-your-writes.md) |
| **write-wide error** | The whole bulk request failed. Always stops the run; the batch is redelivered. | [Index entries and on_error](index-and-on-error.md#on_error) |
| **parked lane** | A lane flusso stopped writing because its index refused a document under `stop` or refuses writes for now; its oldest batch (or a rebuild's seed the old generation's block refuses) is retried alone on the [backoff](config-toml.md#backoff) schedule while the other lanes go on. `/status` shows it `refused` or `blocked`. | [A lane is stalled](../operate/stalled-lane.md) |
| **gap** | The changes a reattach accepted as missing on a lane, from its detach to its attach. A fence inside one is refused; a seed ends it. | [Bring back a detached index](../operate/detached-index.md#what-a-gap-means) |
| **generation** | A concrete OpenSearch index `{logical}_{hash}_{n}` holding data behind the hash alias. Reindex builds `n+1`. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **hash alias** | `{logical}_{hash}`: the addressable name flusso and `flusso-query` use. The hash derives from the schema. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **input-only field** | A field with `emit: false`: fetched and built so scripts can read it, then stripped from the document and the mapping. | [Script fields](scripts.md#emit) |
| **item-level rejection** | The bulk request was applied but one document was refused. Decided by `on_error`. | [Index entries and on_error](index-and-on-error.md#on_error) |
| **lock** | `flusso.lock`: the compiled, deterministic, secret-free deployment file. | [flusso.lock](lock.md) |
| **logical name** | The `name` in an `[[index]]` entry: the index's stable identity across schema changes. | [Index entries and on_error](index-and-on-error.md) |
| **mapping** | The OpenSearch field types derived from a schema, created `dynamic: strict`. | [Field types](field-types.md) |
| **prefix** | A literal prepended to every name flusso owns, so deployments share a cluster. | [flusso.toml top level](config-toml.md#prefix) |
| **publication** | The Postgres object listing which tables are replicated. Must cover every table any index reads. | [Source: Postgres](source-postgres.md#capture) |
| **quarantine** | Skipping a rejected document under `on_error = "skip"`: logged, counted, the batch acked. | [Handle rejected documents](../operate/rejected-documents.md) |
| **read-time field** | A field `flusso-query` decides when a document is read, because it depends on a request-time value. A rule field or a computed field. | [Read-time fields](../query/read-time-fields.md) |
| **reindex** | Rebuilding one index into a fresh generation while the current one serves, then swapping the alias. | [Reindex without downtime](../operate/reindex.md) |
| **reverse resolution** | Mapping a changed related row back to the parent documents it affects. | [Joins](joins.md#what-a-related-change-rebuilds) |
| **root table** | The table a schema's documents are built from; one document per row (subject to root filters). | [Schema top-level keys](schema-top-level.md) |
| **rule field** | A read-time field given by ordered condition → outcome rules over stored fields. Has a value, a filter, a sort and counts, with no script. | [Read-time fields](../query/read-time-fields.md#rule-fields) |
| **script field** | A leaf computed by a Rhai script from the built document (`this` and `root`), typed by its type key like any field. | [Script fields](scripts.md) |
| **searchable** | A fence level: a search at the destination sees the change. Trails applied by the destination's refresh while a backlog drains. | [HTTP endpoints](http.md#get-waitpositionposition) |
| **seed marker** | The `flusso_meta` record saying an index's generation is fully backfilled. | [Sink: OpenSearch](sink-opensearch.md#seeding) |
| **sink** | A destination: OpenSearch or stdout. Each runs its own engine over its own lanes, one per index; `backfill = false` opts a stateless one out of seeding. | [flusso.toml top level](config-toml.md#several-sinks) |
| **slot** | The Postgres logical replication slot flusso consumes. Its confirmed position is the resume point. | [Source: Postgres](source-postgres.md#capture) |
| **slot lag** | Bytes between the WAL head and the slot's confirmed position. | [Metrics](metrics.md) |
| **soft-delete** | A row marked deleted by a column or field rather than a `DELETE`; emits a tombstone. | [Filters and soft_delete](filters-and-soft-delete.md#soft_delete) |
| **source position** | A point in the source's own change order, as a client reads it after a commit (Postgres: `pg_current_wal_lsn()`). What a fence waits for. | [Read your own writes](../operate/read-your-writes.md) |
| **tombstone** | A `delete` operation for a document whose row was deleted, soft-deleted, or left the root filter set. | [Filters and soft_delete](filters-and-soft-delete.md) |
| **type-first** | The schema's field grammar: `- <type>: <name>` plus siblings. | [Schema top-level keys](schema-top-level.md#fields) |
