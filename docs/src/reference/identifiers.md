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

1. The Postgres build reads the **last** one's value and converts it through the **first** one's declaration. A conversion that can't be done fails the document. Constants are the exception both ways: a constant that comes first builds the value it holds, whatever the last one reads, and one that comes last reaches the first one as text (a scalar) or as `null` (a list or an object). A `binary` column reaches it as Postgres hex text, which only a `binary` declaration decodes.
2. Script fields write their output over it.
3. An `emit: false` namesake strips the name.
4. OpenSearch indexes what is left under the last emitted one's mapping, and may refuse it.

The outcomes below come from a live test (`apps/cli/tests/duplicate_names.rs`). It builds every ordered pair of 38 field kinds through the real builder and OpenSearch sink: each leaf type, `array: true`, `json` with and without `indexed`, `map`, `geo`, `custom`, each join kind with and without related rows, `object`, the aggregates (one with `value_type: decimal`), `decimal`, constants (a whole and a fractional number, `true`, a string, a list, a mixed list, an object), scripts and `emit: false`. Each outcome is checked against the classification the warning's wording is built from; the table below is written from that test's results.

| First | Last | A document holds |
| --- | --- | --- |
| any | any, when one of them is `emit: false` | nothing under the name (the mapping still lists the last emitted one) |
| a constant | anything | the first one's constant, under the last one's mapping: **OpenSearch rejects** every document when that mapping can't take it (a number or boolean under a `geo` point or a join, a string that isn't a number under a numeric field, an object under a scalar field) |
| anything but a constant | a constant holding a list or an object | `null` |
| anything but a constant or a `decimal` field | a number constant | the last one's value as text (`"5"`); in a one-element array when the first one is an `array: true` column |
| a `decimal` column, or `sum`/`min`/`max` with `value_type: decimal` | a number constant | the number (a `decimal` field reads text as a number); in a one-element array when the first one is an `array: true` column |
| anything but a constant | a boolean constant | `"true"` or `"false"`, as text |
| anything but a constant or a `binary` column | a `binary` column | its bytes as Postgres hex text (`"\x0102"`), not base64 |
| a value (column, aggregate, geo) | anything else | the last one's value: read as the first one's type when both are values, as raw JSON otherwise |
| a join or object | a join or object whose child fields it declares too | the last one's value, its child fields read as the first one declares them |
| a join or object | a join or object with child fields it doesn't declare | only the child fields both declare; with none, every element (or the object) is empty: `has_many: orders` twice, first with `status`, then with `total` and `note`, gives `"orders": [{}, {}]` |
| an object | a to-many join | the last one's rows, as raw JSON |
| a join or object | a `map`, or a `json` with `indexed: false` | the last one's object cut to the keys named like the first one's child fields |
| a join or object | a scalar column or aggregate | the last one's value, as raw JSON |
| an `array: true` column | a scalar column, an aggregate other than `ids`, a string constant | the last one's value in a one-element array |
| an `array: true` `json` column | an object, a to-one join, a `geo` point, a `map` column | the last one's value in a one-element array |
| an `array: true` column | an array, a to-many join, `ids` | the last one's value, its elements read as the first one's type |
| an `array: true` column of scalars | a join with a related row, an object, a set `geo` point, a `json` or `map` column holding an object | nothing: **the document fails to build** (`the column holds an object at [0], but an array: true field of keyword takes scalar elements`) |
| a join or object | a `geo` point | its point cut to the first one's child fields, which **OpenSearch rejects** |
| a script, anywhere | a field of another type | the script's output under the last one's mapping: **OpenSearch rejects** every document where the script returns a value under a join, an object, a `json` or `map` column or a constant holding one, and every document whose output doesn't read as the last one's type under any other non-text field |
| a script, anywhere | a text-family field, an unindexed one, or a field of the script's own type | the script's output |

The warning names each field by its position (`#3`) and says which row applies. Where the outcome depends on the data or on a declaration it can't see, it says so instead of guessing: "the index maps the last one; what a document holds depends on both declarations: it may hold either value, be empty, fail to build or be refused by the sink (run `flusso script run` on a row to see)". That covers:

- a constant first whose value the last one's mapping may or may not take: an object under a `map`, or under a join, an object or a `json` column whose child fields name every key; a string under a date, `binary` or `geo` field;
- an `array: true` column with a `default` before a constant holding a list or an object (the column's `default` replaces the `null`);
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

### Pairs that stop the start

Two classes of pair never give an indexed document, whatever the rows hold, where every document carries them: at the root, or in a plain object (no `emit: false`, no `options`) under plain objects up to the root. There, `check` reports them as errors, `--offline` included, and `run` stops at start (see [Validation at start](cli.md#validation-at-start)):

| First | Last | Why never | `kind` |
| --- | --- | --- | --- |
| an `array: true` column of scalars | an `object` | an object is built for every row, nulls and all, and the array refuses it | `duplicate_field_name_fails_to_build` |
| a constant holding a scalar or a list | an indexed join, an `object`, an indexed `json` (or `json` array), a `map` | OpenSearch refuses a value that isn't an object under an object mapping, `ignore_malformed` or not | `duplicate_field_name_refused` |
| a constant holding an object | an indexed join, an `object` or an indexed `json` with no child field for one of its keys, or a `binary` column | the strict mapping refuses the key; `binary` takes no `ignore_malformed` | `duplicate_field_name_refused` |

Each holds only when every namesake is emitted and the last one sets no mapping `options` of its own: an object with `enabled: false` takes anything, and one with a `dynamic` of your own takes unknown keys. A `map`'s own `dynamic: true` isn't counted. The refused class stops `run` only while every sink is a [plain OpenSearch sink](cli.md#validation-at-start): a stdout sink, an ingest pipeline or a setting flusso doesn't model may let the document in.

Anywhere else the same pair stays the `duplicate_field_name` warning. Inside a join, a parent with no related row indexes. Under `emit: false` or `indexed: false`, the pair is never mapped.

Every other failing pair can index some document, so it stays the warning: a related row, a point or an object value may be absent, a script may return nothing, and a scalar mapping drops what it can't read under `index.mapping.ignore_malformed`. The live test pins this too: a third row holds only nulls and has no related rows, its scripts return nothing, and a pair no row indexes is written again with `index.mapping.ignore_malformed`. Over the 1,444 pairs at the root, 57 never index (1 fails to build, 56 refused), and the rule above names exactly those. A second live test starts the real `flusso run` on the cases that index some document (a pair inside a join, under `emit: false` or `indexed: false`, with `on_error = "skip"`, a `dynamic` of your own, a pipeline set as an index setting) and checks each starts and indexes what it did before.

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
