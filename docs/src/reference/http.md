# HTTP endpoints

`flusso run` serves two listeners. Trust is gated by port, not path: the public surface is unauthenticated and read-only; the private surface is HTTP Basic auth and can change state.

| Surface | Default bind | Auth | Endpoints |
| --- | --- | --- | --- |
| public | `127.0.0.1:9464` | none | `GET /healthz`, `GET /readyz`, `GET /status`, `GET /metrics` |
| private | `127.0.0.1:9465` | HTTP Basic | `GET /indexes`, `POST /reindex` |

Bind addresses: `--public-address` / `--private-address`, then `FLUSSO_PUBLIC_ADDRESS` / `FLUSSO_PRIVATE_ADDRESS`, then `[server]` in `flusso.toml`, then the default. Credentials: `--admin-user` / `--admin-password` or `FLUSSO_ADMIN_USER` / `FLUSSO_ADMIN_PASSWORD`, defaulting to `admin` / `flusso`. They are never config keys. A warning is logged on every start while the password is the default. Both listeners are bound before the pipeline starts, so a bad address fails fast; a serve-loop error is logged and never stops the pipeline.

## GET /healthz

`200` while the process runs. Liveness.

## GET /readyz

`200` when the phase is `backfilling` or `live`; `503` while `starting` or after `stopped`. A stopped pipeline is deliberately not ready.

## GET /status

The live status document as JSON.

| Field | Type | Meaning |
| --- | --- | --- |
| `phase` | `starting` \| `backfilling` \| `live` \| `stopped` | Where the pipeline is. |
| `uptime_seconds` | int | Since the daemon started. Survives a reindex restart. |
| `indexes` | map of name to `pending` \| `backfilling` \| `seeded` | Per-index lifecycle state. Reaching `live` promotes every index to `seeded`. |
| `changes_captured` | int | |
| `changes_committed` | int | |
| `changes_in_flight` | int | Captured minus committed. |
| `documents_built` | int | |
| `documents_quarantined` | int | Non-zero means data is being dropped under `on_error = "skip"`. |
| `batches` | int | |
| `last_flush_micros` | int | Duration of the most recent flush. |
| `slot_lag_bytes` | int or null | Null until the first lag sample. |
| `errors` | int | |
| `last_error` | string or null | |

```json
{
  "phase": "live",
  "uptime_seconds": 412,
  "indexes": { "orders": "seeded", "products": "seeded", "users": "seeded" },
  "changes_captured": 1520,
  "changes_committed": 1520,
  "changes_in_flight": 0,
  "documents_built": 2103,
  "documents_quarantined": 0,
  "batches": 87,
  "last_flush_micros": 14210,
  "slot_lag_bytes": 3211264,
  "errors": 0,
  "last_error": null
}
```

## GET /metrics

Prometheus exposition text. `404` with a note when metrics are disabled. Series are listed in [Metrics](metrics.md).

## GET /indexes

The `indexes` map from `/status`, alone: `{"users": "seeded", …}`. Basic auth required.

## POST /reindex?index=\<name\>

Stage a from-scratch rebuild of one index into a fresh generation. Basic auth required.

| Response | When |
| --- | --- |
| `202 Accepted` | Queued. The run loop stages the new generation and restarts the pipeline; reads stay on the old generation until the swap. Watch `/status` for the index leaving and returning to `seeded`. |
| `400` | Missing `index` parameter, or not a valid index name. |
| `404` | Not a configured index. |
| `503` | The reindex queue is full or the server is shutting down. |

The runbook is [Reindex without downtime](../operate/reindex.md); the CLI wrapper is [`flusso reindex`](cli.md#reindex).
