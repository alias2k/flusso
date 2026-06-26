# flusso-query-derive — `#[derive(FlussoDocument)]`

The proc-macro behind [`flusso-query`](https://crates.io/crates/flusso-query): it validates
a document struct against the index mapping at compile time and generates the typed query
surface. Don't depend on this crate directly — pull the derives through `flusso-query`'s
`derive` feature:

```rust,ignore
use flusso_query::FlussoDocument;
```

## The derives

| Derive | Stands for |
| --- | --- |
| `FlussoDocument` | A document struct (or a nested element of one). |
| `FlussoValue` | A Rust enum or newtype standing in for a leaf field. |
| `FlussoMap` | A newtype wrapper over a dynamic-key `map` field. |
| `FlussoMultiDocument` | The combined-search union over several document types. |

## What `FlussoDocument` does

It does **not** generate the document struct. You write the struct; the derive, at compile
time and **with no database**:

1. discovers `flusso.toml` (walking up from `CARGO_MANIFEST_DIR`, or via a
   `#[flusso(config = "…")]` attribute / the `FLUSSO_CONFIG` env var) and resolves the
   named index's mapping;
2. validates each declared field against that mapping — exists, type matches,
   nullability matches — reporting every problem at once with precise spans;
3. generates the typed query surface (`Type::field()` handles, `get`/`query`,
   `SCHEMA_HASH`) that targets the `flusso-query` runtime.

A schema change that breaks a field fails the build — that's the safety net.

## Learn more

The query surface, the typed handles, and how the binding works are documented in the
[Querying guide](https://alias2k.github.io/flusso/guides/querying.html) and the
[`flusso-query` crate docs](https://docs.rs/flusso-query). flusso as a whole lives at
[github.com/alias2k/flusso](https://github.com/alias2k/flusso).
