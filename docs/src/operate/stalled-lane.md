# A lane is stalled

Find the index and sink that stopped writing, fix the cause, and know what it costs while it lasts.

## When to use this

`FlussoLaneStalled` fired, flusso logged `a lane has landed nothing for …s`, or `/status` shows a lane with a high `stall_seconds`. A **lane** is one index flowing into one sink. It is stalled when its oldest outstanding change hasn't been written for a while: a busy lane that keeps writing is not stalled, however long its backlog.

**What it costs.** The replication slot is confirmed only as far as the slowest lane has written. A stalled lane therefore keeps Postgres's WAL for **every** index, not just its own: `flusso.lane.wal_behind` grows on it (it counts WAL flusso hasn't read yet too, so on its own it is high during any catch-up; with a stall age it is the real pin), and `pg_replication_slots` shows the slot's retained WAL growing. Past `max_slot_wal_keep_size` Postgres invalidates the slot, and every index on every sink is rebuilt from a backfill ([Recover from a dropped slot](dropped-slot.md)). Dropping and rebuilding just the stalled index is not available yet ([#394](https://github.com/alias2k/flusso/issues/394)). The other indexes keep being written while memory allows: their changes still flow, but the built documents waiting on the stalled lane hold part of the [memory budget](../reference/config-toml.md#concurrency).

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
- **Thresholds.** flusso logs a warning once a lane passes 10 minutes and every 10 minutes after; it changes nothing else. `FlussoLaneStalled` (in `dev/prometheus/rules/flusso.rules.yml` and the Helm chart's `prometheusRule`) fires for 5 minutes of either: a lane landing nothing for 10 minutes (label `reason: age`), or the slot keeping more than 1 GiB of WAL behind a lane that has landed nothing for over a minute (`reason: wal`). The WAL branch needs the age because WAL behind includes WAL not read yet: a catch-up after a restart has gigabytes of it while every lane lands each second. A lane that lands, just slowly, isn't stalled and fires neither; `flusso:backlog_drain_eta_seconds` shows how long it will take. Tune the thresholds in the [Helm values](../reference/helm-values.md) or your own rules.
- **A process restart resets the age.** The age is kept in memory: a sink engine or ingest engine restarting inside one process keeps it, a new process starts it at 0 (and the alert's 5 minutes over again). A process that crash-loops therefore never ages a lane into the alert. Alert on the process instead, in your monitoring: `up{job="<flusso's scrape job>"} == 0` for a few minutes, or, under Kubernetes, the pod's restart count rising (`increase(kube_pod_container_status_restarts_total{container="flusso"}[15m]) > 2`).

## Related

- [`/status`](../reference/http.md#get-status) and [Metrics](../reference/metrics.md)
- [Handle rejected documents](rejected-documents.md)
- [Recover from a dropped slot](dropped-slot.md)
