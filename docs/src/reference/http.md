# HTTP endpoints

`flusso run` serves two listeners. Trust is gated by port, not path: the public surface is unauthenticated and changes nothing; the private surface is HTTP Basic auth and can change state.

| Surface | Default bind | Auth | Endpoints |
| --- | --- | --- | --- |
| public | `127.0.0.1:9464` | none | `GET /healthz`, `GET /readyz`, `GET /status`, `GET /metrics`, `GET /wait` |
| private | `127.0.0.1:9465` | HTTP Basic | `GET /indexes`, `POST /reindex` |

Bind addresses: `--public-address` / `--private-address`, then `FLUSSO_PUBLIC_ADDRESS` / `FLUSSO_PRIVATE_ADDRESS`, then `[server]` in `flusso.toml`, then the default. Credentials: `--admin-user` / `--admin-password` or `FLUSSO_ADMIN_USER` / `FLUSSO_ADMIN_PASSWORD`, defaulting to `admin` / `flusso`. They are never config keys. A warning is logged on every start while the password is the default. Both listeners are bound before the pipeline starts, so a bad address fails fast; a serve-loop error is logged and never stops the pipeline.

## GET /healthz

`200` while the process runs. Liveness.

## GET /readyz

`200` when the phase is `backfilling` or `live`; `503` while `starting`, `stopping`, or after `stopped`. A stopping or stopped pipeline is deliberately not ready.

## GET /status

The live status document as JSON: the deployment, then one block per sink.

