# Identifiers and validation

Two identifier rules, and the list of what loading checks beyond the file grammar.

## Identifiers

| Rule | Applies to | Pattern | Notes |
| --- | --- | --- | --- |
| **Postgres identifier** | table, column, schema, index, and sink names | `^[a-z_][a-z0-9_]*$`, max 63 | Trimmed and lowercased on load, matching Postgres' folding of unquoted names. A column that isn't valid this way must be addressed with an explicit `column:`. |
| **Field name** | a document key (the value of a type key) | `^[a-zA-Z_][a-zA-Z0-9_]*$`, max 63 | Case is preserved: `orderCount` stays camelCase in the document. Only trimmed. |

The split is deliberate: the value comes from a lowercase Postgres column, but lands under a document key chosen to suit the search index.

There is no quoting: a table or column whose name only reads with quotes (`"Users"`, `"my col"`) can't be named.

## One name per level

Two fields at one level of the document (the root, or one join's or object's fields) should not share a name, as the field name rule reads it: ` email` and `email` are one name, `Email` and `email` are two. Such a schema still loads, as it always has, but at most one of them reaches the index, and the document may not match it.

What happens is decided in this order:

1. The Postgres build reads the **last** one's value and converts it through the **first** one's declaration. A conversion that can't be done fails the document.
2. Script fields write their output over it.
3. An `emit: false` namesake strips the name.
4. OpenSearch indexes what is left under the last emitted one's mapping, and may refuse it.

The outcomes below come from a live test (`apps/cli/tests/duplicate_names.rs`). It builds every ordered pair of 31 field kinds through the real builder and OpenSearch sink: each leaf type, `array: true`, `json` with and without `indexed`, `map`, `geo`, `custom`, each join kind with and without related rows, `object`, the aggregates, constants, scripts and `emit: false`. Each outcome is checked against the classification the warning's wording is built from; the table below is written from that test's results.

| First | Last | A document holds |
| --- | --- | --- |
| any | any, when one of them is `emit: false` | nothing under the name (the mapping still lists the last emitted one) |
| a value (column, aggregate, geo, constant) | anything | the last one's value: read as the first one's type when both are values, as raw JSON otherwise |
| a join or object | a join or object whose child fields it declares too | the last one's value, its child fields read as the first one declares them |
| a join or object | a join or object with child fields it doesn't declare | only the child fields both declare; with none, every element (or the object) is empty: `has_many: orders` twice, first with `status`, then with `total` and `note`, gives `"orders": [{}, {}]` |
| an object | a to-many join | the last one's rows, as raw JSON |
| a join or object | a `map`, or a `json` with `indexed: false` | the last one's object cut to the keys named like the first one's child fields |
| a join or object | a scalar column or aggregate | the last one's value, as raw JSON |
| an `array: true` column | a scalar column, an aggregate other than `ids`, a scalar constant | the last one's value in a one-element array |
| an `array: true` `json` column | an object, a to-one join, a `geo` point, a `map` column | the last one's value in a one-element array |
| an `array: true` column | an array, a to-many join, `ids` | the last one's value, its elements read as the first one's type |
| an `array: true` column of scalars | a join with a related row, an object, a set `geo` point, a `json` or `map` column holding an object | nothing: **the document fails to build** (`the column holds an object at [0], but an array: true field of keyword takes scalar elements`) |
| a join or object | a `geo` point | its point cut to the first one's child fields, which **OpenSearch rejects** |
| a script, anywhere | a field of another type | the script's output under the last one's mapping: **OpenSearch rejects** every document where the script returns a value under a join, an object, a `json` or `map` column or a constant holding one, and every document whose output doesn't read as the last one's type under any other non-text field |
| a script, anywhere | a text-family field, an unindexed one, or a field of the script's own type | the script's output |

The warning names each field by its position (`#3`) and says which row applies. Where the outcome depends on the data or on a declaration it can't see, it says so instead of guessing: "the index maps the last one; what a document holds depends on both declarations: it may hold either value, be empty, fail to build or be refused by the sink (run `flusso script run` on a row to see)". That covers:

- a constant holding an object (it builds as `null` today);
- a `custom` column after an `array: true` one or after a join or object;
- an indexed `json` column (or `json` array) after a join, an object, or another array (a `json` field holding keys is refused by the strict mapping on its own).

A script reading `this.email` sees the value the build left under the name: the last one's, or its own `null` placeholder when the script is itself the last one.

What a failing pair does depends on the index's [`on_error`](index-and-on-error.md#on_error):

| Pair | `stop` (the default) | `skip` |
| --- | --- | --- |
| fails to build | the ingest engine stops and restarts on the same change, so **the whole deployment** (every index, every sink) makes no progress | the document keeps its previous indexed version (none on a first backfill); the rest go on |
| rejected by OpenSearch | that sink stops and restarts on the same batch, so **every index it writes** makes no progress, and the replication slot keeps the WAL | the document is quarantined; the rest go on |

Elsewhere:

- **Content hash:** both, in order, as for any field. Swapping them moves the index to a new name.
- **`flusso-query` derive:** a compile error on the two handles, unless one is `emit: false` or `indexed: false`.

`flusso check`, `--offline` included, and `run` warn with [`duplicate_field_name`](cli.md#check). A finding on one of the namesakes carries the position in its path: `orders#2.total`. Rename all but one.

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

Two fields sharing a name at one level still load: `check` warns, see [One name per level](#one-name-per-level).

`flusso check` without `--offline` also confirms, per declared field, that the column exists, that its SQL type is accepted by the declared type key, and that its `NOT NULL` agrees with `required`, and that every table and column a relation names exists. It then reports publication coverage and the replica identity of every table an index reads. A disagreement fails the check with the field named; the full list is in [`check`](cli.md#check).
