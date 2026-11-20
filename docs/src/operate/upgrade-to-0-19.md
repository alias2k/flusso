# Upgrade to 0.19

0.19 keeps a failure of one index on that index. Each change below links to its section.

**Running flusso**

- **An index that can't be written parks alone**; the sink's other indexes keep writing, and a write block no longer drops documents under `on_error = "skip"` (#404): [One index's failure parks its own lane](#one-indexs-failure-parks-its-own-lane).
- **`on_error = "stop"` no longer stops the sink**, and `/readyz` stays `200` while an index is parked; alert on the new `FlussoLaneParked` instead: [Alerting on a parked index](#alerting-on-a-parked-index).
- **The retry wait is configurable**: `[backoff]` sets the first and the longest wait of every restart and probe: [Retry timing is configurable](#retry-timing-is-configurable).
- **A generation deleted mid-backfill is backfilled again in full**, never seeded with rows missing; a cut-down OpenSearch role needs to read index settings (#428): [A generation deleted mid-backfill](#a-generation-deleted-mid-backfill).
- **`pinned_by` can name an index alone**, with `sink` null in `/status` and no `sink` label on `flusso_slot_pinned_by`: [An index's backlog can pin the slot](#an-indexs-backlog-can-pin-the-slot).
- **A sink's committed count waits for every index**: `changes_committed` and `flusso_changes_committed_total` count a change once every lane of the sink has written it, so one slow index holds `changes_in_flight` for the whole sink: [Committed means written on every index](#committed-means-written-on-every-index).

**Code that embeds the crates**

- **New lane-health API, `EngineError::DocumentsRejected` removed, order per index for an ordered sink, `DaemonOptions::max_restart_backoff` replaced by `[backoff]`, `Sink::ensure_index` returns `Ensured`, backfill items carry a `Ticket`, `pinned_by` answers an `engine::Pin`, `Observer::on_changes_settled` removed**: [For code embedding the crates](#for-code-embedding-the-crates).

## One index's failure parks its own lane

**What changed.** In 0.18 a failure about one index ended the sink engine's run, and the daemon restarted the whole sink with backoff: every index on that sink paused. In 0.19 the failure stays on that index's **lane** (one index flowing into one sink), and the sink's other indexes keep being written:

| What OpenSearch answers | 0.18 | 0.19 |
| --- | --- | --- |
| A document refused under `on_error = "stop"` | the sink stops and restarts with backoff | the lane is **refused** |
| A write block (`index.blocks.write`, `read_only`, `read_only_allow_delete`: 403 `cluster_block_exception`) | an item-level rejection: **quarantined under `skip`** (documents lost), the sink stops under `stop` | the lane is **blocked**, whatever `on_error` says; nothing is quarantined |
| An item-level 403 `security_exception` (the role can't write that index) | an item-level rejection | the lane is **blocked** |
| The index's generation deleted while running (`index_not_found_exception`) | the sink stops; its restart recreates the generation | that index alone is **staged again** and backfilled |
| The whole `_bulk` refused, credentials refused, the cluster unreachable, busy past `max_retries` | the sink stops and restarts | unchanged |

A refused or blocked lane is **parked**: it writes its oldest batch again alone (a **probe**) after 1 s, then with the wait doubling up to 60 s ([configurable](#retry-timing-is-configurable)). When a probe lands, the lane opens and the rest follows, with no restart. Meanwhile the lane holds the replication slot, as a stalled lane always has.

**Who is affected.** Every deployment that writes more than one index to a sink: one bad document or one blocked index no longer holds up the others. Deployments on `on_error = "skip"` whose indexes ever got a write block: those documents used to be quarantined, and are now written once the block is lifted.

**What to do.**

1. **Upgrade flusso.** Nothing to rebuild: no content hash moves.
2. **Lift write blocks to resume.** A blocked index is written by the next probe once the block is lifted, at most one `[backoff] max_ms` later (60 s by default). No restart. The runbook is [A lane is stalled](stalled-lane.md).
3. **Fix a refused document as before.** A probe writes the same batch again, so fixing the row in Postgres doesn't unpark the lane yet: change the schema, or switch the index to `skip` and restart ([Handle rejected documents](rejected-documents.md)).

**What to know.**

- **Memory.** A parked lane keeps its built documents until they land, and the documents queued behind it hold part of the [memory budget](../reference/config-toml.md#concurrency). Parked long enough, it fills the budget and the other indexes slow down too, as a stalled sink did in 0.18.
- **A stop with a parked lane exits `1`.** The stop probes every parked lane once more at once, so a block lifted meanwhile still drains. A lane that still can't land holds the drain until `--shutdown-timeout-secs`, and the exit message names it: `primary: 3 changes in flight, lane users blocked since … (blocked: index [users_…] blocked by: [FORBIDDEN/8/index write (api)];)`. The next start writes the parked batches again first.
- **Not yet.** A refused lane writes the same batch on each probe; rebuilding it from the current row first is a later release, as are a reindex of one index without pausing the sink's other writes, and dropping a stalled index automatically ([#394](https://github.com/alias2k/flusso/issues/394)).

## Alerting on a parked index

**What changed.** A refusal under `stop` no longer moves the sink to phase `failed`, restarts nothing, and doesn't count in `errors`. `/readyz` stays `200` while a lane is parked: the other indexes are written and served, and a restart wouldn't unpark it.

**Who is affected.** Anyone alerting on a sink's `failed` phase, on restarts, or on readiness to catch a refused document.

**What to do.** Alert on the lane:

- **`FlussoLaneParked`** is new in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`. It fires once `flusso_lane_parked > 0` has held for 5 minutes, naming the index, the sink and `why` (`refused`, `blocked`, `restaging`). With your own rules, copy it or alert on [`flusso_lane_parked`](../reference/metrics.md) directly.
- `FlussoLaneStalled` still fires after 10 minutes for a parked live change, but never for a refused backfill page, which carries no position: only `FlussoLaneParked` sees that one.
- `/status` shows each lane's state under [`sinks.<sink>.lanes.<index>.health`](../reference/http.md#lane-health) (`open`, `refused`, `blocked`, `restaging`), with `reason` and `next_probe`.

## Retry timing is configurable

**What changed.** Every retry flusso makes on its own waits on one schedule: a failed sink or ingest engine's restart, a parked lane's probe, a restaging lane's next staging. 0.18 fixed it at 1 s doubling to 60 s (only code embedding the daemon could change the cap). 0.19 adds a `[backoff]` table, with a flag and a variable for each key:

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
| A redelivered batch that had already landed | counted twice | counted once; the count never passes `changes_captured` |
| After an ingest engine restart | the failed run's change was never counted: `changes_in_flight` stayed above `0` for good | back to `0` once everything lands |

It applies to `/status` (`changes_in_flight`, and `sinks.<sink>.changes_committed` and `changes_in_flight`) and to `flusso_changes_committed_total` and `flusso_changes_in_flight`. Names and labels are unchanged; [HTTP endpoints](../reference/http.md#get-status) and [Metrics](../reference/metrics.md) have the definitions. The stop's deadline message names a sink while any of its lanes is behind.

**Who is affected.**

- An alert or dashboard on `changes_in_flight` or `rate(flusso_changes_committed_total)`: on a sink with one slow index it may fire earlier, or for longer, than in 0.18.
- Anything that waits for `changes_in_flight` to reach `0` (a deploy script, a test): it now waits until everything read is written on every index.

**What to do.** Nothing in configuration. Review the thresholds of alerts on these series. When a sink's count stays behind, its lanes say which index holds it: `outstanding`, `stall_seconds` and `health` under `sinks.<sink>.lanes.<index>` ([A lane is stalled](stalled-lane.md)).

## For code embedding the crates

- **Removed:** `EngineError::DocumentsRejected` (a refusal no longer ends the run).
- **`sink::WriteReport`** gains `blocked` and `missing`, one `IndexRefusal` per index. A sink reports a missing target there instead of failing the write.
- **`Sink::ensure_index` and `reindex`** may now run beside writes of **other** indexes, never beside one holding the same index.
- **Order is per index** for an ordered sink (`Sink::ordered`). Across indexes, live items go in position order while every lane is written; a parked lane is overtaken by the other indexes' later items. The stdout sink never reports a refusal or a block, so its lines keep position order as in 0.18.
- **`Observer`** gains `on_lane_health` and `on_lane_probe`; `engine::LaneHealth`, `ParkReason` and `ProbeOutcome` are new and re-exported by `daemon`.
- **`daemon::LaneSnapshot`** (a lane of `/status`) gains the public fields `health`, `parked_since`, `next_probe` and `reason`: code building one with a struct literal must set them. `Status::unhealthy_lanes`, `LaneHealthView` and `LaneHealthState` (`Open`, `Refused`, `Blocked`, `Restaging`; `name()` gives the word `/status` writes) are new.
- **Removed: `DaemonOptions::max_restart_backoff`.** The schedule comes from the config: `Config.backoff` (`config::BackoffConfig`, each key a `BackoffValue` with its `BackoffOrigin`), resolved by `daemon::backoff(&Config)`. `Daemon::start` fails when `max_ms` is below `initial_ms`. A struct literal of `DaemonOptions` drops the field.
- **New: `engine::Backoff`** (re-exported as `daemon::Backoff`): `Backoff::new(initial, max)` returns `Err(engine::BackoffError)` for a zero first wait or a cap below it; `initial()`, `max()`, `after(wait)` (doubled, capped); `Default` is 1 s / 60 s. `SinkEngine::with_backoff(Backoff)` sets a sink engine's probe and staging schedule. `config::parse_backoff_ms` parses a flag's or a variable's value.
- **`Sink::ensure_index`** returns `sink::Ensured { target: Option<TargetId>, created }` instead of `()` (build one with `Ensured::existing(target)` or `Ensured::created(target)`; it is `#[non_exhaustive]`). New `sink::TargetId` names a target; a sink that can tell its targets apart gives one created anew a new id. The default implementation names none.
- **New: `SinkError::TargetChanged`.** `mark_seeded` answers it for a target that isn't the one `ensure_index` named; the engine then stages the index again and backfills it in full.
- **`pinned_by` answers an `engine::Pin`** (re-exported as `daemon::Pin`): `Pin::Lane(LaneKey)` or `Pin::Backlog(IndexName)`. `Progress::pinned_by`, `Stalls::pinned_by` and the second half of `Status::stalls()` return `Option<Pin>` instead of `Option<LaneKey>`. `daemon::status::LaneId` is now `PinnedBy` (re-exported as `daemon::PinnedBy`), with `sink: Option<String>`. New `Progress::own_oldest(lane)`: the oldest change the lane holds itself, what its stall age is measured by.
- **Removed: `Observer::on_changes_settled`.** A sink's committed count is no longer an event: read `Progress::committed(sink)`, or `daemon::Status::committed(sink)`. `CommitStats.changes` and `stream::Batch.changes`, which only fed it, are removed: `CommitStats::new` takes `(envelopes, flush, work)`, and a `Batch` literal drops the field (a serialized item that still carries it reads fine).
- **`Progress` counts committed changes by step.** New `engine::Step`, and `Progress::change_read`, `changes_read`, `close_step`, `committed`, `published_step` and `skipped_step`. `Progress::published_build` takes the build's `Step`; `skipped_build` takes an `Option<Token>` and a `Step`. `Progress::steps_kept(lane)` reports how many steps a lane keeps (bounded by its unwritten items), and `RunningDaemon::progress()` hands out the deployment's `Progress`.
- **`daemon::Status::with_progress`** is new: a `Status` built without it reports `0` committed for every sink, so all its captured changes read as in flight.
- **New: `stream::Ticket`.** `Request::Backfill` gains `ticket`, `LaneItem::SnapshotComplete` gains `ticket`, and `Batch` gains `ticket: Option<Ticket>` (`None` on a live batch). Serialized items without them still read (`serde` defaults). Struct literals must set them.

## Related

- [A lane is stalled](stalled-lane.md)
- [Handle rejected documents](rejected-documents.md)
- [Index entries and on_error](../reference/index-and-on-error.md#on_error)
