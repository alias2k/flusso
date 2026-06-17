# flusso environment & runtime configuration

Every environment variable flusso reads, in one place. File formats live
elsewhere: [`SCHEMA.md`](SCHEMA.md) for `flusso.toml` and `*.schema.yml`,
[`SOURCES_AND_SINKS.md`](SOURCES_AND_SINKS.md) for source/sink options.

flusso reads env vars for three jobs:

1. **Filling in config values** — connection strings and credentials kept out of
   `flusso.toml`. This is the secrets story.
2. **Setting CLI flags** — every flag has a `FLUSSO_*` twin.
3. **Logging & telemetry** — `RUST_LOG` / OpenTelemetry.

## Secret & connection values

flusso never bakes a secret into a compiled config. A `flusso.lock` carries only
the *names* of the variables to read; the real values are read in the environment
that runs the pipeline. Compile in CI, run in prod, and the secret never rides
along in between.

### `{ env = "VAR" }` references

Anywhere a secret or connection string is expected, write a literal or an env
reference:

```toml
password = "literal-secret"          # literal — carried as-is
password = { env = "OS_PASSWORD" }   # read from $OS_PASSWORD when the pipeline runs
```

The variable name is yours. An unset variable fails **at run time, not compile
time** — by design, so the compile step needs no secrets.

### Reserved deployment-override variables

A few **well-known names** act as a deployment override layer: set them and the
*same* `flusso.toml` works across environments unedited (12-factor). When set,
they take priority over the file value (and the override is logged at startup):

| Variable | Overrides / fills | Notes |
| --- | --- | --- |
| `DATABASE_URL` | the source `connection_url` | The source is a singleton, so one well-known name is unambiguous. |
| `<SINK>_OPENSEARCH_URL` | a sink's `url` | `<SINK>` is the **uppercased sink name** — `[sinks.primary]` → `PRIMARY_OPENSEARCH_URL`. |
| `<SINK>_OPENSEARCH_USERNAME` | a sink's `username` | Same naming. |
| `<SINK>_OPENSEARCH_PASSWORD` | a sink's `password` | Same naming. |

The per-sink prefix means several OpenSearch sinks never collide (`PRIMARY_…`,
`SECONDARY_…`).

## Precedence

When more than one source could supply a value, highest wins:

