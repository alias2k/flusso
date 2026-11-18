# Metrics

flusso records OpenTelemetry instruments and exposes them two ways: a Prometheus scrape at `/metrics` on the public surface, and an OTLP push when an endpoint is configured. Same instruments, both readers.

| Instrument | Kind | Unit | Labels | Meaning |
| --- | --- | --- | --- | --- |
| `flusso.changes.captured` | counter | | | Changes the ingest engine pulled from the source. |
| `flusso.changes.committed` | counter | | `sink` | Changes whose batch that sink wrote and acked. |
| `flusso.changes.in_flight` | gauge | | `sink` | Captured minus that sink's committed: the back-pressure signal per sink. Read from status at scrape time, so it stays current while a sink is stalled. |
| `flusso.documents.built` | counter | | `index` | Documents assembled by the ingest engine, once for every sink. The unlabeled total is the sum. |
| `flusso.documents.quarantined` | counter | | `sink`, `index` | Documents that sink rejected and its engine skipped under `on_error = "skip"`. Non-zero means data is being dropped. Alert on it. |
| `flusso.script.failures` | counter | | `index`, `field`, `policy` | Documents that failed to build: a [script failed](scripts.md#failures), or an [`array: true`](field-types.md#array) column held an object. `field` is the path with array positions folded (`variants[].gross`); `policy` is `skip` (the document kept its indexed version) or `stop` (the ingest engine stopped; each retry counts). Non-zero means documents are going stale. Alert on it. See [Script fields](scripts.md#failures). |
| `flusso.batches` | counter | | `sink` | Batches that sink wrote. |
| `flusso.flush.duration` | histogram | s | `sink` | Time per write (one batch) at that sink. Several run at once, so the sum can exceed wall time. Buckets from 1 ms to 10 s. |
| `flusso.flush.documents` | histogram | | `sink`, `work` | Documents per write at that sink. A write of nothing (a live batch that resolved to no document) isn't counted. Buckets from 1 to 10,000. |
| `flusso.flush.size` | histogram | By | `sink`, `work` | Bytes per write, where the sink counts them (OpenSearch: the `_bulk` bodies; stdout: none). Same writes as `flusso.flush.documents`. Buckets from 1 KiB to 64 MiB. |
| `flusso.stage.duration` | histogram | s | `stage`, `work`, `sink`, `index` | Time each pipeline stage took, once per batch, lane item, idle stretch or operation. See [Where the time goes](#where-the-time-goes). Buckets from 100 µs to 30 s. |
| `flusso.startup.duration` | gauge | s | `step`, `sink` | Time a start spent in each step. See [Start-up](#start-up). |
| `flusso.indexes` | gauge | | `sink` | Indexes ensured at that sink this run. |
| `flusso.indexes.seeded` | counter | | `sink`, `index` | Indexes whose backfill completed at that sink this run. |
| `flusso.replication.slot_lag` | gauge | By | | Bytes the confirmed position trails the WAL head by. Sampled every `--lag-poll-secs`. The confirmed position is the slowest sink's. |
| `flusso.replication.visibility_wait` | gauge | s | | Seconds the source has held a committed transaction that isn't visible yet; every later change waits behind it. `0` while nothing waits. Read at scrape time. Past a few seconds it means a synchronous standby isn't answering: see [Behind a synchronous standby](source-postgres.md#behind-a-synchronous-standby). Alert on it: the dev stack's `dev/prometheus/rules/flusso.rules.yml` has `FlussoVisibilityWait`, over 30 s for a minute. |
| `flusso.errors` | counter | | `engine` | Errors that stopped an engine: `ingest`, or `sink:<name>`. A sink engine is restarted with backoff; the counter keeps climbing while it fails. |

## Prometheus names

The Prometheus reader applies the usual conventions: dots become underscores, counters get `_total`, units are appended.

| Instrument | Prometheus series |
| --- | --- |
| `flusso.changes.captured` | `flusso_changes_captured_total` |
| `flusso.changes.committed` | `flusso_changes_committed_total{sink}` |
| `flusso.changes.in_flight` | `flusso_changes_in_flight{sink}` |
| `flusso.documents.built` | `flusso_documents_built_total{index}` |
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
| `flusso.replication.slot_lag` | `flusso_replication_slot_lag_bytes` |
| `flusso.replication.visibility_wait` | `flusso_replication_visibility_wait_seconds` |
| `flusso.errors` | `flusso_errors_total{engine}` |

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
| `write` | sink | both | ✓ | Writing one lane item to the destination, one [`Sink::write`](../contribute/pipeline.md#several-writes-per-sink-one-document-at-a-time): for OpenSearch, rendering it and sending its `_bulk` request. Reported once per write, so with `writes` above 1 several overlap and the rate passes 1. Also an empty caught-up write, as when the sink goes idle with a position not yet searchable. `flusso.flush.duration` times the same writes. |
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
