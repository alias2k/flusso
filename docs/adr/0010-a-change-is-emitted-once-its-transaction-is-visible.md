---
status: proposed
---

# A change is emitted once its transaction is visible

flusso reads the database to turn a change into documents: reverse resolution finds the documents a row touches, and the build reads every row a document embeds. Those reads assumed the change's transaction was visible to them. It need not be (issue #343).

Postgres commits in three steps: it flushes the commit record, waits for synchronous standbys if `synchronous_commit` asks it to, and only then takes the transaction out of the proc array, which is when new snapshots see it. A logical walsender decodes up to the flushed WAL, so the change reaches flusso between the first step and the last. Every read flusso makes in that window sees the state before the commit: a re-parented child resolves to its old parent only, an inserted child to none, and every document built then holds the old rows. Nothing builds those documents again, and the position is acknowledged, so a fence reports the stale document as applied.

On a quiet primary the window is microseconds, which a loaded CI runner stretches into flaky tests. Behind a synchronous standby it lasts the standby's round trip on **every** commit. Measured with a standby emulated at 2 ms and 10 ms, 57–59 % and 98–100 % of lone changes ended up indexed with the value before their commit.

**Decision (proposed): the Postgres capture emits a transaction's changes only once the transaction is visible to a new snapshot.** The capture keeps the xid from each `Begin`, holds committed transactions, and before emitting the oldest held one it reads ahead every event the replication client already holds, then asks once for all held xids:

```sql
WITH s AS (SELECT pg_current_snapshot() AS snap,
                  pg_snapshot_xmax(pg_current_snapshot())::text::bigint AS next)
SELECT x FROM s, unnest($1::bigint[]) AS x
WHERE pg_visible_in_snapshot(<x widened to xid8 with next's epoch>, snap)
```

It polls while the oldest held transaction is not visible. The port, the engine and every consumer of a change stay as they are: a change on the stream is readable by definition.

## Considered options

- **Wait per batch in the engine** (a neutral `ChangeCapture` method, "await the source readable up to this position", called before a batch's first read). Measured as cheap as the decision. Rejected: resolution would move from buffering to the batch's commit, the port gains a method every read path must remember to call (the failure mode `fence_before` already has), and the wait would block the ingest task, so snapshots and backfill requests would wait behind a held commit.
- **Hold at the capture, one query per transaction.** The same guarantee with a simpler loop. Measured 3.5–4.5× slower to drain a backlog and 0.7–1.1 s behind a burst of 4,000 single-row transactions from eight writers: the query sits between every two transactions. Rejected for the coalesced form.
- **Check only the batch's newest transaction.** Wrong: transactions leave the proc array in no fixed order (the synchronous-replication wake-up releases waiters together, and each then ends its own transaction), so a newer one can be visible while an older one is not. Every held xid is checked.
- **`pg_xact_status(xid8)`.** It reports `in progress` until the proc array lets go (it checks the proc array before the commit log), so it would work too, but one xid at a time.
- **Read under a snapshot known to include the commit.** Postgres offers no way to wait for a snapshot covering an LSN or an xid. An exported snapshot belongs to slot creation, not to each commit.
- **Rebuild when the row read disagrees with the WAL's post-image.** Only updates carry a post-image (inserts, partitioned tables and tables without a full identity do not), and a mismatch says nothing about the other rows a document embeds. Rejected.
- **Delay every read by a fixed time.** Not a guarantee under any standby. Rejected.

## Consequences

- **A transaction that never becomes visible stalls the stream.** Today it is indexed stale, silently; with the decision flusso waits, as the committing application does (its commit has not returned either). The capture warns once a transaction has waited past a threshold, naming the xid and the likely cause, and the wait is exported as a metric and on `/status`. Nothing else is held: snapshots and backfill requests keep being served.
- **A lone change costs one more round trip** (measured +60–150 µs on a local Postgres, within this machine's noise); a burst costs one query per read-ahead, about one per 40 transactions in the backlog measurement.
- **Behind a synchronous standby**, a change is indexed about 2–3 ms (p50) after its commit returns to the client, bounded by the poll schedule; the stream follows the writers at their own rate.
- **Fences stay honest with no change to `Positions`.** A held transaction is not registered, and the horizon already refuses to move past buffered changes, so no fence covers it until it is emitted.
- **The SQL needs Postgres 13** (`pg_current_snapshot`, `pg_visible_in_snapshot`); flusso requires 14. Both functions run for any role. The query runs on the same database the document builder reads, so a future read replica for the builder would need its own check.
