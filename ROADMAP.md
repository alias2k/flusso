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

**One binary, equal HTTP clients.** flusso stays a single `flusso` binary
(`apps/cli`). `flusso run` is the daemon — it owns the pipeline and serves both
HTTP surfaces; `flusso build` / `check` / `schema` remain the config tooling. Two
new subcommands — `flusso indexes` and `flusso reindex --server … <index>` — are
the operator's control tools, but they hold **no privileged in-process channel**:
they make ordinary HTTP calls to the private port, exactly as the website or
`curl` would. Privilege is the Basic-auth credential, not the caller — so the
CLI, the website, and a raw HTTP client are equal peers of one authenticated API.
(That equal-peers, backdoor-free property is what's worth taking from NATS —
`nats-server` + `nats` over one contract — without paying for a second binary:
with `run` in the same binary the CLI links the whole engine/source/sink stack
anyway, so a split would buy no thinner client.) The client subcommands address
the server with a `--server` flag / `FLUSSO_SERVER` env and supply the Basic-auth
credentials the same way (flag or env, never a file). The private API's
request/response types live next to the HTTP handlers and are shared in-process
by the subcommands — one definition, no drift, no extra crate (the website isn't
Rust, so nothing else needs them).

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
