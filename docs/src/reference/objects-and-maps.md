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
| `values` | yes | The leaf type every value shares, and so the type every key is mapped as. One of `text`, `identifier`, `keyword`, `enum`, `uuid`, a numeric type, `date`, `timestamp`. |
| `column` | no | The `json`/`jsonb` column. Defaults to the document key. |
| `required` | no | `true` maps the field non-null. |
| `options` | no | Extra mapping properties. An explicit `dynamic` wins over the injected one. |

The motivating case is translations: `{"en": "…", "it": "…"}` with an open-ended language set.

```yaml
- map: title
  values: text
  required: true
```

`boolean`, `binary`, `json`, `geo`, and `custom` are rejected as `values`.

**Every key is mapped like a plain field of the `values` type.** `title.en` above gets exactly what a `- text: …` field would: the same analyzer, the same automatic subfields, and both follow the sink's `auto_subfields` and `text_analysis`. A `values: double` key is a `double` even when the first value indexed is `3`.

| `values` | Each key (with the default `auto_subfields`) |
| --- | --- |
| `text` | `text`, `flusso_text` analyzer (case + accent folding), `.keyword`, `.keyword_lowercase` |
| `identifier` | as `text`, with the `flusso_code` analyzer |
| `keyword`, `enum`, `uuid` | `keyword`, `.text` (`flusso_code`), `.keyword_lowercase` |
| numeric, `date`, `timestamp` | the declared OpenSearch type |

How: the map itself is an OpenSearch `object` with `dynamic: true`, so runtime keys are accepted despite the index's `dynamic: strict`. The [OpenSearch sink](sink-opensearch.md#mapping) adds one `dynamic_templates` entry per map, matching the map's full path (`title.*`, or `orders.labels.*` inside a nested join), that maps each new key with the plain-field mapping. `flusso check` lists the map as `map<text>`; its JSON output and the designer's mapping preview show the whole key mapping as the map's `values`. Each key costs as many mapped fields as a plain field of its type (three for a string key with the auto subfields), so a map with many distinct keys counts against `index.mapping.total_fields.limit` like as many plain fields; raise it with [`index_settings`](sink-opensearch.md#index-settings).

**A value that doesn't fit is treated like one in a plain field.** OpenSearch coerces what it would coerce for a plain field (a number in a `text` map is indexed as text, a numeric string in a numeric map as the number). What it refuses (a non-numeric string in a numeric map, an object in a `text` map) is an item-level rejection, so the index's [`on_error`](index-and-on-error.md#on_error) policy applies. A dot in a key counts as an object: OpenSearch reads `{"en.US": "Hi"}` as `{"en": {"US": "Hi"}}`, so it's refused too. A string longer than 256 characters isn't indexed in the `.keyword`/`.keyword_lowercase` subfields, as for a plain field.

> ⚠️ **Warning** — before 0.17 the keys took OpenSearch's dynamic defaults. A schema with a `map` gets a new content hash in 0.17, so its index is rebuilt once, and the rebuild can refuse values 0.16 accepted. See [Upgrade to 0.17](../operate/upgrade-to-0-17.md).

On the query side a map gets a typed handle: `.key("it")` returns a plain-field handle of the declared kind, and the map handle runs that kind's operators across every key, with per-key preference. See [Maps](../query/maps.md).

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
