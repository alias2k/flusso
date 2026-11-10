# Upgrade to 0.17

0.17 maps every `map` key like a plain field of the map's `values` type, so each index with a `map` field is rebuilt once into a new physical index; plan the data check, the consumer rollout, and the cleanup.

## When to use this

Any deployment going from 0.16 to 0.17 whose schemas declare a [`map`](../reference/objects-and-maps.md#map) field anywhere in the document, joined tables included. Without one, nothing moves: no index is rebuilt and this page doesn't apply.

**Why the rebuild.** The content hash is the physical index name. 0.17 changes the hash of every schema with a `map` (and of no other schema), so the first 0.17 run finds a new hash alias, `{prefix}{logical}_{newhash}`, with no seed marker, creates generation `_1` behind it, and backfills it from Postgres on every OpenSearch sink. This happens with a committed `flusso.lock` and `--locked` too: the hash is recomputed when the lock loads.

## Steps

1. **Check the data for values the typed keys refuse.** 0.16 left each key to OpenSearch's dynamic defaults, which accepted anything. 0.17 maps a key like a plain field, so the rebuild rejects:

   - a non-numeric string in a numeric map (a numeric string such as `"3.5"` is coerced and passes);
   - an object as a value, in any map;
   - a key containing a dot, anywhere: `{"en.US": "Hi"}` reads as `{"en": {"US": "Hi"}}`.

   Find them before upgrading. For a map read from column `prices` on `products` (cast a `json` column with `::jsonb`):

   ```sql
   -- numeric map: strings that aren't numbers
   SELECT id, kv.key FROM products, jsonb_each(prices::jsonb) AS kv
   WHERE jsonb_typeof(kv.value) = 'string'
     AND kv.value #>> '{}' !~ '^\s*-?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?\s*$';

   -- any map: objects as values
   SELECT id, kv.key FROM products, jsonb_each(title::jsonb) AS kv
   WHERE jsonb_typeof(kv.value) = 'object';

   -- any map: dotted keys
   SELECT id, kv.key FROM products, jsonb_each(title::jsonb) AS kv
   WHERE kv.key LIKE '%.%';
   ```

   Fix the rows, or plan to skip them (next step).

2. **Decide what a rejected row does during the rebuild.** The default [`on_error = "stop"`](../reference/index-and-on-error.md#on_error) halts that sink on the first rejection, and the redelivered batch hits it again on every restart: the rebuild never finishes. If step 1 left rows you can't fix yet, set `on_error = "skip"` on those indexes for the upgrade, then find and re-index the quarantined documents as in [Handle rejected documents](rejected-documents.md).

3. **Check the field budget of wide maps.** Every distinct key now costs as many mapped fields as a plain field of its type: three for a string key with the default `auto_subfields`, one for a number or date. A map with a few hundred distinct keys reaches `index.mapping.total_fields.limit` (1000), and each document that adds a key past it is rejected. Count the keys:

   ```sql
   SELECT count(DISTINCT k) FROM products, jsonb_object_keys(title::jsonb) AS k;
   ```

   If the total gets close, raise the limit for that index with [`index_settings`](../reference/sink-opensearch.md#index-settings) before upgrading; settings are read when a generation is created, which the rebuild is.

4. **Upgrade flusso, keep the consumers as they are.** Stop the 0.16 process and start 0.17 on the same slot. Every `map` index goes `backfilling`, the others stay `seeded`.

   A `flusso-query` consumer bakes the schema hash at compile time, so the two generations of consumers address different indexes:

   | Consumer | Reads | During the rebuild | After |
   | --- | --- | --- | --- |
   | built against 0.16 | `{logical}_{oldhash}` | complete, but frozen at the moment 0.16 stopped | frozen; stale |
   | rebuilt against 0.17 | `{logical}_{newhash}` | 404 until 0.17 has started, then empty or partial | current |

   So the old consumers keep serving (slightly stale) results until the new index is seeded.

5. **Wait for the new index to be seeded.** Watch each `map` index under every sink in [`/status`](../reference/http.md#get-status) on the public surface, or with [`flusso indexes`](../reference/cli.md#indexes) on the private one:

   ```sh
   watch -n 5 'curl -s localhost:9464/status | jq .sinks.primary.indexes'
   ```

   `seeded` means the backfill finished and the hash alias serves a complete generation.

6. **Deploy the consumers rebuilt against 0.17.** Rebuild them with `flusso-query` 0.17 (the source changes, including `TextMap::search` becoming `matches`, are in [Maps](../query/maps.md#upgrading-from-016)) and roll them out now, not earlier.

7. **Delete the old index.** flusso never removes it: a rebuild only sweeps generations under its *own* hash alias, so `{prefix}{logical}_{oldhash}_{n}` keeps its disk until deleted by hand. List the aliases; the convenience alias `{logical}` points at the new generation, and the hash alias that points elsewhere is the old one:

   ```sh
   curl -s 'localhost:9200/_cat/aliases/products*?v&h=alias,index'
   # alias             index
   # products          products_9f3a1c2e_1
   # products_9f3a1c2e products_9f3a1c2e_1
   # products_4b7d0e11 products_4b7d0e11_1    <- 0.16's
   ```

   Delete its generation(s) (the alias goes with them) and its seed marker in `{prefix}flusso_meta`:

   ```sh
   curl -s 'localhost:9200/_cat/indices/products_4b7d0e11_*?h=index'
   curl -X DELETE 'localhost:9200/products_4b7d0e11_1'
   curl -X DELETE 'localhost:9200/flusso_meta/_doc/products_4b7d0e11'
   ```

   With a `prefix`, every name carries it (`staging_products_4b7d0e11_1`, `staging_flusso_meta`). `products*` also matches other indexes whose name starts with `products`; pick by the logical name.

## Options and variations

- **Ad-hoc readers on the convenience alias** (`{prefix}{logical}`, dashboards, `curl`) move to the new generation as soon as 0.17 starts, so they see an empty or partial index until it's seeded. Point them at the old hash alias for the duration if that matters.
- **Revert `on_error`** to `stop` once the rebuild is done and the quarantined rows are fixed; changing the policy never triggers a reindex.
- **Rolling back to 0.16** after 0.17 ran returns to the old index, which missed every change made while 0.17 ran. Run `flusso reindex <index>` for each `map` index after the rollback, or delete its generation so the restart rebuilds it.
- **Several OpenSearch sinks** each rebuild their own copy; watch every sink's block in `/status`. A `stdout` sink has no index and is unaffected.

## Related

- [Objects and maps](../reference/objects-and-maps.md#map) for how a key is mapped.
- [Sink: OpenSearch](../reference/sink-opensearch.md#index-naming) for hash aliases and generations.
- [Reindex without downtime](reindex.md) for rebuilding an index on demand.
- [Maps](../query/maps.md#upgrading-from-016) for the `flusso-query` changes.
