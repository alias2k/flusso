---
status: accepted
---

# Reverse resolution reads the pre-image

A change in a joined table is resolved to the root documents that embed it by walking the relation path back to the root over the *current* database (issue #140). That walk can't find the old parent of a hard-deleted or re-parented `has_one`/`has_many` child, or of a junction row whose key doesn't carry `left_key`: the link column now holds nothing, or the new parent. The document keeps a stale copy and nothing reports it.

The fix: a change carries the source's **pre-image** (the old row, as far as the change feed sends it), and the first reverse hop resolves parents from the current database **and** the pre-image's link column. Hops further up stay on the current database. For Postgres this needs the child table's `REPLICA IDENTITY` to cover the link column (`FULL`, or `USING INDEX` on a unique index that includes it); flusso reports every table that lacks it in `check` and at `run` startup, prints the `ALTER TABLE`, never runs it, and warns once per table when a delete arrives without one.

## Considered options

- **Ask the sink.** Search the index for documents embedding the child's id. Needs the child key in every document, puts source resolution on a sink's storage, and has no answer for a sink that keeps nothing (stdout).
- **A reverse-dependency map.** Record, on every build, which rows each document embedded; look the changed row up instead of walking. Works with any replica identity and shortens deep walks, but it is new persistent state (where: the sink, the user's database, or a local store), a write on every build, huge entries for popular `belongs_to` targets, and it must stay consistent across redelivery, reindex, and a fresh slot. Still needs one query for a new row, which has no entry yet. Kept as a possible optional feature.
- **Triggers or an audit table.** Works on any replica identity but installs objects in the user's schema and adds a write to every statement.
- **Periodic reconciliation.** Eventually correct, stale in between, and costly at scale.
- **The pre-image (chosen).** No new state, one extra source in the first hop. The cost is a one-time `ALTER` per child table and more WAL from those tables.

## Consequences

- `ChangeEvent` gains `before: Option<RowImage>`; `DocumentBuilder::resolve` takes it. Documents are still built from the current row only.
- A change's key is the table's catalog primary key, not the replica-identity columns: under `FULL` every column is an identity column, which would have turned a document id into every column joined by `:`.
- `CaptureProvisioning` gains a read-only `inspect_pre_image` returning a `PreImageReport` (each gap with its own remediation) beside the publication's `CoverageReport`; it is reported, never provisioned.
- Without the replica identity, a child delete or re-parent is still missed, but no longer silently.
