# Metrics

flusso records OpenTelemetry instruments and exposes them two ways: a Prometheus scrape at `/metrics` on the public surface, and an OTLP push when an endpoint is configured. Same instruments, both readers.

| Instrument | Kind | Unit | Labels | Meaning |
| --- | --- | --- | --- | --- |
| `flusso.changes.captured` | counter | | | Changes pulled from the source into the queue. |
| `flusso.changes.committed` | counter | | | Changes whose documents have been flushed and acked. |
| `flusso.changes.in_flight` | gauge | | | Captured minus committed: the back-pressure signal. Read from status at scrape time, so it stays current while the sink is stalled. |
| `flusso.documents.built` | counter | | `index` | Documents assembled and written. The unlabeled total is the sum. |
| `flusso.documents.quarantined` | counter | | `index` | Documents the sink rejected and the engine skipped under `on_error = "skip"`. Non-zero means data is being dropped. Alert on it. |
| `flusso.batches` | counter | | | Batches flushed. |
| `flusso.flush.duration` | histogram | s | | Time per sink flush. Buckets from 1 ms to 10 s. |
| `flusso.indexes` | gauge | | | Indexes ensured at the sink this run. |
| `flusso.indexes.seeded` | counter | | `index` | Indexes whose backfill completed this run. |
| `flusso.replication.slot_lag` | gauge | By | | Bytes the confirmed position trails the WAL head by. Sampled every `--lag-poll-secs`. |
| `flusso.errors` | counter | | | Errors that stopped the pipeline. |

## Prometheus names

The Prometheus reader applies the usual conventions: dots become underscores, counters get `_total`, units are appended.

| Instrument | Prometheus series |
| --- | --- |
| `flusso.changes.captured` | `flusso_changes_captured_total` |
| `flusso.changes.committed` | `flusso_changes_committed_total` |
| `flusso.changes.in_flight` | `flusso_changes_in_flight` |
| `flusso.documents.built` | `flusso_documents_built_total{index}` |
| `flusso.documents.quarantined` | `flusso_documents_quarantined_total{index}` |
| `flusso.batches` | `flusso_batches_total` |
| `flusso.flush.duration` | `flusso_flush_duration_seconds_bucket`, `_sum`, `_count` |
| `flusso.indexes` | `flusso_indexes` |
| `flusso.indexes.seeded` | `flusso_indexes_seeded_total{index}` |
| `flusso.replication.slot_lag` | `flusso_replication_slot_lag_bytes` |
| `flusso.errors` | `flusso_errors_total` |

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
