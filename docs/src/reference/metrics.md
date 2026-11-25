# Metrics

flusso records OpenTelemetry instruments and exposes them two ways: a Prometheus scrape at `/metrics` on the public surface, and an OTLP push when an endpoint is configured. Same instruments, both readers.

| Instrument | Kind | Unit | Labels | Meaning |
| --- | --- | --- | --- | --- |
| `flusso.changes.captured` | counter | | | Changes the ingest engine pulled from the source. |
| `flusso.changes.committed` | counter | | `sink` | Live changes that sink has written on every lane (or had nothing to write for), in read order: `/status`'s [`changes_committed`](http.md#get-status), read from status at scrape time. One stalled index holds it for the whole sink. |
| `flusso.changes.in_flight` | gauge | | `sink` | Captured minus that sink's committed: the back-pressure signal per sink. Above 0 while any lane of the sink is behind, so one stalled index holds it for all the sink's indexes; `0` means everything read is written there. Read from status at scrape time, so it stays current while a sink is stalled. |
| `flusso.lane.stall_age` | gauge | s | `index`, `sink` | How long the lane's oldest outstanding change has stayed its oldest: the lane has written nothing for that long. Counts only what the lane holds itself: items routed to it and not written, and a built batch it hasn't taken yet (a full lane); its index's backlog, not yet built or handed to a sink, stalls no lane. 0 while the lane holds nothing or while it lands steadily; sampled every second. Alert on it: [A lane is stalled](../operate/stalled-lane.md). |
| `flusso.lane.wal_behind` | gauge | By | `index`, `sink` | Bytes of WAL the replication slot keeps behind the lane, while it holds a change itself (its index's backlog doesn't count). It counts WAL flusso hasn't read yet as well as what it read and hasn't landed (a stalled lane pins both), so a healthy catch-up shows a large value on lanes that land every second: read it with the stall age. It is computed from the slot lag, polled every `--lag-poll-secs` (15 s by default); a failed poll keeps the last value. |
| `flusso.slot.pinned_by` | gauge | | `index`, `sink` | 1 on what holds the slot back the most: the lane (`index` and `sink`), or an index's backlog (`index` alone, no `sink`: documents waiting to be built or being built, on no sink yet). A backlog series shows up for a moment during live builds; one that stays means builds aren't keeping up. Same answer as `/status`'s [`pinned_by`](http.md#get-status). No series while nothing is outstanding. |
| `flusso.lane.parked` | gauge | | `index`, `sink`, `why` | 1 while flusso doesn't write the lane: `why` is the lane's [`health`](http.md#lane-health), `refused` (a document refused under `on_error = "stop"`), `blocked` (the index refuses writes for now: a write block, a closed index, a missing permission; or a rebuild's seed can't [retire the old generation](sink-opensearch.md#an-old-generation-that-cant-be-retired)) or `restaging` (its target was gone, a [reindex couldn't stage](http.md#when-the-reindex-cant-stage), or a [restart couldn't ensure the index](http.md#when-a-restart-cant-ensure-an-index), and the index is being staged again). No series while the lane is written. Read at scrape time. `FlussoLaneParked` alerts on it; it covers a refused backfill page too, which has no position and so no stall age. See [A lane is stalled](../operate/stalled-lane.md). |
| `flusso.lane.probes` | counter | | `index`, `sink`, `outcome` | A parked lane's probe (its oldest batch, or for a `refused` lane a [fresh build](index-and-on-error.md#a-refused-lanes-fresh-build) of it, written alone; for a lane parked on its seed, the seed), counted once, by how it came back: `landed` (the lane opens), `refused`, `blocked`, `missing` (the index is staged again) or `failed` (the write failed as a whole). |
| `flusso.lane.outstanding` | gauge | | `index`, `sink` | Items routed to that `(index, sink)` lane and not written yet. Read at scrape time. The lane that stays high is the one holding back the replication slot and every fence on its index. |
| `flusso.index.documents_waiting` | gauge | | `index` | Documents of the index read and not yet taken by a build: `/status`'s [`documents_waiting`](http.md#per-index-ingest-state). Like every `flusso.index.*` gauge, read at scrape time, one series per enabled index (one index too), none for a disabled index. |
| `flusso.index.backlog_age` | gauge | s | `index` | How long ago the index's oldest change not built yet was read: [`backlog_seconds`](http.md#what-backlog_seconds-shows). Grows only while the index is stuck or exempt. Not a lag behind the source: that is `flusso.lane.wal_behind` and `flusso.replication.slot_lag`. `FlussoIndexBacklog` alerts on it ([runbook](../operate/stalled-lane.md#when-flussoindexbacklog-fires)). |
| `flusso.index.exempt` | gauge | | `index` | 1 while the index is [exempt](config-toml.md#a-stuck-index), growing or frozen; else 0. `FlussoIndexBacklog` alerts on it. |
| `flusso.index.frozen` | gauge | | `index` | 1 while the index is exempt and frozen: it may drain but not grow. |
| `flusso.index.working` | gauge | | `index` | 1 while the index is [`working`](http.md#per-index-ingest-state); 0 once items of it have waited at a sink 5 s with none acknowledged, and while it is down. |
| `flusso.index.held` | gauge | | `index`, `by` | 1 on the series of what holds the index's builds, `by="budget"` (the memory budget refuses its live batch) or `by="lane"` (a publish waits on a full lane), 0 on the other; both 0 while nothing holds it. A hold alone isn't a fault: a held index whose sink keeps acknowledging is working. |
| `flusso.index.backfilling` | gauge | | `index` | 1 while the index's backfill runs; 0 while a failed one waits to retry. |
| `flusso.index.backfill_retrying` | gauge | | `index` | 1 while the index's backfill failed and waits to retry, its live changes flowing ([A failing index](config-toml.md#a-failing-index)). No alert fires on it. |
| `flusso.index.down` | gauge | | `index` | 1 while the index is down: its live work failed and it waits to restart alone ([A failing index](config-toml.md#a-failing-index)). Always 0 with one index, where a failure restarts the whole ingest engine. |
| `flusso.index.restarts` | counter | | `index`, `kind` | How often the index's work started again after a failure: `live` (the index restarted after it was down) or `backfill` (its backfill was retried). `/status`'s [`restarts`](http.md#per-index-ingest-state), read at scrape time. Always 0 with one index. |
| `flusso.reader.waits` | histogram | s | `bound`, `index` | Each time the reader stops at its [lead](config-toml.md#reading-ahead), how long, recorded when the wait ends, by the `bound` that stopped it: `per_index` (an index at `max_waiting_per_index`, or a frozen one at the count it froze at; `index` names it), `total` (the non-exempt indexes' documents at `max_waiting`), `changes` (`max_waiting` changes read ahead of a builder's oldest queued one), `pending` (the exempt indexes at `max_pending_documents`). Only `per_index` carries `index`. A check that finds room at once isn't a wait. Each stretch under one bound is one observation, so a wait whose bound changes counts once per bound. `_count` is how often reading stopped at a bound, `_sum` how long. The memory budget is no bound of the reader's: it holds builds, which then show here as one of these. Buckets from 100 µs to 600 s. |
| `flusso.documents.built` | counter | | `index` | Documents assembled by the ingest engine, once for every sink. The unlabeled total is the sum. |
| `flusso.documents.quarantined` | counter | | `sink`, `index` | Documents that sink rejected and its engine skipped under `on_error = "skip"`. Non-zero means data is being dropped. Alert on it. |
| `flusso.script.failures` | counter | | `index`, `field`, `policy` | Documents that failed to build: a [script failed](scripts.md#failures), or an [`array: true`](field-types.md#array) column held an object. `field` is the path with array positions folded (`variants[].gross`); `policy` is `skip` (the document kept its indexed version) or `stop` (its index went [down](config-toml.md#a-failing-index), or with one index the ingest engine stopped; each retry counts). Non-zero means documents are going stale. Alert on it. See [Script fields](scripts.md#failures). |
| `flusso.batches` | counter | | `sink` | Batches that sink wrote. |
| `flusso.flush.duration` | histogram | s | `sink` | Time per write (one batch) at that sink. Several run at once, so the sum can exceed wall time. Buckets from 1 ms to 10 s. |
| `flusso.flush.documents` | histogram | | `sink`, `work` | Documents per write at that sink, every lane item merged into the write counted (about one batch in a burst). A write of nothing (a live batch that resolved to no document) isn't counted. Buckets from 1 to 10,000. |
| `flusso.flush.size` | histogram | By | `sink`, `work` | Bytes per write, where the sink counts them (OpenSearch: the `_bulk` bodies; stdout: none). Same writes as `flusso.flush.documents`. Buckets from 1 KiB to 64 MiB. |
| `flusso.stage.duration` | histogram | s | `stage`, `work`, `sink`, `index` | Time each pipeline stage took, once per batch, lane item, idle stretch or operation. See [Where the time goes](#where-the-time-goes). Buckets from 100 µs to 30 s. |
| `flusso.memory.built` | gauge | By | `work` | Bytes of built documents not yet acknowledged by every sink, builds in flight included, by `live` and `backfill`. Their sum stays near `[concurrency] max_built_mib`'s budget at most; `backfill` at most half of it, past one batch per index backfilling. Flat at the budget means a sink isn't acknowledging and ingest is paced. |
| `flusso.memory.refused` | counter | | `work` | Times a batch waited for the memory budget before it could be built, once per wait. Steady growth means the budget, or a sink behind it, sets the pace. |
| `flusso.memory.wait` | histogram | s | `work`, `wake` | How long a batch the memory budget held back waited before it was granted, and what let it go: `released` (a sink acknowledged bytes: the mechanism), `landed` (a build landed), `change`, `page`, `tick` (the once-a-second backstop: a few short ones are a race between the tick and a release already there; long ones would mean a release went unheard), `settle` (the end of a run). |
| `flusso.startup.duration` | gauge | s | `step`, `sink` | Time a start spent in each step. See [Start-up](#start-up). |
| `flusso.indexes` | gauge | | `sink` | Indexes the sink's last staging ensured. An index it [couldn't ensure](http.md#when-a-restart-cant-ensure-an-index) isn't counted, even once its lane opens, until the sink stages again. |
| `flusso.indexes.seeded` | counter | | `sink`, `index` | Indexes whose backfill completed at that sink this run. |
| `flusso.backfill.checkpoints` | counter | | `sink`, `index` | Times a sink recorded how far an index's backfill durably got, so a restart resumes after it: about one every 10 s while a backfill lands. |
| `flusso.replication.slot_lag` | gauge | By | | Bytes the confirmed position trails the WAL head by. Sampled every `--lag-poll-secs`. The confirmed position is the slowest sink's. |
| `flusso.replication.visibility_wait` | gauge | s | | Seconds the source has held a committed transaction that isn't visible yet; every later change waits behind it. `0` while nothing waits. Read at scrape time. Past a few seconds it means a synchronous standby isn't answering: see [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby). Alert on it: the dev stack's `dev/prometheus/rules/flusso.rules.yml` has `FlussoVisibilityWait`, over 30 s for a minute. |
| `flusso.errors` | counter | | `engine` | Errors that stopped an engine: `ingest`, or `sink:<name>`. A sink engine is restarted with backoff; the counter keeps climbing while it fails. An index that restarts alone isn't counted here: see `flusso.index.restarts`. |
| `flusso.sink.refreshes` | counter | | `sink` | Requests the sink sent that refresh the destination: for OpenSearch, `_refresh` calls and `_bulk` requests with `?refresh=true`. A caught-up write (its index has nothing else waiting) refreshes; a write during a backlog doesn't. Compare with `flusso.batches` to see how many writes refresh. |

## Prometheus names

The Prometheus reader applies the usual conventions: dots become underscores, counters get `_total`, units are appended.

| Instrument | Prometheus series |
| --- | --- |
| `flusso.changes.captured` | `flusso_changes_captured_total` |
| `flusso.changes.committed` | `flusso_changes_committed_total{sink}` |
| `flusso.changes.in_flight` | `flusso_changes_in_flight{sink}` |
| `flusso.lane.outstanding` | `flusso_lane_outstanding{index,sink}` |
| `flusso.lane.parked` | `flusso_lane_parked{index,sink,why}` |
| `flusso.lane.probes` | `flusso_lane_probes_total{index,sink,outcome}` |
| `flusso.lane.stall_age` | `flusso_lane_stall_age_seconds{index,sink}` |
| `flusso.lane.wal_behind` | `flusso_lane_wal_behind_bytes{index,sink}` |
| `flusso.slot.pinned_by` | `flusso_slot_pinned_by{index,sink}`, or `{index}` alone for an index's backlog (matched by `sink=""`) |
| `flusso.index.documents_waiting` | `flusso_index_documents_waiting{index}` |
| `flusso.index.backlog_age` | `flusso_index_backlog_age_seconds{index}` |
| `flusso.index.exempt` | `flusso_index_exempt{index}` |
| `flusso.index.frozen` | `flusso_index_frozen{index}` |
| `flusso.index.working` | `flusso_index_working{index}` |
| `flusso.index.held` | `flusso_index_held{index,by}` |
| `flusso.index.backfilling` | `flusso_index_backfilling{index}` |
| `flusso.index.backfill_retrying` | `flusso_index_backfill_retrying{index}` |
| `flusso.index.down` | `flusso_index_down{index}` |
| `flusso.index.restarts` | `flusso_index_restarts_total{index,kind}` |
| `flusso.reader.waits` | `flusso_reader_waits_seconds_bucket{bound}`, `{bound,index}` for `per_index`, `_sum`, `_count` |
| `flusso.documents.built` | `flusso_documents_built_total{index}` |
| `flusso.memory.built` | `flusso_memory_built_bytes{work}` |
| `flusso.memory.refused` | `flusso_memory_refused_total{work}` |
| `flusso.memory.wait` | `flusso_memory_wait_seconds_bucket{work,wake}`, `_sum`, `_count` |
| `flusso.documents.quarantined` | `flusso_documents_quarantined_total{sink,index}` |
| `flusso.script.failures` | `flusso_script_failures_total{index,field,policy}` |
| `flusso.batches` | `flusso_batches_total{sink}` |
| `flusso.flush.duration` | `flusso_flush_duration_seconds_bucket{sink}`, `_sum`, `_count` |
| `flusso.flush.documents` | `flusso_flush_documents_bucket{sink,work}`, `_sum`, `_count` |
| `flusso.flush.size` | `flusso_flush_size_bytes_bucket{sink,work}`, `_sum`, `_count` |
| `flusso.stage.duration` | `flusso_stage_duration_seconds_bucket{stage,work,sink,index}`, `_sum`, `_count` |
| `flusso.startup.duration` | `flusso_startup_duration_seconds{step,sink}` |
| `flusso.indexes` | `flusso_indexes{sink}` |
| `flusso.indexes.seeded` | `flusso_indexes_seeded_total{sink,index}` |
| `flusso.backfill.checkpoints` | `flusso_backfill_checkpoints_total{sink,index}` |
| `flusso.replication.slot_lag` | `flusso_replication_slot_lag_bytes` |
| `flusso.replication.visibility_wait` | `flusso_replication_visibility_wait_seconds` |
| `flusso.errors` | `flusso_errors_total{engine}` |
| `flusso.sink.refreshes` | `flusso_sink_refreshes_total{sink}` |

The scope labels (`otel_scope_name`, `otel_scope_version`) are dropped; there is one scope.

## Where the time goes

`flusso.stage.duration` records one sample per batch (ingest side), lane item (sink side), idle stretch or operation, for each stage. A label is present only where the stage has it.

| `stage` | Task | `work` | `sink` | What the time is |
| --- | --- | --- | --- | --- |
| `idle` | ingest | | | Nothing in flight and nothing taken: waiting on the source with no snapshot running and no build running. |
| `page` | ingest | `backfill` | | Getting a backfill's keys: waiting for the next page of root keys and buffering them. While a backfill runs, the ingest task's waits are `page`, not `idle`; while a build runs, they are neither (the `build` covers them). |
| `resolve` | ingest | `live` | | Taking the changes the stream has ready and resolving each to the documents it touches: the reverse lookups. |
| `build` | ingest | both | | Building a batch's documents, from its start to its return. Reported once per build, so with `builds` above 1 several overlap and the rate passes 1. |
| `publish` | ingest | both | | Handing a built batch to every lane, until the last one took it: for a snapshot batch, copying each lane's slice of its documents first. High means a lane is full (`lane_wait` says which), or, in a backfill with several sinks, the copies. |
| `lane_wait` | ingest | both | ✓ | The same, per lane: names the full one. Ingest time, labelled with the sink it waited on; overlaps `publish`. |
| `snapshot` | ingest | `backfill` | | Opening a backfill (gathering the requests it serves) and closing it (`SnapshotComplete` to each lane, acknowledging the requests). |
| `idle` | sink | | ✓ | Nothing in flight and nothing taken: no write running and no lane item waiting. A sink waiting for one of its writes to return is not idle. |
| `write` | sink | both | ✓ | Writing to the destination: one write, which holds every lane item that was ready at once, across indexes, up to `[batch] max_changes` documents ([`Sink::write`](../contribute/pipeline.md#several-writes-per-sink-one-document-at-a-time)): for OpenSearch, rendering them and sending their `_bulk` request. No `index` label: a write can hold several indexes. Reported once per write, so with `writes` above 1 several overlap and the rate passes 1. Also an empty caught-up write, as when the sink goes idle with a position not yet searchable. `flusso.flush.duration` times the same writes. |
| `seed` | sink | `backfill` | ✓ | Recording one index as seeded at the destination (`index` label): for OpenSearch a refresh, the settings restored and the alias moved. |
| `reindex` | sink | | ✓ | Staging a reindex asked for while running. |

`index` is set on `seed` today. A stage that becomes per index later carries it too.

An idle stretch is cut at least once a second, so a quiet hour reads as a flat line rather than one spike when work resumes. The part still open at a scrape is reported up to a second later, so it lands in the next scrape window: over a short window, a task's stages can sum to a second more or less than the window. Each task's stages add up to its wall time when it runs one thing at a time, less small bookkeeping no stage times (acknowledgements, confirmations, the drain at a stop).

The `_sum` is the seconds a stage was busy, so its rate is **busy seconds per wall second**: the mean number of that stage in flight. A stage a task runs one at a time stays at or under 1. Once a task runs a stage concurrently (several writes in flight per sink, several builds), it can pass 1.

```promql
# the ingest task, per stage
sum by (stage, work) (rate(flusso_stage_duration_seconds_sum{sink="", stage!="lane_wait"}[1m]))
# each sink, per stage
sum by (sink, stage) (rate(flusso_stage_duration_seconds_sum{stage=~"idle|write|seed|reindex", sink!=""}[1m]))
```

The busier side sets the pace: a sink rarely `idle` while ingest is often `idle` is sink-bound, and the other way round is ingest-bound. The dev dashboard plots both, the time per batch, and the flush size; `flusso-bench` prints the same as a table per phase ([Where the time went](../contribute/benchmarks.md#where-the-time-went)).

## Start-up

`flusso.startup.duration` holds each step of a start, one gauge per `step`, set again when the step runs again:

| `step` | What the time is |
| --- | --- |
| `source` | Connecting to Postgres and validating the indexes against it. |
| `stage` | Staging one sink (`sink` label): ensuring its indexes, retiring stale seeds, asking for its backfill. Set again on each re-stage after a sink engine restarts. A reindex while running is the `reindex` stage instead. |
| `prepare` | Creating or checking the publication, replica identity and slot. |
| `open_live` | Opening the replication stream. |
| `live` | From `flusso run` starting to the first live stream: config, lock, the steps above. Once per process: a restarted ingest engine doesn't set it again. |

## Derived: backlog drain ETA

The dev stack ships two Prometheus recording rules worth copying (`dev/prometheus/rules/flusso.rules.yml`):

| Rule | Expression | Meaning |
| --- | --- | --- |
| `flusso:slot_lag_bytes_rate5m` | `deriv(flusso_replication_slot_lag_bytes[2m])` | Net drain rate in bytes/s. Negative means catching up. |
| `flusso:backlog_drain_eta_seconds` | `(lag > 4 MiB) / (-rate > 0)` | Seconds until the backlog clears at the current rate. Absent while flat or growing, and below an idle floor of 4 MiB. |

The floor exists because slot lag rarely reaches zero: Postgres writes WAL of its own (checkpoints, autovacuum), and flusso's confirmed position catches up with it only when the server reports that progress in a keepalive, so even an idle deployment trails the head a little. Raise it above your deployment's steady idle lag.

## Export

| Path | When | Where |
| --- | --- | --- |
| Prometheus scrape | always, when the public surface is served | `GET /metrics` on `--public-address` |
| OTLP push | `OTEL_EXPORTER_OTLP_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` is set | every 10 s to that endpoint, over the protocol in `OTEL_EXPORTER_OTLP_PROTOCOL` |

The variables are in [Environment variables](environment.md#logging-and-telemetry); the how-to is [Watch it run](../operate/watch-it-run.md).
