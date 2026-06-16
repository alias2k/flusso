# Roadmap

Work flusso intends to take on, not yet started. Each item states the problem
it solves and a sketch of the approach — neither is a committed API or a fixed
order. For how the system works today, see `CLAUDE.md`; for config and schema
keys, `SCHEMA.md`.

## Resumable backfill

Backfill streams a table through a server-side cursor
(`libs/1-sources/1-postgres/src/cdc/backfill.rs`), but progress is tracked only
as a per-index boolean, flipped to seeded *after* the whole snapshot lands. A
crash midway through a large table therefore restarts it from the first row — and
paired with stop-on-error, a single bad row late in a billion-row table can keep
a run from ever completing.

The fix is to checkpoint a keyset cursor (the last applied primary key per
table) as the snapshot drains, and resume from it instead of restarting the
scope. That same keyset structure also opens the door to **parallel backfill**
across tables and key ranges — today seeding is one cursor feeding one worker, so
large initial loads are strictly serial.

## High availability / failover

A single replication slot ties flusso to a single instance: the Helm chart
pins `replicas: 1` with a `Recreate` strategy, making the process a single point
of failure whose only recovery is a manual restart.

A warm standby with leader election — a Postgres advisory lock or a lease, so
exactly one instance owns the slot at a time — would cut failover from minutes of
manual intervention to seconds, while preserving the one-slot-per-deployment
invariant. This is failover, not sharding; horizontal scale-out is a separate,
larger question.

## Custom document transformations

Documents are shaped by a fixed transform vocabulary declared in `*.schema.yml`
(`schema_index_yaml::Transform`). It covers the common cases, but anything
outside the built-in set — bespoke normalization, derived fields computed from
several columns, enrichment, redaction — isn't expressible.

The plan is a transform stage users can extend through the same plugin model
flusso uses for backends: a registered transform that runs between document
assembly and the sink, addressable from a schema by name. Two delivery options,
likely both:

- **Compiled Rust transforms** registered in the composition root — zero
  overhead, fully trusted, the natural fit for in-tree and first-party logic.
- **WASM transforms** for sandboxed, language-agnostic, hot-loadable logic where
  recompiling the binary isn't acceptable — at the cost of a marshaling boundary
  on the hot path.

Either way the engine gains one well-defined hook (post-build, pre-sink) and the
transform vocabulary stops being closed.

## On-demand reindex over a private control API

Reindexing happens automatically when a schema changes (a new mapping hash
yields a fresh physical index), but there is no way to *trigger* a rebuild of an
unchanged index without restarting the process. After fixing data behind
quarantined documents (`on_error = "skip"`), correcting a source row en masse, or
recovering from an operator mistake, the only recourse today is a restart.

**Two HTTP surfaces, two ports.** The single operational surface
(`apps/cli/src/http.rs`) splits in two, gating by *port* (a physical trust
boundary) rather than by path prefix:

- **Public surface** — read-only, unauthenticated, network-gated exactly as
  today: `/healthz`, `/readyz`, `/status`, `/metrics`. This is what Prometheus
  scrapes. Default port **9464** (unchanged — the scrape config, Helm, and the
  Dockerfile already point there).
- **Private surface** — the mutating control plane: `GET /indexes` (list logical
  indexes with their seeded/physical state) and `POST /reindex?index=…` (rebuild
  one index). Protected by **HTTP Basic auth**. Default port **9465**. Because it
  is a separate port, the routes need no `/admin` prefix.

Expose the public port to the metrics scraper; keep the private port on
localhost / behind a NetworkPolicy / behind TLS. The separate human-facing UI (a
small web app, in the spirit of `dev/search-api`) is just a Basic-auth client of
the private port — sessions, users, and the login live there, never in the
daemon.

**A server and a client, not one binary.** flusso splits into two binaries with
distinct audiences. The **server** (the daemon — today's `flusso run`) is what
runs under Kubernetes / Docker / bare metal: it owns the pipeline and serves both
HTTP surfaces. A separate, lightweight **client CLI** is the operator's tool — it
talks to a *running* server over the private surface (HTTP + Basic auth) to list
indexes and trigger reindexes. The client is a thin HTTP client with **no
privileged channel**: it is an equal peer of the website and of `curl`, holding
no power the private API doesn't grant any caller. (This is the NATS shape —
`nats-server` plus the separately-installed `nats` — minus a custom protocol: the
contract is plain HTTP/JSON, so any client in any language, or a browser, speaks
it.)

To keep the client thin and the contract drift-free, the request/response types
of the private API live in a small **shared crate of pure `serde` types** (the
pattern `daemon::StatusSnapshot` already follows), depended on by both binaries —
so the client need not pull in the engine/source/sink stack just to speak the
protocol. The client addresses the server with a `--server` flag / `FLUSSO_SERVER`
env, supplying the Basic-auth credentials the same way (flag or env, never a
file). Splitting the binary is *orthogonal* to the mechanism below: the server
does all the real work; the client only issues authenticated requests.

**Configuration.** Both ports resolve from, highest precedence first, the **CLI
flag**, then the **`FLUSSO_*` env var**, then the **`flusso.toml` config** —
extending flusso's existing flag-over-env rule with a config layer beneath it.
The Basic-auth credentials resolve from the **CLI flag** and **env var only**,
never the config file: they are secrets, and flusso never bakes a secret into a
compiled `flusso.lock`. The private surface is **fail-closed** — with no
credentials configured it is not mounted at all.

**Mechanism.** A reindex preserves the engine's race-free backfill-*then*-live
ordering by scoping a pipeline restart rather than mutating the live loop: clear
the target index (a new `Sink::unseed` — delete the physical index plus its seed
marker, so the rebuild starts empty and drops documents for rows since deleted in
Postgres), then tear down and rebuild the run. The engine re-backfills only that
index through the existing seed machinery (`is_seeded` / `snapshot` /
`mark_seeded`); other indexes stay seeded and keep serving, and the live changes
that accumulate during the rebuild are retained in the replication slot and
applied when live resumes. So the reindex reuses the tested backfill path end to
end instead of adding a second, concurrent seeding path with its own ordering
hazards.
