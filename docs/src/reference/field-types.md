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
| `float` | `real` | `float` | See [numbers](#numbers). |
| `double` | `double precision` | `double` | See [numbers](#numbers). |
| `decimal` | `numeric`, `money` | `double` | `_source` keeps every digit; search, sort and aggregations are `double`. Use a `custom` `scaled_float` when exact search matters. See [numbers](#numbers). |
| `date` | `date` | `date` | Written as `2024-01-02`. See [dates and times](#dates-and-times). |
| `timestamp` | `timestamp`, `timestamptz`, `time` | `date` | Written as ISO 8601, stored to the millisecond. A `time` column's values are refused by OpenSearch. See [dates and times](#dates-and-times). |
| `binary` | `bytea` | `binary` | The column's bytes, as standard base64 with no line breaks: at the root, as [`array: true`](#array) over `bytea[]`, inside joins, and in an `ids`, `min` or `max` aggregate, whatever the server's `bytea_output`, behind a connection pooler too ([the exceptions](source-postgres.md#behind-pgbouncer)). A domain over `bytea`, and an array of one, read the same way. Under a name another field at the same level shares, a `binary` field declared last is read as Postgres hex text instead: see [One name per level](identifiers.md#one-name-per-level). A `bytea` primary key still gives the document `_id` as Postgres hex text (`\x0102`), not base64. Before 0.18 the Postgres hex text (`\x0102…`) was indexed instead; see [Upgrade to 0.18](../operate/upgrade-to-0-18.md). |
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

## dates and times

A `date` or `timestamp` field is mapped as an OpenSearch `date` with its default format, `strict_date_optional_time||epoch_millis`. Each Postgres type is written in a form that format takes, on backfill and on a live change alike, whatever the database's `TimeZone`:

| Postgres | Written as | Notes |
| --- | --- | --- |
| `date` | `2024-01-02` | |
| `timestamp` (without time zone) | `2024-01-02T03:04:05.123456` | No offset, so OpenSearch reads it as UTC. No fraction when the value has none, else 3 or 6 digits: `.5` is written `.500`. Before 0.18 it was written with a space (`2024-01-02 03:04:05`); see [Upgrade to 0.18](../operate/upgrade-to-0-18.md#a-timestamp-column-is-written-in-iso-8601). |
| `timestamptz` | `2024-01-02T03:04:05.123456+00:00` | The UTC instant, fraction as for `timestamp`. |
| an array of any of these | each element as above | With [`array: true`](#array). |

The same form is written wherever the value goes: a `min`/`max` with `value_type: timestamp`, an `ids` of `timestamp`, a script declared `timestamp`, a field inside an [`indexed: false`](joins.md#indexed) join (stored, not mapped), and the `stdout` sink.

- **Precision.** A `date` field stores milliseconds, so `range` and sort see `.123456` as `.123`. `_source` keeps every digit.
- **A `format` of your own.** `options.format` replaces the default, so it must keep a pattern that reads the forms above: put `strict_date_optional_time` first (`strict_date_optional_time||yyyy-MM-dd HH:mm:ss`). A format with none refuses every document holding a value. `flusso check` (`--offline` too) and `run` warn with [`timestamp_format`](cli.md#check) when every pattern is known to refuse the form.
- **A time of day has no date.** A `time` or `timetz` column is accepted under `timestamp`, but the `date` mapping refuses `03:04:05`, so every document with a non-null value is an [item-level rejection](index-and-on-error.md#on_error). With the database, `check` reports it as [`time_of_day_as_date`](cli.md#check), and `run` stops at start, for a `timestamp` or `date` field with no `options` at the root or in a plain object (see [Validation at start](cli.md#validation-at-start)). A field inside a join, or a `min`/`max` over such a column, is refused only for a parent with a related row, so it isn't reported. Two ways out, checked against OpenSearch 2, each leaving `check` with nothing to report. Declare it as a `custom` keyword; `HH:MM:SS` sorts correctly as text:

  ```yaml
  - custom: opensAt
    column: opens_at
    postgres: [time without time zone]   # [time with time zone] for timetz
    opensearch: keyword
    required: false
  ```

  Or keep a `date` mapping with a `timestamp` field (a `date` field takes only a `date` column) whose format reads a time of day with any number of fraction digits, led by the default pattern:

  ```yaml
  - timestamp: opensAt
    column: opens_at
    required: false
    options:
      format: "strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]"
      # timetz: "strict_date_optional_time||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]XXX||HH:mm:ss[.SSSSSS][.SSSSS][.SSSS][.SSS][.SS][.S]X"
  ```

- **Values outside the format.** A BC date, `infinity` and `-infinity`, and a year past 9999 reach the document as Postgres renders them (`0044-03-15 BC`, `infinity`), which the `date` mapping refuses, so that one document is rejected and `on_error` decides.

## numbers

Every number is written as a JSON number. A `numeric` has no JSON number for `NaN` or an infinity, so neither does any number field.

- **`decimal` keeps every digit Postgres stores.** The JSON number is the column's text: `12.50` stays `12.50`, `1234567890.123456789` keeps all 19 digits, and a value past `rust_decimal`'s range (±7.9 × 10^28, or more than 28 fractional digits) is a number too, `1000000000000000000000000000000`. The same goes for a `decimal` [array](#array) element, a `sum`/`min`/`max` with `value_type: decimal`, and an `ids` of `decimal` keys. OpenSearch indexes the field as `double`, so a query, a sort and an aggregation see the nearest `double`; only `_source` is exact. What a Rust consumer reads it into: [What the derive trusts](../query/document-structs.md#what-the-derive-trusts).
- **`float` and `double` are the nearest float**, whatever the column: a `numeric` `1e30` under `double` is written `1e+30`.
- **`NaN`, `Infinity` and `-Infinity` fail the document** under any number field (`short` to `decimal`), a scalar, an array element or an aggregate alike. The document isn't built: the index's [`on_error`](index-and-on-error.md#on_error) decides, on the ingest side, so no sink receives it. Under `skip` every sink keeps the document's previous version. Keep such a value out with a [filter](filters-and-soft-delete.md), or read the column under `keyword`.
- **A number in a `json` field** is a number, the nearest `double` when it is past `rust_decimal`'s range.

## custom

For a type the named set doesn't cover, name the OpenSearch type and the Postgres types it accepts:

```yaml
- custom: price
  postgres: [numeric]
  opensearch: scaled_float
  required: false
  options: { scaling_factor: 100 }
```

A `custom` field indexes the value as Postgres renders it into JSON, unconverted: `opensearch: binary` over a `bytea` column indexes its hex text (`\x0102…`), which isn't base64. Declare a `bytea` column [`binary`](#field-types) to index its bytes.

## geo

A `geo` field is an OpenSearch `geo_point`, in one of two shapes.

**Two columns.** The document holds `{"lat": …, "lon": …}`. A missing coordinate makes the whole point null, never `{lat: null, lon: null}`, which OpenSearch rejects.

```yaml
- geo: location
  lat: latitude
  lon: longitude
  required: false
```

**One column** (`json`, `jsonb`, `text` or `varchar`) already holding a point. The document holds the value as the column holds it, unconverted, so `_source` carries whichever form the column uses. OpenSearch indexes these as a point (an optional altitude `z` is ignored):

| Form | Example | Order |
| --- | --- | --- |
| object | `{"lat": 41.12, "lon": -71.34}` | by name |
| array | `[-71.34, 41.12]` | lon, lat |
| coordinate string | `"41.12,-71.34"` | lat, lon |
| WKT | `"POINT (-71.34 41.12)"` | lon, lat |
| GeoJSON | `{"type": "Point", "coordinates": [-71.34, 41.12]}` | lon, lat |
| geohash | `"drm3btev3e86"` or `{"geohash": "drm3btev3e86"}` | the cell's south-west corner |

A `text` column holds only the string forms; a `json`/`jsonb` column any of them. A value OpenSearch can't read as a point, or with a latitude outside ±90 or a longitude outside ±180, is an item-level rejection, handled by [`on_error`](index-and-on-error.md#on_error). On the query side, `GeoPoint` reads every form of one point (not `POINT EMPTY`, an empty array, or an array of several points); see [Document structs](../query/document-structs.md#flusso-types-to-rust-types).

```yaml
- geo: location
  column: location_json
  required: false
```

PostGIS `geometry` and the native `point` type aren't accepted directly (WKB and `(x,y)` aren't geo_point shapes). Expose a generated `jsonb` or `text` column instead.

## constant

A fixed value in every document, exactly as written. It has no source column.

```yaml
- constant: source
  value: "crm"
```

| `value` | Document | Mapping |
| --- | --- | --- |
| a string | the string | `keyword` |
| a whole number | the number | `long` |
| any other number | the number | `double` |
| `true` / `false` | the boolean | `boolean` |
| absent or `null` | `null` | `keyword`, nullable |
| a list of one kind of scalar | the list | the first element's type; an empty list is `keyword` |
| a list mixing kinds of scalar | the list | `object`, stored but not indexed, like [`indexed: false`](joins.md#indexed): one field takes one kind |
| an object, or a list holding one | the value, every level included | `object`, stored but not indexed: the strict root refuses keys in an `object` with no declared children |

The kinds are string, number and boolean; a `null` element counts as none, and a list inside a list counts with its elements, as OpenSearch flattens it. A list of numbers is one kind when its first number is fractional (`[2.5, 1]` is a `double`) or every number is whole; a fractional number, or one outside the `long` range, after a whole one mixes kinds, because a `long` field would refuse `1e20` and cut `2.5` to `2`. So `[1, 2, null]`, `[2.5, 1]` and `[["a"], "b"]` are one kind; `[1, "n/a"]`, `[true, 1]` and `[1, 2.5]` mix kinds. A list that starts with `null` or an empty list is mapped `keyword`, which takes any scalar. Before 0.18 a number or boolean was indexed as its text (`"5"`) and an object or list as `null`; see [Upgrade to 0.18](../operate/upgrade-to-0-18.md).

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
