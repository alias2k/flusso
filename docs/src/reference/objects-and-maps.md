# Objects and maps

Two ways to nest without reading another table: an `object` groups same-row columns under one key; a `map` exposes a `json`/`jsonb` column as a dynamic-key object with one shared value type.

## object

| Sibling | Required | Meaning |
| --- | --- | --- |
| `fields` | yes | The members, each with its own type key. |
| `options` | no | Extra properties for the `object` mapping. |

An `object` renders as an OpenSearch `object` and is never null; it's always assembled from the same row.

```yaml
- object: address
  fields:
    - keyword: street
      column: address_street
      required: true
    - keyword: city
      column: address_city
      required: true
    - keyword: zip
      column: address_zip
      required: false
```

The document gets `{ "address": { "street": …, "city": …, "zip": … } }`. An `object` differs from a to-one [join](joins.md): the join reads a related table by key and may be null; an object stays on the current row.

## map

| Sibling | Required | Meaning |
| --- | --- | --- |
| `values` | yes | The leaf type every value shares. One of `text`, `identifier`, `keyword`, `enum`, `uuid`, a numeric type, `date`, `timestamp`. |
| `column` | no | The `json`/`jsonb` column. Defaults to the document key. |
| `required` | no | `true` maps the field non-null. |
| `options` | no | Extra mapping properties. An explicit `dynamic` wins over the injected one. |

The motivating case is translations: `{"en": "…", "it": "…"}` with an open-ended language set.

```yaml
- map: title
  values: text
  required: true
```

The field maps to an OpenSearch `object` with `dynamic: true`, so runtime keys are accepted and searchable despite the index's `dynamic: strict`. `boolean`, `binary`, `json`, `geo`, and `custom` are rejected as `values`.

On the query side a map gets a typed handle: `.key("it")` returns a leaf of the declared kind, and a text map offers cross-key search with per-key preference. See [Maps](../query/maps.md).

## Example

```yaml
fields:
  - object: pricing
    fields:
      - custom: amount
        column: price_cents
        postgres: [integer]
        opensearch: scaled_float
        required: true
        options: { scaling_factor: 100 }
      - keyword: currency
        required: true
  - map: name
    values: text
    required: true
```
