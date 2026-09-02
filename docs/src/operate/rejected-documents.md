# Handle rejected documents

Decide what a single bad document does to the pipeline, find the ones that were skipped, and get them indexed.

## When to use this

OpenSearch accepted a bulk request but refused one document: a value that doesn't fit the mapping, a string too long for a keyword, a malformed date. This is an **item-level rejection**. A flush-wide failure (cluster down, request refused) is different and always stops the run until it recovers.

## Steps

1. **Know your policy.** The default `on_error = "stop"` halts the run on the first rejection and leaves the batch unconfirmed, so it's redelivered on restart and the same document stops it again. That's deliberate: dropping data is opt-in.

2. **Read the error.** With `stop`, the log names the index, the document id, and OpenSearch's reason. `/status` shows `phase: stopped` and `last_error`. Fix the source row or the schema (a `custom` type, a `default`, a wider type), then restart.

3. **Opt into skipping where it's the right trade.** For an index where one bad row shouldn't stall the rest, set the policy per index:

   ```toml
   [[index]]
   name = "analytics"
   schema = "analytics.schema.yml"
   enabled = true
   on_error = "skip"
   ```

   Changing the policy never triggers a reindex; it's operational, not part of the document shape.

4. **Alert on quarantines.** Under `skip`, each rejected document is logged with its reason, counted in `flusso_documents_quarantined_total{index}`, and shown in `/status` as `documents_quarantined`. Non-zero means data is being dropped:

   ```promql
   increase(flusso_documents_quarantined_total[10m]) > 0
   ```

5. **Get a quarantined document indexed.** It never lands until its source row changes again. Fix the cause, then touch the row (any update, or a no-op `UPDATE … SET col = col`) so the WAL carries a change and the document is rebuilt. For many rows, a reindex rebuilds them all; see [Reindex without downtime](reindex.md).

## Options and variations

- **Global default.** `on_error = "skip"` at the top level makes it the default for every index; a per-index `stop` then opts one back in.
- **One `stop` halts the batch.** In a batch touching a `stop` index and a `skip` index, a rejection in the `stop` index halts the whole batch before any quarantine is recorded.
- **The sink never retries a rejection**; re-sending re-rejects. Only transient transport failures are retried, per `max_retries`.

## Related

- [Index entries and on_error](../reference/index-and-on-error.md#on_error) for the two policies' exact semantics.
- [Metrics](../reference/metrics.md).
