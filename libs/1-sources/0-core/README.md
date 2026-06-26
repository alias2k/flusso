# flusso-sources-core

The source abstractions for `flusso`.

A source has two **independent** responsibilities, each its own module.
Neither module references the other; the engine is the only thing that
bridges them.

- [`cdc`] — *what changed?* A pluggable change-capture mechanism that yields
  a stream of thin [`Change`](cdc::Change)s and confirms progress via an
  [`Ack`](cdc::Ack). Logical replication (WAL) is the first mechanism;
  polling or triggers can follow.
- [`document`] — *what to build?* Turns a changed row (named by table and
  key) into the target documents it affects, and assembles each one.

Alongside them, [`validate_indexes`] is the source-independent
half of a check every source can offer: a self-describing schema states its
own types, so the mapping is derived without a database, and a reachable
database is used only to confirm the declared types and nullability match
the real columns. A source supplies only the one store-specific piece — a
[`Catalog`] over its column types.

[`CaptureProvisioning`] is a second such offer: given the tables an index
reads ([`SourceSpec::all_tables`]), a source reports whether it can stream
them and — when privileged — provisions the gap (for Postgres, a publication).
Mechanism-neutral, so the daemon/CLI drive it without naming "publication".

Both build on two shared, mechanism-neutral primitives that belong to
neither concern:

- [`RowKey`] — a row's primary key as ordered column/value pairs.
- [`SnapshotTable`] — a schema-qualified table the engine asks a mechanism
  to snapshot when seeding an index.
- [`SourceSpec`] — the source's own view of what to build (the enabled
  indexes and their schemas), a subset of the top-level config translated by
  the composition root so the backend never sees `Config`.
- [`SourceError`] / [`Result`] — the common error type.

Keeping the two abstractions apart means a deployment can mix any change
mechanism with any document builder, and either can be implemented, tested,
or replaced without touching the other.
