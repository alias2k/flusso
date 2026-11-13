# Upgrade to 0.18

0.18 indexes a `binary` field as base64 of its bytes, so each index with a `binary` field is rebuilt once into a new physical index.

## When to use this

A deployment going from 0.17 to 0.18. The steps apply when its schemas declare, anywhere in the document, joined tables included, a [`binary`](../reference/field-types.md) field, or an `ids` aggregate with `element_type: binary`. Without one, no index is rebuilt.

**What was wrong.** A `binary` field indexed the column's Postgres hex text (`\x0102…`) where OpenSearch's `binary` expects base64, at every position: the root, `bytea[]`, every join, any depth. OpenSearch stored it, so `_source` returned the hex text, not the bytes.

**Why the rebuild.** The content hash is the physical index name. The fix moves the hash of every schema listed above (and of no other schema), so the first run finds a new hash alias with no seed marker and backfills it from Postgres on every OpenSearch sink, with a committed `flusso.lock` and `--locked` too.

## Steps

1. **Upgrade flusso.** Stop 0.17 and start 0.18 on the same slot. Each affected index goes `backfilling`, the others stay `seeded`.
2. **Roll out the consumers and remove the old index** as in [Upgrade to 0.17](upgrade-to-0-17.md#steps), steps 5 to 8: a `flusso-query` consumer bakes the schema hash, so rebuild it against 0.18 once the new index is `seeded`, then delete the old generation and its seed marker by hand.

## Options and variations

- **Rolling back to 0.17** returns to the old index, which missed every change made since. Run `flusso reindex <index>` for each affected index after the rollback.
- **Several OpenSearch sinks** each rebuild their own copy. A `stdout` sink has no index; its output carries the bytes as base64 from the first run.

## Related

- [Field types](../reference/field-types.md) for what a `binary` field indexes.
- [Reindex without downtime](reindex.md) for rebuilding an index on demand.
- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for hash aliases and generations.
