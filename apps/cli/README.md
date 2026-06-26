# flusso-cli — the `flusso` binary

The `flusso` binary: compile a deployment, validate it, run the sync engine, drive a
running server's control surface.

```sh
cargo install flusso-cli
```

flusso keeps OpenSearch in sync with Postgres from declarative config. For the full
story — the two config files, the pipeline, deploying — see the
[manual](https://alias2k.github.io/flusso/).

## Subcommands

The first four run locally/offline; the last two are HTTP clients to a *running*
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

When a `flusso.toml` is present (the default path, or `--config`), `run` recompiles it and
**rewrites `flusso.lock`**, then runs that — the committed lock stays current for free.
With no config it loads the existing `flusso.lock`; `--locked` runs the lock as-is, no
recompile. Connection and credentials resolve here, in the running environment. The
replication slot is created automatically if it's missing.

Common `run` flags:

| Flag | Env | Default | Effect |
| --- | --- | --- | --- |
| `--config` / `-c` | `FLUSSO_CONFIG` | `flusso.toml` | Source config; presence triggers a recompile. |
| `--lock` | `FLUSSO_LOCK` | `flusso.lock` | Compiled artifact path (rewritten each start). |
| `--locked` | `FLUSSO_LOCKED` | off | Run the lock as-is, skip recompile. |
| `--slot` | `FLUSSO_SLOT` | `flusso` | Replication slot name. |
| `--publication` | `FLUSSO_PUBLICATION` | `flusso` | Publication name. |
| `--skip-backfill` | `FLUSSO_SKIP_BACKFILL` | off | Resume live capture only. |
| `--public-address` | `FLUSSO_PUBLIC_ADDRESS` | — | Bind the public HTTP surface. |
| `--private-address` | `FLUSSO_PRIVATE_ADDRESS` | — | Bind the private control surface. |
| `--index-prefix` | `FLUSSO_INDEX_PREFIX` | — | Prefix every owned index name. |

The full flag set lives in
[Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html).

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
both are set) — so the binary configures cleanly from a container or Helm chart. This is
separate from the config's reserved connection/secret variables (`DATABASE_URL`,
`<SINK>_OPENSEARCH_URL`, …), all centralized in
[Configuring a deployment](https://alias2k.github.io/flusso/guides/configuration.html).

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
