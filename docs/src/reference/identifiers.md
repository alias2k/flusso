# Identifiers and validation

Two identifier rules, and the list of what loading checks beyond the file grammar.

## Identifiers

| Rule | Applies to | Pattern | Notes |
| --- | --- | --- | --- |
| **Postgres identifier** | table, column, schema, index, and sink names | `^[a-z_][a-z0-9_]*$`, max 63 | Trimmed and lowercased on load, matching Postgres' folding of unquoted names. A column that isn't valid this way must be addressed with an explicit `column:`. |
| **Field name** | a document key (the value of a type key) | `^[a-zA-Z_][a-zA-Z0-9_]*$`, max 63 | Case is preserved: `orderCount` stays camelCase in the document. Only trimmed. |

The split is deliberate: the value comes from a lowercase Postgres column, but lands under a document key chosen to suit the search index.

## What loading enforces

Loading a `flusso.toml` and its schemas fails with a specific error when any of these is false. None of it needs a database.

- `version` is `1`.
- Every table, column, schema, index, and sink name is a Postgres identifier; every document key is a field name.
- Each field has exactly one type key, and only the siblings that type allows.
- A join carries exactly the key sibling its verb implies: `column` for `belongs_to`, `foreign_key` for `has_one`/`has_many`, `through` for `many_to_many`. To-one verbs take no `limit`; `belongs_to` takes no `order_by`.
- An aggregate names exactly one of `foreign_key` or `through`; `sum`/`avg`/`min`/`max` carry a `column`; `sum`/`min`/`max` carry a `value_type`; `ids` carries a scalar `element_type` (not `geo` or `custom`); `element_type` appears on no other op.
- `required` appears only on scalars, `geo`, `map`, and to-one joins.
- A `default` is a scalar.
- A `map`'s `values` is a leaf kind.
- A `geo` gives `lat` and `lon`, or a single `column`.
- A `between` has exactly two values; `in`/`not_in` take a list.
- `doc_id` is absent.
- In `flusso.toml`: `[source]` is present, each `[[index]]` has `name`, `schema`, and `enabled`, and every `schema` path ends in `.yml`/`.yaml`.
- Unknown keys anywhere.

## What check adds with a database

`flusso check` without `--offline` also confirms, per declared field, that the column exists, that its SQL type is accepted by the declared type key, and that its `NOT NULL` agrees with `required`. It then reports publication coverage. A disagreement fails the check with the field named.
