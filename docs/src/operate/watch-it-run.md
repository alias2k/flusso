# Watch it run

Read flusso's live status, scrape its metrics into Prometheus, and put the Grafana dashboard in front of them.

## When to use this

flusso is running and you want to know whether it's keeping up, what it's done, and when it last failed, from a terminal or a dashboard.

## Steps

1. **Read the status document.** The public surface serves it unauthenticated.

   ```sh
   curl -s localhost:9464/status | jq
   ```

   `phase` should be `live` and every entry in `indexes` should be `seeded`. `changes_in_flight` is the backlog between capture and the sink; `slot_lag_bytes` is how far the confirmed position trails Postgres. The field list is in [HTTP endpoints](../reference/http.md#get-status).

2. **Wire the probes.** `/healthz` is liveness; `/readyz` returns `200` from the start of backfill onward and `503` once the pipeline stops. Point Kubernetes probes at both; the Helm chart already does.

3. **Scrape `/metrics`.** Add a Prometheus job for the public address. In the dev stack this is `host.docker.internal:9464`; in Kubernetes, enable the chart's `ServiceMonitor` or scrape the Service.

   ```yaml
   scrape_configs:
     - job_name: flusso
       static_configs:
         - targets: ["flusso.internal:9464"]
   ```

   The series and their meanings are in [Metrics](../reference/metrics.md).

4. **Add the drain-ETA recording rules.** Copy `dev/prometheus/rules/flusso.rules.yml` from the repository. It derives the net drain rate and a time-to-clear estimate from slot lag alone.

5. **Import the dashboard.** `dev/grafana/provisioning/dashboards/flusso.json` shows change throughput, in-flight backlog, slot lag with its trend, flush duration p95, documents built, errors, and the drain ETA. In the dev stack it's provisioned at `localhost:3000`.

6. **Alert on the two that matter.**

   | Alert | Expression | Why |
   | --- | --- | --- |
   | data being dropped | `increase(flusso_documents_quarantined_total[10m]) > 0` | a document was rejected and skipped under `on_error = "skip"` |
   | falling behind | `flusso:slot_lag_bytes_rate5m > 0 for 15m` | the backlog is growing, not draining |

   Add `flusso_errors_total` increasing, and `up == 0` on the scrape target.

## Options and variations

- **Push instead of scrape.** The same instruments export over OTLP every 10 s when `OTEL_EXPORTER_OTLP_ENDPOINT` is set. See [Ship traces over OTLP](traces-otlp.md).
- **Generate load** to see the numbers move: the dev stack's `dev/load.sql` defines `simulate_production()`, a read-modify-write loop.
- **Structured logs.** `FLUSSO_LOG_FORMAT=json` for a log pipeline; `RUST_LOG=flusso=debug,info` when chasing something.
- **The `just` shortcuts** in the repository: `just status`, `just metrics`, `just eta`, `just grafana`.

## Related

- [Metrics](../reference/metrics.md), [HTTP endpoints](../reference/http.md).
- [Handle rejected documents](rejected-documents.md) when the quarantine alert fires.
