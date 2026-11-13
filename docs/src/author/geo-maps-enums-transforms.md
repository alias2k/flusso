# Geo, maps, enums and transforms

The field kinds that need a little more than `- <type>: <name>`: a geographic point from two columns, a dynamic-key object over `jsonb`, an enum that sorts in a declared order, a column holding an array, and value transforms.

## When to use this

You're adding a location, a translations column, a status or severity with a meaningful order, a tags column (`text[]` or a `jsonb` array), or a value that needs normalizing before it's indexed.

## Steps

1. **A geo point from two columns.** The document gets an OpenSearch `geo_point`; a missing coordinate makes the whole point null.

   ```yaml
   - geo: location
     lat: latitude
     lon: longitude
     required: false
   ```

   A column that already holds a point (`jsonb` `{"lat","lon"}`, `[lon, lat]`, or `text` `"lat,lon"`) uses `column:` instead. PostGIS `geometry` needs a generated column in one of those shapes. See [geo](../reference/field-types.md#geo).

2. **A dynamic-key object with `map`.** Declare the one type every value shares.

   ```yaml
   - map: title
     values: text
     required: true
   ```

   The `jsonb` column's keys become fields at runtime (`title.en`, `title.it`), without declaring each, and each is mapped like a plain field of the `values` type. See [Objects and maps](../reference/objects-and-maps.md#map).

3. **An ordered enum.** Without `variants` an `enum` is a plain keyword and sorts alphabetically. List the variants in rank order and sorting follows that order, with no script.

   ```yaml
   - enum: severity
     required: true
     variants: [low, medium, high, critical]
   ```

   Values outside the list sort after the declared ones. See [enum](../reference/field-types.md#enum).

4. **Normalize before indexing.** Transforms run in order on the column value.

   ```yaml
   - keyword: email
     required: true
     transforms: [trim, lowercase]
   ```

5. **A column holding an array.** Declare the element's type and `array: true`.

   ```yaml
   - keyword: tags
     required: false
     array: true
     transforms: [lowercase]
   ```

   What happens to nulls, lone values and transforms, and what `check` says about a `[]` column without the flag, is in [array](../reference/field-types.md#array).

6. **Validate.** `flusso check --offline` shows the derived mapping: `geo_point` for the geo, `map<text>` for the map (`--format json` shows the mapping every key gets as its `values`), a `.sort` subfield under the enum.

## Options and variations

- **Changing an enum's order** rewrites the mapping, so it rotates the index generation and re-seeds, like any schema change.
- **A map's `values`** must be a leaf kind: string, numeric, or date. `boolean`, `json`, `geo`, and `custom` are rejected.
- **A `custom` scaled_float** is the exact-decimal answer when `decimal`'s lossy `double` won't do. See [custom](../reference/field-types.md#custom).
- **Query-side counterparts.** An array field is a `Vec` of its element, queried with the element's operators ([Arrays](../query/document-structs.md#arrays)). Maps get `.key("it")` and the plain-field operators across every key ([Maps](../query/maps.md)); ordered enums get an `Enum` handle whose `.asc()` sorts by declared order ([Enums and custom values](../query/enums-and-values.md)).

## Related

- [Field types](../reference/field-types.md) for the whole type table.
