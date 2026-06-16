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
compiled `flusso.lock`. They **default to `admin` / `flusso`** so the private
surface works out of the box, and flusso logs a prominent warning on every start
while the password is still the default — so an unconfigured deployment is usable
but never *quietly* insecure. Because those defaults are public, changing them is
mandatory for any network-reachable deployment, and the private port should stay
gated (localhost / NetworkPolicy / TLS) regardless.

**Why reindex (an *unchanged* schema) at all** — a schema *change* already
re-indexes itself, since a new hash yields a fresh physical index. The reasons to
trigger a rebuild without a schema change are all recovery/repair, and they shape
the mechanism:

1. **Drift repair** — the index diverged from Postgres (a bug, a missed change, a
   partial flush, a hand-edit) and you want to guarantee index == current Postgres.
2. **Slot / WAL-gap recovery** — the slot was dropped, flusso was down past WAL
   retention, or a failover left an unknown amount of staleness (ties to the HA
   and resumable-backfill items above).
3. **Quarantine recovery** — `on_error = "skip"` dropped poison documents; after
   fixing the data or mapping you want the missing ones back.
4. **Out-of-band change** — an en-masse correction that bypassed the WAL
   (`pg_restore`, pre-slot data, a replica promotion), or an index someone
   dropped/truncated.
5. **Build-logic change the hash doesn't cover** — a custom transform/enrichment
   changed in *code*, not schema, so the hash didn't move but documents need
   rebuilding.

Three requirements fall out: it must be a **true rebuild that removes orphaned
documents** (an upsert-only reseed leaves stale docs for since-deleted rows —
insufficient for #1/#2/#4); the index is usually **serving reads** while it runs,
so **read downtime must be zero**; and it is **rare and operator-initiated**, so
it must impose **no permanent cost on normal operation**.

**Mechanism — alias indirection + a fresh generation, swapped atomically.** The
config-derived `{logical}_{hash}` stops being a concrete index and becomes a
stable **alias** (still computed from config alone, so the `flusso-query` client
keeps resolving it at compile time with no DB). Behind it sits a concrete,
swappable generation `{logical}_{hash}_{gen}`. A reindex:

1. Creates a new empty generation `{logical}_{hash}_{gen+1}` and seeds it through
   the existing race-free backfill path (`snapshot` → build → sink), addressing it
   directly; reads and writes keep flowing to the current generation the whole
   time, so **reads never go dark**.
2. On completion, repoints `{logical}_{hash}` (and the convenience `{logical}`
   alias) from the old generation to the new in one atomic `_aliases` call, then
   drops the old generation.

Because the new generation is built from empty, orphaned documents are gone — a
true rebuild. `flusso-query` is unaffected: reading through an alias is
transparent. Two known wrinkles to handle: an alias and an index **cannot share a
name**, so existing deployments need a one-time migration (the concrete
`{logical}_{hash}` is reindexed into its first generation and the name freed for
the alias); and that index's *writes* lag briefly while its generation seeds
(reads do not) — acceptable, because the operation is rare and reads stay live.

**Deferred — true write-side zero-downtime.** Eliminating even the brief write
lag would mean dual-writing live changes to both generations during the seed,
which reintroduces a snapshot-vs-live ordering race; the clean fix is OpenSearch
**external versioning** (a per-write version = WAL LSN, so a stale snapshot write
loses to a newer live write). That is deliberately **not** in scope: it taxes
*every* write, forever, on the hot path to make a *rare* operation lag-free —
which none of the reasons above justify. It stays a documented follow-on, purely
additive on top of the alias scheme.
