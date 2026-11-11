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
- **The publication is managed automatically** when `manage_publication` is on and the role can: flusso derives the full table set from the schemas (root tables plus every joined or aggregated table, junctions and `via` tables included) and creates or extends it. Creating or extending a publication needs ownership of those tables plus `CREATE` on the database, or superuser. When the role can't, flusso logs the exact `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` statements and keeps running; `flusso check` prints the same coverage report.
- **Partitioned tables are published via their root.** By default Postgres streams a partitioned table's changes under each partition's name, which no index reads. A publication flusso creates sets `publish_via_partition_root = true`. When an index reads a partitioned table and an existing publication lacks the option, flusso sets it under the same rule as coverage (needs ownership of the publication), or logs `ALTER PUBLICATION … SET (publish_via_partition_root = true);`; `flusso check` lists such tables. A change that arrives for a table no index reads is dropped with a warning, once per table. Changes a partitioned table took before the option was set were dropped this way; [`flusso reindex`](cli.md#reindex) its indexes to repair them.
  - The option applies to the whole publication. Once set, a partition's changes arrive under its topmost published ancestor's name, so an index that reads a partition directly while its partitioned table is also in the publication sees none of them (flusso warns when both are read), and so does any other subscriber of the same publication that expects partition names.
- **Every read table's replica identity is set to `FULL` automatically** when `manage_replica_identity` is on and the role owns it. See [Replica identity](#replica-identity).
- **Idle tables don't pin WAL.** A running flusso advances the slot from server keepalives even while the watched tables are quiet, so writes to unrelated tables aren't retained on its behalf.
- **Retained WAL trails by seconds, not by change volume.** Two clocks decide it:
  - flusso reports its position about a second after every sink has flushed a change, so the slot's `confirmed_flush_lsn` passes the change then. The interval is fixed.
  - Postgres can recycle that WAL (the slot's `restart_lsn` moves) once it has logged its next periodic snapshot record, typically within about 15 seconds.

  A slow or stalled sink still holds the slot back: the confirmed position is the slowest sink's.
- **Backfill** snapshots the root tables of unseeded indexes before live capture. `--skip-backfill` skips it. A partitioned root is read through the parent, every partition included.

## Server requirements

| Requirement | Detail |
| --- | --- |
| Postgres 14 or newer | |
| `wal_level = logical` | Restart-required server setting. |
| `max_wal_senders`, `max_replication_slots` | Room for flusso plus any other consumer. |
| Row identity on every replicated table | A single-column primary key (the default `REPLICA IDENTITY` then carries it), or an explicit `REPLICA IDENTITY`. A keyless table is skipped in backfill and errors on a live change. A change is always keyed by the primary key, whatever the replica identity. |
| `REPLICA IDENTITY FULL` on every table an index reads | Set by flusso when the role owns the table; see [Replica identity](#replica-identity). Without the parent link in a child table's identity, deleting or re-parenting a child row leaves its old parent's document stale. |
| A role with `REPLICATION` and `SELECT` on the read tables | Enough to stream and create the slot. Publication management needs the stronger grant above. |

> ⚠️ **Warning** — Postgres retains WAL until the slot confirms it. A flusso that stays down for days means WAL piling up on the server. Drop the slot when retiring a deployment.

## Replica identity

flusso wants every table an index reads to log its **whole old row** on update and delete: `REPLICA IDENTITY FULL`. Two things read that old row (the WAL **pre-image**):

- **The parent link**, on a child table: without it, a [deleted or re-parented row](#deleted-and-re-parented-rows) leaves its old parent's document stale. A correctness gap.
- **Every other column**, on any table: without them, an update can't be [compared column by column](#updates-that-change-nothing-a-document-reads), so it rebuilds every document that reads the table even when it changed nothing they read. Correct, only slower.

**flusso sets it itself** when `manage_replica_identity` is on (the default) and the role owns the table, the same grant publication management needs. When the ingest engine starts, before the replication slot is created, `flusso run` issues, per table an index reads that isn't `FULL` yet (root tables, joined and aggregated tables, `belongs_to` targets, junctions, `via` tables; a `DEFAULT`, `NOTHING`, or `USING INDEX` identity included, also one set on purpose):

```sql
ALTER TABLE "public"."order_items" REPLICA IDENTITY FULL;
```

- **The cost: more WAL.** Every update and delete on those tables logs the whole old row instead of the primary key, so the server writes, retains, and ships more WAL, to flusso and to every other consumer of the server's WAL (replicas, backups, other slots). Wide rows and update-heavy tables feel it most, and a large `text`/`jsonb` value stored out of line is logged in full with each old row.
- **Other consumers of the table see it too.** Replica identity is per table, not per publication: another logical-replication consumer (Debezium, a native subscriber) starts receiving whole old rows. A native subscriber whose copy of the table has no primary key or usable index then matches rows by comparing every column, which fails on types with no equality operator (`json`, `point`).
- **A brief lock.** The statement needs an `ACCESS EXCLUSIVE` lock. It waits at most 5 seconds for it (`lock_timeout`), blocking that table's readers while it waits, then gives up rather than sit behind a long transaction. Each table is altered in its own transaction, so a timeout on one doesn't stop the others or the start; it is retried whenever the ingest engine (re)starts.
- **Partitioned tables** are set only when they hold a parent link (an update to one [always rebuilds](#updates-that-change-nothing-a-document-reads), so the rest of the row buys nothing), on the parent and on every partition that isn't `FULL`: each partition's own identity decides what its WAL logs, and Postgres doesn't pass the parent's down. That includes partitions created or attached later: flusso sets them at its next start, and until then deleting or re-parenting a row in one leaves its old parent stale. That staleness is **silent**: the stream announces the partitioned table, whose identity already carries the link, so the per-change warning can't see the new partition's. Restart flusso after adding partitions (or set `REPLICA IDENTITY FULL` on them as part of creating them).
- **When a table can't be set** (the role doesn't own it, the lock timed out, or management is off), flusso keeps running and says so once per start, at a severity that matches what's lost: a table missing a parent link is a **warning** with the statement to run, and the stream also warns once per table when such a delete or update arrives; a table that only misses the rest of the row is one **info** line listing those tables and their statements.
- **`flusso check`** reports both groups read-only, apart: per table, whether the next `run` will set it, or the statement to run by hand.

**Opting out.** `manage_replica_identity = false` in `[source]` (or `--manage-replica-identity false`, `FLUSSO_MANAGE_REPLICA_IDENTITY=false`) makes flusso alter no table; it only reports. You then own both: give each child table its parent link (a `FULL` or a `USING INDEX` identity, per the table under [Deleted and re-parented rows](#deleted-and-re-parented-rows)), or deleted and re-parented rows leave stale parents; and every table you leave short of `FULL` rebuilds on every update. Opt out before the first 0.17 start if a table must not be altered (a hot table you can't lock for 5 seconds, a WAL budget you can't grow).

## Deleted and re-parented rows

Documents are rebuilt from the current rows. A child row that holds its parent's key is the exception: once it's deleted, or moved to another parent, the current table no longer says which parent it left. flusso reads that from the WAL **pre-image**, the old row Postgres logs for an update or delete, so the child table's `REPLICA IDENTITY` has to carry the link column.

| Table | Link column |
| --- | --- |
| `has_one` / `has_many` target, direct aggregate | its `foreign_key` |
| aggregate [`via`](aggregates.md#across-several-hops) table | its `foreign_key` |
| `many_to_many` junction | its `left_key` (already carried when it's part of the junction's primary key) |

`belongs_to` targets and the far side of a `many_to_many` need nothing: the rows that point at them are still there. The `related` columns of an [`on`](joins.md#on) pair need nothing either: a row that only changes them keeps its key, so its parent is still found from the current row.

| `REPLICA IDENTITY` | Carries the link? |
| --- | --- |
| `DEFAULT` | Only when the link is in the primary key. |
| `FULL` | Yes. What flusso sets; see [Replica identity](#replica-identity). |
| `USING INDEX` on a unique index that includes the primary key and the link (e.g. `(id, parent_id)`) | Yes, with less WAL than `FULL`, but only with `manage_replica_identity = false`: otherwise flusso replaces it with `FULL` at the next start. The index's columns must be `NOT NULL`. |
| `USING INDEX` on an index without the primary key (e.g. `UNIQUE (parent_id)`) | No: changes are then keyed by the index columns, so the old row adds nothing beyond its key. |
| `NOTHING` | No. |

flusso sets `FULL` on these tables by default; see [Replica identity](#replica-identity).

- A delete or re-parent that happened *before* the identity was set left its old parent stale; [`flusso reindex`](cli.md#reindex) the index to repair it.

## Updates that change nothing a document reads

An update rebuilds only the documents that read a column it changed. Bumping `last_seen_at` on a row that every document embeds, when no schema reads `last_seen_at`, writes nothing to any sink.

A column counts as read by a part of the document when it is:

| Read through | Columns |
| --- | --- |
| The root row's own document | its fields, the index `filters`, the `soft_delete` column and its `when` |
| A join | its fields, `filters`, `order_by`, the link (`foreign_key`), and the `related` side of its [`on`](joins.md#on) pairs |
| An aggregate | its input column (`sum`/`avg`/`min`/`max`), `filters`, the link, and the `related` side of its `on` pairs |
| An aggregate's `via` hop | the hop's `foreign_key`, `filters`, the `related` side of its `on` pairs, and the `local` side of the next hop's (or the aggregate's own, after the last hop) |
| A `many_to_many` junction | `left_key`, `right_key`, and `through.filters` |
| Any of them | the table's primary key, a nested `belongs_to`'s `column`, and the `local` side of the `on` pairs of the relations nested in it (the root row too) |

- A column reached only through `filters` or `order_by` counts: changing it can move the row in or out of the document.
- A `raw` filter can read any column of any table its SQL can name (the root row as `root`, a junction, an enclosing level), so an index with a `raw` filter anywhere (root `filters`, `soft_delete.when`, any join's or aggregate's `filters`, `through.filters`, a `via` hop's `filters`) rebuilds on every update, as it always has.
- Each part is checked on its own. Changing a field the root projects rebuilds the row's own document but not the parents that embed it, unless they read it too.
- Inserts and deletes always rebuild.

The comparison needs the update's old row, so it applies only to tables under `REPLICA IDENTITY FULL`, which flusso sets on every table an index reads by default (see [Replica identity](#replica-identity)). On a table left short of it, every update rebuilds.

- `USING INDEX` logs only the index's columns, so an update there rebuilds unless every read column is in the index.
- A large value Postgres keeps out of line (TOAST) and the update didn't touch counts as unchanged.
- Values are compared as values, not as text: a `numeric` whose scale alone changes (`1.0` → `1.00`) rebuilds nothing, so the document keeps the old scale until something it reads changes.
- A join with a `limit` but no `order_by` that orders its rows totally picks rows in physical order. After a skipped update the document keeps its selection, where a rebuild might have picked different rows. Give a limited join a total `order_by` (end it with a unique column).
- An update to a partitioned table always rebuilds, whatever you set. `ALTER TABLE … REPLICA IDENTITY FULL` on a partitioned parent doesn't reach its partitions, each partition logs its old rows by its own identity, and the stream can't show which identity logged a row, so flusso doesn't compare them. There is nothing to set for this; the partitions flusso sets still serve [deleted and re-parented rows](#deleted-and-re-parented-rows).

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
