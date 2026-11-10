# Read your own writes

After a commit, wait on flusso until the index has it, then read. One request, no polling, no sleep.

## When to use this

A request handler writes to Postgres and must answer from OpenSearch: save a profile and return the indexed document, add an order and re-run the list query. flusso is asynchronous, so a read made right after the commit can miss it. A **fence** closes the gap: the caller names its commit by position and flusso holds the request until a sink has caught up.

Skip it when the reader can tolerate being a moment behind. A fence is a wait, and under a draining backlog a `searchable` one can last as long as the sink's `refresh_interval`.

From Rust, `flusso-query` does the steps below for you: see [Reading your own writes](../query/read-your-writes.md).

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

5. **Handle `408`.** The timeout passed first: flusso is behind, or stopped. Don't fall back to reading anyway unless a stale answer is acceptable; the response carries how far the sink got.

## Options and variations

- **Name the sink the reader uses.** Without `sink`, the wait covers every sink, so a second sink that has stalled holds up readers of a healthy one. All parameters are in [HTTP endpoints](../reference/http.md#get-waitpositionposition).
- **Related tables are covered.** A write to a table an index only joins rebuilds the parent documents; the fence passes once those are in.
- **Several writes, one wait.** Read the position once after the last commit. It covers everything before it.
- **Watch the positions.** `/status` reports `applied_position` and `searchable_position` per sink, comparable with `pg_current_wal_lsn()`.
- **A backfill isn't fenced.** While an index is still seeding, a fence can pass before the snapshot has reached a row. Gate on `/readyz` or the index's state in `/status` for that.

> ⚠️ **Warning** — A position read *inside* the transaction lies before its commit, so the fence can pass before the write is in. A position read from a **replica** is that replica's replay position, not the one flusso follows. Both fail quietly: the wait returns `200` and the read is stale.

> ℹ️ **Info** — flusso never refreshes or flushes because someone waits. A `searchable` fence passes at once when flusso is caught up, because a caught-up flush refreshes anyway; while a backlog drains it passes when the index's periodic refresh is due.

## Related

- [HTTP endpoints](../reference/http.md#get-waitpositionposition) for parameters and responses.
- [Sink: OpenSearch](../reference/sink-opensearch.md#refresh) for when a flush becomes searchable.
- [Watch it run](watch-it-run.md).
