# flusso-sources-core

The source abstractions for `flusso` — split into two independent halves, *what changed?* and *what to build?*

## Quick reference

| Item | Role |
| --- | --- |
| [`cdc`] | *What changed?* — pluggable change capture: a stream of [`Change`](cdc::Change)s, progress confirmed via an [`Ack`](cdc::Ack) |
| [`document`] | *What to build?* — turns a changed row (table + key) into the documents it affects, and assembles each |
| [`validate_indexes`] | Source-independent half of the schema/DB type check; a source supplies only a [`Catalog`] over its column types |
| [`CaptureProvisioning`] | Reports whether a source can stream an index's tables and — when privileged — provisions the gap |
| [`RowKey`] | A row's primary key as ordered column/value pairs |
| [`SnapshotTable`] | A schema-qualified table to snapshot when seeding an index |
| [`SourceSpec`] | The source's own view of what to build — the enabled indexes + their schemas, a subset of the top-level config |
| [`SourceError`] / [`Result`] | The common error type |

The two responsibilities each get their own module. Neither references the other; the engine is the only thing that bridges them. Mix any change mechanism with any document builder, and test or replace either without touching the other.

[`cdc`] yields thin [`Change`](cdc::Change)s and confirms progress via an [`Ack`](cdc::Ack). Logical replication (WAL) is the first mechanism; polling or triggers can follow.

[`validate_indexes`] leans on the fact that a self-describing schema states its own types, so the mapping derives without a database. A reachable database is used only to confirm the declared types and nullability match the real columns.

[`CaptureProvisioning`] is mechanism-neutral by design — the daemon and CLI drive it without ever naming "publication" (Postgres's gap-filling mechanism). Given the tables an index reads ([`SourceSpec::all_tables`]), it reports coverage and provisions what's missing.

> ℹ️ **Info** — [`SourceSpec`] is the composition root's gift to the backend: the source crate never sees the assembled `Config`. It gets only the enabled indexes and their schemas, in `schema-core` types.
