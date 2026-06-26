# flusso-cli — the `flusso` binary

The command-line entry point to flusso: compile a deployment, validate it, run the
sync engine, and drive a running server's control surface. Install it with:

```sh
cargo install flusso-cli
```

flusso keeps OpenSearch in sync with Postgres from declarative config. For the full
story — the two config files, the pipeline, deploying — see the
[manual](https://alias2k.github.io/flusso/).

## Subcommands

The first four are local/offline; the last two are thin HTTP clients to a *running*
server's private control surface.

| Command | What it does |
| --- | --- |
| `flusso build` | Compile a `flusso.toml` + its schemas into one portable `flusso.lock`. No database, no secrets baked in (`{ env = "VAR" }` refs are carried through). |
| `flusso run` | Stream Postgres changes through the engine to the configured sink(s). |
| `flusso check` | Validate the config + every schema and print the fully-typed mapping. With a database (drop `--offline`), it also confirms declared types/nullability match the live DB. |
| `flusso schema config\|index` | Print the embedded editor-assist JSON Schema for `flusso.toml` or `*.schema.yml`. |
| `flusso indexes` | List a running server's indexes and their lifecycle state. |
| `flusso reindex <name>` | Trigger a from-scratch rebuild of one index on a running server (reads stay on the old copy until the rebuild swaps in). |

### `flusso run` compiles first, like `cargo run`

When a `flusso.toml` is present (the default path, or `--config`), `run` recompiles it
and **rewrites `flusso.lock`**, then runs that — so the committed lock stays current for
free. With no config it loads the existing `flusso.lock`; `--locked` runs the lock as-is
without recompiling. Connection and credentials are resolved here, in the running
environment. The replication slot is created automatically if it doesn't exist.

```sh
flusso --help
flusso build  --config flusso.toml -o flusso.lock   # build the portable artifact
flusso check  --config flusso.toml                  # validate (+ check vs database)
flusso check  --config flusso.toml --offline        # validate without a database
flusso run                                          # run the compiled flusso.lock
flusso run    --config flusso.toml                  # compile from source and run
flusso run    --skip-backfill                       # resume live capture only
```

## Flags read from the environment too

**Every flag also reads a `FLUSSO_*` env var** (clap's `env` feature; the flag wins when
both are set) — so the binary configures cleanly from a container or Helm chart. For
example `--config`/`FLUSSO_CONFIG`, `--slot`/`FLUSSO_SLOT`,
`--public-address`/`FLUSSO_PUBLIC_ADDRESS`. The full table of flags, their env twins, and
the reserved connection/secret variables (`DATABASE_URL`, `<SINK>_OPENSEARCH_URL`, …) is
in the manual: [Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html).

## HTTP surfaces

`flusso run` serves two HTTP listeners (bound up front, so a bad address fails fast):

- **Public** (`--public-address`, unauthenticated): `/healthz`, `/readyz`, `/status`,
  `/metrics` (Prometheus; OTLP push when `OTEL_EXPORTER_OTLP_*` is set).
- **Private** (`--private-address`, HTTP Basic auth — default `admin`/`flusso` with a
  loud startup warning): `/indexes`, `/reindex`. The `indexes`/`reindex` subcommands are
  clients for this surface.

Logging honors `RUST_LOG` (default `info`); `FLUSSO_LOG_FORMAT=json` for structured logs.

## Where it sits

`flusso-cli` is the composition root of the workspace: it's the one crate that names the
concrete backends (Postgres source, OpenSearch/stdout sinks) and owns transport,
telemetry, and signal handling. The reusable pieces live in the library crates — see the
[repository](https://github.com/alias2k/flusso) and the
[`libs/` overview](https://github.com/alias2k/flusso/tree/main/libs).
