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
give either a literal string or a reference to an environment variable resolved
at load time:

```toml
password = "literal-secret"          # literal
password = { env = "OS_PASSWORD" }   # read from $OS_PASSWORD at load time
```

If the named variable is unset, loading fails with a clear error.

---

## `config.toml`

Top-level table. Only `[source]` is required.

| Key | Required | Description |
| --- | --- | --- |
| `[source]` | **yes** | The database to read from. |
| `[sinks.<name>]` | no | Named destinations. Zero or more; each key is a sink name (a Postgres identifier). |
| `[[index]]` | no | The indexes to build. Zero or more array entries. |

### `[source]`

The only `type` today is `postgres`.

```toml
[source]
type = "postgres"
connection_url = "postgresql://user:pass@localhost:5432/mydb"
```

`connection_url` takes one of two shapes:

**A full URL** (string or `env_or_value`). Must match
`^(postgresql|postgres)://…`:

```toml
connection_url = { env = "DATABASE_URL" }
```

**Individual parts** (a table). `database` is required; the rest default:

```toml
[source.connection_url]
host     = "127.0.0.1"   # default 127.0.0.1
port     = 5432          # default 5432
user     = "postgres"    # default postgres
password = "secret"      # optional
database = "mydb"        # required
```

### `[sinks.<name>]`

Each entry under `[sinks]` is a named destination; its `type` selects the kind.
Define more than one and flusso **fans out** — every document is written to all
of them. If no sinks are defined, the CLI falls back to a stdout sink.

#### `type = "opensearch"`

Writes documents to an OpenSearch cluster via the bulk API.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `url` | `env_or_value` | — (**required**) | Base URL of the cluster, e.g. `https://search.example.com:9200`. |
| `username` | `env_or_value` | — | HTTP Basic Auth username. |
| `password` | `env_or_value` | — | HTTP Basic Auth password. |
| `tls_verify` | bool | `true` | Verify TLS certificates. Set `false` only for local development. |
| `batch_size` | int ≥ 1 | `1000` | Maximum documents per bulk-request chunk. |
| `max_bytes` | int | `10485760` (10 MiB) | Maximum bytes per bulk chunk; within OpenSearch's recommended 5–15 MB range. |
| `timeout_secs` | int ≥ 1 | `30` | HTTP request timeout, in seconds. |
| `max_retries` | int ≥ 0 | `3` | Additional retry attempts on transient failures (exponential backoff). |
| `pipeline` | string | — | Optional OpenSearch ingest pipeline to apply on every index operation. |

```toml
[sinks.primary]
type = "opensearch"
url = "https://localhost:9200"
password = { env = "OS_PASSWORD" }
batch_size = 2000
```

The OpenSearch sink owns each index: it creates it up front from the resolved,
fully-typed mapping (`dynamic: strict`), and the physical index is named
`{logical}_{hash}` where the hash derives from the parsed schema — so a
structural schema change writes to a fresh index rather than into a mismatched
shape.

#### `type = "stdout"`

Writes each operation to standard output as a JSON envelope — handy for
development and for piping into `jq`.

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `pretty` | bool | `false` | Pretty-print JSON instead of compact one-line NDJSON. |

