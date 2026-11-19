# Handle rejected documents

Decide what a single bad document does to its index, find the documents that were refused, and get them indexed.

## When to use this

OpenSearch accepted a bulk request but refused one document in it: a value that doesn't fit the mapping, a string too long for a keyword, a malformed date. This is an **item-level rejection**, and the index's [`on_error`](../reference/index-and-on-error.md#on_error) policy decides what happens to it.

Three refusals look similar and are **not** rejections. `on_error` never sees them, and this page doesn't apply:

- **The index refuses writes** (a [write block](../reference/sink-opensearch.md#write-blocks), a missing permission): the lane parks as `blocked` until the block is lifted. See [A lane is stalled](stalled-lane.md).
- **The cluster is busy** (HTTP 429 or 503): the documents are [resent](../reference/sink-opensearch.md#retries).
- **The whole write fails** (cluster down, the request refused): the sink restarts with backoff until it recovers.

## Steps

1. **Know your policy.** The default, `stop`, drops nothing: the first rejection **parks** the index's lane on that sink. The batch stays unconfirmed and is written again alone on the [`[backoff]`](../reference/config-toml.md#backoff) schedule (1 s, doubling to 60 s, by default) until it lands. The index's later changes wait behind it; the other indexes, and the other sinks, keep going. `skip` quarantines the document and moves on. Dropping data is opt-in.

2. **Under `stop`, read the error.** The log names the index, the sink, the refused ids and OpenSearch's reason:

   ```text
   WARN the lane is parked: its item is written again alone after the backoff; the other indexes go on index=users sink=primary reason=refused: id 42 (failed to parse field [age] of type [long] in document with id '42'. …) backoff=1s
   ```

   [`/status`](../reference/http.md#lane-health) shows the lane `refused` under `sinks.<sink>.lanes.<index>`, with the same `reason` and the next try in `next_probe`; `flusso_lane_parked{why="refused"}` is 1.

3. **Fix the cause.** Each probe writes the **same** batch, so fixing the row in Postgres doesn't unpark the lane on its own. Pick the fix that matches the reason:

   | Cause | Fix |
   | --- | --- |
   | The schema doesn't fit the data (a wider type, a `custom` type, a `default`) | Change the schema and restart. A schema change builds a new index. |
   | The row is wrong | Set `on_error = "skip"` on the index (step 4) and restart, then fix the row: its next change indexes it. |
   | `Limit of total fields … exceeded` | Raise the limit with [`index_settings`](../reference/sink-opensearch.md#index-settings), restart, and [reindex](reindex.md). |

4. **Opt into skipping where it's the right trade.** For an index where one bad row shouldn't hold up the rest, set the policy per index. Changing it never triggers a reindex.

   ```toml
   [[index]]
   name = "analytics"
   schema = "analytics.schema.yml"
   enabled = true
   on_error = "skip"
   ```

5. **Alert on quarantines.** Under `skip`, each rejected document is logged with its reason, counted in `flusso_documents_quarantined_total{index}`, and shown in `/status` as `documents_quarantined`. Non-zero means data is being dropped:

   ```promql
   increase(flusso_documents_quarantined_total[10m]) > 0
   ```

6. **Get a quarantined document indexed.** It never lands until its source row changes again. Fix the cause, then update a column the document reads, so the WAL carries a change and the document is rebuilt. On a `REPLICA IDENTITY FULL` table an update that changes nothing the document reads (a no-op `UPDATE … SET col = col` included) [rebuilds nothing](../reference/source-postgres.md#updates-that-change-nothing-a-document-reads). For many rows, a [reindex](reindex.md) rebuilds them all.

## Options and variations

- **A global default.** `on_error = "skip"` at the top level makes it the default for every index; a per-index `stop` then opts one back in.
- **One `stop` parks one index.** In a write holding a `stop` index and a `skip` index, a rejection in the `stop` index parks only its lane; the `skip` index's part lands, and its rejections are quarantined.
- **Batches beside a rejected one can land.** With several [writes](../reference/config-toml.md#concurrency) per sink, batches of the same index for other documents, written beside the rejected one or started while it was in flight, can land before the lane parks. The confirmed position stays before the rejected batch, so nothing is lost.
- **A restart retries first.** A new process writes its oldest unconfirmed batch alone before anything else. When that is an older batch already written, a few batches past the rejected one can land again before the lane parks; writing them again is harmless.
- **The sink never retries a rejection**: re-sending re-rejects. Only transient failures are retried, per `max_retries`. See [Retries](../reference/sink-opensearch.md#retries).
- **A script that fails on a document** is governed by the same policy, one step earlier: `skip` publishes nothing for it, so it keeps its indexed version on every sink. It's counted in `flusso_script_failures_total`, not as a quarantine; alert on it too: `increase(flusso_script_failures_total[10m]) > 0`. See [Script fields](../reference/scripts.md#failures).

## Related

- [Index entries and on_error](../reference/index-and-on-error.md#on_error) for the two policies' exact semantics.
- [A lane is stalled](stalled-lane.md) for what a parked lane costs.
- [Metrics](../reference/metrics.md).
