# Reindex without downtime

Rebuild one index from scratch while the current copy keeps serving reads, then swap atomically.

## When to use this

Data in the index has drifted from the source (a bug, a manual edit, rows imported behind flusso's back), a column changed type and the documents built before it should follow (see [column type changes](../reference/source-postgres.md#capture)), a seeded generation was restored from a snapshot or recreated by hand (flusso keeps serving it as found, even empty), or you want a clean generation for its own sake. It is also the way out for a [lane stuck](stalled-lane.md#unstick-a-lane-with-a-reindex) on something that lives in the index, such as `index.blocks.write` that won't be lifted; not for a row the mapping refuses, nor for `index.blocks.read_only`, which also stops the swap below. A **schema change** does not need this: a structural change moves the hash and re-seeds into a new index on the next run by itself. So does the [upgrade to 0.17](upgrade-to-0-17.md) for indexes with a `map` field.

## Steps

1. **Check the index is seeded and the server is live.**

   ```sh
   flusso indexes --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

2. **Trigger the rebuild.**

   ```sh
   flusso reindex users --server 10.0.0.5:9465 --admin-password "$FLUSSO_ADMIN_PASSWORD"
   ```

   The server answers `202` and the command prints `reindex of users queued on primary, audit` (the sinks it was queued on), then a `skipped on <sink>: <reason>` line for each sink it [left out](../reference/http.md#a-reindex-that-is-skipped). Each sink engine stages generation `n+1` behind the same hash alias between two batches and requests a snapshot; the ingest engine serves it alongside live changes. Reads keep hitting generation `n` through the alias the whole time, nothing restarts, and the other sinks are untouched. Add `--sink primary` to rebuild on one sink only.

3. **Watch it land.** In `/status`, `users` goes `backfilling`, then `seeded` under the sink. At that moment the alias flips to `n+1` and `n` is dropped. If `n` carries `index.blocks.read_only` or `read_only_allow_delete`, the swap can't drop it: the lane parks `blocked` and `users` stays `backfilling` until `n` is freed ([A seed that can't retire the old generation](stalled-lane.md#a-seed-that-cant-retire-the-old-generation)).

   ```sh
   watch -n 5 'curl -s localhost:9464/status | jq .sinks.primary.indexes'
   ```

   While the backfill runs, `curl -s localhost:9464/indexes/users/status` shows it under `backfill`, with the sinks it serves ([per-index state](../reference/http.md#per-index-ingest-state)).

   If the lane under that sink turns `restaging` with `the reindex could not stage: …`, the new generation couldn't be created: flusso retries it on its own. See [A reindex that can't stage](stalled-lane.md#a-reindex-that-cant-stage).

4. **Confirm the read side.** `flusso-query` addresses the hash alias, so nothing changes for consumers. Ad-hoc readers on the convenience alias see the new generation too.

## Options and variations

- **With `curl`.** `curl -u admin:… -X POST 'http://10.0.0.5:9465/indexes/users/reindex?sink=primary'` (drop `?sink=` for every sink). Responses are in [HTTP endpoints](../reference/http.md#post-indexesnamereindexsinkname).
- **Rebuild one index by deleting it.** Delete its generation index (`users_<hash>_<n>`) in OpenSearch. On the next start the seed marker is found to name a missing index, so the marker is retracted and the backfill refills it in place. While the server runs, the same happens on the next write to that index: OpenSearch answers `index_not_found_exception` instead of recreating the index, and that index alone is staged again into the rebuild (its lane `restaging` in `/status`) while the sink's other indexes keep being written; nothing restarts. Reads 404 until then and see a partial index while it refills. Deleting an index still mid-backfill restarts that backfill from the first row. Every case is in [A generation deleted or replaced](../reference/sink-opensearch.md#a-generation-deleted-or-replaced).
- **Asking twice.** A reindex of an index that is still rebuilding is accepted and does nothing: the rebuild in flight is already from scratch. On a lane flusso doesn't write, it starts that backfill again from the first row ([details](../reference/http.md#a-reindex-of-a-lane-flusso-doesnt-write)).
- **After a restart.** A reindex still waiting to be staged is kept in memory only. Check the lane after flusso restarts, and ask again if it's still stuck ([what a restart keeps](../reference/http.md#when-the-reindex-cant-stage)).
- **A detached index.** A reindex on a sink that can backfill rebuilds it; one that is already being rebuilt answers `409`, and a sink with `backfill = false` is skipped with the `flusso reattach` command to run. See [Bring back a detached index](detached-index.md).
- **During a reconnect.** While flusso doesn't hold the replication slot the reindex waits, its lane `restaging`, and goes through once the slot is back: [what waits for the slot](one-process-per-slot.md#while-flusso-doesnt-hold-the-slot).
- **Rebuild everything.** Drop the replication slot. Every seeded index is rebuilt into a fresh generation on the next start. See [Recover from a dropped slot](dropped-slot.md).
- **Apply changed index settings.** An `index_settings` change reaches only a newly created generation, and a running flusso keeps the settings it started with. Restart with the new config, then run `flusso reindex <index>`. See [Index settings](../reference/sink-opensearch.md#index-settings).
- **Live changes during the rebuild** are captured into the new generation as it's built; the slot advances only as documents land, as always.
- **Only the private surface** can trigger this. It's Basic-auth and localhost by default; see [Secure the private surface](private-surface.md).

## Related

- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for aliases and generations.
- [CLI](../reference/cli.md#reindex).
