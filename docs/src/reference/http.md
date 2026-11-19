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

`200` when the phase is `backfilling` or `live` and so is every sink's; `503` otherwise: while `starting`, while a sink is `failed` and waits to restart, from `stopping` on. A stopping or stopped pipeline is deliberately not ready.

A [parked lane](#lane-health) leaves `/readyz` at `200`: the other indexes are written and served, and a restart would not unpark it. Watch the lane instead: its `health` in `/status`, the `flusso_lane_parked` series, and the `FlussoLaneParked` alert ([A lane is stalled](../operate/stalled-lane.md)).

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
| `pinned_by` | object or null | The lane holding the replication slot back the most, as `{ "index", "sink" }`: the one whose oldest outstanding change is oldest. Null while no lane has a change outstanding. |
| `visibility_wait_seconds` | number or null | How long the source has held a committed transaction that isn't visible yet; every later change waits behind it. Timed from when that transaction was first found invisible, so an ingest engine restart doesn't reset it. Null while nothing waits, and once the source server is refused. See [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby). |
| `applied_position` | string or null | The source position every sink has applied: a fetch by id sees every commit at or before it. The slowest sink's. Null until the source follows live changes. |
| `searchable_position` | string or null | The source position every sink can search. The slowest sink's. |
| `memory.limit_bytes` | int | `[concurrency] max_built_mib` in bytes. |
| `memory.built_bytes` | int | Built documents not yet acknowledged by every sink, builds in flight included. See [config.toml](config-toml.md#concurrency). |
| `memory.backfill_bytes` | int | The backfill's share of `built_bytes`: at most half the limit, past the one batch a backfill holding nothing is always granted. |
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
| `sinks.<name>.searchable_position` | string or null | The source position this sink can search. Trails `applied_position` by the destination's refresh while a backlog drains. The slowest of its lanes. |
| `sinks.<name>.lanes.<index>.applied_position` | string or null | How far this sink has applied that index: its `(index, sink)` lane. A stalled index holds back only its own lane here. |
| `sinks.<name>.lanes.<index>.searchable_position` | string or null | How far this sink can search that index. |
| `sinks.<name>.lanes.<index>.outstanding` | int | Items routed to the lane and not written yet. A lane that stays high is the one holding back the slot. |
| `sinks.<name>.lanes.<index>.stall_seconds` | number, absent | How long the lane's oldest outstanding change has stayed its oldest: the lane has written nothing for that long. 0 while nothing is outstanding, and while it lands steadily. Sampled every second. Backfill rows aren't counted: they carry no position. See [A lane is stalled](../operate/stalled-lane.md). |
| `sinks.<name>.lanes.<index>.wal_behind_bytes` | int, absent | Bytes of WAL the replication slot keeps behind this lane: the slot lag less how far the lane is ahead of the lane that pins it. It counts WAL flusso hasn't read yet as well as what it read and hasn't landed (a stalled lane pins both), so a healthy catch-up shows a large value on lanes that land every second: read it with the stall age. It is computed from the slot lag, polled every `--lag-poll-secs` (15 s by default); a failed poll keeps the last value. Absent while nothing is outstanding, or before the first lag sample. |
| `sinks.<name>.lanes.<index>.health` | `open` \| `refused` \| `blocked` \| `restaging` | Whether flusso writes the lane. See [Lane health](#lane-health). |
| `sinks.<name>.lanes.<index>.parked_since` | string, absent | When the lane stopped being written (RFC 3339, UTC). Absent while `open`. |
| `sinks.<name>.lanes.<index>.next_probe` | string, absent | When a `refused` or `blocked` lane next writes its oldest batch again (RFC 3339, UTC). A probe further ahead than any date shows as the latest date there is. Absent while `open` or `restaging`. |
| `sinks.<name>.lanes.<index>.reason` | string, absent | Why the lane isn't written, in the destination's words. Absent while `open`. |
| `sinks.<name>.lanes.<index>.backfill_checkpoints` | int, absent | Times this run recorded how far the lane's backfill got, so a restart resumes after it (about one every 10 s while a backfill lands); absent while 0. Staying absent through a long backfill means nothing lands. See [Postgres source](source-postgres.md), "An interrupted backfill resumes". |

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
  "pinned_by": null,
  "memory": { "limit_bytes": 536870912, "built_bytes": 1048576, "backfill_bytes": 0 },
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
      "searchable_position": "0/16B3748",
      "lanes": {
        "orders": { "applied_position": "0/16B3748", "searchable_position": "0/16B3748", "outstanding": 0, "stall_seconds": 0.0, "health": "open" },
        "products": { "applied_position": "0/16B3748", "searchable_position": "0/16B3748", "outstanding": 0, "stall_seconds": 0.0, "health": "open" },
        "users": { "applied_position": "0/16B3748", "searchable_position": "0/16B3748", "outstanding": 0, "stall_seconds": 0.0, "health": "open" }
      }
    }
  }
}
```

### Lane health

Each `(index, sink)` lane is written unless something about that one index stops it. Then the lane is held, its changes stay unconfirmed (so it [holds the replication slot](../operate/stalled-lane.md#what-it-costs-while-it-lasts)), and the sink's other indexes keep being written.

| `health` | Meaning | `reason` holds | Until |
| --- | --- | --- | --- |
| `open` | Written as usual. | (absent) | |
| `refused` | A document of the lane was refused under [`on_error = "stop"`](index-and-on-error.md#on_error). | `refused: id <ids> (<first reason>)`, a few ids at most | a probe lands |
| `blocked` | The destination refuses every write to the index for now: a [write block](sink-opensearch.md#write-blocks), a role without write permission on it. | `blocked: <OpenSearch's reason>` | a probe lands |
| `restaging` | The index's target was deleted while flusso ran: it is staged again and backfilled, alone. | OpenSearch's reason | staging succeeds |

