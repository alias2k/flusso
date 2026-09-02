# Source: Postgres

The `[source]` table with `type = "postgres"`: how flusso connects, how it secures the connection, and what the server must provide.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `type` | `"postgres"` | — | Required. |
| `connection_url` | URL string, `{ env }`, or parts table | — | Required (or supplied by `DATABASE_URL`). See [Connection](#connection). |
| `manage_publication` | bool | `true` | Let flusso create or extend the publication when the role can. `false` reports gaps and never issues DDL. See [Capture](#capture). |
| `ssl_mode` | `disable` \| `prefer` \| `require` \| `verify-ca` \| `verify-full` | URL's `sslmode`, else `prefer` | TLS mode, libpq semantics. See [TLS](#tls). |
| `ssl_root_cert` | path | bundled Mozilla roots | CA bundle PEM for the `verify-*` modes. |
| `ssl_cert` | path | — | Client certificate PEM for mutual TLS. Pairs with `ssl_key`. |
| `ssl_key` | path | — | Client key PEM for mutual TLS. Pairs with `ssl_cert`. |
| `ssl_sni_hostname` | string | connection host | SNI name sent in the handshake. Replication stream only. |

## Connection

`connection_url` takes one of three shapes.

**A URL string**, matching `^(postgresql|postgres)://`:

```toml
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

**An environment reference**, read where the pipeline runs:

```toml
connection_url = { env = "DATABASE_URL" }
```

**Individual parts.** `database` is required; the rest default.

| Part | Type | Default |
| --- | --- | --- |
| `host` | string | `127.0.0.1` |
| `port` | 1–65535 | `5432` |
| `user` | string | `postgres` |
| `password` | string or `{ env }` | none |
| `database` | string | — |

```toml
[source.connection_url]
host = "db.internal"
database = "app"
password = { env = "PGPASSWORD" }
```

Whichever shape is written, the reserved variable `DATABASE_URL` overrides it when set. Precedence and the full rule set live in [Environment variables](environment.md#config-values).

## TLS

TLS settings come from two surfaces, merged: the URL's libpq parameters (`sslmode`, `sslrootcert`, `sslcert`, `sslkey`) and the flat `ssl_*` keys above. **A config key overrides its URL parameter.** With neither, the mode is `prefer`.

| `ssl_mode` | Encrypted | Certificate checked | Hostname checked |
| --- | --- | --- | --- |
| `disable` | no | — | — |
| `prefer` | if the server offers it | no | no |
| `require` | yes | **no** | **no** |
| `verify-ca` | yes | yes | no |
| `verify-full` | yes | yes | yes |

> ⚠️ **Warning** — `require` encrypts but verifies nothing: a self-signed certificate from an attacker is accepted. That is the standard libpq meaning. Use `verify-full` in production, with `ssl_root_cert` when the CA isn't in the bundled Mozilla roots.

- **One decision, both connection kinds.** flusso opens a replication stream *and* ordinary SQL connections from one `connection_url`. The merged settings drive both, so a mode can't apply to half the traffic.
- **`sslmode=allow`** isn't modeled; it's treated as `prefer`.
- **Mutual TLS** needs both `ssl_cert` and `ssl_key` (or both URL parameters). One without the other is a config error.
- **`ssl_sni_hostname`** is for connecting by IP or through a load balancer while the certificate names the real host. `verify-full` to an IP address requires it. It has no URL parameter and applies to the replication stream only.

## Capture

flusso consumes a logical replication **slot** and subscribes to a **publication**. Both names are CLI flags, `--slot` and `--publication`, defaulting to `flusso`.

- **The slot is created automatically** when missing; that needs only the `REPLICATION` attribute. A slot that had to be created has no memory of earlier changes, which is why a missing slot triggers a rebuild of every seeded index. See [Recover from a dropped slot](../operate/dropped-slot.md).
- **The publication is managed automatically** when `manage_publication` is on and the role can: flusso derives the full table set from the schemas (root tables plus every joined or aggregated table) and creates or extends it. Creating or extending a publication needs ownership of those tables plus `CREATE` on the database, or superuser. When the role can't, flusso logs the exact `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` statements and keeps running; `flusso check` prints the same coverage report.
- **Idle tables don't pin WAL.** A running flusso advances the slot from server keepalives even while the watched tables are quiet, so writes to unrelated tables aren't retained on its behalf.
- **Backfill** snapshots the root tables of unseeded indexes before live capture. `--skip-backfill` skips it.

## Server requirements

| Requirement | Detail |
| --- | --- |
| Postgres 14 or newer | |
| `wal_level = logical` | Restart-required server setting. |
| `max_wal_senders`, `max_replication_slots` | Room for flusso plus any other consumer. |
| Row identity on every replicated table | A single-column primary key (the default `REPLICA IDENTITY` then carries it), or an explicit `REPLICA IDENTITY`. A keyless table is skipped in backfill and errors on a live change. `REPLICA IDENTITY FULL` is not needed: documents are rebuilt from the current row, not from the WAL image. |
| A role with `REPLICATION` and `SELECT` on the read tables | Enough to stream and create the slot. Publication management needs the stronger grant above. |

> ⚠️ **Warning** — Postgres retains WAL until the slot confirms it. A flusso that stays down for days means WAL piling up on the server. Drop the slot when retiring a deployment.

## Example

```toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }
manage_publication = false
ssl_mode = "verify-full"
ssl_root_cert = "/etc/ssl/rds-ca.pem"
```