```toml
[sinks.audit]
type = "stdout"
pretty = true
```

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
| `mapping` | object | The OpenSearch mapping for this field. See [Mappings](#mappings). |
| `transforms` | list | Value transforms to apply. See [Transforms](#transforms). |
| `default` | any | Value to coalesce a `null` column to. |
| `join` | object | Fold a related table in as nested documents. See [Joins](#joins). |
| `aggregate` | object | Reduce a related table to a single value. See [Aggregates](#aggregates). |
| `fields` | list | Nested fields — a sub-object (group) or a join's projection. |

#### Field sources

Every field resolves to exactly one source. They are mutually exclusive; the
resolution precedence (when more than one property is present) is:

1. **Relation** — if `join` or `aggregate` is set. Setting *both* is an error
   (`a field cannot have both join and aggregate`).
   - With `join`: the field's nested `fields` are projected from each related row.
   - With `aggregate`: the field is a single reduced scalar.
2. **Group** — if there is no `column` but there *are* nested `fields`. Builds a
   sub-object from sibling values of the **same** row (adds a nesting level
   without reading a related table).
3. **Column** — otherwise. Reads `column` (or `field` if `column` is omitted),
   applies any `transforms`, and coalesces null to `default` if given.
4. **Constant** — a `default` with no column source renders as a fixed value
   (and `null`/absent renders as JSON null).

```yaml
fields:
  # column source, renamed + transformed + defaulted
  - field: email
    column: email_address
    transforms: [lowercase, trim]
    default: "unknown@example.com"

  # group: a same-row sub-object
  - field: address
    fields:
      - { field: city,    column: city }
      - { field: zip,     column: postal_code }
```

#### Mappings

`mapping` declares the OpenSearch field type. `type` is required; **every other
property is passed through as-is** to the destination mapping, so you can set
analyzers, formats, sub-fields, etc.

```yaml
- field: email
  mapping: { type: keyword }

- field: created_at
  mapping:
    type: date
    format: "strict_date_optional_time||epoch_millis"
```

Recognized `type` values (any other string is passed through verbatim):

`text`, `keyword`, `boolean`, `byte`, `short`, `integer`, `long`, `float`,
`double`, `half_float`, `scaled_float`, `date`, `object`, `nested`.

Where a field has no explicit `mapping`, the source infers the type from the
database column. Use `object` for groups and `nested` for one-to-many joins.

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

```yaml
- field: orders
  mapping: { type: nested }
  join:
    table: orders
    type: one_to_many
    foreign_key: user_id
    order_by:
      - { column: created_at, direction: desc }
    limit: 5
  fields: [id, total, status]
```

| Key | Type | Required | Description |
| --- | --- | --- | --- |
| `table` | Postgres identifier | yes | The related table. |
| `type` | enum | yes | `one_to_one`, `one_to_many`, or `many_to_many`. |
| `foreign_key` | Postgres identifier | conditional | The FK tying related rows to the parent. **Required** for `one_to_one`/`one_to_many`. |
| `through` | object | conditional | A junction table. **Required** for `many_to_many` (and mutually exclusive with `foreign_key`). |
| `filters` | list | no | [Filters](#filters) narrowing which related rows are folded in. |
| `order_by` | list | no | Ordering — a list of `{ column, direction }`, where `direction` is `asc` (default) or `desc`. |
| `limit` | int ≥ 1 | no | Cap the number of related rows folded in. |

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
  mapping: { type: keyword }
  join:
    table: tags
    type: many_to_many
    through:
      table: post_tags
      left_key: post_id
      right_key: tag_id
  fields: [name]
```

#### Aggregates

Reduce rows from a related table to a single scalar — a count or an extreme.

```yaml
- field: orderCount
  mapping: { type: integer }
  aggregate:
    table: orders
    op: count
    foreign_key: user_id

- field: lifetimeValue
  mapping: { type: scaled_float }
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
- referenced environment variables (`{ env = "…" }`) are set.

A failure at any of these stops the load with a specific error naming the cause.

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
    mapping: { type: keyword }
    transforms: [lowercase, trim]
  - field: name
    mapping: { type: text }

  - field: orders
    mapping: { type: nested }
    join:
      table: orders
      type: one_to_many
      foreign_key: user_id
      order_by:
        - { column: id, direction: asc }
    fields: [id, total, status]

  - field: orderCount
    mapping: { type: integer }
    aggregate:
      table: orders
      op: count
      foreign_key: user_id
```

A change to a `users` row — or to any of that user's `orders` — rebuilds the
whole `users` document and re-emits it to every sink. Setting `users.deleted =
true` emits a tombstone instead.
