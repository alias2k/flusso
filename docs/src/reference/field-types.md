# Field types

Every scalar field's type key bridges a Postgres column type and an OpenSearch mapping type, so the mapping is derived with no database.

| Type key | Postgres | OpenSearch | Notes |
| --- | --- | --- | --- |
| `text` | `text`, `varchar` | `text` | Analyzed natural language with `flusso_text`. Descriptions, bios. |
| `identifier` | `text`, `varchar` | `text` | Analyzed identifier-like text with `flusso_code`: `C-01234` is found by `C01234`, `c-01234`, `01234`. Names, SKUs, codes. |
| `keyword` | `text`, `varchar` | `keyword` | Exact match, aggregatable, sortable. |
| `enum` | `text`, `varchar` | `keyword` | A closed string set. Add `variants` for order-correct sort. See [enum](#enum). |
| `uuid` | `uuid` | `keyword` | |
| `boolean` | `boolean` | `boolean` | |
| `short` | `smallint` | `short` | |
| `integer` | `integer` | `integer` | |
| `long` | `bigint` | `long` | |
| `float` | `real` | `float` | |
| `double` | `double precision` | `double` | |
| `decimal` | `numeric`, `money` | `double` | Lossy in storage. Use a `custom` `scaled_float` when exactness matters. |
| `date` | `date` | `date` | |
| `timestamp` | `timestamp`, `timestamptz`, `time` | `date` | |
| `binary` | `bytea` | `binary` | Base64 on the wire. |
| `json` | `json`, `jsonb` | `object` | Opaque; no children declared. |
| `custom` | as declared | as declared | See [custom](#custom). |
| `geo` | see below | `geo_point` | Not a scalar; see [geo](#geo). |

With `auto_subfields` on (the default), `text`/`identifier`/`keyword`/`enum`/`uuid` fields gain analyzer and subfield defaults from the sink. See [Sink: OpenSearch](sink-opensearch.md#analysis-and-subfields). A key set in `options` overrides the default for that field.

## text vs identifier

Both are analyzed `text`; only the analyzer differs. `text` tokenizes on word boundaries and folds case and accents. `identifier` also splits on punctuation and letter/digit boundaries, which is what makes a SKU findable by its fragments. For exact match, sort, or aggregation, use `keyword` instead.

## enum

An `enum` is a keyword for a closed set. Without `variants` it is a plain keyword and sorts alphabetically.

```yaml
- enum: severity
  required: true
  variants: [low, medium, high, critical]
```

- **`variants` bakes the rank into the index.** The field gains a `.sort` subfield whose value is each variant's zero-padded rank, so sorting is a plain field sort, no script. Filtering targets the value itself.
- **Values outside the list** sort after every declared variant.
- **Changing the order** rewrites the mapping, so like any schema change it rotates the index generation and re-seeds.
- **The query side** sorts an `Enum` handle by declared order automatically. See [Enums and custom values](../query/enums-and-values.md).

## custom

For a type the named set doesn't cover, name the OpenSearch type and the Postgres types it accepts:

```yaml
- custom: price
  postgres: [numeric]
  opensearch: scaled_float
  required: false
  options: { scaling_factor: 100 }
```

## geo

A `geo` field is an OpenSearch `geo_point`, in one of two shapes.

**Two columns.** A missing coordinate makes the whole point null, never `{lat: null, lon: null}`, which OpenSearch rejects.

```yaml
- geo: location
  lat: latitude
  lon: longitude
  required: false
```

**One column** already holding a point: a `json`/`jsonb` `{"lat": …, "lon": …}` or `[lon, lat]`, or a `text` `"lat,lon"`.

```yaml
- geo: location
  column: location_json
  required: false
```

PostGIS `geometry` and the native `point` type aren't accepted directly (WKB and `(x,y)` aren't geo_point shapes). Expose a generated `jsonb` or `text` column instead.

## constant

A fixed value in every document. `value` absent or `null` renders as JSON null. It has no source column.

```yaml
- constant: source
  value: "crm"
```

## transforms

Applied in order to a column value before it lands in the document.

| Transform | Effect |
| --- | --- |
| `lowercase` | Lowercase the string. |
| `trim` | Strip leading and trailing whitespace. |

```yaml
- keyword: email
  required: true
  transforms: [trim, lowercase]
```

## Nullability

`required: true` maps a scalar, `geo`, or `map` leaf non-null; the default is nullable. The root `primary_key` field and a join's `primary_key` field are forced non-null. `flusso check` against a database confirms each declaration against the column's `NOT NULL`. On the query side a nullable field must be an `Option<T>`; see [Document structs](../query/document-structs.md#nullability).
