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

## Behind PgBouncer

flusso connects through PgBouncer in session and in transaction mode alike, the replication connection included, once PgBouncer is set up for it:

- **PgBouncer 1.23 or newer.** Older versions refuse the replication connection. flusso then logs `unsupported startup parameter: replication` on each retry and indexes nothing.
- **`ignore_startup_parameters = extra_float_digits`.** flusso's SQL connections send that startup parameter and stock PgBouncer refuses it: `unsupported startup parameter: extra_float_digits`. Many PgBouncer images already set it.
- **`max_prepared_statements` above `0` in transaction mode.** flusso uses prepared statements. With `0` the start fails with `prepared statement "sqlx_s_1" already exists`.

No key depends on a setting a pooled connection could drop. Nor does a `bytea` column a document reads as a value: queries render it in a form no session setting changes. That covers any field type, a domain over `bytea`, [`array: true`](field-types.md#array) over `bytea[]`, over an array of such a domain or over a domain of either, joins, and `ids`, `min` and `max` aggregates.

A `bytea` nested inside another value is different: a field of a composite type, an array of composites, a range, or a multi-dimensional `bytea[]` read by a field that isn't `binary`. So is a column changed to `bytea` while flusso runs, until its table's next change or a restart. Postgres renders it by the session's `bytea_output`. flusso sets `bytea_output = hex` on each connection it opens, and a transaction-mode pool may not keep it. If a document reads such a value, set it on the database or role too: `ALTER ROLE flusso SET bytea_output = 'hex'`.

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
- **The replication connection is named `flusso`.** It shows as `application_name = 'flusso'` in `pg_stat_replication` and `pg_stat_activity`, so its walsender is easy to tell from other consumers'. The SQL connections keep whatever `application_name` the `connection_url` sets; that parameter doesn't rename the replication connection.
- **A column type change is followed without a restart.** After an `ALTER TABLE … ALTER COLUMN … TYPE` on a table an index reads, the next change on that table is built with the column's new type. A document query that fails anyway (its table changed before flusso read that change) reads the types afresh, and the ingest engine's restart builds it.
- **Backfill** snapshots the root tables of unseeded indexes before live capture. `--skip-backfill` skips it. A partitioned root is read through the parent, every partition included.

## Server requirements

| Requirement | Detail |
| --- | --- |
| Postgres 14 or newer | |
| `wal_level = logical` | Restart-required server setting. |
| `max_wal_senders`, `max_replication_slots` | Room for flusso plus any other consumer. |
| Row identity on every replicated table | A single-column primary key (the default `REPLICA IDENTITY` then carries it), or an explicit `REPLICA IDENTITY`. A change is always keyed by the primary key, whatever the replica identity. A table without one works only as a junction; see [Tables without a primary key](#tables-without-a-primary-key). |
| `REPLICA IDENTITY FULL` on every table an index reads | Set by flusso when the role owns the table; see [Replica identity](#replica-identity). Without the parent link in a child table's identity, deleting or re-parenting a child row leaves its old parent's document stale. |
| A role with `REPLICATION` and `SELECT` on the read tables | Enough to stream and create the slot. Publication management needs the stronger grant above. |

> ⚠️ **Warning** — Postgres retains WAL until the slot confirms it. A flusso that stays down for days means WAL piling up on the server. Drop the slot when retiring a deployment.

## Replica identity

flusso wants every table an index reads to log its **whole old row** on update and delete: `REPLICA IDENTITY FULL`. Two things read that old row (the WAL **pre-image**):

- **The primary key**, on every table and every partition: a change is addressed to its document by the key in its row, a delete by the key in its old row. Without it the change is keyed by other columns or by nothing: live writes can land under other ids than the backfill's, and a delete can leave its document in the index. A correctness gap.
- **The parent link**, on a child table: without it, a [deleted or re-parented row](#deleted-and-re-parented-rows) leaves its old parent's document stale. A correctness gap.
- **Every other column**, on any table: without them, an update can't be [compared column by column](#updates-that-change-nothing-a-document-reads), so it rebuilds every document that reads the table even when it changed nothing they read. Correct, only slower.

**flusso sets it itself** when `manage_replica_identity` is on (the default) and the role owns the table, the same grant publication management needs. When the ingest engine starts, before the replication slot is created, `flusso run` issues, per table an index reads that isn't `FULL` yet (root tables, joined and aggregated tables, `belongs_to` targets, junctions, `via` tables; a `DEFAULT` or `NOTHING` identity, and a `USING INDEX` one whose index misses a primary-key column or a parent link, or was dropped):

```sql
ALTER TABLE "public"."order_items" REPLICA IDENTITY FULL;
```

- **The cost: more WAL.** Every update and delete on those tables logs the whole old row instead of the primary key, so the server writes, retains, and ships more WAL, to flusso and to every other consumer of the server's WAL (replicas, backups, other slots). Wide rows and update-heavy tables feel it most, and a large `text`/`jsonb` value stored out of line is logged in full with each old row.
- **Other consumers of the table see it too.** Replica identity is per table, not per publication: another logical-replication consumer (Debezium, a native subscriber) starts receiving whole old rows. A native subscriber whose copy of the table has no primary key or usable index then matches rows by comparing every column, which fails on types with no equality operator (`json`, `point`).
- **A brief lock.** The statement needs an `ACCESS EXCLUSIVE` lock. It waits at most 5 seconds for it (`lock_timeout`), blocking that table's readers while it waits, then gives up rather than sit behind a long transaction. Each table is altered in its own transaction, so a timeout on one doesn't stop the others or the start; it is retried whenever the ingest engine (re)starts.
- **A `USING INDEX` identity whose index includes the whole primary key is kept**, on a child table only when it also includes the parent link (see [Deleted and re-parented rows](#deleted-and-re-parented-rows)). The rule holds for every table, a root or a `belongs_to` target too: on an index without the primary key, changes are keyed by the index's columns, so live writes land under other document ids than the backfill's and a delete removes nothing. flusso sets such a table to `FULL`, as it does one whose identity index was dropped (Postgres then logs nothing) or one with no primary key. flusso takes a kept identity as a deliberate choice, usually to hold down WAL for every consumer of the table, and never alters it. Deleted and re-parented rows still reach their old parent; only the [unread-column skip](#updates-that-change-nothing-a-document-reads) is lost there, so every update to that table rebuilds what reads it. `check` and the start-up log list it as kept, with the statement to run if the skip is worth the WAL to you.
- **Partitioned tables** are checked across the parent and every partition: each partition's own identity decides what its WAL logs, and Postgres doesn't pass the parent's down. A partitioned table *between* the parent and its partitions (a sub-partitioned partition) stores no rows and its changes arrive under the parent, so its identity decides nothing: flusso neither reports nor sets it. An update to one [always rebuilds](#updates-that-change-nothing-a-document-reads), so the rest of the row buys nothing, and a partitioned table whose every member logs the primary key and every parent link is left alone. Otherwise:
  - **A member that doesn't log the whole primary key** is set: a `USING INDEX` identity on an index without it, a dropped identity index, or `NOTHING`. Its changes arrive under the parent's name and are keyed by the parent's primary key, which its old row lacks, so a delete there can't reach its document and leaves it in the index, and an update that changes the key leaves the old document behind. Only those members are set. A partition's `DEFAULT` identity always logs the key: Postgres won't drop a partition's primary-key index. (`NOTHING` and a dropped identity index also make Postgres refuse the partition's updates and deletes once it's published.)
  - **A missing parent link** sets the parent and every partition that isn't `FULL` or a `USING INDEX` identity carrying the primary key and the link (kept, as above).

  Partitions created or attached later keep their own identity until flusso's next start, which checks them again and sets them; until then deleting or re-parenting a row in one leaves its old parent stale, and a delete in one without the primary key leaves its document. The stale parent is **silent**: the stream announces the partitioned table, whose identity already carries the link, so the per-change warning can't see the new partition's. A change without its key is not: the stream warns (see below). Restart flusso after adding partitions (or set `REPLICA IDENTITY FULL` on them as part of creating them).
- **When a table can't be set** (the role doesn't own it, the lock timed out, or management is off), flusso keeps running and says so once per start, at a severity that matches what's lost: a table missing a parent link or part of its primary key (on the table or a partition) is a **warning** with the statement to run, and the stream also warns once per table and kind of change when one arrives without its primary key. Such a change addresses no document of its own, so a document keyed by the row stays until a `flusso reindex` of that index; when the old row still carries the parent link (the partitioned table is `FULL`), the parents it sat in are rebuilt from it as usual. An update keeps rebuilding its row's document; only a primary-key change leaves the old one. A table that only misses the rest of the row is one **info** line listing those tables and their statements. Kept tables get their own **info** line.
- **`flusso check`** reports both groups read-only, apart: per table, what its old row misses (`primary key …`, `parent link …`), whether the next `run` will set it, keeps it as is (and why), or the statement to run by hand. In `--format json`, each entry's `missing_key` and `missing` list those columns, and a kept table's `kept` holds the reason and its `will_manage` is `false`.

**Opting out.** `manage_replica_identity = false` in `[source]` (or `--manage-replica-identity false`, `FLUSSO_MANAGE_REPLICA_IDENTITY=false`) makes flusso alter no table; it only reports. You then own all three: give every table and partition an identity that logs the primary key (`DEFAULT`, `FULL`, or a `USING INDEX` one whose index holds it), or changes there are keyed by other columns or by nothing, and a delete leaves its document in the index; give each child table its parent link (a `FULL` or a `USING INDEX` identity, per the table under [Deleted and re-parented rows](#deleted-and-re-parented-rows)), or deleted and re-parented rows leave stale parents; and every table you leave short of `FULL` rebuilds on every update. Opt out before the first 0.17 start if a table must not be altered (a hot table you can't lock for 5 seconds, a WAL budget you can't grow).

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
| `USING INDEX` on a unique index that includes the primary key and the link (e.g. `(id, parent_id)`) | Yes, with less WAL than `FULL`. flusso keeps it as is (see [Replica identity](#replica-identity)). The index's columns must be `NOT NULL`. |
| `USING INDEX` on an index without the whole primary key (e.g. `UNIQUE (parent_id)`) | No: changes would be keyed by the wrong columns, the index's instead of the primary key, so the old row adds nothing beyond that key. flusso sets `FULL` on any table with such an identity, link or not. |
| `NOTHING` | No. |

flusso sets `FULL` on these tables by default, except where a `USING INDEX` identity already carries the primary key and the link; see [Replica identity](#replica-identity).

- A delete or re-parent that happened *before* the identity was set left its old parent stale; [`flusso reindex`](cli.md#reindex) the index to repair it.

## Tables without a primary key

A table without a primary key is keyed by its replica identity's columns. Under `FULL`, which flusso sets on it, that is the whole row, and a null in a column is just data. Under `DEFAULT` or `NOTHING` it logs no key: Postgres refuses its updates and deletes once it's published, and an insert fails the change stream (`carries no key columns`), so keep [`manage_replica_identity`](#replica-identity) on or set `FULL` yourself. Where such a table can sit:

| Position | Works? |
| --- | --- |
| `many_to_many` junction, aggregate `through` junction | Yes. The parent comes from the row's own `left_key`, so an insert, an update (a null `note` included), a re-parent (both parents), and a delete each rebuild the right documents. A row whose `left_key` is null links no parent. |
| `has_one` / `has_many` target, direct aggregate, `via` table, `belongs_to` target, `many_to_many` far table | No. Every document build needs the table's primary key and fails with `has no primary key`, at backfill or at the first change that reaches the index. `flusso check` reports it as `relation_primary_key`; see [Validation at start](cli.md#validation-at-start) for `run`. |
| Index root | No. Backfill skips the table with a warning, live documents are keyed by the whole row, so an update moves a document to a new id, and a row with a null column addresses no document (warned once per table). A composite primary key is skipped by the backfill too. `flusso check` reports both as `root_primary_key`; see [Validation at start](cli.md#validation-at-start) for `run`. |

The fix in each case is the same: give the table a primary key.

## Key types

A root table's primary key is the document id. It must be of a type flusso reads back as a key; the same types work as any relation's key or link column (`foreign_key`, `belongs_to` `column`, junction keys, `via` hops, `on` pairs).

| Key type | Document id |
| --- | --- |
| `smallint`, `integer`, `bigint` | `42` |
| `numeric` | `1.5`, as stored (`1.50` stays `1.50`, a zero keeps its scale: `0.00`); `NaN`, `Infinity`, `-Infinity` |
| `real`, `double precision` | `1.5`, `-0`; `inf`, `-inf`, `NaN` |
| `boolean` | `true` |
| `text`, `varchar`, `char(n)`, `name`, `citext` | the text; `char(n)` keeps its padding (`ab `) |
| `uuid` | `00000000-0000-0000-0000-00000000000a` |
| `bytea` | `\x01ab`, the Postgres hex form |
| `date` | `2024-02-29`, `+10000-01-01`; `infinity`, `-infinity` |
| `time` | `10:00:00`, `23:59:59.500` |
| `timestamp` | `2024-02-29 10:00:00`, `2024-02-29 10:00:00.500`, `+10000-01-01 10:00:00`; `infinity`, `-infinity` |
| `timestamptz` | `2024-02-29T10:00:00+00:00`: RFC 3339, in UTC; `infinity`, `-infinity` |

- **A domain** counts as its base type: a domain over `integer` is an `integer` key.
- **A BC date** counts years from zero, as ISO 8601 does: 15 March 44 BC is `-0043-03-15`. A year past 9999 carries a `+`.
- **Fractional seconds** have three, six or nine digits, as many as the value needs.
- **Session settings don't change an id.** Queries render each key in a form no setting changes, and the replication connection sets `DateStyle`, `TimeZone`, `bytea_output` and `extra_float_digits` itself before it streams; a server that refuses one fails the stream with an error naming it, and flusso retries with backoff.
- **An id OpenSearch can't take**, such as an empty-string key or one longer than 512 bytes: see [Document ids](sink-opensearch.md#document-ids) on the OpenSearch sink.

### A link spelled differently

A link column can hold a value equal to its parent's key but written differently: `AB` for a `citext` key `Ab`, `1.500` for a `numeric` `1.5`, `-0` for a float `0`, or any text under a nondeterministic collation. The document it rebuilds is still the parent's own, under the parent's stored key: flusso reads that key back from the root table.

That costs one query per change, per index and link column, for a root key whose equal values can be written differently (`numeric`, `real`, `double precision`, `citext`, a nondeterministic collation), and for a link column of another type than the key it names. Every other key costs nothing extra.

### A link of another type

A link column (`foreign_key`, `belongs_to` `column`, a junction key, a `via` hop's `foreign_key`, either side of an `on` pair) may be of another type than the key it names, as long as Postgres compares the two with `=`. `flusso check` asks Postgres, so the rule is exactly that: any pair Postgres compares with `=` works, and a domain counts as its base type. The common cases:

| Key | Links it compares with |
| --- | --- |
| `smallint`, `integer`, `bigint`, `numeric`, `real`, `double precision` | any of these |
| `text`, `varchar`, `char(n)`, `name`, `citext` | any of these |
| `date`, `timestamp`, `timestamptz` | any of these |
| any other type | its own type, and any other Postgres compares it with: `oid` and `regclass` with the integers, `"char"` with the text types, `time`, `timetz` and `interval` with each other, `inet` with `cidr`, `bit` with `bit varying` |

- **Compared as the join compares them.** A change is traced to the documents the document query's join reads: `char(5)` against `text` drops the padding, `integer` against `bigint` compares the numbers.
- **A link no key can equal matches no parent.** A `bigint` link past the `integer` range, a `numeric` `NaN` or fraction against an integer key, an infinite timestamp against a `date` key, text longer than a `char(n)` key: the row joins nothing, and a change to it rebuilds only the documents it names itself. The engine doesn't stop on it, in the backfill or live.
- **A pair Postgres can't compare fails every build.** `text` against `uuid`, `integer` against `text`, `bytea` against anything else, `time` against `timestamp`: the document query can't be planned, whatever the rows hold. `flusso check` reports it as `link_type_mismatch` on the relation's field, and [`run` refuses to start](cli.md#validation-at-start). Change one column's type, or join through a column of the key's type.

### Unsupported key types

Any other type (`inet`, an enum, `money`, `json`, `jsonb`, a range, …) reads back as no key, so no document would ever be written: `flusso check` reports it as `unsupported_key_type` under `primary_key`, and [`run` refuses to start](cli.md#validation-at-start). A relation table keyed by such a type still works as a leaf (a `belongs_to` target, a `has_many` child, a `many_to_many` far table); a change two levels below it can't be traced through it.

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

- `USING INDEX` logs only the index's columns, so every update there rebuilds. flusso [keeps](#replica-identity) such an identity when it carries the parent links.
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
