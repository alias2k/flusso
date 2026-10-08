---
status: accepted
---

# Reverse resolution reads the pre-image

> Amended 2026-10-08, before release: flusso now sets the replica identity itself instead of only reporting it (see the last considered option).

A change in a joined table is resolved to the root documents that embed it by walking the relation path back to the root over the *current* database (issue #140). That walk can't find the old parent of a hard-deleted or re-parented `has_one`/`has_many` child, or of a junction row whose key doesn't carry `left_key`: the link column now holds nothing, or the new parent. The document keeps a stale copy and nothing reports it.

The fix: a change carries the source's **pre-image** (the old row, as far as the change feed sends it), and the first reverse hop resolves parents from the current database **and** the pre-image's link column. Hops further up stay on the current database. For Postgres this needs the child table's `REPLICA IDENTITY` to cover the link column (`FULL`, or `USING INDEX` on a unique index that includes it); flusso sets `REPLICA IDENTITY FULL` on every table that lacks it when `run` starts, before the replication slot is created, the way it manages the publication: what flusso needs on the tables is flusso's to set up, while the connection and the server's WAL settings stay with whoever runs the database. It needs ownership of the table, runs under a short `lock_timeout`, and can be turned off (`manage_replica_identity = false`); a gap it can't close is logged with the `ALTER TABLE`, reported by `check`, and warned once per table when a change arrives without the link.

## Considered options

- **Ask the sink.** Search the index for documents embedding the child's id. Needs the child key in every document, puts source resolution on a sink's storage, and has no answer for a sink that keeps nothing (stdout).
- **A reverse-dependency map.** Record, on every build, which rows each document embedded; look the changed row up instead of walking. Works with any replica identity and shortens deep walks, but it is new persistent state (where: the sink, the user's database, or a local store), a write on every build, huge entries for popular `belongs_to` targets, and it must stay consistent across redelivery, reindex, and a fresh slot. Still needs one query for a new row, which has no entry yet. Kept as a possible optional feature.
- **Triggers or an audit table.** Works on any replica identity but installs objects in the user's schema and adds a write to every statement.
- **Periodic reconciliation.** Eventually correct, stale in between, and costly at scale.
- **The pre-image (chosen).** No new state, one extra source in the first hop. The cost is a one-time `ALTER` per child table and more WAL from those tables.
- **Report the `ALTER` but never run it.** The first cut. It left a stock install broken until someone read a warning, against the rule that flusso sets up what it needs on the tables. Kept as the `manage_replica_identity = false` opt-out.

## Consequences

- `ChangeEvent` gains `before: Option<RowImage>`; `DocumentBuilder::resolve` takes it. Documents are still built from the current row only.
- A change's key is the table's catalog primary key, not the replica-identity columns: under `FULL` every column is an identity column, which would have turned a document id into every column joined by `:`.
- `CaptureProvisioning` gains `inspect_pre_image` / `ensure_pre_image` returning a `PreImageReport` (each gap with its privilege verdict and remediation) beside the publication's `CoverageReport`.
- Where flusso can't set the replica identity, a child delete or re-parent is still missed, but no longer silently.
