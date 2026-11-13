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
| `json` | `json`, `jsonb` | `object` | Opaque; no children declared. The strict root refuses any key inside it, so store it with [`indexed: false`](joins.md#indexed) or declare a [`map`](objects-and-maps.md). |
| `custom` | as declared | as declared | See [custom](#custom). |
| `geo` | see below | `geo_point` | Not a scalar; see [geo](#geo). |

With `auto_subfields` on (the default), `text`/`identifier`/`keyword`/`enum`/`uuid` fields gain analyzer and subfield defaults from the sink. See [Sink: OpenSearch](sink-opensearch.md#analysis-and-subfields). A key set in `options` overrides the default for that field, and [`subfields: false`](#subfields) drops the subfields from one field.

| Sibling | Type | Default | Applies to | Meaning |
| --- | --- | --- | --- | --- |
| `subfields` | bool | `true` | `text`, `keyword`, `uuid` | `false` maps the field without the sink's automatic subfields. See [subfields](#subfields). |
| `indexed` | bool | `true` | `json` (and every join) | `false` stores the value without indexing it. See [Joins: indexed](joins.md#indexed). |
| `array` | bool | `false` | every scalar type, `enum`, `custom` | `true` reads a column holding an array of the field's type. See [array](#array). |

A leaf can also be computed from the built document instead of read from a column: see [Script fields](scripts.md). Any field, leaf or not, can be made input-only with [`emit: false`](scripts.md#emit).

## subfields

`subfields: false` maps one string field bare: no `.keyword`, `.keyword_lowercase` or `.text`, whatever the sink's `auto_subfields` says. A `text` field keeps its `flusso_text` analyzer, so it's still full-text searchable.

```yaml
- text: notes
  required: false
  subfields: false
```

- **What it saves.** Each subfield is a mapping field, so a `text`/`keyword` field drops from 3 to 1 toward `index.mapping.total_fields.limit`.
- **What it costs.** No exact filter, aggregation, or sort on a `text` field; no case-insensitive sort or full-text search on a `keyword`. On the query side the handle is `NoSubfields`, so the accessors don't compile; see [Field handles](../query/field-handles.md#subfield-accessors).
- **Only `text`, `keyword`, and `uuid`.** Any other type rejects the key at load, `identifier` and `enum` included even though they get auto subfields too; give those a custom `options: { fields: {} }` to replace the subfields with none. Don't do that on an `enum` with `variants`: the override also drops its `.sort` subfield, so sorting by declared order stops working.
- **Changing it** moves the schema to a new content hash, so the index is rebuilt once.
- **Omitted or `true`** keeps the sink-wide behaviour.

## array

`array: true` declares a column that holds an array of the field's type: a Postgres array (`text[]`, `integer[]`) or a `json`/`jsonb` array.

```yaml
- enum: tags
  variants: [urgent, fragile]
  required: false
  array: true
  transforms: [lowercase]
```

- **Mapping.** Unchanged: OpenSearch has no array type, so the field maps as its element type and every element is indexed. A query matches the document when any element matches.
- **Value.** flusso cleans it up as it builds the document, so a reader always gets an array or null:

  | Column value | Document |
  | --- | --- |
  | `NULL` (or JSON `null`) | `null`, or the `default` |
  | `[]` | `[]` |
  | `["a", null, "b"]` | `["a", "b"]`: null elements are dropped |
  | `"a"` (a lone value in `jsonb`) | `["a"]` |
  | an object or array element | the document fails, and [`on_error`](index-and-on-error.md) decides |
  | a multi-dimensional array (`'{{a,b},{c,d}}'`) | its elements are arrays, so the document fails. Postgres reports a 2-D column as a plain `text[]`, so `check` can't warn |

  Only a `json` field keeps structured elements: every element is kept, objects and nested arrays included, and only top-level nulls are dropped. A `custom` field fails on them like any other, whatever its `opensearch` type.
- **[`transforms`](#transforms)** apply to each string element. They're checked against the element type: the `[]` column's base type, or `text` for `json`/`jsonb`. A `json` array takes none.
- **`default`** is a list, written whole when the column is null: `default: [general]`. Its elements are non-null scalars (any non-null JSON on a `json` field). A scalar `default` with `array: true` is rejected at load.
- **`required`** is about the array: `true` maps `Vec<T>`, `false` `Option<Vec<T>>`. See [Document structs](../query/document-structs.md#arrays).
- **Checked against the database.** `array: true` over a column that's neither a `[]` type nor `json`/`jsonb` is an error that stops `run`; a `[]` column without it is a warning. A domain counts as the type it's declared over. See [check](cli.md#check).
- **Not on** `geo`, `map`, `object`, `constant`, joins, or aggregates: the key is rejected at load.
- **Changing it** moves the schema to a new content hash, so the index is rebuilt once. A schema without it keeps its hash.

A script field takes the same key, for a script that returns an array. See [Script fields](scripts.md).

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

Applied in order to a column value before it lands in the document. Each runs in Postgres (`lower`, `trim`), so the column's type must take it: a text type (`text`, `varchar`, `char`, `name`, `citext`) or a domain over one. Anything else, an `integer`, `jsonb` or an enum, fails every document build; `flusso check` reports it, and [Validation at start](cli.md#validation-at-start) says what `run` does. On an [`array`](#array) field they apply to each element instead, so a `text[]` or `jsonb` column takes them. A `[]` column read without `array: true` gets them on the whole array, which `lower` and `trim` don't take, so every build fails.

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
