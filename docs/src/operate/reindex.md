# Reindex without downtime

Rebuild one index from scratch while the current copy keeps serving reads, then swap atomically.

## When to use this

Data in the index has drifted from the source (a bug, a manual edit, rows imported behind flusso's back), or you want a clean generation for its own sake. A **schema change** does not need this: a structural change moves the hash and re-seeds into a new index on the next run by itself.

## Steps

1. **Check the index is seeded and the server is live.**

   ```sh
   flusso indexes --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

2. **Trigger the rebuild.**

   ```sh
   flusso reindex users --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

   The server answers `reindex of users on every sink queued` with `202`. Each sink engine stages generation `n+1` behind the same hash alias between two batches and requests a snapshot; the ingest engine serves it alongside live changes. Reads keep hitting generation `n` through the alias the whole time, nothing restarts, and the other sinks are untouched. Add `--sink primary` to rebuild on one sink only.

3. **Watch it land.** In `/status`, `users` goes `backfilling`, then `seeded` under the sink. At that moment the alias flips to `n+1` and `n` is dropped.

   ```sh
   watch -n 5 'curl -s localhost:9464/status | jq .sinks.primary.indexes'
   ```

4. **Confirm the read side.** `flusso-query` addresses the hash alias, so nothing changes for consumers. Ad-hoc readers on the convenience alias see the new generation too.

## Options and variations

- **With `curl`.** `curl -u admin:… -X POST 'http://10.0.0.5:9465/reindex?index=users&sink=primary'` (drop `&sink=` for every sink). Responses are in [HTTP endpoints](../reference/http.md#post-reindexindexnamesinkname).
- **Rebuild one index by deleting it.** Delete its generation index (`users_<hash>_<n>`) in OpenSearch. On the next start the seed marker is found to name a missing index, so the marker is retracted and the backfill refills it in place. While the server runs, the same happens on the next write to that index: the write fails instead of recreating the index, and the sink restarts into the rebuild. Reads 404 until then and see a partial index while it refills. Don't do this mid-rebuild; see [Sink: OpenSearch](../reference/sink-opensearch.md#seeding).
- **Asking twice.** A reindex of an index that is still rebuilding is accepted and does nothing: the rebuild in flight is already from scratch.
- **Rebuild everything.** Drop the replication slot. Every seeded index is rebuilt into a fresh generation on the next start. See [Recover from a dropped slot](dropped-slot.md).
- **Live changes during the rebuild** are captured into the new generation as it's built; the slot advances only as documents land, as always.
- **Only the private surface** can trigger this. It's Basic-auth and localhost by default; see [Secure the private surface](private-surface.md).

## Related

- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for aliases and generations.
- [CLI](../reference/cli.md#reindex).
