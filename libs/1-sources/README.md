# 1-sources — the source layer

Where changes come from: `0-core` is the source abstractions, the higher-numbered crates are concrete backends. Postgres is the only backend today.

| Crate | Path | Role |
| --- | --- | --- |
| sources-core | [0-core](0-core) | The source abstractions — `ChangeCapture` (what changed), `DocumentBuilder` (what to build), `CaptureProvisioning` — and the shared primitives every backend implements. |
| sources-postgres | [1-postgres](1-postgres) | The Postgres logical-replication backend: WAL capture, backfill, document assembly. |

Part of [the flusso library crates](../README.md).
