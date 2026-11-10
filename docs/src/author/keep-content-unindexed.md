# Keep content without indexing it

Keep display-only content in the document without paying mapping fields for it: drop a string field's subfields, or store a whole subtree unindexed.

## When to use this

A document carries content that is shown but never searched: notes, price periods, a snapshot of a related row, a long history. Every mapped field counts toward `index.mapping.total_fields.limit` (1000 by default), and a `text`/`keyword` field costs 3–4 with the sink's subfields. A wide parent with a few `has_many` joins hits that wall fast.

Two siblings trade search for size:

| Want | Use | Loses |
| --- | --- | --- |
| a string field searchable, but not sortable/aggregatable | [`subfields: false`](../reference/field-types.md#subfields) | exact filter, sort, aggregation on `text`; full-text and case-insensitive sort on `keyword` |
| a join or `json` blob kept for display only | [`indexed: false`](../reference/joins.md#indexed) | every query on anything under it |

Raising the limit with [`index_settings`](../reference/sink-opensearch.md) is the other route; it keeps everything searchable and costs heap. Don't use either opt-out on content a query needs: the query side refuses to compile against it.

## Steps

1. **Find the heavy fields.** `GET <index>/_mapping` (or the designer's **Mapping** tab) shows the full mapping: a `text` or `keyword` with `fields` costs 3 mapping fields, a `nested` join one per field under it. `flusso check --offline` lists every mapped field. An unindexed one shows as `object (not indexed)`, and the fields under it are not listed.

2. **Drop the subfields of a string you only match on.**

   ```yaml
   - text: description
     required: false
     subfields: false
   ```

   It stays full-text searchable with `flusso_text`.

3. **Store a display-only subtree unindexed.**

   ```yaml
   - has_many: priceHistory
     table: price_changes
     foreign_key: product_id
     primary_key: id
     indexed: false
     fields:
       - decimal: amount
         required: true
       - timestamp: changedAt
         column: changed_at
         required: true
   ```

   The same key on a `json` field stores the blob as-is; see [Field types](../reference/field-types.md) for why a plain `json` field needs it.

4. **Run it.** Either change gives the schema a new content hash, so `flusso run` builds a new index and backfills it once. Verify with `GET <index>/_mapping`: `priceHistory` is `{"type": "object", "enabled": false}`, and a search returns it in `_source`.

5. **Update the query structs.** A `subfields: false` field's handle is `NoSubfields`, so `.keyword()` on it stops compiling; an `indexed: false` field has no handle at all, while its struct field still deserializes. See [Document structs](../query/document-structs.md#stored-only-fields).

## Options and variations

- **Need one value from a stored subtree in a query?** Add an aggregate over the same relation (`count`, `max`, `ids`); it's its own searchable field. See [Aggregates](../reference/aggregates.md).
- **Settings under an unindexed join** are kept for when `indexed` is turned back on; see [Joins: indexed](../reference/joins.md#indexed) for which ones.
- **In the designer** both are checkboxes in the inspector; see [Designer reference](designer-reference.md#inspector).

## Related

- [Field types](../reference/field-types.md#subfields) and [Joins](../reference/joins.md#indexed) for the exact rules.
- [Sink: OpenSearch](../reference/sink-opensearch.md#analysis-and-subfields) for what the subfields are.
