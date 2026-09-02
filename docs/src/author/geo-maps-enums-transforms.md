# Geo, maps, enums and transforms

The field kinds that need a little more than `- <type>: <name>`: a geographic point from two columns, a dynamic-key object over `jsonb`, an enum that sorts in a declared order, and value transforms.

## When to use this

You're adding a location, a translations column, a status or severity with a meaningful order, or a value that needs normalizing before it's indexed.

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

   The `jsonb` column's keys become searchable subfields at runtime (`title.en`, `title.it`), without declaring each. See [Objects and maps](../reference/objects-and-maps.md#map).

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

5. **Validate.** `flusso check --offline` shows the derived mapping: `geo_point` for the geo, `object` with `dynamic: true` for the map, a `.sort` subfield under the enum.

## Options and variations

- **Changing an enum's order** rewrites the mapping, so it rotates the index generation and re-seeds, like any schema change.
- **A map's `values`** must be a leaf kind: string, numeric, or date. `boolean`, `json`, `geo`, and `custom` are rejected.
- **A `custom` scaled_float** is the exact-decimal answer when `decimal`'s lossy `double` won't do. See [custom](../reference/field-types.md#custom).
- **Query-side counterparts.** Maps get `.key("it")` and cross-key search ([Maps](../query/maps.md)); ordered enums get an `Enum` handle whose `.asc()` sorts by declared order ([Enums and custom values](../query/enums-and-values.md)).

## Related

- [Field types](../reference/field-types.md) for the whole type table.
