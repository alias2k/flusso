# A lane is stalled

Find the index and sink that stopped writing, fix the cause, and know what it costs while it lasts.

## When to use this

`FlussoLaneStalled` fired, flusso logged `a lane has landed nothing for …s`, or `/status` shows a lane with a high `stall_seconds`. A **lane** is one index flowing into one sink. It is stalled when its oldest outstanding change hasn't been written for a while: a busy lane that keeps writing is not stalled, however long its backlog.

**What it costs.** The replication slot is confirmed only as far as the slowest lane has written. A stalled lane therefore keeps Postgres's WAL for **every** index, not just its own: `flusso.lane.wal_behind` grows on it, and `pg_replication_slots` shows the slot's retained WAL growing. Past `max_slot_wal_keep_size` Postgres invalidates the slot, and every index on every sink is rebuilt from a backfill ([Recover from a dropped slot](dropped-slot.md)). Dropping and rebuilding just the stalled index is not available yet ([#394](https://github.com/alias2k/flusso/issues/394)). The other indexes keep being written while memory allows: their changes still flow, but the built documents waiting on the stalled lane hold part of the [memory budget](../reference/config-toml.md#concurrency).

## Steps

1. **Find the lane.** The alert's `index` and `sink` labels name it. Without alerting, `/status` names the lane pinning the slot in `pinned_by`, and each lane's `stall_seconds` and `wal_behind_bytes` sit under `sinks.<sink>.lanes.<index>`. `flusso.lane.stall_age` plots it.
2. **Read the sink's errors.** flusso logs why the sink can't write: the reason OpenSearch gave, the index, and for a refused document its id. `last_error` in `/status` has the latest.
3. **Fix the cause:**
   - **A write block on the index** (`index.blocks.write`, or `read_only_allow_delete` after a disk watermark): free disk space or lift the block. The lane resumes at once.
   - **A document refused under `on_error = "stop"`:** fix the row or the schema, or switch the index to `skip` ([Handle rejected documents](rejected-documents.md)). The stall is the policy doing its job.
   - **The cluster down or refusing as busy for good:** bring it back. Busy (429/503) refusals are retried, so a lane that stays stalled behind them means the cluster never had room.
   - **A wrong mapping on an index flusso didn't create:** [reindex](reindex.md) the index on that sink.
4. **Watch it recover.** flusso logs `a stalled lane is landing again`, `stall_seconds` drops to 0, and `pinned_by` moves on. The slot then releases the WAL it kept.

## Options and variations

- **A backfill that stops making progress isn't a stall here.** Backfill rows carry no position, hold back no WAL, and give the lane no stall age. It shows as the index staying `backfilling` in `/status` while `flusso.documents.built` stops moving. A live change queued behind it on the same lane does age, and alerts.
- **Thresholds.** flusso logs a warning once a lane passes 10 minutes and every 10 minutes after; it changes nothing else. The alert in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule` fire past 10 minutes or 1 GiB of WAL, for 5 minutes. Tune them in the [Helm values](../reference/helm-values.md) or your own rules.

## Related

- [`/status`](../reference/http.md#get-status) and [Metrics](../reference/metrics.md)
- [Handle rejected documents](rejected-documents.md)
- [Recover from a dropped slot](dropped-slot.md)