A `refused` or `blocked` lane is **parked**: flusso holds it and probes it, and the logs say `the lane is parked`. The metric [`flusso.lane.parked`](metrics.md) and the `FlussoLaneParked` alert cover every lane that isn't `open`, `restaging` included; their `why` label is the lane's `health`.

A **probe** writes a parked lane's oldest batch again, alone, on the [`[backoff]`](config-toml.md#backoff) schedule (1 s after it parked, doubling to 60 s, by default). Another lane landing never shortens the wait; a stop probes at once. A failed staging of a `restaging` lane is retried on the same schedule. A lane flusso doesn't write:

```json
"users": {
  "applied_position": "0/16B3748", "searchable_position": "0/16B3748", "outstanding": 3, "stall_seconds": 412.0,
  "health": "blocked",
  "parked_since": "2026-11-19T12:03:11Z",
  "next_probe": "2026-11-19T12:10:03Z",
  "reason": "blocked: index [users_1a2b3c4d_1] blocked by: [FORBIDDEN/8/index write (api)];"
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
| `index` | string, repeatable | none | Wait for these indexes only (`index=users&index=orders`). Omitted, the wait covers every index. A read needs only the indexes it reads, so a stalled index doesn't hold up reads of the others. |
| `timeout_ms` | int | `5000` | How long to hold the request. Values above `60000` are cut to `60000`. |

| Response | When |
| --- | --- |
| `200` | Reached. |
| `408` | The timeout passed first. Nothing is wrong with the request; ask again or give up. |
| `400` | `position` is missing or not a position the source can read, or `until`, `sink`, `index`, or `timeout_ms` is malformed. |
| `404` | `sink` is not a configured sink, or an `index` is not an enabled index. |
| `503` | This process can't answer now, or can't vouch for the position yet. `Retry-After: 1`, and `reason` says why: `starting` (the source isn't following live changes yet: startup, or the ingest engine is restarting), `stopping` (the [stop](cli.md#stopping)'s drain is over), `backfilling` or `rebuilding` (below). |

`200` and `408` share a body; errors carry `{"error": "…"}`, a `503` also `"reason"`.

```json
{ "reached": true, "position": "0/16B3780", "until": "searchable", "sink": "primary", "indexes": ["users"] }
```

`position` is how far the lanes waited on have got (the slowest one): at or past the one asked for on `200`, short of it on `408`. `indexes` echoes the `index` parameters, empty when none was given.

A `503` for an index being seeded or rebuilt also names the lanes holding it up:

```json
{ "error": "reads don't see this position yet: users on primary (rebuilding)", "reason": "rebuilding",
  "position": "0/16B3780", "lanes": [{ "index": "users", "sink": "primary", "state": "rebuilding" }] }
```

- **It only waits.** Nothing is flushed or refreshed on a caller's behalf. Under a draining backlog `searchable` trails `applied` by up to the sink's [`refresh_interval`](sink-opensearch.md#refresh).
- **It vouches only for what reads see.** While a sink seeds an index for the first time, reads see it filling; while it rebuilds one (a [reindex](../operate/reindex.md), or every seeded index after the [slot was lost](../operate/dropped-slot.md)), reads see the old copy and live writes go to the new one. A wait that needs such an index holds, and is answered `200` the moment the index is seeded and covers the position. If the timeout comes first it is `503` with `reason: backfilling` or `rebuilding` instead of `408`. During a reindex the old copy still serves everything written before the reindex began, so a wait for such a position is `200` throughout. A process that starts in the middle of a reindex knows nothing of what the old copy holds, so it vouches for nothing of that index until the swap. Without the slot, the old copy misses changes, so after a lost slot nothing is vouched for until the rebuild is seeded. The reason is `rebuilding` when any lane holding the wait rebuilds, else `backfilling`.
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
