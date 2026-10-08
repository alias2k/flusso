# Source: Postgres

The `[source]` table with `type = "postgres"`: how flusso connects, how it secures the connection, and what the server must provide.

{{#include generated/source-postgres.md}}

## Connection

`connection_url` takes one of three shapes.

**A URL string**, matching `^(postgresql|postgres)://`:

```toml
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

**An environment reference**, read where the pipeline runs:

```toml
connection_url = { env = "PG_URL" }
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

Whichever shape is written, the override variable `SOURCE_POSTGRES_CONNECTION_URL` replaces it when set, and supplies it when `connection_url` is omitted. A parts table's `password` takes `SOURCE_POSTGRES_CONNECTION_URL_PASSWORD`. Precedence and the full rule set live in [Environment variables](environment.md#config-values).

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

flusso consumes a logical replication **slot** and subscribes to a **publication**. Both names are the `slot` and `publication` keys above; the `--slot` and `--publication` flags of [`run`](cli.md#run) override them.

- **The slot is created automatically** when missing; that needs only the `REPLICATION` attribute. A slot that had to be created has no memory of earlier changes, which is why a missing slot triggers a rebuild of every seeded index. See [Recover from a dropped slot](../operate/dropped-slot.md).
- **The publication is managed automatically** when `manage_publication` is on and the role can: flusso derives the full table set from the schemas (root tables plus every joined or aggregated table) and creates or extends it. Creating or extending a publication needs ownership of those tables plus `CREATE` on the database, or superuser. When the role can't, flusso logs the exact `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` statements and keeps running; `flusso check` prints the same coverage report.
- **Child tables' replica identity is managed automatically** when `manage_replica_identity` is on and the role owns them. See [Deleted and re-parented rows](#deleted-and-re-parented-rows).
- **Idle tables don't pin WAL.** A running flusso advances the slot from server keepalives even while the watched tables are quiet, so writes to unrelated tables aren't retained on its behalf.
- **Backfill** snapshots the root tables of unseeded indexes before live capture. `--skip-backfill` skips it.

## Server requirements

| Requirement | Detail |
| --- | --- |
| Postgres 14 or newer | |
| `wal_level = logical` | Restart-required server setting. |
| `max_wal_senders`, `max_replication_slots` | Room for flusso plus any other consumer. |
| Row identity on every replicated table | A single-column primary key (the default `REPLICA IDENTITY` then carries it), or an explicit `REPLICA IDENTITY`. A keyless table is skipped in backfill and errors on a live change. A change is always keyed by the primary key, whatever the replica identity. |
| The parent link in every child table's replica identity | Set by flusso when the role owns the table; see [Deleted and re-parented rows](#deleted-and-re-parented-rows). Without it, deleting or re-parenting a child row leaves its old parent's document stale. |
| A role with `REPLICATION` and `SELECT` on the read tables | Enough to stream and create the slot. Publication management needs the stronger grant above. |

> ⚠️ **Warning** — Postgres retains WAL until the slot confirms it. A flusso that stays down for days means WAL piling up on the server. Drop the slot when retiring a deployment.

## Deleted and re-parented rows

Documents are rebuilt from the current rows. A child row that holds its parent's key is the exception: once it's deleted, or moved to another parent, the current table no longer says which parent it left. flusso reads that from the WAL **pre-image**, the old row Postgres logs for an update or delete, so the child table's `REPLICA IDENTITY` has to carry the link column.

| Table | Link column |
| --- | --- |
| `has_one` / `has_many` target, direct aggregate | its `foreign_key` |
| `many_to_many` junction | its `left_key` (already carried when it's part of the junction's primary key) |

`belongs_to` targets and the far side of a `many_to_many` need nothing: the rows that point at them are still there.

| `REPLICA IDENTITY` | Carries the link? |
| --- | --- |
| `DEFAULT` | Only when the link is in the primary key. |
| `FULL` | Yes. Logs the whole old row, so the table's WAL grows. |
| `USING INDEX` on a unique index that includes the primary key and the link (e.g. `(id, parent_id)`) | Yes, with less WAL than `FULL`. The index's columns must be `NOT NULL`. |
| `USING INDEX` on an index without the primary key (e.g. `UNIQUE (parent_id)`) | No: changes are then keyed by the index columns, so the old row adds nothing beyond its key. |
| `NOTHING` | No. |

**flusso sets it itself** when `manage_replica_identity` is on (the default) and the role owns the table, the same grant publication management needs. When the ingest engine starts, before the replication slot is created, `flusso run` issues, per child table missing its link:

```sql
ALTER TABLE "public"."order_items" REPLICA IDENTITY FULL;
```

- The statement needs an `ACCESS EXCLUSIVE` lock. It waits at most 5 seconds for it (`lock_timeout`), blocking that table's readers while it waits, then gives up rather than sit behind a long transaction. A timeout is retried whenever the ingest engine (re)starts.
- A partitioned table is reported but never altered: its partitions stream under their own names, so the parent's identity doesn't help.
- The table then logs whole old rows on update and delete, so its WAL grows.
- When the role can't (not the owner), or `manage_replica_identity = false`, flusso logs the statement and keeps running. It also warns once per table when such a delete or update arrives.
- `flusso check` reports each child table read-only: whether the next `run` will set it, or the statement to run by hand.
- A delete or re-parent that happened *before* the identity was set left its old parent stale; [`flusso reindex`](cli.md#reindex) the index to repair it.

## Example

```toml
[source]
type = "postgres"
connection_url = { env = "PG_URL" }
manage_publication = false
slot = "search"
ssl_mode = "verify-full"
ssl_root_cert = "/etc/ssl/rds-ca.pem"
```
