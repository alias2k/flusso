# Glossary

The project's nouns, each with a pointer to the page that owns it.

| Term | Means | Home |
| --- | --- | --- |
| **backfill** | Seeding an unseeded index by snapshotting its root table through the normal pipeline, before live capture. | [How flusso works](../start/how-it-works.md#seeding-and-continuity) |
| **batch** | The changes grouped into one sink flush: up to 256 changes or 50 ms, whichever first. | [The pipeline](../contribute/pipeline.md) |
| **convenience alias** | The bare logical name (`users`) kept as an alias on the current generation, for humans and dashboards. Best-effort. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **continuity** | Whether the source's resume point (the slot) survived from the last run. `Resumed` or `Fresh`. | [Recover from a dropped slot](../operate/dropped-slot.md) |
| **document** | One search record, assembled from a root row plus its joins and aggregates, written by a deterministic id. | [How flusso works](../start/how-it-works.md#documents-are-assembled-not-copied) |
| **flush-wide error** | The whole bulk request failed. Always stops the run; the batch is redelivered. | [Index entries and on_error](index-and-on-error.md#on_error) |
| **generation** | A concrete OpenSearch index `{logical}_{hash}_{n}` holding data behind the hash alias. Reindex builds `n+1`. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **hash alias** | `{logical}_{hash}`: the addressable name flusso and `flusso-query` use. The hash derives from the schema. | [Sink: OpenSearch](sink-opensearch.md#index-naming) |
| **item-level rejection** | The bulk request was applied but one document was refused. Decided by `on_error`. | [Index entries and on_error](index-and-on-error.md#on_error) |
| **lock** | `flusso.lock`: the compiled, deterministic, secret-free deployment file. | [flusso.lock](lock.md) |
| **logical name** | The `name` in an `[[index]]` entry: the index's stable identity across schema changes. | [Index entries and on_error](index-and-on-error.md) |
| **mapping** | The OpenSearch field types derived from a schema, created `dynamic: strict`. | [Field types](field-types.md) |
| **prefix** | A literal prepended to every name flusso owns, so deployments share a cluster. | [flusso.toml top level](config-toml.md#prefix) |
| **publication** | The Postgres object listing which tables are replicated. Must cover every table any index reads. | [Source: Postgres](source-postgres.md#capture) |
| **quarantine** | Skipping a rejected document under `on_error = "skip"`: logged, counted, the batch acked. | [Handle rejected documents](../operate/rejected-documents.md) |
| **reindex** | Rebuilding one index into a fresh generation while the current one serves, then swapping the alias. | [Reindex without downtime](../operate/reindex.md) |
| **reverse resolution** | Mapping a changed related row back to the parent documents it affects. | [Joins](joins.md#what-a-related-change-rebuilds) |
| **root table** | The table a schema's documents are built from; one document per row (subject to root filters). | [Schema top-level keys](schema-top-level.md) |
| **seed marker** | The `flusso_meta` record saying an index's generation is fully backfilled. | [Sink: OpenSearch](sink-opensearch.md#seeding) |
| **sink** | A destination: OpenSearch or stdout. Each runs its own engine over its own lane; `backfill = false` opts a stateless one out of seeding. | [flusso.toml top level](config-toml.md#several-sinks) |
| **slot** | The Postgres logical replication slot flusso consumes. Its confirmed position is the resume point. | [Source: Postgres](source-postgres.md#capture) |
| **slot lag** | Bytes between the WAL head and the slot's confirmed position. | [Metrics](metrics.md) |
| **soft-delete** | A row marked deleted by a column or field rather than a `DELETE`; emits a tombstone. | [Filters and soft_delete](filters-and-soft-delete.md#soft_delete) |
| **tombstone** | A `delete` operation for a document whose row was deleted, soft-deleted, or left the root filter set. | [Filters and soft_delete](filters-and-soft-delete.md) |
| **type-first** | The schema's field grammar: `- <type>: <name>` plus siblings. | [Schema top-level keys](schema-top-level.md#fields) |
