# Ship traces over OTLP

Export flusso's spans and metrics to an OpenTelemetry collector by setting the standard `OTEL_*` variables, and match the protocol to the port.

## When to use this

You run a collector, Jaeger, Tempo, or a vendor endpoint and want flusso's traces there, or you want metrics pushed rather than scraped.

## Steps

1. **Set the endpoint.** Its presence turns export on; nothing else is needed for traces and metrics over HTTP/protobuf.

   ```sh
   OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318 flusso run …
   ```

   Startup logs `OTLP trace export enabled` and `OTLP metric export enabled`.

2. **Match protocol to port.** The default is HTTP/protobuf, conventionally `:4318`. A gRPC receiver, conventionally `:4317`, needs the protocol switched; flusso does not infer it from the port.

   ```sh
   OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4317 \
   OTEL_EXPORTER_OTLP_PROTOCOL=grpc flusso run …
   ```

   Pointing the HTTP exporter at a gRPC port fails with a repeated `HTTP export failed: network error`.

3. **Add headers or a service name** through the SDK's standard variables: `OTEL_EXPORTER_OTLP_HEADERS=authorization=Bearer …`, `OTEL_SERVICE_NAME=flusso-prod`. The resource's default service name is `flusso`.

4. **Verify.** A span per batch flush appears in the backend; metrics arrive every 10 s.

## Options and variations

- **One signal only.** `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` or `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` enables that signal alone; `…_TRACES_PROTOCOL` / `…_METRICS_PROTOCOL` override the transport per signal.
- **Prometheus keeps working.** The scrape at `/metrics` is independent and needs no variable.
- **Off by default.** With no endpoint set, no exporter is installed and the instruments are no-ops.
- **A failed exporter never blocks startup.** If the exporter can't be built, flusso warns and logs to stderr only.

## Related

- [Environment variables](../reference/environment.md#logging-and-telemetry) for the complete list.
