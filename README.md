# flusso

**Keep OpenSearch in sync with Postgres, driven by declarative config.**

Describe a search document in YAML. flusso derives the index mapping, seeds the index from existing rows, then follows Postgres logical replication so the index stays current. No cron job, no nightly reindex, no hand-rolled sync script.

[![crates.io](https://img.shields.io/crates/v/flusso-cli.svg)](https://crates.io/crates/flusso-cli)
[![docs](https://img.shields.io/badge/manual-alias2k.github.io%2Fflusso-blue)](https://alias2k.github.io/flusso/)

> [!IMPORTANT]
> **Generative AI was used in this project to produce boilerplate and documentation.** Every line of code has been reviewed and revised by a human developer who can be blamed accordingly.

## Why

- **Two files, no code.** A `flusso.toml` names the source, the sinks, and the indexes. A `*.schema.yml` per index says what one document looks like: its table, typed fields, and the related tables folded in as objects, nested arrays, or rollups.
- **Documents, not rows.** Change a user *or one of their orders* and the whole `users` document is rebuilt and re-emitted. flusso works out which documents a changed row touches.
- **The mapping is derived, not guessed.** Every field declares a type that bridges a Postgres column and an OpenSearch mapping, so `flusso check --offline` validates a deployment with no database.
- **At-least-once, zero-downtime.** The replication slot advances only after documents are durable. A reindex builds a fresh generation beside the live one and flips an alias.
- **A typed read side.** `flusso-query` validates your Rust document structs against the same schema at compile time and generates a query surface where a wrong field or operator won't compile.

```yaml
version: 1
table: users
primary_key: id
soft_delete:
  column: deleted
fields:
  - keyword: email
    required: true
    transforms: [lowercase, trim]
  - has_many: orders
    table: orders
    foreign_key: user_id
    primary_key: id
    limit: 5
    fields:
      - decimal: total
        required: true
  - count: orderCount
    table: orders
    foreign_key: user_id
```

## Five-minute quickstart

The `dev/` directory is a complete example: Postgres wired for logical replication, OpenSearch, seed data, and a matching config. With Docker running and [`just`](https://just.systems) installed (`cargo install just --locked`):

```sh
just up        # Postgres + OpenSearch + Dashboards + Prometheus + Grafana
just check     # validate config + schemas against the database
just run       # backfill, then follow live changes
```

Then, in another terminal:

```sh
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
curl -s 'localhost:9200/users/_search?pretty&size=1'
just status
```

The guided version, with what each step did, is the manual's [Quickstart](https://alias2k.github.io/flusso/start/quickstart.html).

## The manual

Everything else lives at **[alias2k.github.io/flusso](https://alias2k.github.io/flusso/)**.

| Part | For |
| --- | --- |
| [Start here](https://alias2k.github.io/flusso/start/how-it-works.html) | how it works, the quickstart |
| [Author](https://alias2k.github.io/flusso/author/first-schema.html) | writing `*.schema.yml`, by hand or in the visual designer |
| [Deploy](https://alias2k.github.io/flusso/deploy/flusso-toml.html) | `flusso.toml`, your own Postgres and OpenSearch, Docker, Helm |
| [Operate](https://alias2k.github.io/flusso/operate/watch-it-run.html) | metrics, OTLP, reindex, rejected documents, a dropped slot, the private surface |
| [Query](https://alias2k.github.io/flusso/query/overview.html) | `flusso-query` and its derives |
| [Reference](https://alias2k.github.io/flusso/reference/config-toml.html) | every key, flag, env var, metric, and endpoint |
| [Contribute](https://alias2k.github.io/flusso/contribute/architecture.html) | architecture, the pipeline, the config layer, testing, releasing |

## Install

```sh
cargo install flusso-cli                # the binary
docker pull alias2k/flusso:latest       # the image (ghcr.io/alias2k/flusso mirrors it)
```

The Helm chart is in [`deploy/helm/flusso/`](deploy/helm/flusso/). Contributors: `just setup` once after cloning enables the repo's git hooks, `just ci` is the local gate, and [`CLAUDE.md`](CLAUDE.md) is the architecture index and house rules.

## License

Licensed under the [Apache License, Version 2.0](LICENSE). Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work shall be licensed as above, without any additional terms or conditions.
