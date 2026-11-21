# A lane is stalled

Find the index that stopped writing to a sink, read why, fix the cause, and know what it costs while it lasts.

## When to use this

A **lane** is one index flowing into one sink. Open this page when any of these shows up:

- `FlussoLaneParked` fired: flusso stopped writing a lane on purpose.
- `FlussoLaneStalled` fired: a lane has landed nothing for 10 minutes.
- The log says `the lane is parked`, `the index's target is gone`, or `a lane has landed nothing for …s`.
- The log says `index held at max_waiting_per_index: exempt, …` or `reading stopped for every index: …`: an index is stuck and its changes are piling up. See [When one index is stuck beside others](#when-one-index-is-stuck-beside-others).
- [`/status`](../reference/http.md#get-status) shows a lane whose `health` isn't `open`, or whose `stall_seconds` keeps climbing.
- `pinned_by` names an index with `"sink": null` and keeps naming it: no lane is stuck, the index's backlog is. See [When an index's backlog holds the slot](#when-an-indexs-backlog-holds-the-slot).

A lane that keeps writing is not stalled, however long its backlog. For a whole sink that is down or refusing requests, the lane is `open` and the sink's `phase` is `failed`: the cause is the sink, covered in [step 3](#the-sink-itself-health-open).

## What it costs while it lasts

- **WAL on the Postgres server.** The replication slot is confirmed only as far as the slowest lane has written, so one stuck lane keeps the WAL of **every** index, and it grows for as long as the lane is stuck, even while the other indexes keep up. Past `max_slot_wal_keep_size`, Postgres invalidates the slot and every index on every sink is rebuilt from a backfill ([Recover from a dropped slot](dropped-slot.md)).
- **Memory.** The other indexes keep being written, but documents queued behind the stuck lane hold part of the [memory budget](../reference/config-toml.md#concurrency). With two or more indexes, the stuck index's live documents fill only its share (`index_budget_share`, half by default) and the others keep building. Its later changes wait as documents to build, about 270 B each, up to a bound ([below](#when-one-index-is-stuck-beside-others)). Meanwhile a healthy index builds one batch per acknowledgement round, since it gets room only while it holds no live documents ([memory budget](../reference/config-toml.md#concurrency)), so its throughput drops though it keeps moving. A stuck index's backfill holds backfill's half of the budget, so a reindex or a new sink's backfill of any other index waits until the lane opens, as before 0.19.
- **Fences on that index.** A [`/wait`](../reference/http.md#get-waitpositionposition) covering the stuck lane times out (`408`); a wait that names only other indexes (`index=…`) still answers.

Dropping and rebuilding just the stuck index is not available yet ([#394](https://github.com/alias2k/flusso/issues/394)).

## Steps

### 1. Find the lane

The alert's `index` and `sink` labels name it. Without alerting, read `/status`: `pinned_by` names the lane holding the slot (a `null` `sink` means [an index's backlog](#when-an-indexs-backlog-holds-the-slot) holds it, not a lane), and each lane sits under `sinks.<sink>.lanes.<index>`:

```sh
curl -s localhost:9464/status | jq '.pinned_by, (.sinks[].lanes | with_entries(select(.value.health != "open")))'
```

A lane flusso has stopped writing looks like this (fields in [`/status`](../reference/http.md#lane-health)):

```json
"users": {
  "applied_position": "0/16B3748",
  "outstanding": 3,
  "stall_seconds": 412.0,
  "health": "blocked",
  "parked_since": "2026-11-19T12:03:11Z",
  "next_probe": "2026-11-19T12:10:03Z",
  "reason": "blocked: index [users_1a2b3c4d_1] blocked by: [FORBIDDEN/8/index write (api)];"
}
```

The same lane shows as `flusso_lane_parked{index="users",sink="primary",why="blocked"} 1`, and the log line names it once when it parks:

```text
WARN the lane is parked: its item is written again alone after the backoff; the other indexes go on index=users sink=primary reason=blocked: index [users_1a2b3c4d_1] blocked by: [FORBIDDEN/8/index write (api)]; backoff=1s
```

### 2. Read its health

| `health` | What happened | What flusso does | Fix |
| --- | --- | --- | --- |
| `blocked` | OpenSearch refuses every write to the index for now: a write block, or a role without write permission on it. | Holds the lane, writes its oldest batch again alone at `next_probe`. Quarantines nothing, whatever `on_error` says. | [Lift the block](#a-write-block) or [grant the permission](#a-role-without-write-permission) |
| `refused` | A document was refused under `on_error = "stop"`; `reason` names the ids. | Holds the lane, writes the same batch again at `next_probe`. | [Fix the schema or switch to `skip`](#a-document-refused-under-stop) |
| `restaging` | The index's target was deleted while flusso ran. | Stages that index again and backfills it, alone. | [Nothing, usually](#a-deleted-index) |
| `open` | flusso writes the lane; something else stops it landing. | Restarts the sink with backoff. | [The sink itself](#the-sink-itself-health-open) |

A `refused` or `blocked` lane is **parked**. While it is, the sink's other indexes keep being written. Each retry of the held lane is a **probe**, on the [`[backoff]`](../reference/config-toml.md#backoff) schedule: 1 s after it parked, then doubling to 60 s, by default. Another index landing never shortens the wait. `flusso_lane_probes_total{outcome}` counts the probes.

### 3. Fix the cause

#### A write block

`reason` carries `cluster_block_exception` text such as `FORBIDDEN/8/index write (api)` (`index.blocks.write`), `FORBIDDEN/5/index read-only (api)` (`index.blocks.read_only`) or `FORBIDDEN/12/index read-only / allow delete (api)` (`index.blocks.read_only_allow_delete`, set by hand). See which block is set, on the generation `reason` names:

```sh
curl -s 'localhost:9200/users_1a2b3c4d_1/_settings/index.blocks*?flat_settings=true'
```

Lift it:

```sh
curl -s -X PUT localhost:9200/users_1a2b3c4d_1/_settings \
  -H 'Content-Type: application/json' \
  -d '{"index.blocks.write": null, "index.blocks.read_only": null, "index.blocks.read_only_allow_delete": null}'
```

The next probe lands and the lane catches up, at most one [`max_ms`](../reference/config-toml.md#backoff) later (60 s by default). No restart, and nothing was lost meanwhile.

#### A disk watermark

The flood-stage block is not a `blocked` lane. OpenSearch answers it with `429`, which flusso treats as **busy**: it [resends](../reference/sink-opensearch.md#retries), and when the retries run out the whole sink restarts with backoff. The lanes stay `open` and the sink's `last_error` says `still refused as busy`. Free disk space (`curl -s 'localhost:9200/_cat/allocation?v'` shows usage per node); OpenSearch lifts the block once usage drops below the high watermark, and the sink's next restart writes.

#### A role without write permission

`reason` carries a `security_exception` naming the role and the index. Grant the role `index` (or `write`) permission on the index pattern flusso writes (`users_*` and its aliases, or the deployment's [prefix](../reference/config-toml.md#prefix)). The next probe lands.

#### A document refused under `stop`

`reason` reads `refused: id 42 (…)` with OpenSearch's words. The probe writes the **same** batch every time, so fixing the row in Postgres doesn't unpark the lane yet. Pick one:

- **The schema is wrong** (a value the mapping can't take: a wider type, a `custom` type, a `default`): fix the schema and restart. A schema change builds a new index.
- **The row is wrong**: set `on_error = "skip"` on that index and restart, then fix the row. The refused document is quarantined; the row's next change indexes it. See [Handle rejected documents](rejected-documents.md).
- **`Limit of total fields … exceeded`**: raise the limit with [`index_settings`](../reference/sink-opensearch.md#index-settings), restart, and [reindex](reindex.md).

#### A deleted index

A `restaging` lane fixes itself: flusso creates a fresh generation for that index, backfills it, and opens the lane, while the other indexes keep writing. Reads through the alias return 404 until the generation exists, and `/wait` answers `503 backfilling` for the index until its backfill completes. A lane that stays `restaging` is one whose staging keeps failing; the log says `could not stage the index again; trying again later` with the cause, retried with the same backoff.

An index deleted **mid-backfill** is backfilled again from the first row, and stays `backfilling` until that fresh backfill lands. Where `action.auto_create_index` lets a write recreate the generation, the lane never goes `restaging`: the writes land in a dynamically mapped index until the seed or the next start replaces it. If that mapping refuses a document under `on_error = "stop"`, the lane parks `refused` and stays parked; restart flusso to replace the index. See [Why forbid auto-create](../reference/sink-opensearch.md#why-forbid-auto-create).

#### The sink itself (health `open`)

The lane is written but nothing lands. The log has OpenSearch's reason and `sinks.<sink>.last_error` the latest:

- **The cluster is down or refusing requests**: bring it back. The sink restarts with backoff until it can write.
- **Busy (`429`/`503`) for good**: busy refusals are resent, so a lane stalled behind them means the cluster never had room. Add capacity or lower the load.
- **A wrong mapping on an index flusso didn't create**: [reindex](reindex.md) the index on that sink.

### 4. Watch it recover

- The log says `the parked lane's probe landed; the lane is open` (or `the index is staged again; the lane is open`), then `a stalled lane is landing again`.
- `health` is back to `open`, `stall_seconds` drops to 0, the `flusso_lane_parked` series disappears.
- `pinned_by` moves on and the slot releases the WAL it kept.
- If the index was exempt, the log says `index below max_waiting_per_index: no longer exempt; it paces the reading again` once its waiting documents are built.

## When one index is stuck beside others

With two or more indexes, a stuck index stops only itself. Once it has [`max_waiting_per_index`](../reference/config-toml.md#reading-ahead) documents waiting (512 by default), it becomes [exempt](../reference/config-toml.md#a-stuck-index), and the log says so once:

```text
WARN index held at max_waiting_per_index: exempt, so reading goes on for the other indexes; its changes are kept as documents waiting, up to max_pending_documents for every exempt index, and the replication slot stays pinned by it, so WAL grows index=users waiting=512
```

What that means:

- **Still working:** the other indexes are read, built and written, and a `/wait` naming only them answers.
- **Growing:** the stuck index's documents waiting to be built (each once, however often it changes), and the WAL the slot keeps. `flusso_replication_slot_lag_bytes` climbs while `pinned_by` names the stuck index.
- **Not shown elsewhere:** `/status` and the metrics have no per-index view of it yet, only this log line.
- **An index that is only slow** (its lane keeps landing) isn't exempt: it paces the reading at `max_waiting_per_index`, as before.
- **With one index** there is nothing to read on for: reading stops at its waiting bound.

**At the cap, everything stops.** When the stuck indexes together have [`max_pending_documents`](../reference/config-toml.md#a-stuck-index) documents waiting (a million by default), reading stops for every index:

```text
WARN reading stopped for every index: the exempt indexes have max_pending_documents documents waiting to be built; it reads on as they are built documents=1000000 max_pending_documents=1000000
```

**What to do.**

1. **Fix the stuck lane** ([steps above](#steps)). Once it lands, the index builds its waiting documents oldest first, beside the others ([Watch it recover](#4-watch-it-recover)). If reading had stopped at the cap, the log says `the exempt indexes are below max_pending_documents: reading on`.
2. **Watch the WAL** against `max_slot_wal_keep_size` while it lasts: the other indexes keeping up doesn't release any of it.
3. **Need more time before the cap?** Raise `max_pending_documents` if memory allows (about 270 B a document). It is read at start, so it takes a restart, and the restart reads again all the WAL the slot kept.

## When an index's backlog holds the slot

`pinned_by` reads `{ "index": "users", "sink": null }`, and `flusso_slot_pinned_by{index="users"}` has no `sink` label. No lane holds the slot: the index's **backlog** does. Those are documents of `users` whose changes flusso has read, waiting to be built or being built, and not yet handed to any sink.

- **For a moment, it's normal.** Every live change passes through the backlog, so the form shows up during builds and moves on.
- **When it stays, builds aren't keeping up.** The backlog is on no lane, so it ages no lane and `FlussoLaneStalled` doesn't fire for it. The slot lag grows instead (`flusso_replication_slot_lag_bytes`); the dev dashboard's WAL panel marks the index as `pinned by users's backlog`.

Find which side is slow:

| What shows | Cause | Fix |
| --- | --- | --- |
| The `build` stage busy ([Where the time goes](../reference/metrics.md#where-the-time-goes)), slow document queries in Postgres | Building that index's documents is slow | Read the slow queries (`pg_stat_statements`) and index the columns the schema joins on, or raise [`[concurrency] builds`](../reference/config-toml.md#concurrency) if Postgres has room |
| `flusso_memory_built_bytes` flat at the limit, `flusso_memory_refused_total` climbing | The [memory budget](../reference/config-toml.md#concurrency) is full, so nothing more is built | Find the sink that isn't acknowledging (a parked lane, a lane whose `outstanding` stays high) and fix it as above; with every sink healthy, raise `max_built_mib` |
| `flusso_memory_built_bytes` below the limit while `pinned_by` keeps naming the index alone; `flusso_memory_refused_total{work="live"}` rose by one when it began, and the wait shows in `flusso_memory_wait_seconds{work="live"}` once it ends | The index holds its [share of the budget](../reference/config-toml.md#concurrency) (`index_budget_share`), so its documents wait while the other indexes build | Find that index's lane that isn't acknowledging and fix it as above; with every sink healthy, raise `max_built_mib` or `index_budget_share` |

## Options and variations

- **The two alerts.** `FlussoLaneParked` fires once `flusso_lane_parked > 0` has held for 5 minutes. `FlussoLaneStalled` fires on a lane landing nothing for 10 minutes (`reason: age`), or on more than 1 GiB of WAL kept behind a lane that has landed nothing for a minute (`reason: wal`). Both ship in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`; tune them in the [Helm values](../reference/helm-values.md). Only `FlussoLaneParked` sees a refused **backfill** page: backfill rows carry no position, so their lane never ages.
- **Probe sooner, or less often.** [`[backoff]`](../reference/config-toml.md#backoff) sets the first and the longest wait between probes, for every lane and every engine restart. A short `max_ms` unparks a lane sooner once its cause is fixed; each probe of a lane still refused costs one bulk request.
- **`/readyz` stays ready.** A parked lane doesn't make flusso unready: the other indexes are written and served, and a restart wouldn't help. Alert on the lane, not on readiness.
- **A slow lane isn't stalled.** A lane that lands, just slowly, fires neither alert; `flusso:backlog_drain_eta_seconds` shows how long it will take. flusso logs a warning once a lane passes 10 minutes without landing, and every 10 minutes after.
- **A backfill that stops making progress** shows as the index staying `backfilling` in `/status` while `flusso.documents.built` stops moving. A live change queued behind it on the same lane does age, and alerts.
- **A stop with a parked lane.** A stop probes every parked lane once more at once. If one still can't land, the drain waits for `--shutdown-timeout-secs`, then flusso exits `1` naming the lane: `primary: 3 changes in flight, lane users blocked since 2026-11-19T12:03:11Z (blocked: …)`. Documents it read for that index and couldn't build are named too: `users: 1200 documents read and not built`. The next start writes those batches again first and reads the rest again. See [Stopping](../reference/cli.md#stopping).
- **A process restart resets the stall age.** The age is kept in memory, so a process that crash-loops never ages a lane into `FlussoLaneStalled`. Alert on the process too: `up{job="<flusso's scrape job>"} == 0` for a few minutes, or under Kubernetes `increase(kube_pod_container_status_restarts_total{container="flusso"}[15m]) > 2`.

## Related

- [`/status`](../reference/http.md#get-status) and [Metrics](../reference/metrics.md)
- [Handle rejected documents](rejected-documents.md)
- [Sink: OpenSearch — write blocks](../reference/sink-opensearch.md#write-blocks)
- [Recover from a dropped slot](dropped-slot.md)
