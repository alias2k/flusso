# `flusso-bench` — the scenario harness

Runs the real `flusso` binary through a named scenario against a Postgres and an OpenSearch container and reports the headline metrics: visible latency, drain throughput, backfill throughput, peak RSS and CPU time. Unpublished; a measuring tool, not shipping code.

```sh
just bench-scenario reference ci      # or: cargo run -p flusso-bench -- --scenario reference --scale ci
```

What the phases measure, the scenarios, the scales, and how CI stores and compares the results are in the manual's [Benchmarks](https://alias2k.github.io/flusso/contribute/benchmarks.html) chapter. The decision record is [ADR 0006](../../docs/adr/0006-benchmarks-real-binary-scenarios-with-published-history.md).

## Layout

```text
src/main.rs        the four phases: seed → backfill → latency → drain; the report
src/scenario.rs    the two scenarios' seed templating and change mixes
src/scale.rs       the `ci` and `default` presets
src/services.rs    containers, or BENCH_PG_URL / BENCH_OS_URL
src/flusso.rs      the child process: spawn, /status, /metrics, ps sampling, stop
src/writer.rs      the paced probed trickle and the concurrent burst
src/probe.rs       when does a stamped row become searchable
src/report.rs      github-action-benchmark JSON + the Prometheus histogram quantile
scenarios/
  reference/       flusso.toml over the dev schemas; seed.sql
  complex/         schema.sql, seed.sql, users.schema.yml, flusso.toml
```
