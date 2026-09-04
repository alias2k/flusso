# Metrics

flusso records OpenTelemetry instruments and exposes them two ways: a Prometheus scrape at `/metrics` on the public surface, and an OTLP push when an endpoint is configured. Same instruments, both readers.

| Instrument | Kind | Unit | Labels | Meaning |
| --- | --- | --- | --- | --- |
| `flusso.changes.captured` | counter | | | Changes the ingest engine pulled from the source. |
| `flusso.changes.committed` | counter | | `sink` | Changes whose batch that sink flushed and acked. |
| `flusso.changes.in_flight` | gauge | | `sink` | Captured minus that sink's committed: the back-pressure signal per sink. Read from status at scrape time, so it stays current while a sink is stalled. |
| `flusso.documents.built` | counter | | `index` | Documents assembled by the ingest engine, once for every sink. The unlabeled total is the sum. |
| `flusso.documents.quarantined` | counter | | `sink`, `index` | Documents that sink rejected and its engine skipped under `on_error = "skip"`. Non-zero means data is being dropped. Alert on it. |
| `flusso.batches` | counter | | `sink` | Batches that sink flushed. |
| `flusso.flush.duration` | histogram | s | `sink` | Time per flush at that sink. Buckets from 1 ms to 10 s. |
| `flusso.indexes` | gauge | | `sink` | Indexes ensured at that sink this run. |
| `flusso.indexes.seeded` | counter | | `sink`, `index` | Indexes whose backfill completed at that sink this run. |
| `flusso.replication.slot_lag` | gauge | By | | Bytes the confirmed position trails the WAL head by. Sampled every `--lag-poll-secs`. The confirmed position is the slowest sink's. |
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
| `flusso.batches` | `flusso_batches_total{sink}` |
| `flusso.flush.duration` | `flusso_flush_duration_seconds_bucket{sink}`, `_sum`, `_count` |
| `flusso.indexes` | `flusso_indexes{sink}` |
| `flusso.indexes.seeded` | `flusso_indexes_seeded_total{sink,index}` |
| `flusso.replication.slot_lag` | `flusso_replication_slot_lag_bytes` |
| `flusso.errors` | `flusso_errors_total{engine}` |

The scope labels (`otel_scope_name`, `otel_scope_version`) are dropped; there is one scope.

## Derived: backlog drain ETA

The dev stack ships two Prometheus recording rules worth copying (`dev/prometheus/rules/flusso.rules.yml`):

| Rule | Expression | Meaning |
| --- | --- | --- |
| `flusso:slot_lag_bytes_rate5m` | `deriv(flusso_replication_slot_lag_bytes[2m])` | Net drain rate in bytes/s. Negative means catching up. |
| `flusso:backlog_drain_eta_seconds` | `(lag > 4 MiB) / (-rate > 0)` | Seconds until the backlog clears at the current rate. Absent while flat or growing, and below an idle floor of 4 MiB. |

The floor exists because slot lag never reaches zero: Postgres advances the WAL on checkpoints and autovacuum while flusso's confirmed position moves only on real changes. Raise it above your deployment's steady idle lag.

## Export

| Path | When | Where |
| --- | --- | --- |
| Prometheus scrape | always, when the public surface is served | `GET /metrics` on `--public-address` |
| OTLP push | `OTEL_EXPORTER_OTLP_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` is set | every 10 s to that endpoint, over the protocol in `OTEL_EXPORTER_OTLP_PROTOCOL` |

The variables are in [Environment variables](environment.md#logging-and-telemetry); the how-to is [Watch it run](../operate/watch-it-run.md).
