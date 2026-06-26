# 1-sources — the source layer

`0-core` defines the source abstractions — `ChangeCapture` (what changed),
`DocumentBuilder` (what to build), and `CaptureProvisioning` — while
higher-numbered crates are concrete backends. Postgres is the only backend today.

- [sources-core](0-core) — the source abstractions and shared primitives every backend implements.
- [sources-postgres](1-postgres) — the Postgres logical-replication backend (WAL capture, backfill, document assembly).

Part of [the flusso library crates](../README.md).
