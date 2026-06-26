# flusso-sources-postgres

The Postgres logical-replication **source** for flusso. It captures row
changes from a Postgres database over logical replication (WAL / pgoutput),
backfills existing rows on first seed, and builds each index document from the
schema's joins and aggregates.

It implements the `sources-core` source traits:

- `ChangeCapture` — streams thin per-row changes (table + primary key) from a
  replication slot and confirms progress via an LSN watermark, so the slot
  advances only as far as the engine has durably written (at-least-once).
- `DocumentBuilder` — resolves which documents a changed row affects and
  assembles each one. Also implements `Catalog`, the store-specific half of
  index validation (a column's SQL type + nullability).
- `CaptureProvisioning` — given the tables an index reads, reports stream
  coverage and a privilege verdict, and (when allowed) provisions the gap by
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
  `json_agg`, so there is no N+1), and decodes the result into the value tree.

The relevant `pgwire-replication` types (`Lsn`, `ReplicationConfig`, `SslMode`,
`TlsConfig`) are re-exported so callers can build a capture without depending on
`pgwire-replication` directly.
