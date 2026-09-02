# Quickstart

Run the bundled dev stack, sync three indexes into OpenSearch, and watch a live change stream through, in about ten minutes.

## What you'll end up with

A Postgres seeded with a small e-commerce store, an OpenSearch holding `users`, `products`, and `orders` documents assembled from it, and a running flusso that turns every row change into a rebuilt document. Prometheus and Grafana come up too, already scraping flusso.

## Before you start

- Docker with Compose, running.
- A Rust toolchain (`rustup`), and [`just`](https://just.systems): `cargo install just --locked`.
- `psql`, for the live-change step.
- A clone of the repository:

```sh
git clone https://github.com/alias2k/flusso && cd flusso
```

## Step 1: bring the stack up

```sh
just up
```

Compose starts Postgres (with `wal_level = logical`, the publication, and seed data), OpenSearch, OpenSearch Dashboards, Prometheus, and Grafana, and waits until each reports healthy. `just ps` shows them.

## Step 2: validate the config

```sh
just check
```

This runs `flusso check` against `dev/flusso.toml`. It prints the source, the sinks, and the fully typed mapping for each index, confirms every declared type against the live columns, and reports publication coverage. The last line is `check passed`.

## Step 3: run it

```sh
just run
```

flusso creates the replication slot, backfills the three indexes, then follows live changes. Logs go to stderr. The `audit` stdout sink prints each rebuilt document, so the terminal shows the documents taking shape. Leave it running.

## Step 4: change some rows

In a second terminal, replay the curated change set:

```sh
psql "postgres://postgres:postgres@127.0.0.1:5432/flusso" -f dev/changes.sql
```

It inserts a user, fills in a profile, places an order, edits a line item, cancels an order, adds a review, and finally soft-deletes a user. Each change rebuilds every document it touches: a line-item edit rebuilds both the order's document and its owner's `users` document.

## Step 5: read the result

```sh
curl -s 'localhost:9200/users/_search?pretty&size=1'
just status
```

The first shows a `users` document with its nested `orders` and rollups. The second prints the live status JSON: phase `live`, every index `seeded`, counters, and slot lag.

## What just happened

- **The schema drove the mapping.** `check` printed a typed mapping with no guessing, from the `*.schema.yml` files alone. See [How flusso works](how-it-works.md).
- **Backfill, then follow.** Unseeded indexes were snapshotted; then the WAL took over. Restart `just run` and the backfill is skipped because the sink remembers.
- **Related rows rebuild the parent.** The line-item edit rebuilt a `users` document three levels up. That is reverse resolution through the joins.
- **The soft-delete became a tombstone.** `users.deleted = true` emitted a `delete`, not an upsert, because the schema declares `soft_delete`.

## Next

- Write a schema of your own: [Your first schema](../author/first-schema.md).
- Point flusso at real infrastructure: [Your own Postgres and OpenSearch](../deploy/own-postgres-opensearch.md).
- Open the visual designer on the same stack with `just design`: [Design a schema visually](../author/design-visually.md).
- Grafana is at `localhost:3000` with a flusso dashboard: [Watch it run](../operate/watch-it-run.md).
