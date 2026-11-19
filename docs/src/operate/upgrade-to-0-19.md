# Upgrade to 0.19

0.19 stops one index, not the sink, when that index can't be written: see [One index's failure parks its own lane](#one-indexs-failure-parks-its-own-lane). A write block on an index no longer drops documents under `on_error = "skip"` (#404).

## One index's failure parks its own lane

**What changed.** In 0.18 a failure about one index ended the sink engine's run, and the daemon restarted it with backoff, so every index on that sink paused. In 0.19 the failure stays on that index's **lane** (one index flowing into one sink):

| What OpenSearch answers | 0.18 | 0.19 |
| --- | --- | --- |
| A document refused under `on_error = "stop"` | the sink stops and restarts with backoff | the lane **parks**; the other indexes keep being written |
| A write block (`index.blocks.write`, `read_only`, `read_only_allow_delete`: 403 `cluster_block_exception`) | an item-level rejection: quarantined under `skip` (documents lost), the sink stops under `stop` | the lane parks as **blocked**, whatever `on_error` says; nothing is quarantined |
| An item-level 403 `security_exception` (the role can't write that index) | an item-level rejection | the lane parks as blocked |
| The index's generation deleted while running (`index_not_found_exception`) | the sink stops; its restart recreates the generation | the lane is **staged again** alone: a fresh generation, backfilled, while the others write |
| The whole `_bulk` refused, credentials refused (401, a request-level 403), the cluster unreachable | the sink stops and restarts | unchanged |

A parked lane writes its oldest item again alone (a **probe**) after 1 s, then with the wait doubling up to `max_restart_backoff` (60 s by default); when it lands the lane opens and the rest follows, with no restart. It holds the replication slot meanwhile, as a stalled lane does. A new alert, `FlussoLaneParked`, fires once a lane has been parked (or stuck being staged again) for 5 minutes, naming the index, the sink and why; `FlussoLaneStalled` still fires after 10 minutes for a parked live change, but never for a refused backfill page, which has no position. The runbook is [A lane is stalled](stalled-lane.md).

## Steps

1. **Upgrade flusso.** Nothing to rebuild: no content hash moves.
2. **Check what `stop` means to your operations.** A document refused under `on_error = "stop"` no longer stops the sink: its index's lane parks and the other indexes stay current. `/status` shows the lane `parked` under `sinks.<sink>.lanes.<index>.health`, with the refused ids in `reason`. An alert that watched for the sink's phase `failed` or for restarts no longer sees it, and `/readyz` stays `200` while a lane is parked (the other indexes are healthy, and a restart wouldn't unpark it): add `FlussoLaneParked` (in the dev rules and the Helm chart's `prometheusRule`), or alert on `flusso_lane_parked` yourself ([Metrics](../reference/metrics.md)).
3. **Lift a write block to resume.** An index blocked for writes is written again by the next probe once the block is lifted, at most `max_restart_backoff` later. No restart.

## Options and variations

- **A stop with a parked lane.** The drain can't finish while a lane stays parked: its items never land. The stop probes every parked lane once more at once (a block lifted meanwhile still drains), then waits for its deadline (`--shutdown-timeout-secs`) and exits `1`, naming the lane: `primary: 3 changes in flight, lane users blocked since … (blocked: index [users_…] blocked by: [FORBIDDEN/8/index write (api)];)`. The next start writes the parked items again first.
- **Memory.** A parked lane keeps its built documents until they land, and the documents queued behind it in its lane hold part of the [memory budget](../reference/config-toml.md#concurrency). A lane parked long enough fills the budget, and then the other indexes slow down too, as with a stalled sink in 0.18.
- **For code embedding the crates.** `EngineError::DocumentsRejected` is gone (a refusal no longer ends the run). `sink::WriteReport` gains `blocked` and `missing` (per index, `IndexRefusal`), and a sink reports a missing target there instead of failing the write. `Observer` gains `on_lane_health` and `on_lane_probe`. `daemon::LaneSnapshot` (a lane of `/status`) gains the public fields `health`, `parked_since`, `next_probe` and `reason`, so code building one with a struct literal must set them; `daemon::Status::unhealthy_lanes` and `LaneHealthView` are new. `Sink::ensure_index` and `reindex` may now run beside writes of other indexes. `SinkEngine::with_max_probe_backoff` sets the longest wait between probes.
- **Not yet.** A refused lane writes the same item again on each probe; rebuilding it from the current row first is a later release, as are a reindex of one index without pausing the sink's other writes, and dropping a stalled index automatically ([#394](https://github.com/alias2k/flusso/issues/394)).

## Related

- [A lane is stalled](stalled-lane.md)
- [Handle rejected documents](rejected-documents.md)
- [Index entries and on_error](../reference/index-and-on-error.md#on_error)
