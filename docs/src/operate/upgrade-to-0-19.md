# Upgrade to 0.19

0.19 keeps a failure of one index on that index. Each change below links to its section.

**Check before upgrading**

- **A cut-down OpenSearch role needs `indices:data/read/mget`** on `{prefix}flusso_meta`: staging now reads three meta documents per index in one `_mget`; a sink whose role can't, or whose proxy answers that request `404`, retries its staging and writes nothing: [Staging reads the meta documents with one request](#staging-reads-the-meta-documents-with-one-request).

**Running flusso**

- **An index that can't be written parks alone**; the sink's other indexes keep writing, and a write block or a closed index no longer drops documents under `on_error = "skip"` (#404): [One index's failure parks its own lane](#one-indexs-failure-parks-its-own-lane).
- **A closed index no longer stops its sink from starting**; only its lane parks, and opening it resumes the lane: [A closed index parks its lane](#a-closed-index-parks-its-lane).
- **An index that can't be ensured no longer holds its sink after a restart**: only its lane waits, `restaging`, and it opens on its own once the index can be ensured; a start with a new replication slot still waits for every index: [One index that can't be ensured waits alone](#one-index-that-cant-be-ensured-waits-alone).
- **`on_error = "stop"` no longer stops the sink**, and `/readyz` stays `200` while an index is parked; alert on the new `FlussoLaneParked` instead: [Alerting on a parked index](#alerting-on-a-parked-index).
- **A reindex unsticks a stuck lane**: it opens a `refused`, `blocked` or `restaging` lane, and one that can't stage is retried on its own instead of dropped: [A reindex unsticks a lane](#a-reindex-unsticks-a-lane).
- **A reindex past a read-only generation no longer restarts the sink**: its seed parks the index's lane `blocked` until the old generation is freed, and `/readyz` stays ready: [A seed parks on a read-only old generation](#a-seed-parks-on-a-read-only-old-generation).
- **The retry wait is configurable**: `[backoff]` sets the first and the longest wait of every restart and probe: [Retry timing is configurable](#retry-timing-is-configurable).
- **A generation deleted mid-backfill is backfilled again in full**, never seeded with rows missing; a cut-down OpenSearch role needs to read index settings (#428): [A generation deleted mid-backfill](#a-generation-deleted-mid-backfill).
- **`pinned_by` can name an index alone**, with `sink` null in `/status` and no `sink` label on `flusso_slot_pinned_by`: [An index's backlog can pin the slot](#an-indexs-backlog-can-pin-the-slot).
- **A sink's committed count waits for every index**: `changes_committed` and `flusso_changes_committed_total` count a change once every lane of the sink has written it, so one slow index holds `changes_in_flight` for the whole sink: [Committed means written on every index](#committed-means-written-on-every-index).
- **One index's live documents hold at most half of the memory budget** by default, with two or more indexes, so the others keep building beside one in a burst; `[concurrency] index_budget_share` sets the share: [Each index has a share of the memory budget](#each-index-has-a-share-of-the-memory-budget).
- **flusso reads ahead of its builds**, up to `[concurrency] max_waiting_per_index` and `max_waiting`, and a stop builds everything it read before it exits: [Reading runs ahead of the builds](#reading-runs-ahead-of-the-builds).
- **Batches are per index**, so a slow index no longer delays a fast one's writes; one build takes at most `[batch] max_changes` documents; a change can be searchable in one index before another, and the stdout sink's `seq` is ordered per index only: [Batches are per index](#batches-are-per-index).
- **A stuck index stops only itself**: with two or more indexes, the others keep being read and written while its changes wait as documents to build, up to `[concurrency] max_pending_documents`; the slot, and so the WAL, still waits for it: [A stuck index stops only itself](#a-stuck-index-stops-only-itself).
- **Backfills of several indexes run at once**, within the same `builds` and Postgres connections; one stuck backfill no longer holds the others, and memory may pass `max_built_mib` by one backfill batch per index: [Backfills run at once](#backfills-run-at-once).
- **One index's failure restarts only that index**: with two or more indexes, a failed build or a document failed under `on_error = "stop"` takes down its index alone, a failed backfill is retried alone while live changes flow, and the others keep indexing; with one index, a build that panics as it starts restarts the ingest engine instead of ending the deployment: [One index's failure restarts only that index](#one-indexs-failure-restarts-only-that-index).
- **`POST /reindex` is gone** (breaking): reindex with `POST /indexes/<name>/reindex[?sink=<name>]`, which answers JSON; `flusso reindex` already does: [Reindex moved to the index path](#reindex-moved-to-the-index-path).
- **Each index's state is shown**: `/status`'s `ingest.indexes` (with `state`, `last_error` and `restarts`), `GET /indexes/<name>/status`, the `flusso_index_*` metrics, `flusso_reader_waits_seconds`, and a new alert, `FlussoIndexBacklog`: [Each index's state is shown](#each-indexs-state-is-shown).

**Code that embeds the crates**

- **New lane-health API, `EngineError::DocumentsRejected` removed, order per index for an ordered sink, `DaemonOptions::max_restart_backoff` replaced by `[backoff]`, `Sink::ensure_index` returns `Ensured`, backfill items carry a `Ticket`, `pinned_by` answers an `engine::Pin`, `Observer::on_changes_settled` removed, `engine::Budget` shared per index, lane items keyed by a `stream::ItemKey`, `max_pending_documents`, the per-index ingest state in `Status`, `Observer::on_reader_wait`, per-index restart (`IngestEngine::with_backoff`, `Observer::on_index_failed` and `on_index_restarted`), and `Sink::detachment`**: [For code embedding the crates](#for-code-embedding-the-crates).

## Staging reads the meta documents with one request

**What changed.** Every staging of an index (a start, a sink's restart, a reindex, and their retries) first reads three documents of `{prefix}flusso_meta` in one `POST /{prefix}flusso_meta/_mget`: the index's seed marker and two reserved ones, `detach:{alias}` and `stuck:{alias}`. No released flusso writes the reserved ones. 0.18 read only the seed marker, with a `GET`. What is read, and which answers fail the staging: [The meta documents](../reference/sink-opensearch.md#the-meta-documents).

**Who is affected.**

- Deployments whose OpenSearch role is cut down to the calls 0.18 made, without `indices:data/read/mget` on `{prefix}flusso_meta`.
- Deployments reaching OpenSearch through a reverse proxy or a path that answers this request with a `404` of its own: only OpenSearch's `index_not_found_exception` reads as "nothing kept".

**What to do.**

1. **Grant the role `indices:data/read/mget`** on `{prefix}flusso_meta`, if it is a cut-down one. The `read` action group has it.
2. **Let `_mget` through to OpenSearch**, if a proxy sits in front of it.
3. **Upgrade flusso.** Nothing is rebuilt and no content hash moves.

**How it shows.** Every index of the sink fails its staging, so the whole sink waits and retries with backoff, and a start reads nothing meanwhile ([when the destination is down](../reference/http.md#when-a-restart-cant-ensure-an-index)). The log says `staging failed; retrying`, each index's error reading `reading the detach records failed: HTTP 403 Forbidden: …` (`HTTP 404 Not Found` behind the proxy). The same deployment started under 0.18.

## One index's failure parks its own lane

**What changed.** In 0.18 a failure about one index ended the sink engine's run, and the daemon restarted the whole sink with backoff: every index on that sink paused. In 0.19 the failure stays on that index's **lane** (one index flowing into one sink), and the sink's other indexes keep being written:

| What OpenSearch answers | 0.18 | 0.19 |
| --- | --- | --- |
| A document refused under `on_error = "stop"` | the sink stops and restarts with backoff | the lane is **refused** |
| A write block (`index.blocks.write`, `read_only`: 403 `cluster_block_exception`) | an item-level rejection: **quarantined under `skip`** (documents lost), the sink stops under `stop` | the lane is **blocked**, whatever `on_error` says; nothing is quarantined |
| An item-level 403 `security_exception` (the role can't write that index) | an item-level rejection | the lane is **blocked** |
| A closed index (`POST /<index>/_close`: 400 `index_closed_exception`) | an item-level rejection: **quarantined under `skip`**, the sink stops under `stop` | the lane is **blocked**, whatever `on_error` says; nothing is quarantined ([details](#a-closed-index-parks-its-lane)) |
| The index's generation deleted while running (`index_not_found_exception`) | the sink stops; its restart recreates the generation | that index alone is **staged again** and backfilled |
| The whole `_bulk` refused, credentials refused, the cluster unreachable, busy past `max_retries` (`read_only_allow_delete` included: 429, busy) | the sink stops and restarts | unchanged |

A refused or blocked lane is **parked**: it writes its oldest batch again alone (a **probe**) after 1 s, then with the wait doubling up to 60 s ([configurable](#retry-timing-is-configurable)). A refused lane's probe writes a [fresh build](../reference/index-and-on-error.md#a-refused-lanes-fresh-build) of that batch, built again from the database, so fixing or deleting the row lets it land. When a probe lands, the lane opens and the rest follows, with no restart. Meanwhile the lane holds the replication slot, as a stalled lane always has.

**Who is affected.** Every deployment that writes more than one index to a sink: one bad document or one blocked index no longer holds up the others. Deployments on `on_error = "skip"` whose indexes ever got a write block or were closed: those documents used to be quarantined, and are now written once the block is lifted or the index opened.

**What to do.**

1. **Upgrade flusso.** Nothing to rebuild: no content hash moves.
2. **Lift write blocks, or open closed indexes, to resume.** A blocked index is written by the next probe once the block is lifted or the index opened, at most one `[backoff] max_ms` later (60 s by default). No restart. The runbook is [A lane is stalled](stalled-lane.md).
3. **Fix a refused document in the database.** The next probe builds it again from the row and lands, with no restart. Deleting the row works too, as do switching the index to `skip` and changing the schema ([A lane is stalled](stalled-lane.md#a-document-refused-under-stop)).

**What to know.**

- **Memory.** A parked lane keeps its built documents until they land, and the documents queued behind it hold part of the [memory budget](../reference/config-toml.md#concurrency). With two or more indexes it fills only that index's [share](#each-index-has-a-share-of-the-memory-budget), and the other indexes keep building. Parked long enough, its later changes wait as documents to build while the others keep going, up to `max_pending_documents` ([A stuck index stops only itself](#a-stuck-index-stops-only-itself)).
- **A stop with a parked lane exits `1`.** The stop probes every parked lane once more at once, so a block lifted meanwhile still drains. A lane that still can't land holds the drain until `--shutdown-timeout-secs`, and the exit message names it: `primary: 3 changes in flight, lane users blocked since … (blocked: index [users_…] blocked by: [FORBIDDEN/8/index write (api)];)`. The next start writes the parked batches again first.
- **Not yet.** A reindex of one index without pausing the sink's other writes, and dropping a stalled index automatically, are a later release ([#394](https://github.com/alias2k/flusso/issues/394)).

## Alerting on a parked index

**What changed.** A refusal under `stop` no longer moves the sink to phase `failed`, restarts nothing, and doesn't count in `errors`. `/readyz` stays `200` while a lane is parked: the other indexes are written and served, and a restart wouldn't unpark it.

**Who is affected.** Anyone alerting on a sink's `failed` phase, on restarts, or on readiness to catch a refused document.

**What to do.** Alert on the lane:

- **`FlussoLaneParked`** is new in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`. It fires once `flusso_lane_parked > 0` has held for 5 minutes, naming the index, the sink and `why` (`refused`, `blocked`, `restaging`). With your own rules, copy it or alert on [`flusso_lane_parked`](../reference/metrics.md) directly.
- `FlussoLaneStalled` still fires after 10 minutes for a parked live change, but never for a refused backfill page, which carries no position: only `FlussoLaneParked` sees that one.
- `/status` shows each lane's state under [`sinks.<sink>.lanes.<index>.health`](../reference/http.md#lane-health) (`open`, `refused`, `blocked`, `restaging`), with `reason` and `next_probe`.

## A reindex unsticks a lane

**What changed.** A [reindex](reindex.md) of an index whose lane is `refused`, `blocked` or `restaging` opens the lane as soon as it is staged. The item the lane was stuck on comes from the new generation's backfill instead of being written again. A reindex whose new generation can't be created is retried on the [`[backoff]`](#retry-timing-is-configurable) schedule, its lane `restaging` meanwhile, while the sink's other indexes keep writing.

**Who is affected.** Anyone with a lane stuck on something in the index: `index.blocks.write` that won't be lifted, a mapping or settings changed by hand.

**What to do.** Nothing to configure. Reindex the index on that sink: [Unstick a lane with a reindex](stalled-lane.md#unstick-a-lane-with-a-reindex).

**What to know.**

- **Not every lane.** A row the mapping refuses is refused again by the new backfill: fix the row. `index.blocks.read_only` stops the new generation's seed: [lift it](#a-seed-parks-on-a-read-only-old-generation).
- **The slot waits for the backfill.** Until the new generation is seeded, the lane reads `open` but holds the slot, so `FlussoLaneStalled` can fire on a long backfill.
- **A waiting reindex lives in memory.** After a restart of flusso, ask again if the lane is still stuck ([what a restart keeps](../reference/http.md#when-the-reindex-cant-stage)).
- **A stop doesn't wait for such a lane**: [Lanes flusso doesn't write at a stop](../reference/cli.md#lanes-flusso-doesnt-write-at-a-stop).

## A closed index parks its lane

**What changed.** An index closed with `POST /<index>/_close` is a `blocked` lane, like a write block:

| | 0.18 | 0.19 |
| --- | --- | --- |
| Writes to the closed index | rejected item by item: quarantined under `skip` (documents lost), the sink stops under `stop` | the lane parks `blocked`, whatever `on_error` says; nothing is quarantined |
| Starting with the index closed | the sink's start fails, so none of its indexes is written | the sink starts; only the closed index's lane parks, at its first write |
| Opening the index | under `stop`, the sink's next restart writes again; under `skip`, later writes land and the quarantined ones stay lost | the next probe lands, with no restart, and nothing was lost |

`/status` shows the lane's `reason` as `blocked: index [<generation>] is closed`, and `flusso_lane_parked{why="blocked"}` is `1`.

**Who is affected.** Anyone who closes an index flusso writes.

**What to do.** Nothing to configure. To resume, open the generation the reason names, or reindex the index on that sink: [A closed index](stalled-lane.md#a-closed-index).

**What to know.**

- **The closed lane holds the slot** and its fences, as any parked lane does; the sink's other indexes keep writing.
- **A stop doesn't wait it out.** The stop probes the lane once; still closed, it holds the drain until `--shutdown-timeout-secs` and flusso exits `1`.
- **`flusso_meta` is not covered.** A closed `{prefix}flusso_meta` still fails the sink's start: open it.

## One index that can't be ensured waits alone

**What changed.** When a sink's run restarts, flusso ensures each of its indexes again (checks the generation and its `flusso_meta` record, creating what's missing). In 0.18 one index that couldn't be ensured failed that step for the whole sink. In 0.19 that index's lane waits alone:

| | 0.18 | 0.19 |
| --- | --- | --- |
| One index can't be ensured after a sink's restart | the sink retries with backoff; none of its indexes is written | that index's lane is `restaging` with `could not be ensured: <error>`; the sink's other indexes write |
| The same at a start whose replication slot was kept | the start retries; nothing is read for any sink | the start goes on without that index |
| The same at a start with a new slot | the start retries until every index is ensured | unchanged |
| `/readyz` meanwhile | `503`: the sink is `failed`, or the deployment `starting` | `200`, unless something else holds it |
| The cluster down | the sink retries with backoff | unchanged |
| A cluster that accepts connections and never answers, at a restart | read as down after one request timeout (30 s by default) | after up to nine (270 s by default) |

The waiting lane is tried again at once, then on the [`[backoff]`](#retry-timing-is-configurable) schedule, and opens with no restart. A backfill that was running for it goes on from where it got, or from the first row if its generation was recreated meanwhile. Meanwhile it holds the replication slot and its fences, and `FlussoLaneParked` fires after 5 minutes (`why="restaging"`).

**Who is affected.** Deployments with more than one index on a sink: a refusal about one index no longer stops the others after a failed write or a restart.

**What to do.** Nothing to configure. Alert on `FlussoLaneParked` ([Alerting on a parked index](#alerting-on-a-parked-index)): `/readyz` and the sink's `failed` phase no longer show it. The runbook is [An index that can't be ensured after a restart](stalled-lane.md#an-index-that-cant-be-ensured-after-a-restart).

**What to know.**

- **How flusso tells one index from a cluster that is down.** If no index can be ensured, the cluster is down. A sink whose first sixteen indexes, in index-name order, are all refused while the rest are fine still waits as a whole ([details](../reference/http.md#when-a-restart-cant-ensure-an-index)).
- **`flusso_indexes{sink}`** counts the indexes the sink's last staging ensured, so after a staging that left an index out it reads one lower, even once that lane opens, until the sink stages again.

## A seed parks on a read-only old generation

**What changed.** A rebuild (a reindex, or every seeded index after a lost slot) ends with a seed that moves the aliases to the new generation and deletes the old one, in one request. OpenSearch refuses that request when the old generation carries `index.blocks.read_only` or `read_only_allow_delete`:

| | 0.18 | 0.19 |
| --- | --- | --- |
| The seed | fails the sink's run | parks the index's lane `blocked`, and is tried again on the `[backoff]` schedule |
| The sink | restarts over and over with backoff, never seeding; `/readyz` answers `503` while it waits | keeps running, `/readyz` stays ready, the other indexes keep being written |
| Once the old generation is freed | a later restart seeds | the next probe seeds, with no restart |

`/status` shows the lane `blocked`, with `parked_since`, `next_probe` and a `reason` naming both generations, OpenSearch's block and the ways out: `blocked: the new generation <new> is built, but the old generation <old> can't be retired: …`. The index stays `backfilling`, readers stay on the old generation.

**Who is affected.** Anyone who reindexes while the old generation is read-only: set by hand, or `read_only_allow_delete` from a full disk on its node.

**What to do.** Nothing to configure. To finish the seed, lift the block on the old generation; under `read_only_allow_delete`, freeing disk space or deleting the old generation works too: [A seed that can't retire the old generation](stalled-lane.md#a-seed-that-cant-retire-the-old-generation).

**What to know.**

- **Alerts.** `FlussoLaneParked` fires after 5 minutes (`why="blocked"`); `FlussoLaneStalled` can follow, since the lane's live changes wait behind the seed and it holds the replication slot.
- **A reindex doesn't help.** It runs the backfill again into the same new generation and parks at the same seed.
- **A stop doesn't wait for it.** The seed is tried once more; still refused, the index is left unseeded and the next start resumes its backfill, then seeds or parks again.
- **`index.blocks.write` and a closed old generation** never stopped the seed, and still don't.

## Retry timing is configurable

**What changed.** Every retry flusso makes on its own waits on one schedule: a failed sink or ingest engine's restart, a parked lane's probe, a restaging lane's next staging, and, new in 0.19, [one index's restart and its backfill's retry](#one-indexs-failure-restarts-only-that-index), each index with its own schedule per kind. 0.18 fixed it at 1 s doubling to 60 s (only code embedding the daemon could change the cap). 0.19 adds a `[backoff]` table, with a flag and a variable for each key:

| `flusso.toml` | Flag | Variable | Default |
| --- | --- | --- | --- |
| `[backoff] initial_ms` | `--backoff-initial-ms` | `FLUSSO_BACKOFF_INITIAL_MS` | `1000` |
| `[backoff] max_ms` | `--backoff-max-ms` | `FLUSSO_BACKOFF_MAX_MS` | `60000`, or `initial_ms` when larger |

The OpenSearch sink's bulk retry (200 ms doubling, `max_retries`) is unchanged.

**Who is affected.** Nobody, unless they set it: the defaults are 0.18's schedule.

**What to do.** Nothing. To tune it, see [backoff](../reference/config-toml.md#backoff): how the keys resolve, and what is refused (`0`, an unknown key, `max_ms` below `initial_ms`). Like `[concurrency]`, it is never written into `flusso.lock`.

## A generation deleted mid-backfill

**What changed.** In 0.18 an unseeded index (a first backfill, or a rebuild in progress) whose generation was deleted mid-backfill was marked seeded when that backfill ended, with the pages written before the delete missing. In 0.19 it is never seeded that way:

| What happened | 0.18 | 0.19 |
| --- | --- | --- |
| Deleted mid-backfill, `action.auto_create_index` forbids flusso's indexes | seeded with the early pages missing | staged again and backfilled from the first row; seeded once that backfill lands |
| Deleted mid-backfill, auto-create allowed (OpenSearch's default) | recreated with a dynamic mapping by the next write, then seeded | the auto-created index is never seeded: flusso replaces it with a strict one and backfills it in full, at the seed or at the next start |
| Deleted while the sink engine restarts, or while a staging fails part way | as above | as above |
| A staging fails right after recreating a deleted seeded generation | the empty generation could be served as seeded | the next start backfills it |
| A seeded generation under another `index.uuid` (restored from a snapshot, recreated by hand) | served as found | served as found, logged at info; flusso doesn't rebuild it |

To tell a generation apart from one recreated under its name, flusso records each generation's `index.uuid` in its `flusso_meta` doc. [Each generation's identity](../reference/sink-opensearch.md#each-generations-identity) has the details; every case is in [A generation deleted or replaced](../reference/sink-opensearch.md#a-generation-deleted-or-replaced).

**Who is affected.**

- Anyone who deletes generation indexes by hand, or whose cluster tooling does.
- Deployments whose OpenSearch role is cut down to exactly the calls flusso made in 0.18: staging now reads `GET /{generation}/_settings/index.uuid` (it was a `HEAD`), and seeding reads it once more.

**What to do.**

1. **Let the role read index settings** of flusso's indexes (`{prefix}{logical}_*`), if it is a cut-down one. Without it, staging fails and the sink restarts with backoff.
2. **Upgrade flusso.** Nothing is rebuilt: an older meta doc gains `uuid` on the first start, and no content hash moves.
3. **Keep forbidding auto-create** for flusso's generations, as [Cluster requirements](../reference/sink-opensearch.md#cluster-requirements) recommends. Without it the replacement waits for the seed or the next start; [why](../reference/sink-opensearch.md#why-forbid-auto-create).

**What to know.**

- **One extra backfill.** A deletion mid-backfill costs a second full backfill of that index. It stays `backfilling` in `/status`, and `/wait` answers `503`, until the second one lands.
- **Requests.** Creating a generation writes its meta doc twice. Nothing is added on the write path.

## An index's backlog can pin the slot

**What changed.** Before a change reaches a lane, the documents it touches sit in their index's **backlog**: waiting to be built, or being built, and not yet handed to any sink. When the backlog holds the oldest change flusso hasn't written, the backlog is what holds the replication slot, and `pinned_by` now says so:

| Where | 0.18 | 0.19 |
| --- | --- | --- |
| `/status` `pinned_by` | `{ "index", "sink" }` or `null` | also `{ "index", "sink": null }`: that index's backlog |
| `flusso_slot_pinned_by` | `{index, sink}` | also `{index}` alone, no `sink` label |
| `stall_seconds`, `flusso_lane_stall_age` | items routed to the lane and not written | the same, plus a built batch the lane hasn't taken yet (a full lane). The backlog ages no lane. |

The backlog form shows up for a moment during live builds. One that stays means builds aren't keeping up: [When an index's backlog holds the slot](stalled-lane.md#when-an-indexs-backlog-holds-the-slot).

**Who is affected.**

- A client that reads `pinned_by.sink` as a string.
- A dashboard or alert that joins `flusso_slot_pinned_by` with a lane series `on (index, sink)`: the backlog series has no lane to match, so it drops out of the join.

**What to do.**

1. **Accept a `null` `sink`** in anything that parses `/status`.
2. **Show the backlog series on its own**, matched by `sink=""`. The dev dashboard's WAL panel does it with the slot's whole lag:

   ```promql
   max by (index) (flusso_slot_pinned_by{sink=""} * on () group_left max(flusso_replication_slot_lag_bytes))
   ```

## Committed means written on every index

**What changed.** A sink's committed count follows its slowest index. A change counts for a sink once **every lane** of it (one per index) has written it, or had nothing to write for it, and every change read before it counts too. In 0.18 a change could count while some of its documents were still unwritten on that sink.

| | 0.18 | 0.19 |
| --- | --- | --- |
| A change counts as committed | when the sink wrote its first item of the batch; other indexes of the sink could still be behind | once every lane of the sink has written it, in read order |
| `changes_in_flight` at `0` | could hide an index still behind | everything read is written on that sink |
| One stalled or [parked](#one-indexs-failure-parks-its-own-lane) index | the count could keep rising | the sink's count stops there, for all its indexes |
| A change that gives no document for the sink (an update of a column no document reads) | counted when its empty batch was written | counted as soon as it is read and everything read before it is written |
| A redelivered batch that had already landed | counted twice | counted once; the count never passes `changes_captured` |
| After an ingest engine restart | the failed run's change was never counted: `changes_in_flight` stayed above `0` for good | back to `0` once everything lands. A change the ended run read but hadn't written counts no sooner than its index's first build after the restart; one that gave no document counts at once |
| The count going down | possible | never, also across an ingest engine restart |
| A sink with no index | — | counts every change read |

It applies to `/status` (`changes_in_flight`, and `sinks.<sink>.changes_committed` and `changes_in_flight`) and to `flusso_changes_committed_total` and `flusso_changes_in_flight`. Names and labels are unchanged; [HTTP endpoints](../reference/http.md#get-status) and [Metrics](../reference/metrics.md) have the definitions. The stop's deadline message names a sink while any of its lanes is behind.

**Who is affected.**

- An alert or dashboard on `changes_in_flight` or `rate(flusso_changes_committed_total)`: on a sink with one slow index it may fire earlier, or for longer, than in 0.18.
- Anything that waits for `changes_in_flight` to reach `0` (a deploy script, a test): it now waits until everything read is written on every index.

**What to do.** Nothing in configuration. Review the thresholds of alerts on these series. When a sink's count stays behind, its lanes say which index holds it: `outstanding`, `stall_seconds` and `health` under `sinks.<sink>.lanes.<index>` ([A lane is stalled](stalled-lane.md)).

## Each index has a share of the memory budget

**What changed.** In 0.18 one index could fill the whole [memory budget](../reference/config-toml.md#concurrency) (`max_built_mib`): when its sink fell behind in a burst, every other index waited behind it. In 0.19, with two or more indexes, one index's live documents hold at most a share of the budget, and the other indexes keep building in the rest. A backfill or reindex doesn't count against its index's share: backfills together stay within half the budget, as in 0.18, so an index being reindexed keeps building its live changes.

| | 0.18 | 0.19 |
| --- | --- | --- |
| One index in a burst, its sink behind | fills the whole budget; the others wait | its live documents stop at its share; the others keep building |
| The share | none | `[concurrency] index_budget_share`, default `0.5` (256 MiB of the default 512) |
| A burst over many distinct documents of one index, or one index stuck behind a parked lane | reading pauses for every index once the budget is full | the others keep building and reading; that index's changes wait as documents to build, up to [`max_pending_documents`](#a-stuck-index-stops-only-itself) (a million by default), then reading pauses for every index |
| One index | the whole budget | unchanged |

The share has a flag and a variable, like the table's other keys:

| `flusso.toml` | Flag | Variable | Default |
| --- | --- | --- | --- |
| `[concurrency] index_budget_share` | `--index-budget-share` | `FLUSSO_INDEX_BUDGET_SHARE` | `0.5` |

**Who is affected.** Deployments running two or more indexes. A single index sees no change.

**What to do.**

1. **Nothing, usually.** Bursts of one index stop holding up the others.
2. **Raise `index_budget_share` towards `1`** if one index does nearly all the work and should keep using the whole budget, as in 0.18. `1` is the old behaviour.
3. **Check the memory headroom.** An index holding no live documents always gets one live batch, even with the budget full, so one index stuck at its share leaves the others building ([how long](#a-stuck-index-stops-only-itself)). Plan for one live and one backfill batch per index above `max_built_mib` ([sizing](../reference/config-toml.md#concurrency)); 0.18 needed one of each in all.

**What to know.**

- **Refused at start.** `check`, `build` and `run` refuse a share outside (0, 1], `nan` included, naming where the value came from. Like the rest of `[concurrency]`, it is never written into `flusso.lock`.
- **Metrics are unchanged.** `flusso.memory.*` and `/status` `memory` still report the whole budget; nothing per index yet. An index held at its share shows as its backlog in [`pinned_by`](#an-indexs-backlog-can-pin-the-slot) and as live waits in `flusso.memory.wait`.

## Reading runs ahead of the builds

**What changed.** In 0.18 flusso stopped reading changes once a batch was ready and waiting for a build. In 0.19 reading and building are two steps: flusso reads ahead, queuing the documents the changes touch, until one index has `max_waiting_per_index` documents waiting or every index together has `max_waiting`. Batches are cut per index ([below](#batches-are-per-index)). Two new keys set the bound ([Reading ahead](../reference/config-toml.md#reading-ahead)):

| `flusso.toml` | Flag | Variable | Default |
| --- | --- | --- | --- |
| `[concurrency] max_waiting_per_index` | `--max-waiting-per-index` | `FLUSSO_MAX_WAITING_PER_INDEX` | `2 × [batch] max_changes` (512) |
| `[concurrency] max_waiting` | `--max-waiting` | `FLUSSO_MAX_WAITING` | `2 × builds × [batch] max_changes` (2,048) |

- **A stop builds what was read.** 0.18 landed the batch being built and the builds in flight. 0.19 builds and lands everything already read, up to the lead plus the run being read, so a stop under a backlog can take a little longer, within `--shutdown-timeout-secs`.
- **An index stuck at its share, or behind a full lane, stops nobody at `max_waiting_per_index`** while another index is working: it becomes exempt ([below](#a-stuck-index-stops-only-itself)).

**Who is affected.** Nobody needs to change a deployment that sets neither key: the defaults follow `[batch] max_changes` and `builds`. A short `--shutdown-timeout-secs` with slow builds may now pass its deadline under a backlog.

**What to do.**

1. **Nothing, usually.**
2. **Under a tight stop deadline**, give the stop more time, or lower both keys.
3. **To leave the others running longer beside a stuck index**, raise `max_pending_documents` ([below](#a-stuck-index-stops-only-itself)).

**What to know.**

- **Refused at start.** `check`, `build` and `run` refuse `0`, a `max_waiting` set below a `max_waiting_per_index` that is set too, and a flag or variable that isn't a whole number, each naming where the value came from. Setting one alone is always valid: an unset `max_waiting` rises to the per-index bound, an unset `max_waiting_per_index` falls to the total. Like the rest of `[concurrency]`, they are never written into `flusso.lock`.
- **Confirmation is unchanged.** A change read but not yet built is never confirmed to the slot or covered by a fence.

## Batches are per index

**What changed.** 0.18 grouped the changes of every index into one batch. 0.19 batches each index on its own, and caps what one build of an index takes. `[batch] max_changes` now means both: a batch groups at most this many changes, and one index's build takes at most this many documents ([batch](../reference/config-toml.md#batch)).

| | 0.18 | 0.19 |
| --- | --- | --- |
| A batch | the changes of every index, one deadline | one index's changes, with its own `max_delay_ms` |
| A slow index (large documents, a busy build) | its batch held every index's writes back | it delays only its own writes |
| One change touching two indexes | built in one batch, written to both at about the same time | built in a batch of each; it can be searchable in one well before the other |
| One change touching more than `max_changes` documents of one index (a renamed brand embedded in 50,000 products) | one build and one large `_bulk` request | parts of at most `max_changes` documents, up to `[concurrency] builds` built at once, each its own write. Committed, confirmed to Postgres and covered by `/wait` only once its last part is written; until then a search can see part of it |
| The stdout sink's `seq` | position order across indexes | never goes down within one index, may repeat, no order across indexes ([Order](../reference/sink-stdout.md#order)) |

- **After an ingest engine restart**, reading resumes from the change the furthest-behind index still needs, so an index that was ahead of another receives some changes again. Writes are idempotent: the index ends in the same state, but OpenSearch's `_version` of those documents moves.
- **`/wait` is unchanged.** A wait naming an index passes once that index has the commit; one without `index` waits for every index, as before ([`/wait`](../reference/http.md#get-waitpositionposition)).

**Who is affected.**

- Anything that assumes a change shows up in all its indexes at once: a read of two indexes right after a commit, or a check that compares them.
- A consumer of the stdout sink that relies on order across indexes, or keeps one `seq` checkpoint for every index.
- Deployments with large fan-out changes: they see more, smaller writes instead of one large one.

**What to do.**

1. **Read several indexes after a commit through a fence that names them all**, or none: `index=users&index=orders`, or the `flusso-query` client, which names a read's own indexes ([Read your own writes](read-your-writes.md)).
2. **Track stdout progress per index**, from the last `seq` inclusive ([Order](../reference/sink-stdout.md#order)).
3. **Nothing to configure for large fan-out.** `_bulk` requests stay at most `max_changes` documents of an index, and each part holds at most that many documents in memory.

## A stuck index stops only itself

**What changed.** In 0.18 a stuck index stopped every index once the memory budget filled. In 0.19, with two or more indexes, an index that is **stuck** (the budget refuses its live batch, or a batch of it waits on a full lane) and has `max_waiting_per_index` documents waiting becomes **exempt**, as long as another index is **working**: not stuck, and not waiting 5 seconds on an acknowledgement. It stops pacing the reading: its changes are kept as documents to build, each once, and the other indexes keep being read, built and written. One new key bounds what the exempt indexes keep together ([A stuck index](../reference/config-toml.md#a-stuck-index)):

| `flusso.toml` | Flag | Variable | Default |
| --- | --- | --- | --- |
| `[concurrency] max_pending_documents` | `--max-pending-documents` | `FLUSSO_MAX_PENDING_DOCUMENTS` | `1000000` |

| | 0.18 | 0.19 |
| --- | --- | --- |
| One index's lane parked for hours, beside other indexes | every index stops once the budget fills | the others keep being read and written |
| A `/wait` naming only the other indexes | times out once reading stops | answers |
| The WAL the slot keeps | grows | grows, unchanged: the slot waits for the stuck index |
| An index that is slow but keeps landing | paces ingest | paces the reading at `max_waiting_per_index`, as does a stuck index once it recovers and drains |
| The exempt indexes reach `max_pending_documents` | — | reading stops for every index until some are built |
| Every index stuck at once (a whole sink down) | every index stops once the budget fills | reading stops within about 5 seconds at `max_waiting_per_index` / `max_waiting`; an index that gets no changes meanwhile counts as working, so the others read on toward `max_pending_documents` until it sends something the sink doesn't acknowledge |
| One index | — | unchanged |

**Who is affected.** Deployments running two or more indexes, when one of them gets stuck: a parked lane, or an index whose sink stops acknowledging. A single index sees no change.

**What to do.**

1. **Nothing, usually.**
2. **Keep alerting on the WAL.** The other indexes looking healthy doesn't mean the slot moves: watch `flusso_replication_slot_lag_bytes` and `pinned_by` as before ([When one index is stuck beside others](stalled-lane.md#when-one-index-is-stuck-beside-others)).
3. **Plan memory for the backlog**: about 270 B a waiting document, 270–320 MB at the default, only while an index is exempt. Lower `max_pending_documents` on a tight host ([A stuck index](../reference/config-toml.md#a-stuck-index)).

**What to know.**

- **The log and `/status` show it.** A warning names the index when it becomes exempt, another when it may drain but not grow, an info line when it is no longer exempt; a warning says when reading stops at the cap. At most one line a minute per index, and the last one names the current state ([the log lines](../reference/config-toml.md#a-stuck-index)). `/status`, the metrics and an alert show it per index: [Each index's state is shown](#each-indexs-state-is-shown).
- **A slow acknowledgement pauses it for a while.** A healthy sink taking more than 5 seconds over one write (a large backfill or reindex bulk), or a sink engine restart whose failing write runs to its timeout, makes its index count as not working for that long: an exempt neighbour stops growing, and reading may stop, until that write is acknowledged ([details](../reference/config-toml.md#a-stuck-index)). Never worse than 0.18.
- **A stop builds the backlog too**, within `--shutdown-timeout-secs`. When it can't, flusso exits `1` naming each index left with documents read and not built (`orders: 1200 documents read and not built`), and the next start reads them again ([Stopping](../reference/cli.md#stopping)).
- **Refused at start.** `check`, `build` and `run` refuse `0`, and a flag or variable that isn't a whole number, naming where the value came from. A value below `max_waiting_per_index` is accepted. Like the rest of `[concurrency]`, it is never written into `flusso.lock`. The designer's config form sets it.

## Backfills run at once

**What changed.** In 0.18 one index's backfill waited for another's whole walk: a small index waited behind a large one. In 0.19 each index's backfill (a first seed, a reindex, a fresh backfill after its target was deleted) starts as soon as its sinks ask, beside the others'.

| | 0.18 | 0.19 |
| --- | --- | --- |
| Several indexes to backfill | one after another | at once; each is marked seeded when its own backfill lands |
| Builds and Postgres connections | `builds` builds, `builds` + 2 connections | unchanged: the backfills share them, and root keys are read one page at a time over every index |
| One index's backfill stuck on a sink that stops acknowledging | every other index's backfill waits | the others go on, one batch in flight at a time while the stuck one holds backfill's half of the budget |
| Backfill memory | half of `max_built_mib`, past one batch | half of `max_built_mib`, past one batch per index backfilling |
| A stop | the backfill in progress is dropped and served again on the next start | every backfill in progress is |
| One index | — | unchanged |

**Who is affected.** Deployments that backfill two or more indexes at a time: a first start, a [reindex](reindex.md) of several, a [lost slot](dropped-slot.md), an upgrade that moves several content hashes.

**What to do.**

1. **Nothing, usually.**
2. **Check the memory headroom** on a tight host: plan for one backfill batch per index above `max_built_mib`, and up to 1,024 root keys per index backfilling (about 110 KB for a one-column integer key) ([sizing](../reference/config-toml.md#concurrency)).
3. **Don't rely on the order** indexes finish in: wait on each index's own state in [`/status`](../reference/http.md#get-status).

## One index's failure restarts only that index

**What changed.** In 0.18 a failed build ended the ingest engine's run: the daemon restarted it with backoff, every index paused, and the stream was read again from the slot. In 0.19, with two or more enabled indexes, a failure of one index's own work restarts that index alone, after a wait, and the others keep indexing ([A failing index](../reference/config-toml.md#a-failing-index)):

| Failure | 0.18 | 0.19, two or more indexes |
| --- | --- | --- |
| A live build fails or panics, or a live document fails to build under `on_error = "stop"` | the ingest engine restarts and every index waits (a build that panicked as it started ended the deployment) | the index is **down**: its changes wait as documents to build, and it restarts alone after the wait |
| A backfill fails: reading root keys, a backfill batch, a backfill document under `stop` | the ingest engine restarts; every index waits | only the backfill is retried, each sink after the last batch it received; the index's live changes keep flowing |
| A document that fails every time under `stop` | every index stalls | its index stays down, retried every `[backoff] max_ms`; the others go on until it has `max_pending_documents` documents waiting |
| The change stream, resolving a change, a lane, any failure while stopping | the ingest engine restarts | unchanged |
| A stop with an index down | — | the index is tried once more at once; if that fails, flusso exits `1` |
| One index, a build that panics as it starts | the deployment ends: `ingest engine task failed` | the ingest engine restarts with backoff |

Every other failure with one index restarts the ingest engine, as in 0.18. The waits are `[backoff]`'s, one schedule per index and kind of failure: no new setting ([Retry timing is configurable](#retry-timing-is-configurable)).

**Who is affected.** Deployments with two or more enabled indexes, when one index's builds or documents fail: a script that throws, a schema Postgres refuses, a row that overflows an aggregate. With one index, only the panic above changes.

**What to do.**

1. **Nothing, usually.**
2. **Move alerts that watch the engine.** A failing index no longer moves `flusso_errors_total{engine="ingest"}`, `errors` or `phase`. Read `state`, `last_error` and `restarts` per index, and `flusso_index_down`, `flusso_index_backfill_retrying`, `flusso_index_restarts_total` ([Each index's state is shown](#each-indexs-state-is-shown)).
3. **Add a rule for what pages nobody.** `FlussoIndexBacklog` fires for a down index only while documents of it wait. A down index with nothing waiting, and a backfill that keeps failing while live changes flow, need a rule of your own ([runbook](stalled-lane.md#when-an-index-is-down-or-its-backfill-keeps-failing)).

**What to know.**

- **The slot still waits for a down index**, so WAL grows, and its documents can grow to `max_pending_documents` beside a healthy index. flusso keeps resolving every change for it meanwhile. A database-wide fault still restarts the whole ingest engine. The full list is in [A failing index](../reference/config-toml.md#a-failing-index).
- **A stop that fails costs a replay.** The next start reads again from the down index's oldest change, so every other index's changes since then are read and built again.
- **One index, a panic every time** is now an endless restart loop paced by `[backoff]`, where 0.18 exited. Alert on `flusso_errors_total{engine="ingest"}` climbing.

## Reindex moved to the index path

**What changed.** The private surface's reindex route is now `POST /indexes/<name>/reindex[?sink=<name>]`. `POST /reindex?index=…` is removed and answers `404`. Every answer is JSON ([the route](../reference/http.md#post-indexesnamereindexsinkname)):

| | 0.18 | 0.19 |
| --- | --- | --- |
| The request | `POST /reindex?index=users&sink=primary` | `POST /indexes/users/reindex?sink=primary` |
| `202` | text: `reindex of users on primary queued` | `{"index": "users", "sink": "primary", "queued": true}`, `sink` null for every sink |
| Refusals (`400`, `404`, `503`) | text | `{"error": "…"}`; an unknown index adds `"indexes"`, the enabled ones |
| A disabled index | accepted | `404` |

**Who is affected.** Scripts and tools that call `POST /reindex` themselves. `flusso reindex` is updated and prints what it printed before; use it at the same version as the server.

**What to do.** Move each call to the new path: the index goes into the path, `sink` stays a query parameter. Read the `202`'s JSON instead of its text, if the script reads it.

## Each index's state is shown

**What changed.** What 0.19 does per index (the [exemption](#a-stuck-index-stops-only-itself), holds, backfills) shows outside the log too:

- **`/status`** gains `ingest.indexes.<index>` for each enabled index: documents waiting, `backlog_seconds`, `state` (`running` or `down`), `down_since`, `last_error` and `restarts`, `exemption`, `working`, `held_by` and `held_sinks`, `backfill` (`walking`, `retrying` or `waiting`), `pacing_reader` ([per-index state](../reference/http.md#per-index-ingest-state)).
- **`GET /indexes/<name>/status`**, on the public surface, serves one index's entry ([the route](../reference/http.md#get-indexesnamestatus)).
- **Metrics**, one series per enabled index: `flusso_index_documents_waiting`, `flusso_index_backlog_age_seconds`, `flusso_index_exempt`, `flusso_index_frozen`, `flusso_index_working`, `flusso_index_held{by}`, `flusso_index_backfilling`, `flusso_index_backfill_retrying`, `flusso_index_down`, `flusso_index_restarts_total{kind}`; and `flusso_reader_waits_seconds`, how often and how long reading stops, by the bound that stopped it ([Metrics](../reference/metrics.md)).
- **`FlussoIndexBacklog`**, new in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`: an index exempt, or stuck past 60 s, for 10 minutes. New chart values `prometheusRule.indexBacklogSeconds` and `indexBacklogFor` ([runbook](stalled-lane.md#when-flussoindexbacklog-fires)).
- **The dev dashboard** has a per-index row, with which indexes are down or retrying a backfill and how often each restarted.

**Who is affected.** Nobody has to change anything. With the chart's `prometheusRule` on, the new alert pages.

**What to know.**

- **`backlog_seconds` is not a lag.** It grows only while an index is stuck or exempt; an index merely behind its source stays seconds old. Read lag from `flusso_lane_wal_behind_bytes` and the slot lag ([what it shows](../reference/http.md#what-backlog_seconds-shows)).
- **A long catch-up can page.** With several indexes, an index slower than the others is exempt while it trails, so a catch-up longer than `indexBacklogFor` fires `FlussoIndexBacklog`. Silence it, or raise the period.
- **Disabled indexes** have no `ingest.indexes` entry and no series; `indexes` in `/status` and `GET /indexes` still list them.

## For code embedding the crates

- **Removed:** `EngineError::DocumentsRejected` (a refusal no longer ends the run).
- **`sink::WriteReport`** gains `blocked` and `missing`, one `IndexRefusal` per index. A sink reports a missing target there instead of failing the write.
- **`Sink::ensure_index` and `reindex`** may now run beside writes of **other** indexes, never beside one holding the same index.
- **Order is per index** for an ordered sink (`Sink::ordered`). Across indexes, live items go in position order while every lane is written; a parked lane is overtaken by the other indexes' later items. Each index is built on its own, so an ordered sink gets no order across indexes: the stdout sink's `seq` never goes down within an index, may repeat, and has no order across indexes ([Batches are per index](#batches-are-per-index)).
- **`Observer`** gains `on_lane_health` and `on_lane_probe`; `engine::LaneHealth`, `ParkReason` and `ProbeOutcome` are new and re-exported by `daemon`.
- **`daemon::LaneSnapshot`** (a lane of `/status`) gains the public fields `health`, `parked_since`, `next_probe` and `reason`: code building one with a struct literal must set them. `Status::unhealthy_lanes`, `LaneHealthView` and `LaneHealthState` (`Open`, `Refused`, `Blocked`, `Restaging`; `name()` gives the word `/status` writes) are new.
- **Removed: `DaemonOptions::max_restart_backoff`.** The schedule comes from the config: `Config.backoff` (`config::BackoffConfig`, each key a `BackoffValue` with its `config::Origin`), resolved by `daemon::backoff(&Config)`. `Daemon::start` fails when `max_ms` is below `initial_ms`. A struct literal of `DaemonOptions` drops the field.
- **New: `engine::Backoff`** (re-exported as `daemon::Backoff`): `Backoff::new(initial, max)` returns `Err(engine::BackoffError)` for a zero first wait or a cap below it; `initial()`, `max()`, `after(wait)` (doubled, capped); `Default` is 1 s / 60 s. `SinkEngine::with_backoff(Backoff)` sets a sink engine's probe and staging schedule. `config::parse_backoff_ms` parses a flag's or a variable's value.
- **`Sink::ensure_index`** returns `sink::Ensured { target: Option<TargetId>, created }` instead of `()` (build one with `Ensured::existing(target)` or `Ensured::created(target)`; it is `#[non_exhaustive]`). New `sink::TargetId` names a target; a sink that can tell its targets apart gives one created anew a new id. The default implementation names none.
- **New: `SinkError::TargetChanged`.** `mark_seeded` answers it for a target that isn't the one `ensure_index` named; the engine then stages the index again and backfills it in full.
- **`pinned_by` answers an `engine::Pin`** (re-exported as `daemon::Pin`): `Pin::Lane(LaneKey)` or `Pin::Backlog(IndexName)`. `Progress::pinned_by`, `Stalls::pinned_by` and the second half of `Status::stalls()` return `Option<Pin>` instead of `Option<LaneKey>`. `daemon::status::LaneId` is now `PinnedBy` (re-exported as `daemon::PinnedBy`), with `sink: Option<String>`. New `Progress::own_oldest(lane)`: the oldest change the lane holds itself, what its stall age is measured by.
- **Removed: `Observer::on_changes_settled`.** A sink's committed count is no longer an event: read `Progress::committed(sink)`, or `daemon::Status::committed(sink)`. `CommitStats.changes` and `stream::Batch.changes`, which only fed it, are removed: `CommitStats::new` takes `(envelopes, flush, work)`, and a `Batch` literal drops the field (a serialized item that still carries it reads fine).
- **`Progress` counts committed changes by step.** New `engine::Step`, and `Progress::change_read`, `changes_read`, `close_step`, `committed`, `published_step` and `skipped_step`. `Progress::published_build` takes the build's `Step`; `skipped_build` takes an `Option<Token>` and a `Step`. `Progress::steps_kept(lane)` reports how many steps a lane keeps beside its unwritten items (the first is at most the second plus two), and `RunningDaemon::progress()` hands out the deployment's `Progress`. A step is per index: `Progress::close_steps(frontiers)` closes one grant's steps under one number, each carrying its index's frontier, which `Progress::frontier(index)` reads and a take reports as `Taken::frontier`; `close_step` is right only while nothing is waiting. A lane's position moves to an item's `last` only while nothing of its index marked at or before `last` waits or sits in another build not yet released, so `published`, `published_step`, `published_build`, `skipped`, `skipped_step` and `skipped_build` hold it back meanwhile; with nothing marked, as in 0.18, they move it as before. `Progress::mark` marks as the next change read, `mark_reads` takes each change's read number, `read_unmarked` counts a run read but never marked, and `routed(position)` tells `Progress` the reader routed every change through it.
- **`daemon::Status::with_progress`** is new: a `Status` built without it reports `0` committed for every sink, so all its captured changes read as in flight.
- **`engine::Budget` serves many reservers and is shared per index.** Removed: `fits`, `want_release` and `released`. New: `admit(work, parts)` answers an `engine::Admission` (`admitted`, `refused`, `generation`, `grants()`); `reserve(work, parts)` admits and reserves in one step, one `Reservation` per index, or `Err` with the refusal's generation; `released_since(generation)` waits for a release after it. `with_index_share(indexes, share)` caps one index's live bytes at `share` of the limit when `indexes` is 2 or more; `held_by`, `index_share`, `generation` and `Reservation::index` read the state. `try_reserve` stays, counted toward no index. A backfill reservation is granted to an index holding no backfill bytes whatever the limit says, so each index gets one (0.18: one while every backfill held nothing).
- **`[concurrency] index_budget_share`** is `ConcurrencyConfig.index_budget_share: Option<config::ShareValue>` (`share`, `origin`), and `ConcurrencyConfig` no longer derives `Eq`. `daemon::index_budget_share(&Config)` resolves and checks it (`daemon::DEFAULT_INDEX_BUDGET_SHARE` is `0.5`), and `Daemon::start` fails on a share outside (0, 1]. `config::parse_index_budget_share` parses a flag's or a variable's value. New `Progress::waiting_by_index`.
- **`[concurrency] max_waiting_per_index` and `max_waiting`** are `ConcurrencyConfig.max_waiting_per_index` and `max_waiting: Option<config::CountValue>` (`count`, `origin`): a struct literal of `ConcurrencyConfig` must set them. `daemon::max_waiting(&Config)` resolves and checks them into an `engine::MaxWaiting { per_index, total }` (re-exported by `daemon`; `MaxWaiting::derived(batch, builds)` gives the defaults), and `Daemon::start` fails on the refusals above. `IngestEngine::with_max_waiting` sets the lead, `max_waiting()` reads it. `config::parse_max_waiting` parses a flag's or a variable's value. New on `Progress`: `take_through`, `waiting_through`, `read_changes`, `most_waiting`, and for one index's builds `take_through_at_most` (a take capped at a number of documents), `waiting_of` (each index's count stopped at a limit, `most`), `waiting_on`, `newest_waiting` and `waits_through` (whether a document marked at or before a position still waits). New `engine::Holds`, what `waiting_through` and `waiting_of` take: a visitor offered the documents running builds hold, one at a time, until it answers `true`.
- **`[concurrency] max_pending_documents`** is `ConcurrencyConfig.max_pending_documents: Option<config::CountValue>`: a struct literal of `ConcurrencyConfig` must set it. `daemon::max_pending_documents(&Config)` resolves and checks it (`engine::DEFAULT_MAX_PENDING_DOCUMENTS` is 1,000,000), and `Daemon::start` fails on `0`. `IngestEngine::with_max_pending_documents` sets it (at least 1); `config::parse_max_waiting` parses its flag or variable. New `EngineError::DocumentsLeft`: every builder returned with documents read and not built, named per index. New `daemon::Status::waiting_by_index`.
- **Lane items are keyed per index.** New `stream::ItemKey`, and `stream::Batch` gains `key: Option<ItemKey>` (`None` on snapshot rows; a serialized item without it still reads). `Progress::published(lane, key, oldest, last)` takes the item's key, and `Progress::applied` and `Progress::searchable` take an `ItemKey` instead of a `Position`; the new `published_step` and `published_build` take it too. `Progress::next_key(index)` numbers an index's next item, the same on every lane of the index.
- **Per-index ingest state.** New `engine::IndexStates`, shared by the ingest engine (`IngestEngine::index_states`), with `IndexView`, `ExemptionState`, `HeldBy` and the guards `LaneFull`, `PacingReader` and `Walking`; `engine::STUCK_AFTER` (5 s). `Observer::on_reader_wait(bound, index, waited)` is new, with a no-op default, and `engine::ReaderBound`. `Progress::oldest_unbuilt`, `daemon::Stalls::backlog` and `forget_backlog`, and `daemon::BACKLOG_SPAN` (an hour of samples) are new. `daemon::Status` gains `with_ingest_indexes` (the enabled indexes), `attach_index_states`, `index_ingest`, `ingest_indexes`, `tracks` and `index_names`; `StatusSnapshot` gains the public field `ingest` (`IngestSnapshot`, `IndexIngestSnapshot`, `Exemption`, `IndexHold`, `BackfillSnapshot`), so a struct literal must set it. `DaemonControl::detached` builds a control with no deployment behind it, for a transport's tests.
- **Per-index restart.** `IngestEngine::with_backoff(Backoff)` sets the wait before one index restarts alone or retries its backfill (default 1 s doubling to 60 s; the daemon passes `[backoff]`). `Observer` gains `on_index_failed(index, kind, error, retry_in)` and `on_index_restarted(index, kind)`, `kind` an `engine::Work` (`Live` or `Backfill`), both with no-op defaults; an index's failure is not an `on_engine_error`. New in `daemon`: `IngestState` (`Running`, `Down`), `BackfillState` (`Walking`, `Retrying`, `Waiting`), `IndexFailure` (`message`, `at`), and `IndexErrors` and `IndexRestarts` (`live`, `backfill`); `IndexIngestSnapshot` carries `state`, `down_since`, `last_error` and `restarts`, and `BackfillSnapshot` carries `state`, `retry_at` and `sinks: Option<Vec<String>>`. The channel stream adapter now wakes a `recv` already waiting when a delivery is released (nacked, or dropped unacknowledged), so it is redelivered at once.
- **New: `stream::Ticket`.** `Request::Backfill` gains `ticket`, `LaneItem::SnapshotComplete` gains `ticket`, and `Batch` gains `ticket: Option<Ticket>` (`None` on a live batch). Serialized items without them still read (`serde` defaults). Struct literals must set them.
- **Fresh builds.** `stream::Batch` gains `keys: Arc<[RowKey]>` (each envelope's root key, in envelope order; empty when unknown) and `build: Option<u64>` (the build's number on its index). `Request::Rebuild { index, sink, keys, ticket }` is new, with `Request::index()` and `Request::sink()`; code matching on `Request` must handle it. `Stream::substitutes(sink)` returns a `SubstituteLane` of `stream::Substitute` items; its default answers the new `QueueError::Unsupported`, and a parked lane over such a stream writes its own batch again. `RowKey` moved to `kernel`; `source` still re-exports it. `Consumer`'s contract now states the redelivery order, which an adapter must keep: items come in publication order, and a returned delivery (nacked, or dropped unacknowledged) comes back in its order among the returned ones, before any item not yet delivered.
- **New: `Sink::detachment(&IndexMapping)`**, called at the start of every staging step, before `reindex` and `ensure_index`; its default keeps nothing. It returns a `sink::Detachment` (`record: Option<DetachRecord>`, `stuck: Option<StuckClock>`, `restored`, `unreadable_marker`), all new and `#[non_exhaustive]`. An `Err` fails the step. `SinkEngine::detachment(index)` returns what the last staging of the index read.

## Related

- [A lane is stalled](stalled-lane.md)
- [Handle rejected documents](rejected-documents.md)
- [Index entries and on_error](../reference/index-and-on-error.md#on_error)
