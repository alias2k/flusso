# Run one flusso per slot

One replication slot, one flusso process. A second process on the same slot can stop the first, with nothing lost; and while a process waits for the slot, the writes that could erase another's records wait with it.

## When to use this

- `flusso run` exited `1` with `another flusso process changed the index's detach record or seed marker …`.
- A deploy could run two processes on one slot at once: a rolling update, a second host, a 0.18 process left running beside 0.19.
- During a reconnect, a seed, a reindex or a reattach waits instead of going through.

A single deployment, one process at a time, never stops this way.

## The rule

flusso keeps a few records per index in each OpenSearch sink's [meta documents](../reference/sink-opensearch.md#the-meta-documents): the seed marker and the detach record. Every write of them is stamped with the process that made it. After each opening of the replication stream a process reads the records again, before writing anything new for the index:

- **What it wrote or last read** carries on as usual.
- **Anything else** means another process changed them while this one didn't hold the slot, for instance a second flusso that seeded or reindexed the index during a reconnect. flusso doesn't carry on over another process's changes: the deployment stops.
- **A start adopts what it finds.** Only a process already running can find another's change.

## If flusso stopped

The log has an `ERROR` with `index`, `sink` and `what`:

```text
another flusso process changed the index's detach record or seed marker while this one didn't hold the slot: stopping the deployment; no change of this index confirmed past what they cover
```

and the process exits `1` with:

```text
Error: sink `primary`, index `users`: another flusso process changed the index's detach record or seed marker while this one didn't hold the replication slot (<what changed>). Stopping: flusso doesn't carry on over another process's changes to them. No change of this index was confirmed past what they cover; start flusso again (one process per slot) and its start decides from them
```

`<what changed>` names the record or the seed marker before and after, and who wrote each: `writer <16 hex digits> (write <n>)`, one writer per sink engine per process start, or `nobody known (no stamp)` for a write with no stamp (an older flusso's).

1. **Find the other process.** Only one process can hold the slot at a time; the one holding it now is in Postgres:

   ```sql
   SELECT a.pid, a.client_addr, a.application_name
   FROM pg_replication_slots s JOIN pg_stat_activity a ON a.pid = s.active_pid
   WHERE s.slot_name = 'flusso';
   ```

2. **Stop it**, or give it its own `--slot` (and, for a separate deployment, its own [`prefix`](../reference/config-toml.md#prefix)).
3. **Start flusso again.** Its start reads the records and decides from them.

**Nothing is lost.** No change of the index was confirmed to the slot past what its detach record and seed marker cover, so the next start reads the rest again.

## While flusso doesn't hold the slot

Between the end of a stream and the next opening (Postgres dropped the connection, an opening failed), flusso keeps writing what its lanes hold, and a stop drains as usual. What could erase or loosen a record another process relies on waits until the stream is open again, and is retried:

| What | Meanwhile |
| --- | --- |
| A backfill that completes | Its documents are all written; only the seed marker waits, and the index stays `backfilling`. It lands once the slot is held and the records are read again. A stop meanwhile drains, and the next start backfills the index again. |
| A [reindex](reindex.md) | Owed: the lane turns `restaging`, its reason `the reindex could not stage: … the reindex waits: <why>`, and it is staged again on the [`[backoff]`](../reference/config-toml.md#backoff) schedule. |
| A [rebuild](detached-index.md) of a detached index | Owed: the lane stays `detached`, its reason `the rebuild could not begin: <why>; tried again in <d>`. |
| A [reattach](detached-index.md) | Refused with `409`, the reason saying why: ask again. |
| Putting back a detach or another process's rebuild, found at staging | The lane is held `detached` or `restaging` (`the records ask for a demotion or a detach, which waits until this process holds the slot`), nothing of it dropped. |

`<why>` is one of:

- `this process doesn't hold the slot (its stream ended or its opening failed), so the record may be another process's; ask again once it holds the slot`
- `the records, read under an earlier stream, could not be read again before the write (<error>); ask again`
- `another process changed the records: the deployment stops, and its next start decides from them`

Fences are affected too: right after each opening, a lane answers `/wait` with `starting` until its records are read under the new stream, normally well under a second ([details](../reference/http.md#get-waitpositionposition)).

## Options and variations

- **Helm** runs `replicas: 1` with the `Recreate` strategy, so a deploy never overlaps: [Deploy with Helm](../deploy/helm.md).
- **Docker and supervisors.** Stop the old container or process before starting the new one; two services on one slot are two processes: [Run under a task runner or supervisor](../deploy/compile-and-run.md#run-under-a-task-runner-or-supervisor).
- **Upgrading from 0.18.** Stop the 0.18 process before starting 0.19: [the upgrade page](upgrade-to-0-19.md#one-process-per-slot).
- **Several deployments on one cluster** each need their own slot and their own `prefix`.

## Related

- [Bring back a detached index](detached-index.md).
- [Source: Postgres](../reference/source-postgres.md#capture) for the slot.
- [CLI](../reference/cli.md#stopping): how a stop drains.
