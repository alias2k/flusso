# Schema top-level keys

One `*.schema.yml` describes one search document: the root table, the fields, and how related tables fold in.

| Key | Type | Default | Meaning |
| --- | --- | --- | --- |
| `version` | int | — | Format version. Only `1` is accepted. |
| `table` | [Postgres identifier](identifiers.md) | — | The root table the document is built from. |
| `schema` | Postgres identifier | `public` | The database schema holding `table`. |
| `primary_key` | Postgres identifier | none | The root primary-key column. Derives the document id and anchors reverse resolution; every join and aggregate requires it. |
| `doc_id` | Postgres identifier | none | **Not supported.** Setting it is a hard error; the id is always `primary_key`. Kept so older files still parse. |
| `soft_delete` | table | none | Emit a tombstone instead of an upsert for rows matching a condition. See [Filters and soft_delete](filters-and-soft-delete.md#soft_delete). |
| `filters` | list of filters | none | Root filters: only matching rows become documents. See [Filters and soft_delete](filters-and-soft-delete.md#root-filters). |
| `fields` | list of fields | — | The document's fields. See [Fields](#fields). |

Unknown keys are rejected. Editor completion comes from `flusso schema index`, or the published copy at `https://alias2k.github.io/flusso/schemas/latest/index.schema.yml`.

## Fields

`fields` is a list. Each item is **type-first**: exactly one type key whose value is the document key, plus the siblings that type allows.

```yaml
fields:
  - keyword: email          # type key `keyword`, document key `email`
    required: true
  - has_many: orders        # type key `has_many`, document key `orders`
    table: orders
    foreign_key: user_id
    primary_key: id
    fields: [ { decimal: total, required: true } ]
```

| Type key | Kind | Reference |
| --- | --- | --- |
| `text` `identifier` `keyword` `enum` `uuid` `boolean` `short` `integer` `long` `float` `double` `decimal` `date` `timestamp` `binary` `json` `custom` | scalar leaf | [Field types](field-types.md) |
| `geo` | geographic point | [Field types](field-types.md#geo) |
| `object` | same-row sub-object | [Objects and maps](objects-and-maps.md#object) |
| `map` | dynamic-key object | [Objects and maps](objects-and-maps.md#map) |
| `belongs_to` `has_one` `has_many` `many_to_many` | join | [Joins](joins.md) |
| `count` `sum` `avg` `min` `max` `ids` | aggregate | [Aggregates](aggregates.md) |
| `constant` | fixed value | [Field types](field-types.md#constant) |

Which siblings a field accepts depends on its type key:

| Sibling | Applies to | Meaning |
| --- | --- | --- |
| `required` | scalar, `geo`, `map`, to-one join | `true` maps the leaf or object non-null; omitted or `false` means nullable. Rejected on to-many joins and aggregates, whose nullability is structural. |
| `column` | scalar, `geo`, `belongs_to` | The source column. Defaults to the document key. On a `belongs_to`, this table's column pointing at the related row. |
| `options` | anything with a mapping | Extra OpenSearch mapping properties merged beside the derived type (`analyzer`, `format`, `scaling_factor`, …). |
| `transforms` | scalar | Value transforms, in order. See [Field types](field-types.md#transforms). |
| `default` | scalar | Value to coalesce a `null` column to. A string, number, bool, or date; an array, object, or binary default is an error. |
| `variants` | `enum` | The variants in rank order. See [Field types](field-types.md#enum). |
| `postgres`, `opensearch` | `custom` | Accepted Postgres types, emitted OpenSearch type. |
| `lat`, `lon` | `geo` | The two coordinate columns. |
| `values` | `map` | The shared leaf type of every value. Required. |
| `fields` | `object`, joins | The nested projection. |
| `table`, `primary_key`, `column` / `foreign_key` / `through`, `order_by`, `limit`, `filters` | joins | See [Joins](joins.md). |
| `table`, `column`, `value_type`, `element_type`, `foreign_key` / `through`, `filters` | aggregates | See [Aggregates](aggregates.md). |
| `value` | `constant` | The fixed value. |

## Example

```yaml
version: 1
table: users
schema: public
primary_key: id

soft_delete:
  column: deleted

fields:
  - integer: id
    required: false
  - keyword: email
    required: true
    transforms: [lowercase, trim]
  - text: name
    required: false
  - has_many: orders
    table: orders
    foreign_key: user_id
    primary_key: id
    fields:
      - decimal: total
        required: true
  - count: orderCount
    table: orders
    foreign_key: user_id
```
