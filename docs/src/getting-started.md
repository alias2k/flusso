# Getting started

## Quickstart

The [`dev/`](https://github.com/alias2k/flusso/tree/main/dev) directory is a complete,
runnable example — a docker-compose stack (Postgres wired for logical replication,
OpenSearch, Dashboards, Prometheus, Grafana), seeded data, and a matching config. With
[`just`](https://just.systems) installed (`cargo install just --locked`):

```sh
just up        # bring the whole stack up and wait for it to be healthy
just check     # validate the config + schemas against the database
just run       # backfill OpenSearch, then follow live changes (serves /status + /metrics)
```

Then, in another terminal, poke it and watch changes stream through:

```sh
just psql                                            # make some changes
curl -s localhost:9200/users/_search?pretty          # see them land in OpenSearch
just status                                          # live pipeline status
```

That's it. Run `just` on its own to see every recipe. The full walk-through —
resetting state, inspecting the slot, OpenSearch Dashboards on `:5601` — lives in the
[`dev/` README](https://github.com/alias2k/flusso/blob/main/dev/README.md).

> No `just`? Every recipe is a thin wrapper; the raw `cargo run -- …` and
> `docker compose …` commands are right there in the
> [`justfile`](https://github.com/alias2k/flusso/blob/main/justfile).

## The CLI

Three subcommands, and **every flag also reads a `FLUSSO_*` env var** (the flag wins
when both are set) — handy for containers.

- **`flusso build`** — compile a config and its schemas into one portable
  `flusso.lock`. No database, no secrets baked in (`{ env = "VAR" }` refs are carried
  through and resolved wherever it runs). Ship one file instead of a tree of YAML.
- **`flusso run`** — stream changes through the engine. Like `cargo run`, it compiles
  first: when a `flusso.toml` is present (the default, or `--config`) it recompiles and
  **rewrites `flusso.lock`**, then runs — so the committed lock stays current for free.
  With no config it loads the existing `flusso.lock`, and `--locked` runs the lock as-is
  without recompiling. Credentials are resolved here, in the running environment.
- **`flusso check`** — validate the config and print the fully-typed mapping, with no
  database. Drop `--offline` and it also confirms the declared types match the live
  database and grumbles about any that don't.

```sh
flusso --help
flusso build  --config flusso.toml -o flusso.lock   # build the portable artifact
flusso check  --config flusso.toml                  # validate (+ check vs database)
flusso check  --config flusso.toml --offline        # validate without a database
flusso run                                          # run the compiled flusso.lock
flusso run    --config flusso.toml                  # compile from source and run
flusso run    --skip-backfill                       # resume live capture only
```

Logging honors `RUST_LOG` (default `info`); `FLUSSO_LOG_FORMAT=json` for structured
logs. Set the standard `OTEL_EXPORTER_OTLP_ENDPOINT` and traces export there too. Every
environment variable flusso reads — secrets, the `FLUSSO_*` flags, telemetry — is
collected in **[Configuring a deployment](guides/configuration.md)**.

## Requirements

flusso doesn't own Postgres or OpenSearch — it's a guest in both. A few things have to
be true *before* it can run. The `dev/` stack sets all of this up for you; here's what
you'd replicate against your own infrastructure. Full per-source/per-sink options are in
**[Configuring a deployment](guides/configuration.md)**.

**Postgres (the source):**

- **PG 14+** with **`wal_level = logical`** (a restart-required setting), and
  `max_wal_senders` / `max_replication_slots` high enough for flusso plus any other
  consumers.
- **A publication** covering *every* table any index reads — root tables *and* every
  table a join or aggregate pulls from. flusso derives that set from your schema and
  **manages the publication for you** when the source role is privileged enough: it
  creates it on first connect and extends it as you add tables, exactly as it does the
  slot. The catch is privilege — creating or extending a publication needs table
  ownership plus `CREATE` on the database (or superuser), a stronger grant than the
  read-only role below. If the role can't, flusso doesn't fail: it logs the exact
  `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` to run, and `flusso check`
  prints the same. Set `[source] manage_publication = false` (or `--no` via
  `FLUSSO_MANAGE_PUBLICATION=false`) to turn management off and manage the publication
  yourself.
- **A replication slot** — this one flusso always creates on first connect (it needs
  only the `REPLICATION` attribute). Heads-up: Postgres hoards WAL until flusso confirms
  it, so a flusso that's down for a long time means WAL piling up on the server. Drop the
  slot when you retire a deployment, unless you're a fan of disk-full pages.
- **Row identity on every replicated table** — a primary key (usual case) or an explicit
  `REPLICA IDENTITY`. Keyless tables can't be addressed, so flusso skips them in backfill
  and errors on a live change it can't key.
- **A role with `REPLICATION` + `SELECT`** on the published tables, via the usual
  `postgres://user:pass@host:port/db` URL. That's enough to stream and to create the
  slot; for flusso to also *manage the publication* (above) the role must own those
  tables and hold `CREATE` on the database — otherwise flusso just prints the SQL for you
  to run with a privileged role.

**OpenSearch (the sink):**

- **OpenSearch 2.x** (also speaks Elasticsearch 7.x on the query side via
  [`flusso-query`](guides/querying.md)).
- **A reachable HTTP(S) endpoint** as the sink `url`. Optional HTTP basic auth
  (`username` / `password`); `tls_verify` defaults to `true` — flip it off only for
  self-signed dev clusters.
- **A user that can manage and write flusso's indexes** plus the small hidden
  `flusso_meta` index where it records "this one's seeded". flusso owns the index
  lifecycle: it derives a strict typed mapping per schema and names each index from a
  hash of that schema (`users_<hash>`), so a structural change rolls onto a fresh index
  and re-seeds instead of fighting a mismatched one. The plain logical name (`users`) is
  kept as an alias on the current index, so you can always query it without knowing the
  hash.

## Deploying it

- **Container image** — the [`Dockerfile`](https://github.com/alias2k/flusso/blob/main/Dockerfile)
  builds a registry-ready, config-less image (you mount a config or bake your own
  `flusso.lock`). It also has a `demo` target with the dev config baked in, which is what
  `just demo` runs. See **[Deploying](guides/deploying.md)** for the recipes.
- **Kubernetes** — the [Helm chart](https://github.com/alias2k/flusso/tree/main/deploy/helm/flusso)
  deploys flusso as a single instance (it consumes one replication slot, so it's *firmly*
  a party of one) with config via ConfigMap, secrets via env, a Service, and an optional
  Prometheus `ServiceMonitor`.
