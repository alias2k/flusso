---
name: flusso-postgres
description: How flusso's Postgres source side works — logical replication, the replication slot, the publication (manage_publication), wal_level, REPLICA IDENTITY / row identity, how relational structure (tables, foreign keys, junctions) maps to flusso joins and aggregates, and the privileges the source role needs. Use to understand or debug what flusso requires of the database, why a change isn't captured, or why a join needs a publication. Covers the flusso-relevant slice of Postgres, not the whole manual.
---

# flusso's Postgres source (how the read-from side works)

flusso is a **logical-replication consumer**. It snapshots tables to seed an index, then follows the Postgres write-ahead log (WAL) so the index stays current. This skill explains what flusso needs from Postgres and why — for the *schema* side (how tables become a document) see **flusso-schema**; for the sink side see **flusso-opensearch**.

Postgres is the *substrate*. This covers only the slice flusso touches; for the mechanics themselves, the [PostgreSQL logical replication docs](https://www.postgresql.org/docs/current/logical-replication.html) are the authority.

## The three things Postgres must provide

| Requirement | Why | If missing |
| --- | --- | --- |
| `wal_level = logical` | Logical decoding can't run without it. Server-wide setting, **needs a restart**. | flusso can't start capture. |
| A **replication slot** | The server's bookmark of how far flusso has consumed; it gates WAL retention. | flusso **creates it automatically** on first connect (needs only the `REPLICATION` role attribute). |
| **Row identity** on every replicated table — a primary key (usual) or an explicit `REPLICA IDENTITY` | A change has to be addressable to a row so flusso can rebuild the right document. | A keyless table is **skipped in backfill** and a live change against it **errors** ("relation … carries no key columns"). |

## The replication slot — the one operational gotcha

flusso always creates the slot on first connect (default name `flusso`, override `--slot` / `FLUSSO_SLOT`). The catch lives in how slots work:

> **Postgres retains WAL until the slot's consumer confirms it.** A flusso that is down for a long time means WAL piling up on the server — eventually a disk-full outage.

So: **one slot → one running instance** (this is why the Helm chart pins `replicas: 1`). And **drop the slot when you retire a deployment** (`SELECT pg_drop_replication_slot('flusso');`), or it leaks WAL forever.

## The publication — which tables stream

A publication is the server-side allowlist of tables whose changes are decoded. flusso derives the **full table set from your schema** — every root table plus every table a join or aggregate reads — and, by default, manages the publication for you (default name `flusso`, override `--publication`):

- `manage_publication = true` (the default): on `run`, flusso **auto-creates or extends** the publication to cover that table set — *if* the source role can (it must own those tables and hold `CREATE` on the database, or be superuser).
- When the role **can't**, flusso doesn't fail — it logs the exact `CREATE PUBLICATION` / `ALTER PUBLICATION … ADD TABLE` SQL for you to run with a privileged role, and carries on.
- `flusso check` prints the same coverage report read-only (no writes), so you see the gap before running.
- Opt out with `manage_publication = false` (or `FLUSSO_MANAGE_PUBLICATION=false` / `--manage-publication false`) to own the publication yourself:
  ```sql
  CREATE PUBLICATION flusso FOR TABLE users, orders, order_items;  -- or FOR ALL TABLES
  ```

**A new join/aggregate pulls in a new table** → the publication must grow to include it. With management on, flusso extends it automatically; with management off, that's a manual `ALTER PUBLICATION`. This is the usual answer to "I added a `has_many` and its rows aren't syncing."

## REPLICA IDENTITY — what the WAL carries for updates and deletes

`REPLICA IDENTITY` decides what *old* row data the WAL sends on UPDATE/DELETE:

- **default** (primary key) — the PK columns of the old row. Enough for flusso to identify and rebuild the document in the common case.
- **FULL** — the entire old row. Needed when flusso must see the *pre-image* of a non-PK column — e.g. a foreign key that **moves a row from one parent to another**: to fix up *both* the old and new parent documents, flusso needs the old FK value, which only `FULL` carries.
- **NOTHING / keyless** — no identity; the table can't be addressed (see the table above).

Rule of thumb: a PK is enough until a join keys off a *mutable* foreign key; if rows re-parent, give that child table `REPLICA IDENTITY FULL`.

## How relational structure becomes a document

flusso reads your existing schema; the mapping is declared in `*.schema.yml` (see **flusso-schema**), but it mirrors the relational shape:

| Postgres shape | flusso field | Where the key lives |
| --- | --- | --- |
| The document's own table | `table:` (root) | — |
| FK on **this** table → parent | `belongs_to` | this table's `column` |
| FK on a **related** table → this row (one) | `has_one` | related table's `foreign_key` |
| FK on a **related** table → this row (many) | `has_many` | related table's `foreign_key` |
| Two FKs through a junction table | `many_to_many` | the junction's `through` keys |
| Reduce related rows to a scalar | `count`/`sum`/`avg`/`min`/`max` | `foreign_key` xor `through` |

Reverse resolution (a change to a *related* row → which root documents to rebuild) is computed per join kind from these keys — which is why **`primary_key` is mandatory on the root once any relation exists**.

## Privileges, minimally

- **Stream + create the slot:** a role with `REPLICATION` + `SELECT` on the published tables. That's the floor.
- **Also manage the publication:** the role must additionally **own** those tables and hold `CREATE` on the database (or be superuser). Short of that, flusso prints the SQL and you run it as someone who can.

## Debugging checklist ("changes aren't showing up")

1. `wal_level = logical`? (`SHOW wal_level;` — needs a restart if you just changed it.)
2. Is the changed table **in the publication**? (New join target? `flusso check` shows coverage.)
3. Does the table have a **key** (PK or `REPLICA IDENTITY`)? Keyless = skipped/errored.
4. Re-parenting rows but the old parent's doc is stale? → `REPLICA IDENTITY FULL` on the child.
5. Is another flusso (or a leftover slot) consuming the same slot? One slot, one consumer.
