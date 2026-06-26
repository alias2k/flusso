# flusso-schema-index-yaml

Parses a `*.schema.yml` index definition into the core [`IndexSchema`](schema_core::IndexSchema) — parse, then convert + validate.

## At a glance

| Stage | What happens | Type |
| --- | --- | --- |
| 1. parse | deserialize the file; type tag picks each field's body shape; check `version` against [`SUPPORTED_VERSIONS`] | [`SchemaYaml`] |
| 2. convert | validate identifiers + the arity rules YAML can't express, lift into the core model | `TryFrom<SchemaYaml>` → [`IndexSchema`](schema_core::IndexSchema) |

Fields are **type-first** — `- <type>: <name>` (`keyword: email`,
`has_many: orders`, `count: orderCount`, `geo: location`) — carrying only the
siblings that type allows.

## The two stages

**Parse.** [`SchemaYaml`] deserializes the file; each field's type tag selects
the body shape it parses into (see [`Field`]).
[`ParseFrom`](schema_core::ParseFrom) also checks the declared `version`
against [`SUPPORTED_VERSIONS`].

**Convert.** `TryFrom<SchemaYaml>` lifts the parsed shape into the core model,
validating identifiers and the arity rules YAML alone can't express:

- a join takes exactly the key its verb implies — `column` for `belongs_to`,
  `foreign_key` for `has_one`/`has_many`, `through` for `many_to_many`
- `sum`/`min`/`max` aggregates need a `column` and a `value_type`
- a `between` filter takes exactly two values
- a `geo` field needs either `lat`+`lon` or a single `column`

> ℹ️ **Info** — the relationship verb names where the key lives: `belongs_to`
> means this table holds the pointing column; `has_one`/`has_many` means the
> related table holds the foreign key. That's why each verb takes a different
> sibling.
