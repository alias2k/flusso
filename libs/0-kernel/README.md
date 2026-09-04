# flusso-kernel

The cross-cutting vocabulary for `flusso` — the validated types every other layer trades in.

## Quick reference

| Item | Role |
| --- | --- |
| [`common`] | Validated primitives — [`TableName`], [`ColumnName`], and the other identifier newtypes |
| [`config`] | The structures built from them — [`IndexSchema`], [`Field`], [`Join`], [`Aggregate`], [`Filter`], [`IndexMapping`] |
| [`options`] | The neutral, ordered value tree a port entry carries to its adapter — [`Options`], [`OptionValue`] |
| [`adapter`] | What an adapter declares about its configuration — [`AdapterConfig`], [`AdapterDescription`], [`Port`], [`override_var`] |
| [`traits`] | The conversion the config crate implements — [`ParseFrom`] (text into entities) |

This is the kernel: every other crate produces or consumes these types. They're the canonical, already-validated shape of a search document and its building blocks, carrying no trace of the file format they were parsed from and naming no adapter.

> ℹ️ **Info** — the *assembled* deployment config (`Config`/`Index`/the port entries) is a composition concern and lives a layer up in the `config` crate; each adapter's own settings live in that adapter. Keeping both out of here lets the adapters depend on the vocabulary without reaching the top-level config, and lets the config layer carry an adapter's options without knowing their shape.

Identifier types are built with [`nutype`]: they're constructed only through `try_new`, so an invalid name never reaches the model.

[`nutype`]: https://docs.rs/nutype