1. **An explicit `{ env = "X" }` reference** — names its own source, **never**
   overridden by a reserved variable. (If `X` is unset, that's an error.)
2. **The reserved variable**, if set — overrides a literal in the file and fills a
   value the file omitted.
3. **The literal value** in the config.
4. Otherwise → an error, for anything required (the source URL, a sink `url`).

Shortcut: *"I asked for a specific variable"* beats *"the deployment set the
well-known one"* beats *"whatever's written in the file."*

```toml
[source]
type = "postgres"
connection_url = "postgres://localhost/dev"   # $DATABASE_URL wins if set

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"                 # $PRIMARY_OPENSEARCH_URL wins if set
# username / password omitted → filled from
# $PRIMARY_OPENSEARCH_USERNAME / $PRIMARY_OPENSEARCH_PASSWORD
```

## CLI flags as env vars

Every `flusso` flag also reads a `FLUSSO_*` environment variable. **The flag wins
when both are set** — env is the fallback.

| Variable | Flag | Commands |
| --- | --- | --- |
| `FLUSSO_CONFIG` | `--config` | `build`, `check`, `run` |
| `FLUSSO_OUT` | `--out` | `build` |
| `FLUSSO_ARTIFACT` | `--artifact` | `run` |
| `FLUSSO_SLOT` | `--slot` | `run` |
| `FLUSSO_PUBLICATION` | `--publication` | `run` |
| `FLUSSO_SKIP_BACKFILL` | `--skip-backfill` | `run` |
| `FLUSSO_PRETTY` | `--pretty` | `run` |
| `FLUSSO_QUEUE_CAPACITY` | `--queue-capacity` | `run` |
| `FLUSSO_PUBLIC_ADDRESS` | `--public-address` | `run` |
| `FLUSSO_PRIVATE_ADDRESS` | `--private-address` | `run` |
| `FLUSSO_ADMIN_USER` | `--admin-user` | `run`, `indexes`, `reindex` |
| `FLUSSO_ADMIN_PASSWORD` | `--admin-password` | `run`, `indexes`, `reindex` |
| `FLUSSO_SERVER` | `--server` | `indexes`, `reindex` |
| `FLUSSO_LAG_POLL_SECS` | `--lag-poll-secs` | `run` |
| `FLUSSO_OFFLINE` | `--offline` | `check` |
| `FLUSSO_FORMAT` | `--format` | `check` |
| `FLUSSO_SCHEMA` | the schema-kind argument | `schema` |

`flusso <cmd> --help` shows the matching `[env: FLUSSO_…]` next to each flag.

The two operational HTTP surfaces have an extra fallback for their bind addresses:
a `[server]` table in `flusso.toml`. So precedence for `--public-address` /
`--private-address` is **flag > `FLUSSO_*` env > `[server]` config > built-in
default** (`127.0.0.1:9464` public read-only, `127.0.0.1:9465` private control).
The Basic-auth credentials (`--admin-user` / `--admin-password`, default **`admin`
/ `flusso`** — change them before exposing the private port) are flag/env only,
never the config file, because they're secrets. The `indexes` / `reindex` client
subcommands reuse those credentials and take `--server` / `FLUSSO_SERVER` to
address a running server's private surface.

## Logging & telemetry

| Variable | Default | Effect |
| --- | --- | --- |
| `RUST_LOG` | `info` | Log verbosity / filtering ([`tracing` env filter](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/filter/struct.EnvFilter.html) syntax, e.g. `flusso=debug,info`). |
| `FLUSSO_LOG_FORMAT` | text | Set to `json` for structured JSON logs (one object per line). |
| `NO_COLOR` | unset | Set to anything to disable colored CLI output (also auto-off when not a TTY). |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | unset | Base OTLP endpoint. Its presence *turns on* trace + metric export. |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | unset | Traces-only endpoint (enables trace export on its own). |
| `OTEL_EXPORTER_OTLP_METRICS_ENDPOINT` | unset | Metrics-only endpoint (enables metric export on its own). |

With no OTLP endpoint set, the exporters aren't installed and cost nothing —
telemetry is opt-in. When an endpoint *is* configured, the rest of the standard
`OTEL_*` variables (`OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`,
`OTEL_EXPORTER_OTLP_PROTOCOL`, …) are honored by the OpenTelemetry SDK.

> Prometheus metrics are a separate, pull-based path: served at `/metrics` on the
> public surface (default `127.0.0.1:9464`), no env var required.

## The derive (compile-time)

`#[derive(FlussoDocument)]` (the [`flusso-query`](CLIENT.md) client) reads
**`FLUSSO_CONFIG` at compile time** to locate `flusso.toml` when it can't be found
by walking up from the crate's `CARGO_MANIFEST_DIR`. Same name as the CLI flag,
consumed by the proc-macro instead of the binary. (You can also point a single
struct at a config with `#[flusso(config = "…")]`.)

## Cheat sheet

```sh
# secrets & connections (resolved at run time)
DATABASE_URL=postgres://user:pass@host:5432/db
PRIMARY_OPENSEARCH_URL=https://opensearch:9200
PRIMARY_OPENSEARCH_USERNAME=flusso
PRIMARY_OPENSEARCH_PASSWORD=…           # plus any names you used in { env = "…" }

# CLI flags (flag wins if both set) — see the table above for the full list
FLUSSO_CONFIG=flusso.toml
FLUSSO_SLOT=flusso
FLUSSO_PUBLICATION=flusso
FLUSSO_PUBLIC_ADDRESS=0.0.0.0:9464      # read-only surface (health/status/metrics)
FLUSSO_PRIVATE_ADDRESS=0.0.0.0:9465     # control surface (indexes/reindex), Basic auth
FLUSSO_ADMIN_USER=admin                 # change these before exposing the private port!
FLUSSO_ADMIN_PASSWORD=change-me
FLUSSO_SKIP_BACKFILL=true

# logging & telemetry
RUST_LOG=flusso=debug,info
FLUSSO_LOG_FORMAT=json
OTEL_EXPORTER_OTLP_ENDPOINT=http://collector:4318
```
