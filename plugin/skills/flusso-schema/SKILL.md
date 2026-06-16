---
name: flusso-schema
description: Author or edit a flusso index schema (*.schema.yml) or deployment config (flusso.toml). Use whenever creating, editing, or reviewing a flusso schema file — the type-first field syntax, joins, aggregates, geo points, filters, soft-delete, and how to validate. Trigger on any *.schema.yml or flusso.toml work.
---

# Authoring flusso schemas

flusso syncs OpenSearch from Postgres off declarative files. You write two kinds:

- **`flusso.toml`** — one per deployment: the source DB, the sink(s), which indexes to build.
- **`*.schema.yml`** — one per index: a single search document — its root table, fields, and how related tables fold in.

`schema::load("flusso.toml")` reads the config and every schema it references, validates both, and produces the index mapping with no database needed.

## First: get live validation, never guess the format

The binary emits the authoritative JSON Schemas. Generate them and wire your editor to them:

```sh
flusso schema index  > index.schema.yml   # the *.schema.yml format
flusso schema config > config.schema.json # the flusso.toml format
```

Then add the language-server line to the **top of every `*.schema.yml`**:

```yaml
# yaml-language-server: $schema=./index.schema.yml
```

Validate the whole deployment before declaring done:

```sh
flusso check --config flusso.toml            # validates + prints the typed mapping
flusso check --config flusso.toml --offline  # skip the DB; format/rules only
```

`check` against a live DB also confirms each declared type/nullability against the real columns. The structured forms below are preferred over raw SQL because `check` can reason about them.

## The one rule that explains the whole format: type-first

Every field is written **type-first** — a single **type key** whose value is the document key, plus the siblings that type allows:

```yaml
fields:
  - keyword: email          # type `keyword`, document key `email`
    required: true
  - text: bio               # analyzed full text
  - integer: age
```

There is **exactly one type key per field**. There is **no** `- field: x` + `type:` form. Adding a sibling the type doesn't allow is a load-time error.

## Schema file skeleton

```yaml
# yaml-language-server: $schema=./index.schema.yml
version: 1            # required; only 1 is supported
table: users          # required; the root table
schema: public        # optional; defaults to public
primary_key: id       # needed for the doc id AND for relations/reverse-resolution
fields:               # required
  - integer: id
  - keyword: email
    required: true
```

If you use any join or aggregate, you **must** set `primary_key` — reverse resolution depends on it.

## Choosing a scalar type

| Want | Use |
| --- | --- |
| Exact match / sort / aggregation | `keyword` |
| Natural-language search (bios, descriptions) | `text` |
| Codes / SKUs / statuses, searchable by parts (`C-01234` ↔ `01234`) | `identifier` |
| Closed string set | `enum` |
| Numbers | `short` `integer` `long` `float` `double` `decimal` |
| Time | `date` `timestamp` |
| Other | `boolean` `uuid` `binary` `json` |
| None of the above | `custom` (declare `postgres:` + `opensearch:`) |

`decimal` → OpenSearch `double` is **lossy**; when exactness matters use a `custom` `scaled_float` (see `examples/aggregate.schema.yml`). A bare column with no type defaults to `keyword`.

Scalar/`geo` siblings: `required`, `column` (source column, defaults to the document key), `transforms` (`lowercase`, `trim`), `default`, `options` (extra OpenSearch mapping props).

## Joins — fold a related table in. The verb names where the key lives.

| Verb | Key lives on | Key sibling | Renders as |
| --- | --- | --- | --- |
| `belongs_to` | **this** table | `column` (defaults to field name) | object, nullable |
| `has_one` | the **related** table | `foreign_key` | object, nullable |
| `has_many` | the **related** table | `foreign_key` | array, never null |
| `many_to_many` | a junction | `through: {table,left_key,right_key}` | array, never null |

**Key arity is strict:** a join takes *exactly* the key sibling its verb implies — nothing else. Every join also needs `table`, `primary_key`, and `fields` (the projection). `order_by`/`limit` apply to `has_many`/`many_to_many` (not `belongs_to`). See `examples/join.schema.yml`.

## Aggregates — reduce a related table to a scalar. The op is the type key.

`count` `sum` `avg` `min` `max`. Each needs `table` and **exactly one** of `foreign_key` xor `through`.

- `count` → non-null `long`; `avg` → nullable `double` → neither takes `column`/`value_type`.
- `sum`/`min`/`max` → **must** declare both `column` and `value_type` (it mirrors the column).

See `examples/aggregate.schema.yml`.

## Geo — a `geo` field → OpenSearch `geo_point`

Two columns (`lat:` + `lon:`) **or** one `column:` holding a geo-shaped value (`{lat,lon}` json, `[lon,lat]`, or `"lat,lon"` text). PostGIS `geometry` / PG `point` are **not** accepted — expose a generated `jsonb`/`text` column. See `examples/geo.schema.yml`.

## Filters — narrow related rows, root rows, or soft-delete scope

```yaml
filters:
  - { column: status, op: eq,  value: paid }
  - { column: status, op: in,  value: [paid, shipped] }
  - { column: total,  op: between, value: [10, 100] }   # exactly two
  - { column: deleted_at, op: is_null }                  # no value
  - { raw: "amount > 0 AND currency = 'USD'" }           # escape hatch
```

Ops: `eq neq lt lte gt gte like ilike` (scalar) · `in not_in` (list) · `between` (two) · `is_null is_not_null` (none). Wrong arity is a load-time error.

- **Top-level `filters:`** — only matching root rows become documents. A row that leaves the set emits a tombstone.
- **`soft_delete:`** — `column:` or `field:` (exactly one), optional `when:` filters. A matching row deletes instead of upserts.

## Identifier rules (catches people out)

- **Postgres identifiers** (table/column/schema/index/sink names): `^[a-z_][a-z0-9_]*$`, lowercased on load. A name that isn't a valid identifier must be addressed via `column:`.
- **Field names** (the document key): `^[a-zA-Z_][a-zA-Z0-9_]*$`, **case preserved** — `count: orderCount` stays camelCase in the document.

So: value comes from a lowercase Postgres column, lands under a document key you choose.

## Secrets and connection values

In `flusso.toml`, anywhere a secret/URL is expected, use a literal **or** `{ env = "VAR" }`. Env refs resolve at **run time**, not parse time — so a compiled `flusso.lock` carries no baked secret. Reserved: `DATABASE_URL` and `<SINK>_OPENSEARCH_URL` override the config.

## Before you call it done

1. `# yaml-language-server` line present at the top of every `*.schema.yml`.
2. Exactly one type key per field; only allowed siblings used.
3. Every join: correct key sibling for its verb, plus `table` + `primary_key` + `fields`.
4. `primary_key` set on the root if any relation exists.
5. `flusso check --config flusso.toml` passes (use `--offline` if no DB reachable).

See `examples/` for a worked join, aggregate, and geo schema.
