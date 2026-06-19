# flusso configuration reference

flusso is driven entirely by declarative files. A deployment is described by two
kinds:

| File | Count | Format | Describes |
| --- | --- | --- | --- |
| [`flusso.toml`](#flussotoml) | one per deployment | TOML | the source database, the sink destinations, and which indexes to build |
| [`*.schema.yml`](#schemayml) | one per index | YAML | a single search document — its root table, fields, and how related tables fold in |

`schema::load("flusso.toml")` is the front door: it reads the config and every
schema it references, validates both layers, and returns one fully-validated
`Config`. Schema paths in `flusso.toml` resolve **relative to the config file's
directory**.

This file covers the config *structure* and the index document *format*. Source
and sink **types** live in [Sources and sinks](SOURCES_AND_SINKS.md); environment
variables (secrets, overrides, CLI-flag twins) in [`CONFIG.md`](CONFIG.md); the
query side in [`CLIENT.md`](CLIENT.md).

> Two JSON Schemas ship alongside this reference and are the machine-readable
> source of truth for the file formats:
> [`config.schema.json`](libs/2-schema/1-config-toml/config.schema.json) and
> [`index.schema.yml`](libs/2-schema/1-index-yaml/index.schema.yml). Point
> your editor at them for completion and inline validation.

---

## `flusso.toml`

Top-level table. Only `[source]` is required.

| Key | Required | Description |
| --- | --- | --- |
| `[source]` | **yes** | The database to read from. |
| `[sinks.<name>]` | no | Named destinations. Zero or more; each key is a sink name (a Postgres identifier). |
| `[[index]]` | no | The indexes to build. Zero or more array entries. |
| `on_error` | no | What to do when a sink rejects a document at the item level: `"stop"` (default) or `"skip"`. See [`on_error`](#on_error). |

### `on_error`

When a sink accepts a flush but rejects a *specific* document — a mapping
conflict, a value the destination can't index — `on_error` decides what happens.
It governs only these **item-level rejections**; a flush-wide failure (the
destination unreachable, the whole request refused) always stops the run.

| Value | Behavior |
| --- | --- |
| `"stop"` (default) | Stop the run. The batch is left unconfirmed and redelivered on restart, so a persistently-bad document halts sync until the data is fixed or the policy changes. Dropping data is opt-in. |
| `"skip"` | Quarantine the document (logged, counted in `flusso.documents.quarantined` and the `/status` `documents_quarantined`) and continue. The rest of the batch is applied and acked; the document never lands until its source row changes again. |

A global `on_error` is the default for every index; override it per index with
`on_error` inside an `[[index]]` entry. The policy is operational, not part of the
document shape, so changing it never triggers a reindex.

```toml
on_error = "stop"   # global default

[[index]]
name = "analytics"
schema = "analytics.schema.yml"
enabled = true
on_error = "skip"   # this index tolerates bad rows
```

### `[source]`

The database documents are read from — one per deployment. `type` selects the
kind:

| `type` | Reference |
| --- | --- |
| `postgres` | [Postgres source](SOURCES_AND_SINKS.md#postgres) |

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

Connection options (full-URL and individual-parts forms, the `DATABASE_URL`
override) and capture behavior live in
[Sources and sinks](SOURCES_AND_SINKS.md#sources) and [`CONFIG.md`](CONFIG.md).

### `[sinks.<name>]`

Named destinations; each key is a sink name (a Postgres identifier) and `type`
selects the kind. Define more than one and flusso **fans out** — every document
is written to all of them. If no sinks are defined, the CLI falls back to a stdout
sink.

| `type` | Reference |
| --- | --- |
| `opensearch` | [OpenSearch sink](SOURCES_AND_SINKS.md#opensearch) |
| `stdout` | [Stdout sink](SOURCES_AND_SINKS.md#stdout) |

```toml
[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[sinks.audit]
type = "stdout"
pretty = true
```

Each type's full option set and behavior is documented in
[Sources and sinks](SOURCES_AND_SINKS.md#sinks).

### `[[index]]`

One array entry per index to build.

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | Postgres identifier | yes | The logical index name — the pipeline's stable identity. |
| `schema` | path | yes | Path to the index's `*.schema.yml`, relative to the config file. Must end in `.yml`/`.yaml`. |
| `enabled` | bool | yes | Whether this index is built on this run. |
| `on_error` | `"stop"` \| `"skip"` | no | Override the global [`on_error`](#on_error) for this index. Omitted inherits the global default. |

```toml
[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true
```

---

## `*.schema.yml`

Each schema file describes **one** search document: the root table it is built
from, the fields it contains, and how related tables fold in.

### Top-level keys

| Key | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `version` | int | **yes** | — | Schema format version. Only `1` is supported. |
| `table` | Postgres identifier | **yes** | — | The root table the document is built from. |
| `schema` | Postgres identifier | no | `public` | The database schema the root table lives in. |
| `primary_key` | Postgres identifier | no | — | The root table's primary-key column. Used to derive the document id and to resolve which documents a related-row change affects. Relations and reverse-resolution require it. |
| `doc_id` | string | no | — | Column whose value becomes the document id. Parsed and validated by the schema layer; the current Postgres source derives the id from `primary_key`. |
| `soft_delete` | object | no | — | Treat rows as deleted based on a column/field rather than a physical `DELETE`. See [below](#soft_delete). |
| `filters` | list | no | — | Root filters: only root rows matching every filter become documents. See [below](#root-filters). |
| `fields` | list | **yes** | — | The document's fields. See [Fields](#fields). |

```yaml
version: 1
table: users
schema: public
primary_key: id
```

### soft_delete

When `soft_delete` is set, a row matching the soft-delete condition emits a
**tombstone** (a `delete` to the sink) instead of an upsert. Key it off either a
mapped field or a raw column, and optionally narrow it with `when` filters.

```yaml
# Off a column: users.deleted = true → delete.
soft_delete:
  column: deleted

# Off a mapped field, narrowed to a subset of rows.
soft_delete:
  field: status
  when:
    - { column: archived, op: eq, value: true }
```

| Key | Required | Description |
| --- | --- | --- |
| `column` **or** `field` | exactly one | The column (Postgres identifier) or mapped field (field name) signalling deletion. |
| `when` | no | A list of [filters](#filters); the soft-delete applies only to matching rows. |

### Root filters

When only a *subset* of a table should be an index, a top-level `filters` list
(same [filter forms](#filters) as joins use) scopes which root rows become
documents:

```yaml
version: 1
table: item
primary_key: id
filters:
  - { column: item_type, op: eq, value: serialized }
  - { column: archived_at, op: is_null }
```

A row outside the set never produces a document; a row that *leaves* the set (an
`UPDATE` that stops matching) emits a **tombstone** on its next rebuild, exactly
like [`soft_delete`](#soft_delete) — both fold into the document query's `WHERE`,
so "no row came back" means "this document should not exist". A row that enters
the set upserts. Backfill walks the whole root table and lets the same predicate
decide, so filtered-out rows cost a no-op delete during seeding.

---

## Fields

`fields` is a list. Each item is written **type-first**: a single **type key**
whose value is the document key, plus the siblings that type allows.

```yaml
fields:
  - keyword: email        # a `keyword` scalar; document key `email`
    required: true
  - text: bio             # analyzed full text
    required: false
  - integer: age
    required: false
```

The type key is one of:

- a **scalar** type — `text`, `identifier`, `keyword`, `enum`, `uuid`,
  `boolean`, `short`, `integer`, `long`, `float`, `double`, `decimal`, `date`,
  `timestamp`, `binary`, `json` (see [Types](#types)) — or `custom` (an explicit
  Postgres/OpenSearch pair);
- `geo` — a geographic point (see [Geo points](#geo-points));
- `object` — a same-row sub-object (see [Objects](#objects));
- `belongs_to` / `has_one` / `has_many` / `many_to_many` — a related table
  folded in (see [Joins](#joins));
- `count` / `sum` / `avg` / `min` / `max` — a rollup over a related table (see
  [Aggregates](#aggregates));
- `constant` — a fixed value.

There is exactly one type key per field. Which siblings a field accepts depends on
that type:

| Sibling | Applies to | Description |
| --- | --- | --- |
| `required` | scalar, `geo` | **Mandatory** on a scalar/geo leaf. `true` forces the field non-null; nullable otherwise. Joins and aggregates are structural — their nullability is fixed — so they take no `required`. |
| `column` | scalar, `geo`, `belongs_to` | The source column — for a `belongs_to`, this table's column pointing at the related row. Defaults to the document key when omitted. |
| `options` | types with a mapping | Extra OpenSearch mapping properties merged beside the derived type (e.g. `analyzer`, `format`, `scaling_factor`). |
| `transforms` | scalar | Value transforms to apply. See [Transforms](#transforms). |
| `default` | scalar | Value to coalesce a `null` column to. |
| `postgres` / `opensearch` | `custom` | The Postgres types accepted and the OpenSearch type emitted. |
| `lat` / `lon` | `geo` | The two coordinate columns (two-column form). |
| `fields` | `object`, joins | The nested projection. |
| `table`, `primary_key`, `column`/`foreign_key`/`through`, `order_by`, `filters`, `limit` | joins | Which key sibling applies depends on the verb. See [Joins](#joins). |
| `table`, `column`, `value_type`, `element_type`, `foreign_key`, `through`, `filters` | aggregates | See [Aggregates](#aggregates). |
| `value` | `constant` | The fixed value (`null`/absent renders as JSON null). |

```yaml
fields:
  # column source, renamed + transformed + defaulted
  - keyword: email
    column: email_address
    required: false
    transforms: [lowercase, trim]
    default: "unknown@example.com"
```

### Objects

An `object` nests sibling columns of the **same row** under one document key,
without reading a related table. It renders as an OpenSearch `object` and is never
null; its members declare their own types.

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

→ `{ "address": { "street": …, "city": …, "zip": … } }`, all from one row.

An `object` differs from a to-one [join](#joins) (`belongs_to`/`has_one`): the
join reads a *related table* by key, an object stays put on the current row.
Optional `options` pass extra properties to the `object` mapping.

### Types

A scalar field declares its **`type`** from a fixed set. Each type bridges a
Postgres column type and an OpenSearch mapping type, so the schema describes the
document fully — flusso derives the index mapping (and validates a config) without
a database. Shorthand fields and columns with no `type` default to `keyword`.

| `type` | Postgres | OpenSearch | Notes |
| --- | --- | --- | --- |
| `text` | `text`, `varchar` | `text` | Analyzed natural-language full text (descriptions, bios) — the default analyzed type. Plain tokenize + accent/case fold. |
| `identifier` | `text`, `varchar` | `text` | Analyzed identifier-like short text (names, SKUs, codes, statuses) — splits on punctuation/case so `C-01234` is found by `C01234`, `c-01234`, or `01234`. |
| `keyword` | `text`, `varchar` | `keyword` | Exact, aggregatable. |
| `enum` | `text`, `varchar`, PG enum | `keyword` | A closed string set stored as text, indexed exactly. |
| `uuid` | `uuid` | `keyword` | |
| `boolean` | `boolean` | `boolean` | |
| `short` | `smallint` / `int2` | `short` | |
| `integer` | `integer` / `int4` | `integer` | |
| `long` | `bigint` / `int8` | `long` | |
| `float` | `real` / `float4` | `float` | |
| `double` | `double precision` / `float8` | `double` | |
| `decimal` | `numeric` / `money` | `double` | Lossy; use a `custom` `scaled_float` when exactness matters. |
| `date` | `date` | `date` | |
| `timestamp` | `timestamp(tz)`, `time` | `date` | |
| `binary` | `bytea` | `binary` | |
| `json` | `json`, `jsonb` | `object` | |

(A geographic point is a `geo` field, not a scalar `type` — see
[Geo points](#geo-points).)

For anything the named types don't cover, declare a **`custom`** field with the
OpenSearch type and the Postgres types it accepts:

```yaml
- custom: price
  postgres: [numeric]
  opensearch: scaled_float
  required: false
  options: { scaling_factor: 100 }
```

`options` carries any extra OpenSearch mapping properties (analyzers, formats, …)
merged beside the derived type. Objects, joins, aggregates, and geo points carry
their own type keys rather than a scalar type; their shape is structural.

> **Production-ready defaults.** The OpenSearch sink does **not** emit your
> `text`/`keyword` fields bare. By default it attaches a strong analyzer and a set
> of subfields (`keyword`, `keyword_lowercase`, `text`) so search, exact
> filtering, and case-insensitive sort all work out of the box — see
> [Index analysis & subfields](SOURCES_AND_SINKS.md#index-analysis--subfields).
> Anything in `options` overrides the default for that field.

#### `text` vs `identifier`

Both are analyzed (full-text searchable) `text` fields; they differ only in the
analyzer:

- **`text`** — natural language (descriptions, bios, comments). The default
  analyzed type; tokenizes on word boundaries with accent/case folding.
- **`identifier`** — short structured strings (names, SKUs, codes, statuses).
  Splits on punctuation/case so `C-01234` is found by `C01234`, `c-01234`, or
  `01234`.

```yaml
fields:
  - text: bio             # natural-language analyzer + default subfields
    required: false
  - identifier: sku       # punctuation/case-splitting analyzer + default subfields
    required: false
```

(Use `keyword` instead for exact match, sort, or aggregation rather than full-text
search.) Both apply only to scalar column fields, and an explicit `analyzer` in
`options` always wins over the type's default. The analyzers themselves are
documented in
[Index analysis & subfields](SOURCES_AND_SINKS.md#index-analysis--subfields).

### Geo points

A `geo` field is a geographic point → OpenSearch `geo_point`. Two forms:

**Two columns** — a latitude and a longitude column assembled into a point. A
missing coordinate makes the whole point null (never `{lat: null, lon: null}`,
which OpenSearch rejects):

```yaml
- geo: location
  lat: latitude
  lon: longitude
  required: false
```

**Single column** — a column already holding a `geo_point`-shaped value: a
`json`/`jsonb` `{"lat": …, "lon": …}` or `[lon, lat]`, or a `text` `"lat,lon"`:

```yaml
- geo: location
  column: location_json
  required: false
```

PostGIS `geometry` and PG-native `point` aren't accepted directly (they serialize
as WKB / `(x,y)`, which OpenSearch won't take); expose a generated `jsonb`/`text`
column in one of the shapes above. The two-column form needs no such column —
flusso assembles the point in the document query.

### Transforms

A list applied in order to a column value before it lands in the document:

| Transform | Effect |
| --- | --- |
| `lowercase` | Lowercase the string value. |
| `trim` | Strip leading/trailing whitespace. |

```yaml
- keyword: email
  required: false
  transforms: [trim, lowercase]
```

### Joins

Fold rows from a related table into the document as nested documents. The join's
**relationship verb is its type key**, and the verb names **which table holds the
key.**

| Type key | The key lives on… | Reads as | Renders as |
| --- | --- | --- | --- |
| `belongs_to` | **this** table (`column`) | "my `column` points at the related row" | object (nullable) |
| `has_one` | the **related** table (`foreign_key`) | "one related row points back at me" | object (nullable) |
| `has_many` | the **related** table (`foreign_key`) | "many related rows point back at me" | nested array (never null) |
| `many_to_many` | a junction table (`through`) | "we connect through a junction" | nested array (never null) |

The `fields` — the projection from each related row — and the related table's
`primary_key` are siblings of the type key. The field reading that `primary_key`
is marked non-null automatically, like the root `primary_key`.

```yaml
# My column points at them: embed the user a `created_by` column references.
# `column` defaults to the field name — here, the FK column IS `created_by`.
- belongs_to: created_by
  table: users
  primary_key: id
  fields:
    - keyword: email
      required: true
    - text: name
      required: false

# Their column points at me: fold in the rows holding my key.
- has_many: orders
  table: orders
  foreign_key: user_id
  primary_key: id
  order_by:
    - { column: created_at, direction: desc }
  limit: 5
  fields:
    - integer: id
      required: false
    - double: total
      required: true
    - keyword: status
      required: true
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| *(type key)* | field name | yes | `belongs_to`, `has_one`, `has_many`, or `many_to_many`; its value is the document key. |
| `table` | Postgres identifier | yes | The related table. |
| `primary_key` | Postgres identifier | yes | The related table's primary key. The projected field reading it is forced non-null. |
| `column` | Postgres identifier | `belongs_to` only | **This** table's column pointing at the related row. Defaults to the field name (so `belongs_to: created_by` reads the `created_by` column). |
| `foreign_key` | Postgres identifier | `has_one`/`has_many` | The **related** table's column pointing back at the parent. |
| `through` | object | `many_to_many` | A junction table. |
| `filters` | list | no | [Filters](#filters) narrowing which related rows are folded in. |
| `order_by` | list | no | Ordering — a list of `{ column, direction }`, where `direction` is `asc` (default) or `desc`. Not allowed on `belongs_to` (its target is unique); on `has_one` it picks *which* row becomes the object. |
| `limit` | int ≥ 1 | `has_many`/`many_to_many` only | Cap the number of related rows folded in (the to-one verbs imply their own `LIMIT 1`). |
| `fields` | list | yes | The fields projected from each related row. |

**Key arity rule:** a join takes *exactly* the key sibling its verb implies —
`column` for `belongs_to`, `foreign_key` for `has_one`/`has_many`, `through` for
`many_to_many`. Anything else is a load-time error naming the right one.

A `belongs_to` target that changes — or is deleted — re-emits every document
pointing at it: flusso finds the referrers on the parent table itself
(`WHERE column = <changed key>`), so a deleted target rebuilds those documents with
a null object rather than leaving them stale.

The `through` object (junction table for many-to-many):

| Key | Required | Description |
| --- | --- | --- |
| `table` | yes | The junction table. |
| `left_key` | yes | Column joining the junction to the parent. |
| `right_key` | yes | Column joining the junction to the related table. |

```yaml
- many_to_many: tags
  table: tags
  through:
    table: post_tags
    left_key: post_id
    right_key: tag_id
  primary_key: id
  fields:
    - keyword: name
      required: true
```

### Aggregates

Reduce rows from a related table to a single value. The **operation is the type
key**: `count`, `sum`, `avg`, `min`, `max`, or `ids`.

A `count` is always a non-null `long` and an `avg` a nullable `double`, so they
take no `value_type`. A `sum`/`min`/`max` mirrors the aggregated column, so it
**must** declare a `column` and a `value_type`.

```yaml
- count: orderCount
  table: orders
  foreign_key: user_id

- sum: lifetimeValue
  table: orders
  column: total
  value_type: decimal
  foreign_key: user_id
  filters:
    - { column: status, op: eq, value: paid }
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| *(type key)* | field name | yes | `count`/`sum`/`avg`/`min`/`max`/`ids`; its value is the document key. |
| `table` | Postgres identifier | yes | The related table. |
| `column` | Postgres identifier | conditional | The column to reduce. **Required** for `sum`/`avg`/`min`/`max`; not used by `count`/`ids`. |
| `value_type` | type name | conditional | The result type. **Required** for `sum`/`min`/`max` (it mirrors the column); not used by `count`/`avg`/`ids`. |
| `element_type` | type name | conditional | **Required** for `ids` (and only `ids`): the scalar type of each collected primary key — `long` or `keyword`. |
| `foreign_key` | Postgres identifier | conditional | The aggregated table's column pointing back at the parent (exactly one of `foreign_key` **xor** `through`). |
| `through` | object | conditional | Junction table for aggregating across many-to-many. |
| `filters` | list | no | [Filters](#filters) restricting which rows count. |

#### `ids` — a flat array of a related table's primary keys

`ids` collects the related table's **primary key** into a flat scalar array (it
takes no `column` — the key is always the related table's PK). OpenSearch has no
array type, so the field's mapping type is just the element type
(`element_type: long` → `type: long`, `keyword` → `type: keyword`); the value is
multi-valued. An empty relation yields `[]`, never null, so the field is non-null
(project it as a bare `Vec<…>`, not `Option<Vec<…>>`).

```yaml
# one-to-many: orders.user_id points back at this row
- ids: orderIds
  table: orders
  foreign_key: user_id
  element_type: long

# many-to-many: collected straight off the junction's right_key
- ids: tagIds
  table: tags
  through: { table: post_tags, left_key: post_id, right_key: tag_id }
  element_type: long
```

### Filters

Filters narrow which related rows a join or aggregate sees, which rows a
`soft_delete` applies to, and — as the top-level [`filters` key](#root-filters) —
which root rows become documents at all. Three forms:

**Raw SQL** — an escape hatch for conditions the structured forms can't express:

```yaml
- { raw: "amount > 0 AND currency = 'USD'" }
```

**Null check** — no value operand:

```yaml
- { column: deleted_at, op: is_null }
- { column: confirmed_at, op: is_not_null }
```

**Value comparison** — operator plus a value whose shape matches its arity:

```yaml
- { column: status, op: eq,  value: paid }
- { column: status, op: in,  value: [paid, shipped] }
- { column: total,  op: between, value: [10, 100] }
```

| `op` | Value shape |
| --- | --- |
| `eq`, `neq`, `lt`, `lte`, `gt`, `gte`, `like`, `ilike` | a single scalar |
| `in`, `not_in` | a list |
| `between` | a list of **exactly two** values `[lower, upper]` |
| `is_null`, `is_not_null` | *(no value)* |

A value op with a missing value, a list op given a scalar, or a `between` with
other than two values is a load-time error.

---

## Conventions

### Identifiers

Two distinct identifier rules apply depending on what is being named:

| Rule | Applies to | Pattern | Notes |
| --- | --- | --- | --- |
| **Postgres identifier** | table, column, schema, index, and sink names | `^[a-z_][a-z0-9_]*$`, max 63 chars | Lowercased and trimmed on load, matching Postgres' folding of unquoted identifiers. A name that isn't a valid identifier this way must be addressed explicitly (e.g. set `column:`). |
| **Field name** | the document key a field lands under (`field:`) | `^[a-zA-Z_][a-zA-Z0-9_]*$`, max 63 chars | Case is **preserved** — `field: orderCount` stays camelCase in the emitted document. Only trimmed. |

The split is deliberate: the value comes from a Postgres column (lowercase
identifier) but lands under a document key you choose (which may be camelCase to
suit the search index).

### env_or_value

Anywhere a secret or connection string is expected in `flusso.toml`, give either a
literal string or a reference to an environment variable:

```toml
password = "literal-secret"          # literal
password = { env = "OS_PASSWORD" }   # read from $OS_PASSWORD at run time
```

Either form is accepted wherever this doc says a value is an `env_or_value`.
Resolution is **deferred to run time** — which lets a [compiled
artifact](#compiling) travel without baking in its secrets. The full story
(resolution timing, the reserved deployment-override variables, the precedence
rules) lives in [`CONFIG.md`](CONFIG.md#secret--connection-values).

---

## Validation, in one place

Loading enforces — beyond what the file format itself can express — that:

- the schema `version` is supported (only `1`);
- all table/column/schema/index/sink names are valid Postgres identifiers, and
  field names are valid field-name identifiers;
- each field has **exactly one** type key, and only the siblings that type allows;
- a join carries exactly the key sibling its verb implies — `column` for
  `belongs_to` (defaulting to the field name), `foreign_key` for
  `has_one`/`has_many`, `through` for `many_to_many` — and the to-one verbs take
  no `limit` (nor `order_by`, for `belongs_to`);
- an aggregate specifies **exactly one** of `foreign_key` or `through`;
- `sum`/`avg`/`min`/`max` aggregates carry a `column`, and `sum`/`min`/`max` also
  declare a `value_type` (it mirrors the column);
- an `ids` aggregate declares an `element_type` (a scalar type) and takes no
  `column` or `value_type`; `element_type` is rejected on every other op;
- a `geo` field gives either `lat` **and** `lon`, or a single `column`;
- a `between` filter has **exactly two** values, and `in`/`not_in` get a list.

A failure at any of these stops the load with a specific error naming the cause.
None of it needs a database. When the source **is** reachable, `flusso check`
additionally confirms each declared type and nullability against the live columns
and reports any disagreement.

---

## Compiling

`flusso build --config config.toml -o flusso.lock` runs everything above and
writes the whole validated configuration — every schema inlined — to a single
binary artifact (MessagePack). Because schemas are self-describing and secrets are
[deferred](#env_or_value), compiling needs no database and bakes in no secret:
`{ env = … }` references travel as references, not values.

`flusso run` with no `--config` loads that artifact and resolves the connection
and credentials in its own environment; `flusso run --config flusso.toml` compiles
from source and runs that. So a deployment ships one file — no YAML tree, no source
checkout — and the same artifact runs anywhere its environment provides the
secrets.

---

## A complete example

`flusso.toml`:

```toml
[source]
type = "postgres"
connection_url = { env = "DATABASE_URL" }

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }

[sinks.audit]
type = "stdout"
pretty = true

[[index]]
name = "users"
schema = "users.schema.yml"
enabled = true
```

`users.schema.yml`:

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
    order_by:
      - { column: id, direction: asc }
    fields:
      - integer: id
        required: false
      - double: total
        required: true
      - keyword: status
        required: true

  - count: orderCount
    table: orders
    foreign_key: user_id
```

A change to a `users` row — or to any of that user's `orders` — rebuilds the whole
`users` document and re-emits it to every sink. Setting `users.deleted = true`
emits a tombstone instead.
