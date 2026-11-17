# Read your own writes

After a commit, wait on flusso until the index has it, then read. One request, no polling, no sleep.

## When to use this

A request handler writes to Postgres and must answer from OpenSearch: save a profile and return the indexed document, add an order and re-run the list query. flusso is asynchronous, so a read made right after the commit can miss it. A **fence** closes the gap: the caller names its commit by position and flusso holds the request until a sink has caught up.

Skip it when the reader can tolerate being a moment behind. A fence is a wait, and under a draining backlog a `searchable` one can last as long as the sink's `refresh_interval`.

From Rust, `flusso-query` runs the steps below itself: see [Reading your own writes](../query/read-your-writes.md).

## Steps

1. **Make the public surface reachable.** `/wait` is served next to `/status`, on `127.0.0.1:9464` by default. Bind it where the application can reach it with `--public-address` (see [HTTP endpoints](../reference/http.md)); the Helm chart's Service already exposes it.

2. **Commit, then read the source position.** On a connection to the **primary**, **after** the commit and outside the transaction:

   ```sql
   SELECT pg_current_wal_lsn();   -- 0/16B3748
   ```

   Every commit made before that statement lies at or before the position it returns, the caller's own included.

3. **Wait.**

   ```sh
   curl -s 'http://flusso:9464/wait?position=0/16B3748&until=applied&sink=primary'
   ```

   ```json
   { "reached": true, "position": "0/16B3780", "until": "applied", "sink": "primary" }
   ```

   `200` means every commit at or before the position is in the sink. Pick the level by what the next read is:

   - `until=applied`: a fetch by id (`GET <index>/_doc/<id>`) sees the write.
   - `until=searchable` (the default): a search sees it too.

4. **Read from OpenSearch**, as usual. The read doesn't go through flusso.

5. **Handle `408`.** The timeout passed first: flusso is behind, or a sink is stalled. Don't fall back to reading anyway unless a stale answer is acceptable; the response carries how far the sink got.

6. **Ride out a restart.** A refused connection, a `503`, or a gateway's `502`/`504` means flusso is stopping, starting, or down: pause and ask again until the caller's own deadline, honouring `Retry-After`. A `400` or `404` won't change: don't retry it. `flusso-query` does all of this itself.

## Options and variations

- **Name the sink the reader uses.** Without `sink`, the wait covers every sink, so a second sink that has stalled holds up readers of a healthy one. All parameters are in [HTTP endpoints](../reference/http.md#get-waitpositionposition).
- **Related tables are covered.** A write to a table an index only joins rebuilds the parent documents; the fence passes once those are in.
- **Several writes, one wait.** Read the position once after the last commit. It covers everything before it.
- **Watch the positions.** `/status` reports `applied_position` and `searchable_position` per sink, comparable with `pg_current_wal_lsn()`.
- **Behind a synchronous standby too.** A commit returns to the writer only once it is visible, and flusso indexes it only then, so the position read after it covers the committed state. A fence on a position another session read while a commit still waited on the standby doesn't pass until that commit is visible and indexed. See [Behind a synchronous standby](../reference/source-postgres.md#behind-a-synchronous-standby).
- **During a deploy.** flusso is one process with one replication slot, so a deploy always has a gap: the old process stops, the new one starts following the source. The old process [drains](../reference/cli.md#stopping): a wait open then stays open and is answered `200` if its position lands during the drain, which is when in-flight commits reach the sinks; once the drain is over, every wait still open gets `503` with `Retry-After` and the reason `stopping` ([the reasons](../query/read-your-writes.md#when-the-wait-fails)). Until the new process is live, `/wait` refuses connections or answers `503` with the reason `starting`. The new process answers the same question, since the position is Postgres's own, and after a graceful stop it starts from everything the old one indexed; a client that retries until its deadline sees a slow read instead of an error. Size that deadline above a typical gap (the old process's [stop](../reference/cli.md#stopping), plus the new one's start until `/readyz` answers) and below the caller's own request timeout. `flusso-query` waits twenty seconds by default; the worst restart can outlast that, and the read then fails rather than read stale.
- **A rebuilding index isn't vouched for.** While a sink rebuilds an index (a first backfill, a rebuild after the slot was lost, a reindex), a fence can pass before the document is readable ([#379](https://github.com/alias2k/flusso/issues/379)). Once that's fixed, `/wait` answers `503` with the reason `rebuilding` there instead.
- **A backfill isn't fenced.** While an index is still seeding, a fence can pass before the snapshot has reached a row. Gate on `/readyz` or the index's state in `/status` for that.
- **Neither is an index mid-reindex.** During a [reindex](reindex.md), live writes land in the new generation while readers stay on the old one until the swap. A fence passes when the write is in; a read of that index sees it after the swap. The index shows `backfilling` in `/status` meanwhile.

> ⚠️ **Warning** — A position read *inside* the transaction lies before its commit, so the fence can pass before the write is in. A position read from a **replica** is that replica's replay position, not the one flusso follows. Both fail quietly: the wait returns `200` and the read is stale.

> ℹ️ **Info** — flusso never refreshes or flushes because someone waits. A `searchable` fence passes at once when flusso is caught up, because a caught-up flush refreshes anyway; while a backlog drains it passes when the index's periodic refresh is due.

## Related

- [HTTP endpoints](../reference/http.md#get-waitpositionposition) for parameters and responses.
- [Sink: OpenSearch](../reference/sink-opensearch.md#refresh) for when a flush becomes searchable.
- [Watch it run](watch-it-run.md).
