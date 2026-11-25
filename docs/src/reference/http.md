# HTTP endpoints

`flusso run` serves two listeners. Trust is gated by port, not path: the public surface is unauthenticated and changes nothing; the private surface is HTTP Basic auth and can change state.

| Surface | Default bind | Auth | Endpoints |
| --- | --- | --- | --- |
| public | `127.0.0.1:9464` | none | `GET /healthz`, `GET /readyz`, `GET /status`, `GET /metrics`, `GET /wait`, `GET /indexes/<name>/status` |
| private | `127.0.0.1:9465` | HTTP Basic | `GET /indexes`, `POST /indexes/<name>/reindex` |

Each surface answers `404` for the other's routes.

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
| `indexes` | map of name to `pending` \| `backfilling` \| `seeded` | Per-index state across sinks: the least advanced sink's state. Disabled indexes are listed too. |
| `ingest.indexes.<index>` | object | What the ingest side knows of each enabled index: is it behind, stuck or down, and why. See [Per-index ingest state](#per-index-ingest-state). |
| `changes_captured` | int | Changes the ingest engine pulled from the source. |
| `changes_in_flight` | int | Captured minus the slowest sink's `changes_committed`. `0` means every sink has written everything read. |
| `documents_built` | int | Documents assembled by the ingest engine (once, however many sinks). |
| `slot_lag_bytes` | int or null | Null until the first lag sample. |
| `pinned_by` | object or null | What holds the replication slot back the most: whatever holds the oldest change not yet written. Two forms. **A lane**, `{ "index", "sink" }`: that index's oldest change on that sink. **An index's backlog**, `{ "index", "sink": null }`: documents of that index waiting to be built or being built, not yet handed to any sink. The backlog form shows up for a moment during live builds; one that stays means builds aren't keeping up ([A lane is stalled](../operate/stalled-lane.md#when-an-indexs-backlog-holds-the-slot)). On a tie the lane is named. Null while nothing is outstanding. |
| `visibility_wait_seconds` | number or null | How long the source has held a committed transaction that isn't visible yet; every later change waits behind it. Timed from when that transaction was first found invisible, so an ingest engine restart doesn't reset it. Null while nothing waits, and once the source server is refused. See [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby). |
| `applied_position` | string or null | The source position every sink has applied: a fetch by id sees every commit at or before it. The slowest sink's. Null until the source follows live changes. |
| `searchable_position` | string or null | The source position every sink can search. The slowest sink's. |
| `memory.limit_bytes` | int | `[concurrency] max_built_mib` in bytes. |
| `memory.built_bytes` | int | Built documents not yet acknowledged by every sink, builds in flight included. See [config.toml](config-toml.md#concurrency). |
| `memory.backfill_bytes` | int | Backfills' share of `built_bytes`, every index's together: at most half the limit, past one batch per index backfilling ([memory budget](config-toml.md#concurrency)). |
| `errors` | int | Engine errors, ingest and sink. An index that restarts alone isn't one: its failure is under [`ingest.indexes.<index>`](#per-index-ingest-state). |
| `last_error` | string or null | The latest engine error. |
| `script_failures.<index>.count` | int | Documents of that index that failed to build: a [script failed](scripts.md#failures), or an [`array: true`](field-types.md#array) column held an object. Under either `on_error` policy (a `stop` index counts each retry). The key is absent until one fails. |
| `script_failures.<index>.last` | object | The most recent: `id`, `field` (path, e.g. `variants[2].gross`), `message`, `line` and `column` in the script (null without a position), `at` (RFC 3339). |
| `sinks.<name>.phase` | `starting` \| `backfilling` \| `live` \| `failed` \| `stopped` | `failed` while the daemon waits to restart that sink engine; the others keep running. |
| `sinks.<name>.indexes` | map of name to state | That sink's own view of each index. |
| `sinks.<name>.changes_committed` | int | Live changes this sink has written, counted in read order. A change counts once **every lane** of the sink (one per index) has written it, or had nothing to write for it, and every change read before it counts. A change that gives no document for the sink (an update of a column no document reads) counts as soon as it is read and everything read before it is written. A redelivered batch that had already landed counts once, so it never passes `changes_captured`. Never goes down, also across an ingest engine restart. A sink with no index counts every change read. |
| `sinks.<name>.changes_in_flight` | int | Captured minus this sink's committed. Above `0` while any lane of the sink is behind: one stalled or [parked](#lane-health) index holds the count for all the sink's indexes. `0` means everything read is written on this sink. When the ingest engine fails and restarts, changes it had read but not yet written stay in flight from the moment it fails until the restarted engine builds their index again. |
| `sinks.<name>.envelopes_applied` | int | Documents written to this sink. |
| `sinks.<name>.batches` | int | |
| `sinks.<name>.documents_quarantined` | int | Non-zero means data is being dropped under `on_error = "skip"`. |
| `sinks.<name>.last_flush_micros` | int | Duration of this sink's most recently finished write. |
| `sinks.<name>.applied_position` | string or null | The source position this sink has applied. |
| `sinks.<name>.searchable_position` | string or null | The source position this sink can search. Trails `applied_position` by the destination's refresh while a backlog drains. The slowest of its lanes. |
| `sinks.<name>.lanes.<index>.applied_position` | string or null | How far this sink has applied that index: its `(index, sink)` lane. A stalled index holds back only its own lane here. |
| `sinks.<name>.lanes.<index>.searchable_position` | string or null | How far this sink can search that index. |
| `sinks.<name>.lanes.<index>.outstanding` | int | Items routed to the lane and not written yet. A lane that stays high is the one holding back the slot. |
| `sinks.<name>.lanes.<index>.stall_seconds` | number, absent | How long the lane's oldest outstanding change has stayed its oldest: the lane has written nothing for that long. Only what the lane holds itself counts: items routed to it and not written yet, and a built batch being handed to it that it hasn't taken (a full lane). Its index's backlog (documents not built or not yet handed to a sink) stalls no lane: `pinned_by` names it instead. 0 while the lane holds nothing, and while it lands steadily. Sampled every second. Backfill rows aren't counted: they carry no position. See [A lane is stalled](../operate/stalled-lane.md). |
| `sinks.<name>.lanes.<index>.wal_behind_bytes` | int, absent | Bytes of WAL the replication slot keeps behind this lane: the slot lag less how far the lane is ahead of whatever pins the slot (another lane, or an index's backlog). It counts WAL flusso hasn't read yet as well as what it read and hasn't landed (a stalled lane pins both), so a healthy catch-up shows a large value on lanes that land every second: read it with the stall age. It is computed from the slot lag, polled every `--lag-poll-secs` (15 s by default); a failed poll keeps the last value. Absent while the lane holds nothing itself (an index's backlog doesn't count), or before the first lag sample. |
| `sinks.<name>.lanes.<index>.health` | `open` \| `refused` \| `blocked` \| `restaging` | Whether flusso writes the lane. See [Lane health](#lane-health). |
| `sinks.<name>.lanes.<index>.parked_since` | string, absent | When the lane stopped being written (RFC 3339, UTC). Absent while `open`. |
| `sinks.<name>.lanes.<index>.next_probe` | string, absent | When a `refused` or `blocked` lane next writes its oldest batch again, or tries its seed again (RFC 3339, UTC). A probe further ahead than any date shows as the latest date there is. Absent while `open` or `restaging`. |
| `sinks.<name>.lanes.<index>.reason` | string, absent | Why the lane isn't written, in the destination's words. Absent while `open`. |
| `sinks.<name>.lanes.<index>.backfill_checkpoints` | int, absent | Times this run recorded how far the lane's backfill got, so a restart resumes after it (about one every 10 s while a backfill lands); absent while 0. Staying absent through a long backfill means nothing lands. See [Postgres source](source-postgres.md), "An interrupted backfill resumes". |

A source position is in the source's own text form: for Postgres, a WAL LSN such as `0/16B3748`, comparable with `pg_current_wal_lsn()`. Both positions keep moving while the watched tables are idle, because the source follows unrelated WAL too.

```json
{
  "pid": 48213,
  "phase": "live",
  "ingest": {
    "indexes": {
      "orders": { "documents_waiting": 0, "backlog_seconds": 0.0, "state": "running", "last_error": { "live": null, "backfill": null }, "restarts": { "live": 0, "backfill": 0 }, "exemption": "none", "working": true, "held_by": null, "backfill": null, "pacing_reader": false },
      "products": { "documents_waiting": 0, "backlog_seconds": 0.0, "state": "running", "last_error": { "live": null, "backfill": null }, "restarts": { "live": 0, "backfill": 0 }, "exemption": "none", "working": true, "held_by": null, "backfill": null, "pacing_reader": false },
      "users": { "documents_waiting": 0, "backlog_seconds": 0.0, "state": "running", "last_error": { "live": null, "backfill": null }, "restarts": { "live": 0, "backfill": 0 }, "exemption": "none", "working": true, "held_by": null, "backfill": null, "pacing_reader": false }
    }
  },
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

### Per-index ingest state

`ingest.indexes.<index>` answers "which index is behind, stuck or down, and why", from the ingest side: documents read and not built yet, whether the index is [down](config-toml.md#a-failing-index) and what failed last, the [exemption](config-toml.md#a-stuck-index), what holds the index's builds, and its backfill. What each sink has landed of it stays under `sinks.<sink>.lanes.<index>`. One entry per **enabled** index, with one index too. [`GET /indexes/<name>/status`](#get-indexesnamestatus) serves one entry alone.

| Field | Type | Meaning |
| --- | --- | --- |
| `documents_waiting` | int | Documents of the index read and not yet taken by a build. |
| `backlog_seconds` | number | How long ago the index's oldest change not built yet was read (waiting, or in a build not yet handed to a sink). `0` when there is none. Grows only while the index is stuck or exempt: [below](#what-backlog_seconds-shows). |
| `state` | `running` \| `down` | `down` from a failure of the index's live work until it restarts alone ([A failing index](config-toml.md#a-failing-index)); `running` otherwise. Always `running` with one index, where a failure restarts the whole ingest engine. |
| `down_since` | string, absent | When its live work failed (RFC 3339, UTC). A restart clears it, so in a loop of failures it is the latest one's time: `restarts.live` counts the loop. Absent while `running`. |
| `last_error` | object | The index's last failure of each kind: `{"live": …, "backfill": …}`. Each is `null` before any failure of that kind, else `{"message", "at"}`, `at` in RFC 3339, UTC. One kind never hides the other. Kept after the index recovered, for the life of the process. `message` is the error as logged, cut at 4 KiB with ` … (cut)` appended. Both stay `null` with one index. |
| `restarts` | object | `{"live": n, "backfill": n}`: how often the index restarted after a live failure, and its backfill started again after a backfill failure, since the process started. A failure that ends the whole run restarts nothing here. Always `0` with one index. |
| `exemption` | `none` \| `exempt` \| `frozen` | `exempt`: held at `max_waiting_per_index` while another index works, so reading goes on for the others; its documents count only toward `max_pending_documents`. `frozen`: exempt, but its hold ended or no other index works, so it may drain but not grow. Always `none` with one index, and between engine runs. See [A stuck index](config-toml.md#a-stuck-index). |
| `exemption_since` | string, absent | When it entered that state (RFC 3339, UTC), to within a second. Absent while `none`. |
| `working` | bool | `false` while items of the index wait at a sink and none has been acknowledged for 5 s, and while the index is `down`. About progress, not holds: an index held by the memory budget whose sink keeps acknowledging is working. [Below](#working-with-one-index-and-with-several). |
| `held_by` | `budget` \| `lane` \| null | What holds the index's builds right now. `budget`: its live batch is refused by the [memory budget](config-toml.md#concurrency). `lane`: a publish (a batch, or a backfill's end) waits on a full lane. For information: a hold alone isn't a fault. Null while the index is `down`: it holds nothing, and `state` says why it builds nothing. Cleared when an engine run ends. |
| `held_sinks` | list of string, absent | With `lane`, the sinks whose lanes are full. Absent otherwise. |
| `held_since` | string, absent | When this hold began (RFC 3339, UTC); a new hold starts its own. Absent while nothing holds it. |
| `backfill` | object or null | The index's backfill, by its `state`. `{"state": "walking", "sinks": [...]}`: it runs, for these sinks. `{"state": "retrying", "retry_at": "…"}`: it failed and waits to retry, its live changes flowing; `retry_at` (RFC 3339, UTC) is when it is due, and can be in the past while another backfill of the index runs, or while the index waits on a full lane or is down. `{"state": "waiting"}`: the index went `down` while it ran, and it starts again once the index restarts. Null while none. |
| `pacing_reader` | bool | The reader is stopped at this index's bound right now (`max_waiting_per_index`, or the count a frozen index froze at): this index sets the pace of reading for every index. |

An index that is exempt and waits on a full lane of `primary`:

```json
"users": {
  "documents_waiting": 48211, "backlog_seconds": 734.0,
  "state": "running", "last_error": { "live": null, "backfill": null }, "restarts": { "live": 0, "backfill": 0 },
  "exemption": "exempt", "exemption_since": "2026-11-19T12:03:14Z",
  "working": false,
  "held_by": "lane", "held_sinks": ["primary"], "held_since": "2026-11-19T12:03:09Z",
  "backfill": null, "pacing_reader": false
}
```

An index that is down after a document failed under `on_error = "stop"`, restarted twice already, and exempt while the others read on:

```json
"orders": {
  "documents_waiting": 1840, "backlog_seconds": 312.0,
  "state": "down", "down_since": "2026-11-19T12:08:41Z",
  "last_error": { "live": { "message": "document `42` in index `orders` could not be built: …", "at": "2026-11-19T12:08:41Z" }, "backfill": null },
  "restarts": { "live": 2, "backfill": 0 },
  "exemption": "exempt", "exemption_since": "2026-11-19T12:04:02Z",
  "working": false, "held_by": null, "backfill": null, "pacing_reader": false
}
```

#### What `backlog_seconds` shows

- **It grows only while the index is stuck or exempt**, a second a second, and falls as the index catches up.
- **It is not a lag behind the source.** Reading stops at each index's cap, so an index that paces the reader stays within its read-ahead (seconds), at its sink's limit or however far behind the source it falls. How far an index is behind is its lanes' `wal_behind_bytes` (`flusso_lane_wal_behind_bytes{sink,index}`) and `slot_lag_bytes`.
- **`0` while no engine run reads**: starting, a restart's backoff, a source down. A restarted run starts from `0`.
- **`0` for a build blocked at its publish** with nothing else waiting, while its lane is full: its documents were taken. That lane's stall age shows it, and `FlussoLaneStalled` covers it.
- **Sampled every second, at most an hour.** A change read longer ago reads as 3600.

#### `working` with one index, and with several

- **With two or more indexes**, backfill items count: a backfill nothing acknowledges for 5 s makes the index not working.
- **With one index**, only live changes count (it is read off the lanes' stall ages), so a backfill nothing acknowledges still reads as working. `FlussoLaneParked` covers a refused backfill page.
- **Not the exemption's test.** To decide whether another index is working, the exemption also counts a held index as not working ([A stuck index](config-toml.md#a-stuck-index)); this field doesn't.

### Lane health

Each `(index, sink)` lane is written unless something about that one index stops it. Then the lane is held, its changes stay unconfirmed (so it [holds the replication slot](../operate/stalled-lane.md#what-it-costs-while-it-lasts)), and the sink's other indexes keep being written.

| `health` | Meaning | `reason` holds | Until |
| --- | --- | --- | --- |
| `open` | Written as usual. | (absent) | |
| `refused` | A document of the lane was refused under [`on_error = "stop"`](index-and-on-error.md#on_error). | `refused: id <ids> (<first reason>)`, a few ids at most | a probe lands |
| `blocked` | The destination refuses every write to the index for now: a [write block](sink-opensearch.md#write-blocks), a closed index, a role without write permission on it. Or a rebuild's seed can't [retire the old generation](sink-opensearch.md#an-old-generation-that-cant-be-retired), which is blocked. | `blocked: <OpenSearch's reason>`; for a closed index, `blocked: index [<generation>] is closed`; for a seed, `blocked: the new generation <new> is built, but the old generation <old> can't be retired: <OpenSearch's reason>; <the ways out>. The seed then finishes on its own` | a probe lands |
| `restaging` | The index's target was deleted while flusso ran, a [reindex couldn't stage](#when-the-reindex-cant-stage) its new generation, or a [restart couldn't ensure the index](#when-a-restart-cant-ensure-an-index): it is staged again, alone. | OpenSearch's reason; for a reindex, `the reindex could not stage: <error>` or `the reindex could not ask for its backfill: <error>`; after a restart, `could not be ensured: <error>` or `not tried: 8 indexes in a row could not be ensured before it` | staging succeeds |

A `refused` or `blocked` lane is **parked**: flusso holds it and probes it, and the logs say `the lane is parked`. The metric [`flusso.lane.parked`](metrics.md) and the `FlussoLaneParked` alert cover every lane that isn't `open`, `restaging` included; their `why` label is the lane's `health`.

A **probe** writes a parked lane's oldest batch again, alone, on the [`[backoff]`](config-toml.md#backoff) schedule (1 s after it parked, doubling to 60 s, by default). A lane parked on its seed tries the seed again instead. A `refused` lane's probe writes a [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build) of that batch instead; while it waits for one, the lane stays `refused` and `next_probe` is the end of the wait. Another lane landing never shortens the wait; a stop probes at once. A failed staging of a `restaging` lane is retried on the same schedule. A [reindex](#a-reindex-of-a-lane-flusso-doesnt-write) of the index opens a lane in any of the three states, except one parked on its seed. A lane flusso doesn't write:

```json
"users": {
  "applied_position": "0/16B3748", "searchable_position": "0/16B3748", "outstanding": 3, "stall_seconds": 412.0,
  "health": "blocked",
  "parked_since": "2026-11-19T12:03:11Z",
  "next_probe": "2026-11-19T12:10:03Z",
  "reason": "blocked: index [users_1a2b3c4d_1] blocked by: [FORBIDDEN/8/index write (api)];"
}
```

#### When a restart can't ensure an index

Each time a sink's run restarts, flusso **ensures** every index of the sink again: it checks the index's generation and its `flusso_meta` record, creating what's missing. When one index can't be ensured while the destination answers for others, only that index waits:

- **The lane** turns `restaging`. Its `reason` is `could not be ensured: <error>`, with the destination's answer. An index flusso didn't get to, because the destination stopped answering part way, reads `not tried: 8 indexes in a row could not be ensured before it`. The log says `the index was not ensured; its lane is staged again on its own while the sink's other indexes go on`.
- **The sink's other indexes** start and keep being written. The sink's `phase` is `live` (or `backfilling`), so `/readyz` isn't held by it.
- **The retry.** The lane is tried again at once (a lane that had only just reopened waits its backoff first), then on the [`[backoff]`](config-toml.md#backoff) schedule. Each try logs `staging the index again: it could not be ensured`; a failed one logs `could not stage the index again; trying again later` with that try's error, while `reason` keeps the error of the staging that left the lane out. The try that succeeds opens the lane, with no restart: `the index is staged again; the lane is open`.
- **Its backfill.** One that was running goes on from where it got. If the index's generation was recreated meanwhile, the backfill starts again from the first row. An unseeded index nobody asked a backfill for yet gets one.
- **Meanwhile** the lane writes nothing, and holds the replication slot and its fences: a `/wait` that needs the index doesn't pass. `flusso_lane_parked{why="restaging"}` is `1`, so `FlussoLaneParked` fires after 5 minutes.

The same holds at a start whose replication slot was kept. A start with a new slot (the first one, or after the slot was [dropped](../operate/dropped-slot.md)) doesn't go on without the index: flusso can't tell whether an index it couldn't reach needs a rebuild before it starts reading, so the start retries with backoff, nothing read meanwhile, and the log says `staging failed; retrying` with the error of each index that failed.

**When the destination is down instead.** flusso can't tell one index's refusal from a destination that is down by the error alone, so it reads it from the indexes:

- **Some index was ensured:** the destination answers, and only the indexes that failed wait, as above. Once 8 indexes in a row fail, flusso stops trying more (the destination stopped answering) and leaves the rest `not tried`.
- **None of the first 8 was:** flusso tries up to 8 more, one at a time. The first that is ensured shows the destination answers.
- **None of those either:** the destination is down. The whole sink waits and retries with backoff, as for any failed restart, and the log says `staging failed; retrying`. At a start, nothing is read until it answers.

Two limits follow:

- **Sixteen refusals read as down.** A sink whose first sixteen indexes, in index-name order, are all refused while the rest are fine waits as if the destination were down.
- **A silent destination is slow to read as down.** Refused connections fail at once. A destination that accepts connections and never answers costs up to nine request timeouts before a restart reads it as down (the first 8 indexes at once, then 8 one at a time): 270 s at the OpenSearch sink's default [`timeout_secs`](sink-opensearch.md) of 30. A stop asked meanwhile starts no further try.

A closed `{prefix}flusso_meta` is read by every index, so none is ensured: the whole sink waits ([Write blocks](sink-opensearch.md#write-blocks)).

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
- **It vouches only for what reads see.** While a sink seeds an index for the first time, reads see it filling; while it rebuilds one (a [reindex](../operate/reindex.md), including one [waiting to stage](#when-the-reindex-cant-stage), or every seeded index after the [slot was lost](../operate/dropped-slot.md)), reads see the old copy and live writes go to the new one. A wait that needs such an index holds, and is answered `200` the moment the index is seeded and covers the position. If the timeout comes first it is `503` with `reason: backfilling` or `rebuilding` instead of `408`. During a reindex the old copy still serves everything written before the reindex began, so a wait for such a position is `200` throughout. A process that starts in the middle of a reindex knows nothing of what the old copy holds, so it vouches for nothing of that index until the swap. Without the slot, the old copy misses changes, so after a lost slot nothing is vouched for until the rebuild is seeded. The reason is `rebuilding` when any lane holding the wait rebuilds, else `backfilling`.
- **Each index gets there on its own.** One commit that touches two indexes can be written to one well before the other, or one index can be [down](config-toml.md#a-failing-index) while the other is written. A wait naming one index passes once that index has it; a wait naming both, or none, passes once both have it.
- **A position already passed answers at once**, however old it is.
- **During a stop it keeps answering.** A wait open when the stop begins, or opened during the drain, is answered `200` if its position lands meanwhile: that is when in-flight commits reach the sinks. While a wait on `searchable` is open the drain also waits out the sink's refresh interval, inside the stop's deadline. Once the drain is over every wait still open is answered `503` with `reason: stopping`, and so is a new one.
- **A `503` means ask again**, against the same address, until your own timeout. The position is the source's own (a WAL LSN), so the next process answers the same question: it starts from the position the last one confirmed, which after a graceful stop covers everything the last one indexed.

The runbook is [Read your own writes](../operate/read-your-writes.md); the Rust client is [Reading your own writes](../query/read-your-writes.md).

## GET /indexes/\<name\>/status

One index's [ingest state](#per-index-ingest-state): the same object as `/status`'s `ingest.indexes.<name>`. Public surface; the private one answers `404`.

```sh
curl -s localhost:9464/indexes/users/status
```

| Response | When |
| --- | --- |
| `200` | The entry. |
| `400` | `{"error": "invalid index name \"…\""}`: not an index name. |
| `404` | `{"error": "unknown index <name>", "indexes": ["orders", …]}`: not an enabled index. `indexes` lists the enabled ones. |

## GET /indexes

Every sink's `indexes` map from `/status`: `{"primary": {"users": "seeded", …}, "audit": {…}}`. Basic auth required.

## POST /indexes/\<name\>/reindex[?sink=\<name\>]

Stage a from-scratch rebuild of one index into a fresh generation on one sink, or on every sink when `sink` is omitted. Basic auth required; the public surface answers `404`. New in 0.19; the route it replaces is on the [upgrade page](../operate/upgrade-to-0-19.md#reindex-moved-to-the-index-path).

```sh
curl -u admin:… -X POST 'http://10.0.0.5:9465/indexes/users/reindex?sink=primary'
```

| Response | When |
| --- | --- |
| `202 Accepted` | Queued: `{"index": "users", "sink": "primary", "queued": true}`, with `"sink": null` for every sink. Each targeted sink engine stages the new generation between two batches and requests its own snapshot; nothing restarts, reads stay on the old generation until the swap, and untargeted sinks are untouched. A sink with `backfill = false` logs and ignores it. Watch `/status` for the index leaving and returning to `seeded` under that sink. |
| `400` | `{"error": "invalid index name \"…\""}` or `{"error": "invalid sink name \"…\""}`. |
| `401` | Credentials missing or wrong. |
| `404` | `{"error": "unknown index <name>", "indexes": [...]}`: not an enabled index. `{"error": "unknown sink <name>"}`: not a configured sink. |
| `503` | `{"error": "…"}`: flusso is [stopping](cli.md#stopping), that sink's operation queue is full (retry shortly), or its engine has stopped for good. |

Without `sink`, each sink stages on its own: one that can't stage doesn't hold up the others.

### A reindex of a lane flusso doesn't write

A reindex is also the way out for a [lane](#lane-health) stuck on something that lives in the index. What it does depends on the lane's `health` when it is staged:

| Lane | What the reindex does |
| --- | --- |
| `open` | Stages a new generation and backfills it. If the index's backfill is still running, it does nothing: that backfill is already rebuilding. |
| `refused` or `blocked` | Opens the lane. The item it was stuck on isn't written again: its documents come from the new generation's backfill, which reads every row after the reindex. Items behind it are written into the new generation. |
| `blocked` on its seed | Stages nothing: the index is still unseeded. The backfill runs again into the same new generation, and the lane parks at the seed again while the old generation's block stands ([details](sink-opensearch.md#an-old-generation-that-cant-be-retired)). |
| `restaging` | Opens the lane; the items it held are written into the new generation. |
| not `open`, with the index's backfill still running | Cuts that backfill and starts it again from the first row. |

**Until the new generation is seeded:**

- The stuck item's change and every later change on the lane count as not landed, so the replication slot stays before them, and `pinned_by` can name the lane.
- The lane reads `open`, yet its `stall_seconds` keeps growing: a long backfill can fire `FlussoLaneStalled` ([A lane is stalled](../operate/stalled-lane.md#unstick-a-lane-with-a-reindex)).
- A [`/wait`](#get-waitpositionposition) that needs the index holds, and answers `503` with reason `rebuilding` at its timeout.

At the seed the stuck item is acknowledged and the lane's positions move on. The log says `reindexed: the lane is open; its parked item waits for the new generation's backfill` when the lane opens.

**What it cures.** The new generation comes from the same schema and mapping, so a reindex cures what lives in the index: `index.blocks.write` on that index, a closed index (the seed drops the closed generation), settings or a mapping changed by hand, an `index_settings` change picked up by a restart. It doesn't cure:

- **A row the mapping refuses.** The new backfill is refused at that row, the lane parks `refused` again, and the reindex completes once the row is fixed ([fresh build](index-and-on-error.md#a-refused-lanes-fresh-build)).
- **A block on the whole cluster.** The new generation can't be created ([below](#when-the-reindex-cant-stage)).
- **`index.blocks.read_only` or `read_only_allow_delete` on the old generation.** The seed can't retire it, so the lane parks `blocked` at the seed until the block is lifted ([details](sink-opensearch.md#an-old-generation-that-cant-be-retired)).

### When the reindex can't stage

The call has already answered `202`. When the sink can't create the new generation, or can't ask for its backfill, the reindex waits and is tried again on its own:

- **The lane** turns `restaging`, its `reason` `the reindex could not stage: <error>` (or `the reindex could not ask for its backfill: <error>`), and the log says `the reindex could not stage; the lane waits for it and it is tried again`. A lane that was `open` stops being written; a parked lane stays held.
- **The retry** follows [`[backoff]`](config-toml.md#backoff): the first wait, doubling to the longest. A momentary error heals by itself. The retry that stages is the reindex, as in the table above.
- **The sink's other indexes** keep being written.
- **Alerts.** `flusso_lane_parked{why="restaging"}` is `1`, so `FlussoLaneParked` fires after 5 minutes.
- **Fences.** A `/wait` that needs the index answers `503 rebuilding` at its timeout, until the new generation is seeded.
- **The slot** stays before the lane's oldest change not written.

**A restart keeps or drops it.** The waiting reindex is kept in memory. A sink engine that fails and restarts keeps it. A restart of flusso keeps only what the sink already recorded:

| When flusso stopped | The next start |
| --- | --- |
| The new generation was created; a later step failed | Backfills it. |
| The index's backfill was still running when the reindex was asked | Resumes that backfill. |
| Anything else | The reindex is gone. Ask for it again if the lane is still stuck. |

Either way the item the lane was stuck on is written again like any other, so a cause still in place parks the lane again. A stop drops a reindex not staged yet, too ([Stopping](cli.md#lanes-flusso-doesnt-write-at-a-stop)).

The runbook is [Reindex without downtime](../operate/reindex.md); the CLI wrapper is [`flusso reindex`](cli.md#reindex).
