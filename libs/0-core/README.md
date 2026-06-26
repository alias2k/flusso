# flusso-schema-core

The cross-cutting vocabulary for `flusso`.

Every other crate produces or consumes these types. They are the canonical,
already-validated shape of a search document and its building blocks,
carrying no trace of the file format they were parsed from. The *assembled*
deployment config (`Config`/`Index`/`Source`/the `Sink` enum) is a
composition concern and lives a layer up in the `schema` crate, not here, so
the backends can depend on this vocabulary without reaching the top-level
config.

- [`common`] holds the validated primitives — newtypes such as [`TableName`]
  and [`ColumnName`] that enforce Postgres identifier rules at construction.
- [`config`] holds the structures built from them: [`IndexSchema`],
  [`Field`], [`Join`], [`Aggregate`], [`Filter`], [`IndexMapping`], and the rest.
- [`traits`] defines the conversion the format crates implement —
  [`ParseFrom`] (text into entities).

Identifier types are built with [`nutype`]: they can only be constructed
through `try_new`, so an invalid name never reaches the model.

[`nutype`]: https://docs.rs/nutype
