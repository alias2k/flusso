# flusso-schema-core

The cross-cutting vocabulary for `flusso` — the validated types every other layer trades in.

## Quick reference

| Item | Role |
| --- | --- |
| [`common`] | Validated primitives — [`TableName`], [`ColumnName`], and the other identifier newtypes |
| [`config`] | The structures built from them — [`IndexSchema`], [`Field`], [`Join`], [`Aggregate`], [`Filter`], [`IndexMapping`] |
| [`traits`] | The conversion the format crates implement — [`ParseFrom`] (text into entities) |

This is layer 0: every other crate produces or consumes these types. They're the canonical, already-validated shape of a search document and its building blocks, carrying no trace of the file format they were parsed from.

> ℹ️ **Info** — the *assembled* deployment config (`Config`/`Index`/`Source`/the `Sink` enum) is a composition concern and lives a layer up in the `schema` crate. Keeping it out of here lets the backends depend on the vocabulary without reaching the top-level config.

Identifier types are built with [`nutype`]: they're constructed only through `try_new`, so an invalid name never reaches the model.

[`nutype`]: https://docs.rs/nutype
