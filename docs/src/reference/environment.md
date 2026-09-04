# Environment variables

flusso reads the environment for three jobs: filling in config values, setting CLI flags, and configuring logging and telemetry. This page is the complete list.

## Config values

Anywhere `flusso.toml` accepts a secret or connection value, it takes either a literal or a reference resolved where the pipeline runs:

```toml
password = "literal-secret"
password = { env = "OS_PASSWORD" }
```

A reference is **deferred**: a compiled `flusso.lock` carries the variable's name, never its value, and an unset variable fails at run time, not at build time. Any variable name works.

Every secret-valued option also has an **override variable**, so one file runs unedited across environments. The name follows one rule for every adapter: `<ENTRY>_<TYPE>_<KEY>`, uppercased, where `<ENTRY>` is the table (`SOURCE`, `STREAM`, or the sink's name) and nested keys join with `_`.

| Variable | Fills or overrides |
| --- | --- |
| `SOURCE_POSTGRES_CONNECTION_URL` | the source `connection_url` (any shape, or omitted) |
| `SOURCE_POSTGRES_CONNECTION_URL_PASSWORD` | the `password` of a parts-table `connection_url` |
| `<NAME>_OPENSEARCH_URL` | the `url` of `[sinks.<name>]` |
| `<NAME>_OPENSEARCH_USERNAME` | that sink's `username` |
| `<NAME>_OPENSEARCH_PASSWORD` | that sink's `password` |

`<NAME>` is the uppercased sink name, so several OpenSearch sinks never collide. Each adapter's Reference page lists its variables beside the option they override.

**Precedence**, highest first:

1. An explicit `{ env = "X" }` reference. It names its own source and is never overridden. Unset `X` is an error.
2. The reserved variable, if set. It overrides a literal and fills an omitted value.
3. The literal in the file.
4. Otherwise, an error for anything required.

## CLI flags

Every flag reads a `FLUSSO_*` variable. **The flag wins** when both are set. `flusso <cmd> --help` shows each flag's variable.

| Variable | Flag | Commands |
| --- | --- | --- |
| `FLUSSO_CONFIG` | `--config` | `build`, `check`, `run`, `design` (and the query derive at compile time, see below) |
| `FLUSSO_OUT` | `--out` | `build` |
| `FLUSSO_LOCK` | `--lock` | `run` |
| `FLUSSO_LOCKED` | `--locked` | `run` |
| `FLUSSO_SLOT` | `--slot` | `run` |
| `FLUSSO_PUBLICATION` | `--publication` | `run`, `check` |
| `FLUSSO_MANAGE_PUBLICATION` | `--manage-publication` | `run`, `check` |
| `FLUSSO_SKIP_BACKFILL` | `--skip-backfill` | `run` |
| `FLUSSO_PRETTY` | `--pretty` | `run` |
| `FLUSSO_QUEUE_CAPACITY` | `--queue-capacity` | `run` |
| `FLUSSO_PUBLIC_ADDRESS` | `--public-address` | `run` |
| `FLUSSO_PRIVATE_ADDRESS` | `--private-address` | `run` |
| `FLUSSO_ADMIN_USER` | `--admin-user` | `run`, `indexes`, `reindex` |
| `FLUSSO_ADMIN_PASSWORD` | `--admin-password` | `run`, `indexes`, `reindex` |
| `FLUSSO_LAG_POLL_SECS` | `--lag-poll-secs` | `run` |
| `FLUSSO_INDEX_PREFIX` | `--index-prefix` | `run` |
| `FLUSSO_SERVER` | `--server` | `indexes`, `reindex` |
| `FLUSSO_OFFLINE` | `--offline` | `check` |
| `FLUSSO_FORMAT` | `--format` | `check` |
| `FLUSSO_DESIGN_ADDRESS` | `--address` | `design` |
| `FLUSSO_DESIGN_NO_OPEN` | `--no-open` | `design` |
| `FLUSSO_SCHEMA` | the positional schema kind | `schema` |

Defaults and semantics are on the [CLI](cli.md) page.

**The derive reads `FLUSSO_CONFIG` at compile time.** `#[derive(FlussoRoot)]` uses it to locate `flusso.toml` when walking up from the consuming crate's `CARGO_MANIFEST_DIR` finds nothing. Same name, consumed by the proc-macro instead of the binary.

## Logging and telemetry

| Variable | Default | Effect |
| --- | --- | --- |
| `RUST_LOG` | `info` | Log filter, [`tracing` env-filter syntax](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html), e.g. `flusso=debug,info`. Logs go to stderr. |
| `FLUSSO_LOG_FORMAT` | text | `json` for one JSON object per line. |
| `NO_COLOR` | unset | Any value disables colored `check` output. Color is also off when stdout isn't a terminal. |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset | Base OTLP endpoint. Its presence turns on trace **and** metric export. |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | unset | Traces-only endpoint; enables trace export alone. |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | unset | Metrics-only endpoint; enables metric export alone. |
| `OTEL_EXPORTER_OTLP_PROTOCOL` | `http/protobuf` | Transport for both signals: `http/protobuf` or `grpc`. An unrecognized value warns and falls back. |
| `OTEL_EXPORTER_OTLP_TRACES_PROTOCOL` | the general one | Per-signal override. |
| `OTEL_EXPORTER_OTLP_METRICS_PROTOCOL` | the general one | Per-signal override. |

With no OTLP endpoint set, no exporter is installed and telemetry costs nothing. When one is set, the rest of the standard `OTEL_*` variables (`OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`, …) are honored by the SDK. The protocol is **not** inferred from the port: pointing the default HTTP exporter at a gRPC `:4317` fails with a network error. Prometheus is a separate pull path at `/metrics` and needs no variable. The runbook is [Ship traces over OTLP](../operate/traces-otlp.md).

## Example

```sh
# secrets and connections, resolved at run time
SOURCE_POSTGRES_CONNECTION_URL=postgres://user:pass@host:5432/db
PRIMARY_OPENSEARCH_URL=https://opensearch:9200
PRIMARY_OPENSEARCH_USERNAME=flusso
PRIMARY_OPENSEARCH_PASSWORD=…

# flags
FLUSSO_CONFIG=/config/flusso.toml
FLUSSO_PUBLIC_ADDRESS=0.0.0.0:9464
FLUSSO_ADMIN_PASSWORD=change-me

# logging and telemetry
RUST_LOG=flusso=debug,info
FLUSSO_LOG_FORMAT=json
OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318
```
