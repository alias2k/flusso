# flusso-schema-index-yaml

Parse a `*.schema.yml` index definition into the core
[`IndexSchema`](schema_core::IndexSchema).

A schema file describes one search document: its root table, its fields, and
how related tables fold in through joins and aggregates. Each field is
written **type-first** — `- <type>: <name>` (`keyword: email`,
`has_many: orders`, `count: orderCount`, `geo: location`) — and carries
only the siblings that type allows. Parsing is two stages:

1. [`SchemaYaml`] deserializes the file. Each field's type tag selects the
   body shape it parses into (see [`Field`]).
   [`ParseFrom`](schema_core::ParseFrom) also checks the declared `version`
   against [`SUPPORTED_VERSIONS`].
2. `TryFrom<SchemaYaml>` converts it into the core model, validating
   identifiers and the arity rules YAML alone can't express: a join takes
   exactly the key its verb implies (`column` for `belongs_to`,
   `foreign_key` for `has_one`/`has_many`, `through` for `many_to_many`),
   `sum`/`min`/`max` aggregates need a `column` and a `value_type`, a
   `between` filter takes exactly two values, and a `geo` field needs either
   `lat`+`lon` or a single `column`.
