# flusso-sources-postgres

The Postgres logical-replication **source** for flusso: capture row changes over the WAL, backfill existing rows on first seed, and build each index document from the schema's joins and aggregates.

## At a glance

| Trait (`sources-core`) | Role |
| --- | --- |
| `ChangeCapture` | streams thin per-row changes (table + primary key) from a replication slot; confirms progress via an LSN watermark |
| `DocumentBuilder` | resolves which documents a changed row affects, then assembles each one. Also implements `Catalog` (a column's SQL type + nullability, for index validation) |
| `CaptureProvisioning` | reports stream coverage + a privilege verdict for an index's tables, and provisions the gap (create/extend a publication) when allowed |

**Re-exports** (so callers build a capture without depending on `pgwire-replication`
directly): `Lsn`, `ReplicationConfig`, `SslMode`, `TlsConfig`.

> 💡 **Did you know** — flusso only reads the WAL; it never polls your tables, so
> follow-mode load on Postgres stays flat regardless of table size.

## The traits

- **`ChangeCapture`** streams thin per-row changes (table + primary key) from a
  replication slot and confirms progress via an LSN watermark. The slot advances
  only as far as the engine has durably written — at-least-once.
- **`DocumentBuilder`** resolves which documents a changed row affects and
  assembles each one. It also implements `Catalog`, the store-specific half of
  index validation (a column's SQL type + nullability).
- **`CaptureProvisioning`** takes the tables an index reads, reports stream
  coverage and a privilege verdict, and — when allowed — provisions the gap by
  creating or extending a Postgres publication.

## Module layout

- `cdc/` — change capture. Decodes the pgoutput message stream (the
  crate-private decoder, fuzzed via the `fuzzing` feature), tracks relation
  metadata to extract each changed row's key, buffers per transaction and emits
  on commit tagged with the commit LSN, manages the replication slot and
  publication, and snapshots tables for backfill.
- `document/` — document assembly. Resolves a changed row back to the document
  keys it affects (reverse resolution per join kind), generates one server-side
  SQL query per document (nested relations assembled with `json_build_object` /
  `json_agg`, so there's no N+1), and decodes the result into the value tree.
