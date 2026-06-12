# flusso dev task runner.
#
# Requires `just` (https://just.systems):  cargo install just --locked
# Run `just` with no arguments to list every recipe.
#
# Recipes assume the dev stack in docker-compose.yml and the host toolchain.
# Override any variable on the CLI, e.g.  `just config=other.toml check`.

set shell := ["bash", "-c"]

config    := "dev/flusso.toml"
http_addr := "127.0.0.1:9464"
db_url    := "postgres://postgres:postgres@127.0.0.1:5432/flusso"
prom      := "127.0.0.1:9090"

# Show the menu when run with no recipe.
default:
    @just --list

# ── dev stack (Docker) ───────────────────────────────────────────────────────

# Bring up Postgres + OpenSearch + Dashboards + Prometheus + Grafana (waits for healthy).
up:
    docker compose up -d --wait

# Stop the stack, keeping data volumes.
down:
    docker compose down

# Wipe volumes and bring the stack back up fresh (re-seeds Postgres + OpenSearch).
reset:
    docker compose down -v
    docker compose up -d --wait

# Show stack status.
ps:
    docker compose ps

# Follow logs for all services, or one: `just logs prometheus`.
logs *svc:
    docker compose logs -f {{svc}}

# Self-contained demo: flusso running *inside* the cluster (no host toolchain).
demo:
    docker compose -f docker-compose.yml -f docker-compose.demo.yml up --build

# Open the Grafana dashboard.
grafana:
    open http://localhost:3000

# ── flusso CLI (host) ─────────────────────────────────────────────────────────

# Validate config + schemas against the database; prints the typed mapping.
check: up
    cargo run -- check --config {{config}}

# Validate config + schemas without a database.
check-offline:
    cargo run -- check --config {{config}} --offline

# Bring the stack up, then backfill + follow live changes; serves /status + /metrics.
run: up
    cargo run -- run --config {{config}} --http-addr {{http_addr}}

# Bring the stack up, then backfill + follow live changes; serves /status + /metrics.
help:
    cargo run -- help

# Same as `run` but skip the backfill (resume live capture only).
run-live: up
    cargo run -- run --config {{config}} --http-addr {{http_addr}} --skip-backfill

# Serve the dev read API (axum, dev/search-api) over the synced indexes (:8080).
api: up
    cargo run -p flusso-dev-search-api

# Full dev suite: the sync engine + the axum search API together; Ctrl-C stops both.
dev: up
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build -p flusso-cli -p flusso-dev-search-api
    trap 'kill 0 2>/dev/null' INT TERM EXIT
    cargo run -p flusso-dev-search-api &
    cargo run -p flusso-cli -- run --config {{config}} --http-addr {{http_addr}}

# Install the flusso CLI locally (into ~/.cargo/bin).
install:
    cargo install --path apps/cli --locked

# Compile config + schemas into a portable flusso.lock (no DB, no secrets baked in).
build-lock:
    cargo run -- build --config {{config}}

# ── quality (mirrors CI) ───────────────────────────────────────────────────────

# Fast tests: unit + parse/convert, no external deps.
test:
    cargo nextest run --workspace

# Everything incl. the Postgres e2e tests (needs Docker; uses testcontainers).
test-all:
    cargo nextest run --workspace --run-ignored all

# Doctests (nextest does not run these).
doc:
    cargo test --doc --workspace

# Lint — workspace lints are strict; deliberately NOT --all-targets (see CLAUDE.md).
lint:
    cargo clippy --workspace

# Format every crate.
fmt:
    cargo fmt --all

# Check formatting without writing.
fmt-check:
    cargo fmt --all --check

# Full local CI gate: lint → e2e tests → doctests.
ci: lint test-all doc

# ── load & observability ───────────────────────────────────────────────────────

# Production-like load benchmark for N users (default 20000); needs `just run` going too.
bench users="20000": up
    ./scripts/bench-users.sh {{users}}

# Live pipeline status (phase, in-flight, slot lag, counters).
status:
    @curl -s http://{{http_addr}}/status | python3 -m json.tool

# Raw Prometheus metrics exposition.
metrics:
    @curl -s http://{{http_addr}}/metrics

# Backlog drain ETA, from the Prometheus recording rule.
eta:
    @curl -s "http://{{prom}}/api/v1/query?query=flusso:backlog_drain_eta_seconds" | python3 -c 'import sys,json; r=json.load(sys.stdin)["data"]["result"]; print(str(round(float(r[0]["value"][1])/60,1))+" min to drain" if r else "caught up (nothing draining)")'

# Open a psql shell on the dev database.
psql:
    psql "{{db_url}}"