| Field | Type | Meaning |
| --- | --- | --- |
| `pid` | int | The flusso process's id, to stop it by. |
| `phase` | `starting` \| `backfilling` \| `live` \| `stopping` \| `stopped` | Where the deployment is: `starting` until the ingest engine follows the source and every sink has staged (and again while a failed ingest engine restarts), `backfilling` while any sink still seeds, `stopping` from a [stop](cli.md#stopping) until its drain ends, `stopped` once the source stream ended. |
| `uptime_seconds` | int | Since the daemon started. A reindex does not restart anything. |
| `indexes` | map of name to `pending` \| `backfilling` \| `seeded` | Per-index state across sinks: the least advanced sink's state. |
| `changes_captured` | int | Changes the ingest engine pulled from the source. |
| `changes_in_flight` | int | Captured minus committed by the slowest sink. |
| `documents_built` | int | Documents assembled by the ingest engine (once, however many sinks). |
| `slot_lag_bytes` | int or null | Null until the first lag sample. |
| `visibility_wait_seconds` | number or null | How long the source has held a committed transaction that isn't visible yet; every later change waits behind it. Timed from when that transaction was first found invisible, so an ingest engine restart doesn't reset it. Null while nothing waits, and once the source server is refused. See [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby). |
| `applied_position` | string or null | The source position every sink has applied: a fetch by id sees every commit at or before it. The slowest sink's. Null until the source follows live changes. |
| `searchable_position` | string or null | The source position every sink can search. The slowest sink's. |
| `errors` | int | Engine errors, ingest and sink. |
| `last_error` | string or null | |
| `script_failures.<index>.count` | int | Documents of that index that failed to build: a [script failed](scripts.md#failures), or an [`array: true`](field-types.md#array) column held an object. Under either `on_error` policy (a `stop` index counts each retry). The key is absent until one fails. |
| `script_failures.<index>.last` | object | The most recent: `id`, `field` (path, e.g. `variants[2].gross`), `message`, `line` and `column` in the script (null without a position), `at` (RFC 3339). |
| `sinks.<name>.phase` | `starting` \| `backfilling` \| `live` \| `failed` \| `stopped` | `failed` while the daemon waits to restart that sink engine; the others keep running. |
| `sinks.<name>.indexes` | map of name to state | That sink's own view of each index. |
| `sinks.<name>.changes_committed` | int | Changes whose batch this sink wrote and acked. |
| `sinks.<name>.changes_in_flight` | int | Captured minus this sink's committed. |
| `sinks.<name>.envelopes_applied` | int | Documents written to this sink. |
| `sinks.<name>.batches` | int | |
| `sinks.<name>.documents_quarantined` | int | Non-zero means data is being dropped under `on_error = "skip"`. |
| `sinks.<name>.last_flush_micros` | int | Duration of this sink's most recently finished write. |
| `sinks.<name>.applied_position` | string or null | The source position this sink has applied. |
| `sinks.<name>.searchable_position` | string or null | The source position this sink can search. Trails `applied_position` by the destination's refresh while a backlog drains. |

A source position is in the source's own text form: for Postgres, a WAL LSN such as `0/16B3748`, comparable with `pg_current_wal_lsn()`. Both positions keep moving while the watched tables are idle, because the source follows unrelated WAL too.

```json
{
  "pid": 48213,
  "phase": "live",
  "uptime_seconds": 412,
  "indexes": { "orders": "seeded", "products": "seeded", "users": "seeded" },
  "changes_captured": 1520,
  "changes_in_flight": 0,
  "documents_built": 2103,
  "slot_lag_bytes": 3211264,
  "visibility_wait_seconds": null,
  "applied_position": "0/16B3748",
  "searchable_position": "0/16B3748",
  "errors": 0,
  "last_error": null,
  "sinks": {
    "primary": {
      "phase": "live",
      "indexes": { "orders": "seeded", "products": "seeded", "users": "seeded" },
      "changes_committed": 1520,
      "changes_in_flight": 0,
      "envelopes_applied": 2103,
      "batches": 87,
      "documents_quarantined": 0,
      "last_flush_micros": 14210,
      "applied_position": "0/16B3748",
      "searchable_position": "0/16B3748"
    }
  }
}
```

## GET /metrics

Prometheus exposition text. `404` with a note when metrics are disabled. Series are listed in [Metrics](metrics.md).

## GET /wait?position=\<position\>

Hold the request until a commit is readable: a fence. `position` is what the client read from the source after its commit; for Postgres, the result of `SELECT pg_current_wal_lsn()`. The answer comes once every source commit at or before it has reached the requested level, or the timeout passes.

| Parameter | Type | Default | Meaning |
| --- | --- | --- | --- |
| `position` | string | — | The source position to wait for, in the source's own text form. |
| `until` | `applied` \| `searchable` | `searchable` | `applied`: the sink's writes up to it returned, so a fetch by id sees the commit. `searchable`: a search sees it too. |
| `sink` | string | none | Wait for this sink only. Omitted, the wait covers every sink, so the slowest one decides. |
| `timeout_ms` | int | `5000` | How long to hold the request. Values above `60000` are cut to `60000`. |

| Response | When |
| --- | --- |
| `200` | Reached. |
| `408` | The timeout passed first. Nothing is wrong with the request; ask again or give up. |
| `400` | `position` is missing or not a position the source can read, or `until`, `sink`, or `timeout_ms` is malformed. |
| `404` | `sink` is not a configured sink. |
| `503` | This process can't answer now. `Retry-After: 1`, and `reason` says why: `starting` (the source isn't following live changes yet: startup, or the ingest engine is restarting) or `stopping` (the [stop](cli.md#stopping)'s drain is over). |

`200` and `408` share a body; errors carry `{"error": "…"}`, a `503` also `"reason"`.

```json
{ "reached": true, "position": "0/16B3780", "until": "searchable", "sink": "primary" }
```

`position` is how far the sink has got (the slowest one, without `sink`): at or past the one asked for on `200`, short of it on `408`.

- **It only waits.** Nothing is flushed or refreshed on a caller's behalf. Under a draining backlog `searchable` trails `applied` by up to the sink's [`refresh_interval`](sink-opensearch.md#refresh).
- **A backfill isn't covered, nor an index mid-reindex.** Positions belong to live changes; an index that is still seeding, or being rebuilt behind its alias, is reported by `/status`.
- **A position already passed answers at once**, however old it is.
- **During a stop it keeps answering.** A wait open when the stop begins, or opened during the drain, is answered `200` if its position lands meanwhile: that is when in-flight commits reach the sinks. While a wait on `searchable` is open the drain also waits out the sink's refresh interval, inside the stop's deadline. Once the drain is over every wait still open is answered `503` with `reason: stopping`, and so is a new one.
- **A `503` means ask again**, against the same address, until your own timeout. The position is the source's own (a WAL LSN), so the next process answers the same question: it starts from the position the last one confirmed, which after a graceful stop covers everything the last one indexed.

The runbook is [Read your own writes](../operate/read-your-writes.md); the Rust client is [Reading your own writes](../query/read-your-writes.md).

## GET /indexes

Every sink's `indexes` map from `/status`: `{"primary": {"users": "seeded", …}, "audit": {…}}`. Basic auth required.

## POST /reindex?index=\<name\>[&sink=\<name\>]

Stage a from-scratch rebuild of one index into a fresh generation on one sink, or on every sink when `sink` is omitted. Basic auth required.

| Response | When |
| --- | --- |
| `202 Accepted` | Queued. Each targeted sink engine stages the new generation between two batches and requests its own snapshot; nothing restarts, reads stay on the old generation until the swap, and untargeted sinks are untouched. A sink with `backfill = false` logs and ignores it. Watch `/status` for the index leaving and returning to `seeded` under that sink. |
| `400` | Missing `index` parameter, or `index`/`sink` is not a valid name. |
| `404` | Not a configured index, or not a configured sink. |
| `503` | That sink's operation queue is full, its engine has stopped for good, or flusso is [stopping](cli.md#stopping). |

The runbook is [Reindex without downtime](../operate/reindex.md); the CLI wrapper is [`flusso reindex`](cli.md#reindex).
