# flusso configuration reference

flusso is driven entirely by declarative files — there is no imperative setup.
A deployment is described by two kinds of file:

| File | Count | Format | Describes |
| --- | --- | --- | --- |
| [`config.toml`](#configtoml) | one per deployment | TOML | the source database, the sink destinations, and which indexes to build |
| [`*.schema.yml`](#schemayml) | one per index | YAML | a single search document — its root table, fields, and how related tables fold in |

`schema::load("config.toml")` reads the config and every schema it references,
validates both layers, and returns one fully-validated `Config`. Schema paths in
`config.toml` are resolved **relative to the config file's directory**.

The supported source and sink **types** — their connection options and behavior
— are documented separately in
[**Sources and sinks**](SOURCES_AND_SINKS.md). This file covers the config
structure and the index document format.

> Two JSON Schemas ship alongside this reference and are the machine-readable
> source of truth for the file formats:
> [`schemas/config.schema.json`](schemas/config.schema.json) and
> [`schemas/index.schema.yml`](schemas/index.schema.yml). Point your editor at
> them for completion and inline validation.

---

## Conventions

### Identifiers

Two distinct identifier rules apply depending on what is being named:

| Rule | Applies to | Pattern | Notes |
| --- | --- | --- | --- |
| **Postgres identifier** | table, column, schema, index, and sink names | `^[a-z_][a-z0-9_]*$`, max 63 chars | Lowercased and trimmed on load, matching Postgres' folding of unquoted identifiers. A name that isn't a valid identifier this way must be addressed explicitly (e.g. set `column:`). |
| **Field name** | the document key a field lands under (`field:`) | `^[a-zA-Z_][a-zA-Z0-9_]*$`, max 63 chars | Case is **preserved** — `field: orderCount` stays camelCase in the emitted document. Only trimmed. |

This split is deliberate: the value comes from a Postgres column (lowercase
identifier) but lands under a document key you choose (which may be camelCase to
suit the search index).

### `env_or_value`

Anywhere a secret or connection string is expected in `config.toml`, you may
give either a literal string or a reference to an environment variable:

```toml
password = "literal-secret"          # literal
password = { env = "OS_PASSWORD" }   # read from $OS_PASSWORD at run time
```

Resolution is **deferred to run time**, not load time: a literal is carried
through, and an `{ env = … }` reference is read in the environment where the
pipeline runs. This is what lets a [compiled artifact](#compiling) travel
without baking in its secrets — and an unset variable fails at run, not compile.
Precedence and the reserved-variable overrides below are applied at that same
point.

### Reserved environment variables

On top of the explicit `{ env = … }` form, a set of **reserved variables** act
as a deployment override layer, so the same `config.toml` works across
environments without edits (the 12-factor pattern). When set, they take
**priority over** the config file:

| Variable | Supplies | Notes |
| --- | --- | --- |
| `DATABASE_URL` | the source connection URL | The source is a singleton, so one well-known name is unambiguous. |
| `<SINK>_OPENSEARCH_URL` | a sink's `url` | `<SINK>` is the **uppercased sink name** — `[sinks.primary]` → `PRIMARY_OPENSEARCH_URL`. |
| `<SINK>_OPENSEARCH_USERNAME` | a sink's `username` | Same naming. |
| `<SINK>_OPENSEARCH_PASSWORD` | a sink's `password` | Same naming. |

Per-sink namespacing means multiple OpenSearch sinks never collide — each reads
its own `PRIMARY_…`, `SECONDARY_…`, etc.

**Precedence**, highest to lowest:

1. An explicit `{ env = "X" }` reference in the config — it names its own source
   and is **never** overridden by a reserved variable. (If `X` is unset, that's
   an error.)
2. The reserved variable, if set — it **overrides** a literal written in the
   file (the override is logged at startup, never silent) and **fills** a value
   omitted from the file.
3. The literal value in the config.
4. else → error, for required values (the source URL, a sink `url`).

```toml
# config.toml ships a default; the deployment overrides via env.
[source]
type = "postgres"
connection_url = "postgres://localhost/dev"   # overridden by $DATABASE_URL if set

[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"                 # overridden by $PRIMARY_OPENSEARCH_URL
# username / password omitted here — supplied by
# $PRIMARY_OPENSEARCH_USERNAME / $PRIMARY_OPENSEARCH_PASSWORD
```

---

## `config.toml`

Top-level table. Only `[source]` is required.

| Key | Required | Description |
| --- | --- | --- |
| `[source]` | **yes** | The database to read from. |
| `[sinks.<name>]` | no | Named destinations. Zero or more; each key is a sink name (a Postgres identifier). |
| `[[index]]` | no | The indexes to build. Zero or more array entries. |

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

Connection options (the full-URL and individual-parts forms, the `DATABASE_URL`
override) and capture behavior live in
[**Sources and sinks**](SOURCES_AND_SINKS.md#sources).

### `[sinks.<name>]`

Named destinations; each key is a sink name (a Postgres identifier) and `type`
selects the kind. Define more than one and flusso **fans out** — every document
is written to all of them. If no sinks are defined, the CLI falls back to a
stdout sink.

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
[**Sources and sinks**](SOURCES_AND_SINKS.md#sinks).

### `[[index]]`

One array entry per index to build.

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | Postgres identifier | yes | The logical index name — the pipeline's stable identity. |
| `schema` | path | yes | Path to the index's `*.schema.yml`, relative to the config file. Must end in `.yml`/`.yaml`. |
| `enabled` | bool | yes | Whether this index is built on this run. |

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
| `fields` | list | **yes** | — | The document's fields. See [Fields](#fields). |

```yaml
version: 1
table: users
schema: public
primary_key: id
```

### `soft_delete`

When set, a row matching the soft-delete condition emits a **tombstone**
(a `delete` to the sink) instead of an upsert. Key it off either a mapped field
or a raw column, and optionally narrow it with `when` filters.

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

### Fields

`fields` is a list whose items take one of two forms.

**Shorthand** — a bare string is a scalar field backed by a column of the same
name:

```yaml
fields:
  - id          # reads column `id`, document key `id`
  - email       # reads column `email`, document key `email`
```

**Full** — an object. `field` (the document key) is the only required property;
the rest are optional and which ones you set determines the field's *source*:

| Key | Type | Description |
| --- | --- | --- |
| `field` | field name | **Required.** The key this field lands under in the document. |
| `column` | Postgres identifier | The source column. Defaults to `field` when omitted. |
| `type` | type name | The field's declared [type](#types). Required on a `sum`/`min`/`max` aggregate; on a column it defaults to `keyword`. Rejected on a group or join (their shape is structural). |
| `required` | bool | Force the field non-null. Fields are nullable by default. |
| `options` | object | Extra OpenSearch mapping properties merged beside the derived `type` (e.g. `analyzer`, `format`, `scaling_factor`). |
| `kind` | `code` \| `prose` | Full-text shorthand for a text field. See [Field kinds](#field-kinds). |
| `transforms` | list | Value transforms to apply. See [Transforms](#transforms). |
| `default` | any | Value to coalesce a `null` column to. |
| `join` | object | Fold a related table in as nested documents (carries its own `fields`). See [Joins](#joins). |
| `aggregate` | object | Reduce a related table to a single value. See [Aggregates](#aggregates). |

#### Field sources

Every field resolves to exactly one source. They are mutually exclusive; the
resolution precedence (when more than one property is present) is:

1. **Relation** — if `join` or `aggregate` is set. Setting *both* is an error
   (`a field cannot have both join and aggregate`).
   - With `join`: the `fields` declared **inside** the join are projected from
     each related row.
   - With `aggregate`: the field is a single reduced scalar.
2. **Column** — otherwise. Reads `column` (or `field` if `column` is omitted),
   applies any `transforms`, and coalesces null to `default` if given.
3. **Constant** — a `default` with no column source renders as a fixed value
   (and `null`/absent renders as JSON null).

```yaml
fields:
  # column source, renamed + transformed + defaulted
  - field: email
    column: email_address
    type: keyword
    transforms: [lowercase, trim]
    default: "unknown@example.com"
```

#### Types

A scalar field declares its **`type`** from a fixed set. Each type bridges a
Postgres column type and an OpenSearch mapping type, so the schema describes the
document fully — flusso derives the index mapping (and validates a config)
without a database. Shorthand fields and columns with no `type` default to
`keyword`.

| `type` | Postgres | OpenSearch | Notes |
| --- | --- | --- | --- |
| `text` | `text`, `varchar` | `text` | Analyzed full-text. |
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

For anything the named types don't cover, declare a **custom** type with the
OpenSearch type and the Postgres types it accepts:

```yaml
- field: price
  type: { custom: { postgres: [numeric], opensearch: scaled_float } }
  options: { scaling_factor: 100 }
```

`options` carries any extra OpenSearch mapping properties (analyzers, formats,
…) merged beside the derived `type`. Groups are `object` and one-to-many joins
are `nested` automatically — their shape is structural, so they take no `type`.

> **Production-ready defaults.** The OpenSearch sink does **not** emit your
> `text`/`keyword` fields bare. By default it attaches a strong analyzer and a
> set of subfields (`keyword`, `keyword_lowercase`, `text`) so search, exact
> filtering, and case-insensitive sort all work out of the box — see
> [Index analysis & subfields](SOURCES_AND_SINKS.md#index-analysis--subfields).
> Anything you put in `options` overrides the default for that field.

#### Field kinds

`kind` is shorthand for a full-text **text** field with the right analyzer —
sugar for `type: text` plus the matching analyzer in `options`:

| `kind` | Equivalent | Use for |
| --- | --- | --- |
| `code` | `type: text`, `options: { analyzer: flusso_code }` | Identifier-like short text — names, SKUs, codes, statuses. Splits on punctuation/case so `C-01234` is found by `C01234`, `c-01234`, or `01234`. |
| `prose` | `type: text`, `options: { analyzer: flusso_text }` | Longer free text — descriptions, bios. Plain tokenize + accent/case folding. |

```yaml
fields:
  - field: sku
    kind: code            # → text + flusso_code, plus the default subfields
  - field: bio
    kind: prose           # → text + flusso_text
```

`kind` only applies to scalar column fields. Setting it alongside a non-`text`
`type` is an error, and an explicit `analyzer` in `options` always wins over the
shorthand. The analyzers themselves are documented in
[Index analysis & subfields](SOURCES_AND_SINKS.md#index-analysis--subfields).

#### Transforms

A list applied in order to a column value before it lands in the document:

| Transform | Effect |
| --- | --- |
| `lowercase` | Lowercase the string value. |
| `trim` | Strip leading/trailing whitespace. |

```yaml
- field: email
  transforms: [trim, lowercase]
```

#### Joins

Fold rows from a related table into the document as nested documents. The
nested `fields` (siblings of `join`) project columns from each related row.

A join is `nested` (or `object` for `one_to_one`) by structure, so it takes no
`type`. Its `fields` — the projection from each related row — and its
`primary_key` live **inside** the `join`. The field reading that `primary_key`
is marked non-null automatically, the same way the root `primary_key` works.

```yaml
- field: orders
  join:
    table: orders
    type: one_to_many
    foreign_key: user_id
    primary_key: id
    order_by:
      - { column: created_at, direction: desc }
    limit: 5
    fields:
      - id
      - field: total
        type: double
      - field: status
        type: keyword
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `table` | Postgres identifier | yes | The related table. |
| `type` | enum | yes | `one_to_one`, `one_to_many`, or `many_to_many`. |
| `primary_key` | Postgres identifier | yes | The related table's primary key. The projected field reading it is forced non-null. |
| `foreign_key` | Postgres identifier | conditional | The FK tying related rows to the parent. **Required** for `one_to_one`/`one_to_many`. |
| `through` | object | conditional | A junction table. **Required** for `many_to_many` (and mutually exclusive with `foreign_key`). |
| `filters` | list | no | [Filters](#filters) narrowing which related rows are folded in. |
| `order_by` | list | no | Ordering — a list of `{ column, direction }`, where `direction` is `asc` (default) or `desc`. |
| `limit` | int ≥ 1 | no | Cap the number of related rows folded in. |
| `fields` | list | yes | The fields projected from each related row. |

**Key arity rule:** a join must specify *exactly one* of `foreign_key` or
`through` — never both, never neither.

The `through` object (junction table for many-to-many):

| Key | Required | Description |
| --- | --- | --- |
| `table` | yes | The junction table. |
| `left_key` | yes | Column joining the junction to the parent. |
| `right_key` | yes | Column joining the junction to the related table. |

```yaml
- field: tags
  join:
    table: tags
    type: many_to_many
    through:
      table: post_tags
      left_key: post_id
      right_key: tag_id
    primary_key: id
    fields: [name]
```

#### Aggregates

Reduce rows from a related table to a single scalar — a count or an extreme.

A `count` is always a non-null `long` and an `avg` a nullable `double`, so they
take no `type`. A `sum`/`min`/`max` mirrors the aggregated column, so it
**must** declare a `type`.

```yaml
- field: orderCount
  aggregate:
    table: orders
    op: count
    foreign_key: user_id

- field: lifetimeValue
  type: double
  aggregate:
    table: orders
    op: sum
    column: total
    foreign_key: user_id
    filters:
      - { column: status, op: eq, value: paid }
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `table` | Postgres identifier | yes | The related table. |
| `op` | enum | yes | `count`, `sum`, `avg`, `min`, or `max`. |
| `column` | Postgres identifier | conditional | The column to reduce. **Required** for `sum`/`avg`/`min`/`max`; ignored by `count`. |
| `foreign_key` | Postgres identifier | conditional | The FK tying related rows to the parent (same `foreign_key` **xor** `through` rule as joins). |
| `through` | object | conditional | Junction table for aggregating across many-to-many. |
| `filters` | list | no | [Filters](#filters) restricting which rows count. |

#### Filters

Filters narrow which related rows a join or aggregate sees (and which rows a
`soft_delete` applies to). Three forms:

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

## Validation, in one place

Loading enforces — beyond what the file format itself can express — that:

- the schema `version` is supported (only `1`);
- all table/column/schema/index/sink names are valid Postgres identifiers, and
  field names are valid field-name identifiers;
- a join specifies **exactly one** of `foreign_key` or `through`;
- `sum`/`avg`/`min`/`max` aggregates carry a `column`;
- a `between` filter has **exactly two** values, and `in`/`not_in` get a list;
- a field is not **both** a `join` and an `aggregate`;
- a scalar's declared `type` is one of the recognized types (or a `custom` one);
- a `sum`/`min`/`max` aggregate declares a `type`, and a `type` is not set on a
  structural group or join.

A failure at any of these stops the load with a specific error naming the cause.
None of it needs a database. When the source **is** reachable,
`flusso check` additionally confirms each declared type and nullability against
the live columns and reports any disagreement.

---

## Compiling

`flusso compile --config config.toml -o flusso.bin` runs everything above and
writes the whole validated configuration — every schema inlined — to a single
binary artifact (MessagePack). Because schemas are self-describing and secrets
are [deferred](#env_or_value), compiling needs no database and bakes in no
secret: `{ env = … }` references travel as references.

`flusso run` with no `--config` loads that artifact and resolves the connection
and credentials in its own environment; `flusso run --config config.toml`
compiles from source and runs that. So a deployment ships one file — no YAML
tree, no source checkout — and the same artifact runs anywhere its environment
provides the secrets.

---

## A complete example

`config.toml`:

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
  - id
  - field: email
    type: keyword
    transforms: [lowercase, trim]
  - field: name
    type: text

  - field: orders
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      primary_key: id
      order_by:
        - { column: id, direction: asc }
      fields:
        - id
        - field: total
          type: double
        - field: status
          type: keyword

  - field: orderCount
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
```

A change to a `users` row — or to any of that user's `orders` — rebuilds the
whole `users` document and re-emits it to every sink. Setting `users.deleted =
true` emits a tombstone instead.
