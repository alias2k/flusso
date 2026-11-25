# A lane is stalled

Find the index that stopped writing to a sink, read why, fix the cause, and know what it costs while it lasts.

## When to use this

A **lane** is one index flowing into one sink. Open this page when any of these shows up:

- `FlussoLaneParked` fired: flusso stopped writing a lane on purpose.
- `FlussoLaneStalled` fired: a lane has landed nothing for 10 minutes.
- `FlussoIndexBacklog` fired: an index has been exempt or stuck for 10 minutes. See [When FlussoIndexBacklog fires](#when-flussoindexbacklog-fires).
- The log says `the lane is parked`, `the index's target is gone`, or `a lane has landed nothing for …s`.
- The log says `index held at max_waiting_per_index: exempt, …`, `exempt index frozen: …` or `reading stopped for every index: …`: an index is stuck and its changes are piling up. See [When one index is stuck beside others](#when-one-index-is-stuck-beside-others).
- [`/status`](../reference/http.md#get-status) shows a lane whose `health` isn't `open`, or whose `stall_seconds` keeps climbing.
- A lane reads `blocked` with a `reason` starting `blocked: the new generation`. See [A seed that can't retire the old generation](#a-seed-that-cant-retire-the-old-generation).
- A lane reads `restaging` with a `reason` starting `the reindex could not stage`. See [A reindex that can't stage](#a-reindex-that-cant-stage).
- A lane reads `restaging` with a `reason` starting `could not be ensured` or `not tried`. See [An index that can't be ensured after a restart](#an-index-that-cant-be-ensured-after-a-restart).
- `pinned_by` names an index with `"sink": null` and keeps naming it: no lane is stuck, the index's backlog is. See [When an index's backlog holds the slot](#when-an-indexs-backlog-holds-the-slot).
- The log says `the index's builder failed` or `the index's backfill failed`, or `/status` shows an index `"state": "down"` or its `backfill` `retrying`: one index's own work failed and restarts alone. See [When an index is down or its backfill keeps failing](#when-an-index-is-down-or-its-backfill-keeps-failing).

A lane that keeps writing is not stalled, however long its backlog. For a whole sink that is down or refusing requests, the lane is `open` and the sink's `phase` is `failed`: the cause is the sink, covered in [step 3](#the-sink-itself-health-open).

## What it costs while it lasts

- **WAL on the Postgres server.** The replication slot is confirmed only as far as the slowest lane has written, so one stuck lane keeps the WAL of **every** index, and it grows for as long as the lane is stuck, even while the other indexes keep up. Past `max_slot_wal_keep_size`, Postgres invalidates the slot and every index on every sink is rebuilt from a backfill ([Recover from a dropped slot](dropped-slot.md)).
- **Memory.** The other indexes keep being written, but documents queued behind the stuck lane hold part of the [memory budget](../reference/config-toml.md#concurrency). With two or more indexes, the stuck index's live documents fill only its share (`index_budget_share`, half by default) and the others keep building. Its later changes wait as documents to build, about 270 B each, up to a bound ([below](#when-one-index-is-stuck-beside-others)). Meanwhile a healthy index builds one batch per acknowledgement round, since it gets room only while it holds no live documents ([memory budget](../reference/config-toml.md#concurrency)), so its throughput drops though it keeps moving. A stuck index's backfill can hold backfill's half of the budget; another index's backfill (a reindex, a new sink) still goes on beside it, one batch at a time.
- **Fences on that index.** A [`/wait`](../reference/http.md#get-waitpositionposition) covering the stuck lane times out (`408`); a wait that names only other indexes (`index=…`) still answers.

A [reindex](#unstick-a-lane-with-a-reindex) rebuilds the stuck index alone, on one sink, but the lane keeps holding the slot until that backfill is seeded. Dropping a stalled index so it stops holding the slot is not available yet ([#394](https://github.com/alias2k/flusso/issues/394)).

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
| `blocked` | OpenSearch refuses every write to the index for now: a write block, a closed index, or a role without write permission on it. Or a rebuild's seed can't retire the blocked old generation. | Holds the lane, writes its oldest batch again alone (or tries the seed again) at `next_probe`. Quarantines nothing, whatever `on_error` says. | [Lift the block](#a-write-block), [open the index](#a-closed-index), [grant the permission](#a-role-without-write-permission) or [free the seed](#a-seed-that-cant-retire-the-old-generation) |
| `refused` | A document was refused under `on_error = "stop"`; `reason` names the ids. | Holds the lane, writes a [fresh build](../reference/index-and-on-error.md#a-refused-lanes-fresh-build) of the batch at `next_probe`. | [Fix the row](#a-document-refused-under-stop) |
| `restaging` | The index's target was deleted while flusso ran, a reindex couldn't create its new generation, or a restart couldn't ensure the index. | Stages that index again, alone, retrying on its own. | [Nothing, usually](#a-deleted-index), [lift what blocks the reindex](#a-reindex-that-cant-stage), or [lift what refuses the index](#an-index-that-cant-be-ensured-after-a-restart) |
| `open` | flusso writes the lane; something else stops it landing. | Restarts the sink with backoff. | [The sink itself](#the-sink-itself-health-open) |

A `refused` or `blocked` lane is **parked**. While it is, the sink's other indexes keep being written. Each retry of the held lane is a **probe**, on the [`[backoff]`](../reference/config-toml.md#backoff) schedule: 1 s after it parked, then doubling to 60 s, by default. Another index landing never shortens the wait. `flusso_lane_probes_total{outcome}` counts the probes.

### 3. Fix the cause

#### A write block

`reason` carries `cluster_block_exception` text such as `FORBIDDEN/8/index write (api)` (`index.blocks.write`) or `FORBIDDEN/5/index read-only (api)` (`index.blocks.read_only`). See which block is set, on the generation `reason` names:

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

#### A closed index

`reason` reads `blocked: index [users_1a2b3c4d_1] is closed`: someone closed that generation (`POST /<index>/_close`).

- **What it means.** A closed index takes no writes and serves no reads. flusso holds the lane and probes it like any `blocked` lane; nothing is quarantined, whatever `on_error` says.
- **Meanwhile.** The sink's other indexes keep writing. This lane holds the slot, and a [`/wait`](../reference/http.md#get-waitpositionposition) that needs the index times out. A search or get on the index fails with `index_closed_exception`, so a fenced read gets an error, never stale data.
- **What to do.** Open the generation `reason` names:

  ```sh
  curl -s -X POST localhost:9200/users_1a2b3c4d_1/_open
  ```

  Opening through the index's alias (`users`) opens the same generation. Or [reindex](#unstick-a-lane-with-a-reindex) the index on that sink: once the new generation is seeded, the closed one is dropped.
- **How to tell it worked.** `curl -s 'localhost:9200/_cat/indices/users_*?h=index,status'` shows the generation `open`. The next probe lands, at most one [`max_ms`](../reference/config-toml.md#backoff) later, and the lane catches up with no restart. Everything the index held is searchable again.

A closed index doesn't stop flusso from starting: the sink starts and only that lane parks, at its first write. flusso's own `flusso_meta` index is the exception: closed, it fails the sink's start until it's opened ([details](../reference/sink-opensearch.md#write-blocks)).

#### A disk watermark

The flood-stage block, and `index.blocks.read_only_allow_delete` set by hand (the same block), is not a `blocked` lane, except [at a seed](#a-seed-that-cant-retire-the-old-generation). OpenSearch answers it with `429`, which flusso treats as **busy**: it [resends](../reference/sink-opensearch.md#retries), and when the retries run out the whole sink restarts with backoff. The lanes stay `open` and the sink's `last_error` says `still refused as busy`. Free disk space (`curl -s 'localhost:9200/_cat/allocation?v'` shows usage per node); OpenSearch lifts the block once usage drops below the high watermark, and the sink's next restart writes.

#### A seed that can't retire the old generation

`reason` reads `blocked: the new generation users_1a2b3c4d_2 is built, but the old generation users_1a2b3c4d_1 can't be retired: …`, then OpenSearch's words and the ways out.

- **What it means.** A reindex (or the rebuild after a lost slot) finished its backfill, but the generation it replaces carries `index.blocks.read_only` or `read_only_allow_delete`, so OpenSearch refuses to move the aliases off it. The seed is the lane's probe, tried again on the backoff schedule ([details](../reference/sink-opensearch.md#an-old-generation-that-cant-be-retired)).
- **Meanwhile.** Nothing restarts and `/readyz` stays ready. Readers stay on the old generation, the index stays `backfilling`, and the sink's other indexes keep being written. This lane holds the slot: its live changes wait behind the seed. `FlussoLaneParked` fires after 5 minutes, and `FlussoLaneStalled` can follow after 10.
- **What to do.** Free the generation the reason names as old. The next probe seeds, with no restart:

  | Block on the old generation | Way out |
  | --- | --- |
  | `index.blocks.read_only` | Lift it ([as above](#a-write-block)). OpenSearch refuses a delete too. |
  | `index.blocks.read_only_allow_delete` | Free disk space, if disk usage set it: OpenSearch lifts the block once usage falls below the high watermark. Or lift it. Or, last, delete the old generation (`curl -s -X DELETE localhost:9200/users_1a2b3c4d_1`): both aliases go with it, so reads through them answer `404` until the next probe, at most one [`max_ms`](../reference/config-toml.md#backoff) later. |

  A [reindex](#unstick-a-lane-with-a-reindex) doesn't help: it walks the backfill again into the same new generation and parks at the seed again.
- **How to tell it worked.** The log says `the parked lane's probe landed; the lane is open`, the aliases name the new generation, the old one is gone, and the index reads `seeded`.

#### A role without write permission

`reason` carries a `security_exception` naming the role and the index. Grant the role `index` (or `write`) permission on the index pattern flusso writes (`users_*` and its aliases, or the deployment's [prefix](../reference/config-toml.md#prefix)). The next probe lands.

#### A document refused under `stop`

`reason` reads `refused: id 42 (…)` with OpenSearch's words. Each probe builds the refused documents again from Postgres (a [fresh build](../reference/index-and-on-error.md#a-refused-lanes-fresh-build)), so a fix in the database lands at the next probe, with no restart. The ways out, in order:

1. **Fix the row.** The next probe lands and the lane opens.
2. **Delete the row.** The next probe writes the delete, which lands.
3. **Set `on_error = "skip"`** on that index and restart. The refused document is quarantined; the row's next change indexes it. See [Handle rejected documents](rejected-documents.md).
4. **Change the schema** when the mapping can't take the value (a wider type, a `custom` type, a `default`), and restart. A schema change builds a new index.
5. **Reindex**, for `Limit of total fields … exceeded`: raise the limit with [`index_settings`](../reference/sink-opensearch.md#index-settings), restart, and [reindex](#unstick-a-lane-with-a-reindex).

#### A deleted index

A `restaging` lane fixes itself: flusso creates a fresh generation for that index, backfills it, and opens the lane, while the other indexes keep writing. Reads through the alias return 404 until the generation exists, and `/wait` answers `503 backfilling` for the index until its backfill completes. A lane that stays `restaging` is one whose staging keeps failing; the log says `could not stage the index again; trying again later` with the cause, retried with the same backoff.

An index deleted **mid-backfill** is backfilled again from the first row, and stays `backfilling` until that fresh backfill lands. Where `action.auto_create_index` lets a write recreate the generation, the lane never goes `restaging`: the writes land in a dynamically mapped index until the seed or the next start replaces it. If that mapping refuses a document under `on_error = "stop"`, the lane parks `refused` and stays parked; restart flusso to replace the index. See [Why forbid auto-create](../reference/sink-opensearch.md#why-forbid-auto-create).

#### Unstick a lane with a reindex

A [reindex](reindex.md) of the stuck index on that sink opens a `refused`, `blocked` or `restaging` lane as soon as it is staged, unless the lane is parked on its [seed](#a-seed-that-cant-retire-the-old-generation):

```sh
flusso reindex users --sink primary --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
```

The item the lane was stuck on comes from the new generation's backfill instead of being written again ([details](../reference/http.md#a-reindex-of-a-lane-flusso-doesnt-write)).

| The lane is stuck on | Reindex? |
| --- | --- |
| `index.blocks.write` on that index you can't or won't lift | Yes |
| `index.blocks.read_only` on that index | No: the seed can't retire the blocked generation and parks. Lift the block |
| A seed that can't retire the old generation | No: the backfill runs again and parks at the same seed. [Free the seed](#a-seed-that-cant-retire-the-old-generation) |
| A closed index you won't open | Yes: the closed generation is dropped at the seed |
| A mapping or settings changed by hand on the index | Yes |
| A `restaging` lane whose staging keeps failing | Yes, unless the cause is the whole cluster |
| A `refused` row the mapping can't take | No: the new backfill is refused at the same row. [Fix the row](#a-document-refused-under-stop), or set `on_error = "skip"` |
| A block on the whole cluster | No: the reindex can't stage. Lift the block |

- **Until the backfill is seeded** the lane reads `open` but still holds the slot, and its `stall_seconds` keeps growing: `FlussoLaneStalled` can fire on a long backfill. `pinned_by` can name the lane meanwhile.
- **If it can't stage**, the lane turns `restaging`: [A reindex that can't stage](#a-reindex-that-cant-stage).
- **After a restart of flusso**, check the lane: a reindex still waiting may be gone. Ask again if it's still stuck.

#### A reindex that can't stage

`reason` reads `the reindex could not stage: …` (or `the reindex could not ask for its backfill: …`) followed by the error, and the log says `the reindex could not stage; the lane waits for it and it is tried again`.

- **What it means.** A [reindex](#unstick-a-lane-with-a-reindex) of this index was accepted (`202`), but its new generation couldn't be created, or its backfill couldn't be asked for. flusso retries it on its own, on the [`[backoff]`](../reference/config-toml.md#backoff) schedule; a momentary error heals by itself.
- **Meanwhile.** The sink's other indexes keep being written. This lane writes nothing and holds the slot before its oldest change. A `/wait` on the index answers `503 rebuilding` at its timeout. `FlussoLaneParked` fires after 5 minutes (`why="restaging"`).
- **What to do.** Lift what `reason` names: often a block on the whole cluster, or a role that can't create indexes. The next retry completes the reindex.
- **How to tell it worked.** The lane turns `open` and the log says `reindexed: the lane is open; …`. The index goes `backfilling`, then `seeded`.
- **After a restart of flusso** the retry may be gone: ask for the reindex again if the lane is still stuck ([what a restart keeps](../reference/http.md#when-the-reindex-cant-stage)).

#### An index that can't be ensured after a restart

`reason` reads `could not be ensured: …` with OpenSearch's answer, or `not tried: 8 indexes in a row could not be ensured before it`. The log says `the index was not ensured; its lane is staged again on its own while the sink's other indexes go on`.

- **What it means.** The sink's run restarted (a failed write, the cluster briefly unreachable), or flusso started with its replication slot kept, and this index couldn't be ensured while others could. OpenSearch refuses that index alone: say the role can't read its settings or create its generation, or a generation that must be created hits the cluster's shard limit. `not tried` means flusso didn't get to the index: the cluster stopped answering part way.
- **Meanwhile.** The sink's other indexes keep being written. This lane writes nothing and holds the slot and its fences. `FlussoLaneParked` fires after 5 minutes (`why="restaging"`).
- **What to do.** Lift what `reason` names; for `not tried`, check the cluster is answering. flusso retries on its own, at once and then on the [`[backoff]`](../reference/config-toml.md#backoff) schedule, with no restart. The log line `could not stage the index again; trying again later` carries each try's error.
- **How to tell it worked.** The log says `the index is staged again; the lane is open`, `health` turns `open` and the lane catches up. A backfill that was running goes on from where it got (from the first row if the generation was recreated meanwhile).

A cluster that is down still holds the whole sink, and a start with a new replication slot still waits for every index: [When a restart can't ensure an index](../reference/http.md#when-a-restart-cant-ensure-an-index) has what tells the two apart, and its limits.

#### The sink itself (health `open`)

The lane is written but nothing lands. The log has OpenSearch's reason and `sinks.<sink>.last_error` the latest:

- **The cluster is down or refusing requests**: bring it back. The sink restarts with backoff until it can write.
- **Busy (`429`/`503`) for good**: busy refusals are resent, so a lane stalled behind them means the cluster never had room. Add capacity or lower the load.
- **A wrong mapping on an index flusso didn't create**: [reindex](reindex.md) the index on that sink.

### 4. Watch it recover

- The log says `the parked lane's probe landed; the lane is open` (or `the index is staged again; the lane is open`, or `reindexed: the lane is open; …`), then `a stalled lane is landing again`.
- `health` is back to `open`, `stall_seconds` drops to 0, the `flusso_lane_parked` series disappears.
- `pinned_by` moves on and the slot releases the WAL it kept.
- If the index was exempt, the log says `index no longer exempt: it paces the reading again at max_waiting_per_index` once fewer than `max_waiting_per_index` of its documents wait. While it drains, it may say `exempt index frozen: …` first.

## When one index is stuck beside others

With two or more indexes, a stuck index stops only itself while another index is working. Once it has [`max_waiting_per_index`](../reference/config-toml.md#reading-ahead) documents waiting (512 by default), it becomes [exempt](../reference/config-toml.md#a-stuck-index), and the log says so:

```text
WARN index held at max_waiting_per_index: exempt, so reading goes on for the other indexes; its changes are kept as documents waiting, up to max_pending_documents for every exempt index, and the replication slot stays pinned by it, so WAL grows index=users waiting=512
```

What that means:

- **Still working:** the other indexes are read, built and written, and a `/wait` naming only them answers.
- **Growing:** the stuck index's documents waiting to be built (each once, however often it changes), and the WAL the slot keeps. `flusso_replication_slot_lag_bytes` climbs while `pinned_by` names the stuck index.
- **Shown per index:** `/status`'s `ingest.indexes.<index>` reads `"exemption": "exempt"`, `flusso_index_exempt{index}` is 1, and `FlussoIndexBacklog` fires after 10 minutes ([below](#when-flussoindexbacklog-fires)). The log lines repeat at most once a minute, and the last one names the current state ([the log lines](../reference/config-toml.md#a-stuck-index)).
- **`exempt index frozen: …`** means the index stopped growing: its lane recovered, or no other index is working any more. It builds its backlog down, and reading stops for every index while it has that many documents waiting.
- **With one index** there is nothing to read on for: reading stops at its waiting bound.

**At the cap, everything stops.** When the exempt indexes together have [`max_pending_documents`](../reference/config-toml.md#a-stuck-index) documents waiting (a million by default), reading stops for every index:

```text
WARN reading stopped for every index: the exempt indexes have max_pending_documents documents waiting to be built; it reads on as they are built documents=1000000 max_pending_documents=1000000
```

**A whole sink down** (every index stuck at once) stops the reading within about 5 seconds, at `max_waiting_per_index` or `max_waiting`. One case reads on: an index that gets no changes during the outage counts as working, so the stuck ones become exempt and read on up to `max_pending_documents`. flusso can't tell that from one stuck index beside a healthy, quiet one. The first document that index sends and the sink doesn't acknowledge within 5 seconds stops the reading ([details](../reference/config-toml.md#a-stuck-index)). Either way, fix the sink ([The sink itself](#the-sink-itself-health-open)).

**What to do.**

1. **Fix the stuck lane** ([steps above](#steps)). Once it lands, the index builds its waiting documents oldest first, beside the others ([Watch it recover](#4-watch-it-recover)). If reading had stopped at the cap, the log says `the exempt indexes are below max_pending_documents: reading on`.
2. **Watch the WAL** against `max_slot_wal_keep_size` while it lasts: the other indexes keeping up doesn't release any of it.
3. **One index stuck while others keep working, and the cap is near?** Raise `max_pending_documents` if memory allows (about 270 B a document). It is read at start, so it takes a restart, and the restart reads again all the WAL the slot kept. Don't raise it for a whole sink down: it only takes more memory before the reading stops.

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

## When FlussoIndexBacklog fires

`FlussoIndexBacklog` names one index (its `index` label) that has been **exempt** or **stuck** for 10 minutes:

```promql
max by (index) (flusso_index_exempt) > 0 or max by (index) (flusso_index_backlog_age_seconds) > 60
```

- **Exempt:** held at `max_waiting_per_index` while the other indexes read on. Its changes are kept as documents to build, up to `max_pending_documents`, and the slot stays pinned by it ([above](#when-one-index-is-stuck-beside-others)).
- **Stuck:** its oldest change not built yet was read over 60 s ago, so nothing of it is being built ([`backlog_seconds`](../reference/http.md#what-backlog_seconds-shows)).

It ships in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`, with `for: 10m` and severity `page`. The chart sets the 60 s and the 10 minutes ([Helm values](../reference/helm-values.md)).

**Read why.** Ask flusso about that index ([fields](../reference/http.md#per-index-ingest-state)):

```sh
curl -s localhost:9464/indexes/users/status | jq
```

| What it shows | What it means | What to do |
| --- | --- | --- |
| `held_by: "lane"`, `held_sinks: ["primary"]` | A lane of that sink is full: the sink is slow, refusing or down. `FlussoLaneStalled` or `FlussoLaneParked` usually fires beside it. | Read the lane, `sinks.primary.lanes.users` in `/status`, and fix it from [step 2](#2-read-its-health). |
| `working: false` | A sink has acknowledged nothing of the index for 5 s. | The same: find its lane whose `stall_seconds` climbs or whose `health` isn't `open`. |
| `held_by: "budget"`, `working: true` | The memory budget refuses its live batch, but its sinks keep acknowledging: slow, not stuck. | Find the slow side in [When an index's backlog holds the slot](#when-an-indexs-backlog-holds-the-slot). |
| `held_by: null`, `working: true`, `backlog_seconds` past 60 | Nothing holds it and its sinks acknowledge, yet its oldest change has waited a minute: its builds are that slow. | The `build` stage and the document queries, in [the same table](#when-an-indexs-backlog-holds-the-slot). |
| `exemption: "frozen"` | Its hold ended, or no other index works: it drains but doesn't grow. | Watch `documents_waiting` fall. |
| `pacing_reader: true` | Reading for every index waits on this one right now. | Nothing on its own: this index sets the pace. |
| `state: "down"` | Its live work failed, and it waits to restart alone. | Read `last_error`: [When an index is down](#when-an-index-is-down-or-its-backfill-keeps-failing). |

Once the cause is fixed, the index builds its waiting documents down, `exemption` returns to `none`, `backlog_seconds` falls to 0, and the alert clears.

- **A planned catch-up pages too.** With several indexes, after a sink outage or a burst, an index slower than the others is exempt while they read on, for as long as it trails. A catch-up longer than 10 minutes fires the alert: silence it for the catch-up, or raise `indexBacklogFor`.
- **Quiet** for a down index with nothing waiting and for a backfill that keeps failing ([above](#when-an-index-is-down-or-its-backfill-keeps-failing)), for a healthy index at its sink's limit, for an index merely behind its source (its backlog stays seconds old), while flusso starts, between engine runs (a restart's backoff, a source down) and while it stops.
- **Not a lag alert.** How far an index trails the source is its lanes' WAL behind (`wal_behind_bytes` in `/status`, `flusso_lane_wal_behind_bytes{sink,index}`) and the slot lag (`flusso_replication_slot_lag_bytes`).

## When an index is down or its backfill keeps failing

With two or more indexes, a failure of one index's own work restarts that index alone, after a wait ([A failing index](../reference/config-toml.md#a-failing-index)). It isn't a lane: every lane stays `open`. It comes in two shapes:

| | An index is down | A backfill keeps failing |
| --- | --- | --- |
| **What failed** | its live work: a build, or a live document under `on_error = "stop"` | its backfill: reading root keys, a backfill batch, a backfill document under `stop` |
| **The log** | `ERROR the index's builder failed; …` with `kind="live"`, then `INFO the index's builder restarted: …` after the wait | `ERROR the index's backfill failed; …` with `kind="backfill"`, then `INFO the index's backfill started again, …` |
| **`/status`** | `"state": "down"`, `down_since`, `working: false` | `"state": "running"`, `"backfill": {"state": "retrying", "retry_at": …}` |
| **Metrics** | `flusso_index_down{index} 1` | `flusso_index_backfill_retrying{index} 1` |
| **Still working** | every other index; a `/wait` naming only them; `/readyz` | the index's live changes, and every other index |
| **Growing** | its documents waiting, and the WAL the slot keeps | nothing; the index stays unseeded on the sinks that asked until the backfill completes |
| **Pages** | `FlussoIndexBacklog` after 10 minutes, **while documents of it wait** | nobody |

Both count in `flusso_index_restarts_total{index,kind}` each time the work starts again. The dev dashboard plots both: *Indexes down or retrying a backfill* and *Index restarts, by kind*.

**Two cases page nobody**, since no alert ships for them yet ([#455](https://github.com/alias2k/flusso/issues/455)):

- **A down index with nothing waiting**: no live change touched it since it went down (its backfill failed, or the index is quiet). Its backlog is 0 and it isn't exempt.
- **A backfill that keeps failing while live changes flow**: the index is running and working.

To page on them, add a rule of your own on `flusso_index_down`, `flusso_index_backfill_retrying` or a climbing `flusso_index_restarts_total`, with the `for:` your team tolerates. For example:

```promql
max by (index) (flusso_index_down) > 0 or max by (index) (flusso_index_backfill_retrying) > 0
```

**Read why.** `last_error` holds the index's last failure of each kind, `live` and `backfill`, and stays after it recovered ([fields](../reference/http.md#per-index-ingest-state)):

```sh
curl -s localhost:9464/indexes/orders/status | jq '{state, down_since, last_error, restarts, backfill}'
```

`message` is the error as the log printed it, cut at 4 KiB. What it usually says:

| `message` | Cause | What to do |
| --- | --- | --- |
| ``document `42` in index `orders` could not be built: …`` | A script failed on that row, or an `array: true` column held an object, under `on_error = "stop"`. It fails every time. | Fix the row or the script; or set `on_error = "skip"` for the index and restart, then fix it ([Run scripts in production](../author/scripts/production.md)). |
| A query Postgres refuses, naming the index and `flusso check` | The schema and the database disagree: a column dropped or retyped, a link Postgres can't compare. | Run `flusso check`, then fix the schema or the database. The next retry builds it, with no restart. |
| An integer out of range | An [aggregate expression](../reference/aggregates.md) overflows on one row. | Fix the schema or the row. |
| A connection lost, a timeout | Postgres was briefly unreachable for that build. | Usually nothing: the next retry builds it. Many in a row mean the database, not the index. |

**What to do.**

1. **Read `last_error`** as above, and fix the cause. A fix in the data or the database needs no restart: the next retry picks it up, at most `[backoff] max_ms` later (60 s by default). A schema, script or `on_error` change needs one.
2. **Watch the WAL** while an index is down: `flusso_replication_slot_lag_bytes` climbs while `pinned_by` names the index. Past `max_slot_wal_keep_size`, Postgres drops the slot ([Recover from a dropped slot](dropped-slot.md)).
3. **Watch the cap.** A down index's documents count toward `max_pending_documents` once it is exempt; when they reach it, reading stops for every index ([When one index is stuck beside others](#when-one-index-is-stuck-beside-others)).
4. **Don't stop flusso to fix it**, if you can avoid it. A stop tries the down index once more; if that fails, flusso exits `1`, and the next start reads again every change since the down index's oldest, for every index.

**Watch it recover.** The `INFO` line logs, `state` returns to `running` (or `backfill` to `walking`, then `null`), `flusso_index_down` and `flusso_index_backfill_retrying` drop to 0, `restarts` counts one more, and `last_error` stays as the record of what happened.

**With one index**, none of this applies: a failure restarts the whole ingest engine. `phase` goes back to `starting`, `flusso_errors_total{engine="ingest"}` climbs, and the per-index `state` stays `running`.

## Options and variations

- **The lane alerts.** `FlussoLaneParked` fires once `flusso_lane_parked > 0` has held for 5 minutes. `FlussoLaneStalled` fires on a lane landing nothing for 10 minutes (`reason: age`), or on more than 1 GiB of WAL kept behind a lane that has landed nothing for a minute (`reason: wal`). Both ship in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`; tune them in the [Helm values](../reference/helm-values.md). Only `FlussoLaneParked` sees a refused **backfill** page: backfill rows carry no position, so their lane never ages. The per-index alert is [`FlussoIndexBacklog`](#when-flussoindexbacklog-fires).
- **Probe sooner, or less often.** [`[backoff]`](../reference/config-toml.md#backoff) sets the first and the longest wait between probes, for every lane, every engine restart and every index restart. A short `max_ms` unparks a lane sooner once its cause is fixed; each probe of a lane still refused costs one bulk request.
- **`/readyz` stays ready.** A parked lane doesn't make flusso unready: the other indexes are written and served, and a restart wouldn't help. Alert on the lane, not on readiness.
- **A slow lane isn't stalled.** A lane that lands, just slowly, fires neither alert; `flusso:backlog_drain_eta_seconds` shows how long it will take. flusso logs a warning once a lane passes 10 minutes without landing, and every 10 minutes after.
- **A backfill that stops making progress** shows as the index staying `backfilling` in `/status` while `flusso.documents.built` stops moving. A live change queued behind it on the same lane does age, and alerts.
- **A stop with a lane not open.** A parked lane is probed once more at once; one that still can't land holds the drain until `--shutdown-timeout-secs`, then flusso exits `1` naming it: `primary: 3 changes in flight, lane users blocked since 2026-11-19T12:03:11Z (blocked: …)`. A lane parked on its seed, a `restaging` lane, or one waiting on a reindex's backfill, doesn't hold the stop: its changes are left for the next start. Each case is in [Lanes flusso doesn't write at a stop](../reference/cli.md#lanes-flusso-doesnt-write-at-a-stop).
- **A process restart resets the stall age.** The age is kept in memory, so a process that crash-loops never ages a lane into `FlussoLaneStalled`. Alert on the process too: `up{job="<flusso's scrape job>"} == 0` for a few minutes, or under Kubernetes `increase(kube_pod_container_status_restarts_total{container="flusso"}[15m]) > 2`.

## Related

- [`/status`](../reference/http.md#get-status) and [Metrics](../reference/metrics.md)
- [Handle rejected documents](rejected-documents.md)
- [Sink: OpenSearch — write blocks](../reference/sink-opensearch.md#write-blocks)
- [Recover from a dropped slot](dropped-slot.md)
