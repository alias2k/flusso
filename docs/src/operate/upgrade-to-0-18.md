# Upgrade to 0.18

0.18 indexes a `binary` field as base64 of its bytes and a `constant` as written, so each index with a `binary` field or a `constant` that is stored unindexed (an object, or a list holding one or mixing kinds) is rebuilt once into a new physical index.

## When to use this

A deployment going from 0.17 to 0.18. The steps apply when its schemas declare, anywhere in the document, joined tables included:

- a [`binary`](../reference/field-types.md) field, or an `ids` aggregate with `element_type: binary`;
- a [`constant`](../reference/field-types.md#constant) whose `value` is an object, or a list holding one or mixing kinds of scalar (`[1, "n/a"]`, or `[1, 2.5]`: a fractional number after a whole one).

Without either, no index is rebuilt. A `constant` holding a number, a boolean, or a list of one kind of scalar needs nothing either: see the last step.

**What was wrong.** A `binary` field indexed the column's Postgres hex text (`\x0102…`) where OpenSearch's `binary` expects base64, at every position: the root, `bytea[]`, every join, any depth. OpenSearch stored it, so `_source` returned the hex text, not the bytes. A `constant` object or list built `null`, a number or boolean its text (`"5"`, `"true"`).

**Why the rebuild.** The content hash is the physical index name. The fix moves the hash of every schema listed above (and of no other schema), so the first run finds a new hash alias with no seed marker and backfills it from Postgres on every OpenSearch sink, with a committed `flusso.lock` and `--locked` too. Such a constant is mapped as an unindexed `object`, which the old index can't take: the strict root refuses an object's keys, and a field mapped from a list's first element refuses an element of another kind (`[1, "n/a"]` in a `long`).

## Steps

1. **Upgrade flusso.** Stop 0.17 and start 0.18 on the same slot. The sink backfills the affected indexes. Until that snapshot completes, `/status` shows the phase `backfilling`, each affected index `backfilling`, and every other index `pending`, although those keep their seeded index and serve reads throughout.
2. **Switch consumers that decoded the hex text.** A consumer that read a `binary` field's `\x…` text and decoded it itself must decode base64 instead. A `flusso-query` document still types the field as `String`; only its contents change.
3. **Roll out the consumers and remove the old index** as in [Upgrade to 0.17](upgrade-to-0-17.md#steps), steps 5 to 8: a `flusso-query` consumer bakes the schema hash, so rebuild it against 0.18 once the new index is `seeded`, then delete the old generation and its seed marker by hand.
4. **Rewrite scalar constants, if `_source` matters.** A number or boolean constant was indexed from its text, which OpenSearch coerced, so search, sort, and aggregations are unchanged; only `_source` held `"5"` for `5`. A document picks up the typed value at its next write. To rewrite all of them now, run [`flusso reindex <index>`](../reference/cli.md#reindex). A list of one kind of scalar was `null`; the same reindex fills it in.

## Options and variations

- **Rolling back to 0.17** returns to the old index, which missed every change made since. Run `flusso reindex <index>` for each affected index after the rollback.
- **Several OpenSearch sinks** each rebuild their own copy. A `stdout` sink has no index; its output carries the corrected values from the first run.

## Related

- [Field types](../reference/field-types.md) for what a `binary` field and a `constant` index.
- [Reindex without downtime](reindex.md) for rebuilding an index on demand.
- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for hash aliases and generations.
