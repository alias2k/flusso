# Upgrade to 0.19

0.19 keeps a failure of one index on that index. Each change below links to its section.

**Running flusso**

- **An index that can't be written parks alone**; the sink's other indexes keep writing, and a write block no longer drops documents under `on_error = "skip"` (#404): [One index's failure parks its own lane](#one-indexs-failure-parks-its-own-lane).
- **`on_error = "stop"` no longer stops the sink**, and `/readyz` stays `200` while an index is parked; alert on the new `FlussoLaneParked` instead: [Alerting on a parked index](#alerting-on-a-parked-index).

**Code that embeds the crates**

- **New lane-health API, `EngineError::DocumentsRejected` removed, order per index for an ordered sink**: [For code embedding the crates](#for-code-embedding-the-crates).

## One index's failure parks its own lane

**What changed.** In 0.18 a failure about one index ended the sink engine's run, and the daemon restarted the whole sink with backoff: every index on that sink paused. In 0.19 the failure stays on that index's **lane** (one index flowing into one sink), and the sink's other indexes keep being written:

| What OpenSearch answers | 0.18 | 0.19 |
| --- | --- | --- |
| A document refused under `on_error = "stop"` | the sink stops and restarts with backoff | the lane is **parked** |
| A write block (`index.blocks.write`, `read_only`, `read_only_allow_delete`: 403 `cluster_block_exception`) | an item-level rejection: **quarantined under `skip`** (documents lost), the sink stops under `stop` | the lane is **blocked**, whatever `on_error` says; nothing is quarantined |
| An item-level 403 `security_exception` (the role can't write that index) | an item-level rejection | the lane is **blocked** |
| The index's generation deleted while running (`index_not_found_exception`) | the sink stops; its restart recreates the generation | that index alone is **staged again** and backfilled |
| The whole `_bulk` refused, credentials refused, the cluster unreachable, busy past `max_retries` | the sink stops and restarts | unchanged |

A parked or blocked lane writes its oldest batch again alone (a **probe**) after 1 s, then with the wait doubling up to 60 s. When a probe lands, the lane opens and the rest follows, with no restart. Meanwhile the lane holds the replication slot, as a stalled lane always has.

**Who is affected.** Every deployment that writes more than one index to a sink: one bad document or one blocked index no longer holds up the others. Deployments on `on_error = "skip"` whose indexes ever got a write block: those documents used to be quarantined, and are now written once the block is lifted.

**What to do.**

1. **Upgrade flusso.** Nothing to rebuild: no content hash moves.
2. **Lift write blocks to resume.** A blocked index is written by the next probe once the block is lifted, at most 60 s later. No restart. The runbook is [A lane is stalled](stalled-lane.md).
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
- `/status` shows each lane's state under [`sinks.<sink>.lanes.<index>.health`](../reference/http.md#lane-health), with `reason` and `next_probe`.

## For code embedding the crates

- **Removed:** `EngineError::DocumentsRejected` (a refusal no longer ends the run).
- **`sink::WriteReport`** gains `blocked` and `missing`, one `IndexRefusal` per index. A sink reports a missing target there instead of failing the write.
- **`Sink::ensure_index` and `reindex`** may now run beside writes of **other** indexes, never beside one holding the same index.
- **Order is per index** for an ordered sink (`Sink::ordered`). Across indexes, live items go in position order while every lane is written; a parked lane is overtaken by the other indexes' later items. The stdout sink never reports a refusal or a block, so its lines keep position order as in 0.18.
- **`Observer`** gains `on_lane_health` and `on_lane_probe`; `engine::LaneHealth`, `ParkReason` and `ProbeOutcome` are new and re-exported by `daemon`.
- **`daemon::LaneSnapshot`** (a lane of `/status`) gains the public fields `health`, `parked_since`, `next_probe` and `reason`: code building one with a struct literal must set them. `Status::unhealthy_lanes`, `LaneHealthView` and `LaneHealthState` are new.
- **`SinkEngine::with_max_probe_backoff`** sets the longest wait between probes; the daemon passes `DaemonOptions::max_restart_backoff` (60 s by default).

## Related

- [A lane is stalled](stalled-lane.md)
- [Handle rejected documents](rejected-documents.md)
- [Index entries and on_error](../reference/index-and-on-error.md#on_error)
