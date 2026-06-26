# Getting started

Run flusso against the bundled dev stack in three commands, then point it at your own Postgres and OpenSearch.

## Quickstart

The [`dev/`](https://github.com/alias2k/flusso/tree/main/dev) directory is a complete, runnable example — a docker-compose stack (Postgres wired for logical replication, OpenSearch, Dashboards, Prometheus, Grafana), seeded data, and a matching config. With [`just`](https://just.systems) installed (`cargo install just --locked`):

```sh
just up        # bring the whole stack up and wait for it to be healthy
just check     # validate the config + schemas against the database
just run       # backfill OpenSearch, then follow live changes (serves /status + /metrics)
```

Then, in another terminal, make changes and watch them stream through:

```sh
just psql                                            # make some changes
curl -s localhost:9200/users/_search?pretty          # see them land in OpenSearch
just status                                          # live pipeline status
```

Run `just` on its own to see every recipe. The full walk-through — resetting state, inspecting the slot, OpenSearch Dashboards on `:5601` — lives in the [`dev/` README](https://github.com/alias2k/flusso/blob/main/dev/README.md).

> ℹ️ **Info** — No `just`? Every recipe is a thin wrapper; the raw `cargo run -- …` and `docker compose …` commands are in the [`justfile`](https://github.com/alias2k/flusso/blob/main/justfile).

## The CLI

Three subcommands. **Every flag also reads a `FLUSSO_*` env var** (the flag wins when both are set) — handy for containers.

| Command | Does | Database? |
| --- | --- | --- |
| `flusso build` | Compile config + schemas into one portable `flusso.lock`. No secrets baked in (`{ env = "VAR" }` refs carry through). | no |
| `flusso run` | Stream changes through the engine. Like `cargo run`: with a `flusso.toml` present it recompiles + **rewrites `flusso.lock`**, then runs; with no config it loads the existing lock; `--locked` runs the lock as-is. Credentials resolve here, at run time. | yes |
| `flusso check` | Validate and print the fully-typed mapping. `--offline` skips the database; without it, declared types are also confirmed against live columns. | optional |

```sh
flusso --help
flusso build  --config flusso.toml -o flusso.lock   # build the portable artifact
flusso check  --config flusso.toml                  # validate (+ check vs database)
flusso check  --config flusso.toml --offline        # validate without a database
flusso run                                          # run the compiled flusso.lock
flusso run    --config flusso.toml                  # compile from source and run
flusso run    --skip-backfill                       # resume live capture only
```

Logging honors `RUST_LOG` (default `info`); `FLUSSO_LOG_FORMAT=json` for structured logs. Set the standard `OTEL_EXPORTER_OTLP_ENDPOINT` and traces export there too. Every environment variable flusso reads — secrets, the `FLUSSO_*` flags, telemetry — is collected in **[Configuring a deployment](guides/configuration.md)**.

## Requirements

flusso doesn't own Postgres or OpenSearch — it's a guest in both. A few things have to be true *before* it runs. The `dev/` stack sets all of this up; below is what to replicate against your own infrastructure. Full per-source/per-sink options are in **[Configuring a deployment](guides/configuration.md)**.

### Postgres (the source)

| Requirement | Detail |
| --- | --- |
| **PG 14+, `wal_level = logical`** | A restart-required setting. `max_wal_senders` / `max_replication_slots` high enough for flusso plus any other consumers. |
| **A publication** | Covers *every* table any index reads — root tables and every table a join or aggregate pulls from. flusso manages it when the role is privileged enough (see below); otherwise it logs the exact SQL. |
| **A replication slot** | flusso always creates it on first connect (needs only `REPLICATION`). |
| **Row identity** | A primary key (usual case) or an explicit `REPLICA IDENTITY` on every replicated table. Keyless tables are skipped in backfill and error on a live change. |
| **A role with `REPLICATION` + `SELECT`** | Enough to stream and create the slot. Managing the publication needs more — see below. |

> ℹ️ **Info — managing the publication.** flusso derives the table set from the schema and creates/extends the publication itself, exactly as it does the slot — *if* the source role can. Creating or extending a publication needs table ownership plus `CREATE` on the database (or superuser), a stronger grant than the read-only role above. When the role can't, flusso doesn't fail: it logs the exact `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` to run, and `flusso check` prints the same. Set `[source] manage_publication = false` (or `FLUSSO_MANAGE_PUBLICATION=false`) to manage it yourself.

> ⚠️ **Warning** — Postgres retains WAL until flusso confirms it. A flusso that's down for a long time means WAL piling up on the server. Drop the slot when you retire a deployment.

### OpenSearch (the sink)

| Requirement | Detail |
| --- | --- |
| **OpenSearch 2.x** | Also speaks Elasticsearch 7.x on the query side via [`flusso-query`](guides/querying.md). |
| **A reachable HTTP(S) endpoint** | The sink `url`. Optional HTTP Basic auth (`username` / `password`); `tls_verify` defaults to `true` — turn it off only for self-signed dev clusters. |
| **A user that can manage and write flusso's indexes** | Plus the small hidden `flusso_meta` index where seeded state is recorded. |

> 💡 **Did you know** — flusso owns the index lifecycle. It derives a strict typed mapping per schema and names each index from a hash of that schema (`users_<hash>`), so a structural change rolls onto a fresh index and re-seeds instead of fighting a mismatched one. The plain logical name (`users`) is kept as an alias on the current index, queryable without knowing the hash.

## Deploying it

- **Container image** — the [`Dockerfile`](https://github.com/alias2k/flusso/blob/main/Dockerfile) builds a registry-ready, config-less image (mount a config or bake your own `flusso.lock`). Its `demo` target bakes the dev config in, which is what `just demo` runs. See **[Deploying](guides/deploying.md)** for the recipes.
- **Kubernetes** — the [Helm chart](https://github.com/alias2k/flusso/tree/main/deploy/helm/flusso) deploys flusso as a single instance (one replication slot → `replicas: 1`) with config via ConfigMap, secrets via env, a Service, and an optional Prometheus `ServiceMonitor`.
