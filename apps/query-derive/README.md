# flusso-query-derive — `#[derive(FlussoDocument)]`

The proc-macro behind [`flusso-query`](https://crates.io/crates/flusso-query). You don't
use this crate directly — pull the derives in through `flusso-query`'s `derive` feature:

```rust,ignore
use flusso_query::FlussoDocument;
```

## What it does

It does **not** generate the document struct. You write the struct; this derive, at
compile time and **with no database**:

1. discovers `flusso.toml` (walking up from `CARGO_MANIFEST_DIR`, or via a
   `#[flusso(config = "…")]` attribute / the `FLUSSO_CONFIG` env var) and resolves the
   named index's mapping;
2. validates each declared field against that mapping — exists, type matches,
   nullability matches — reporting every problem at once with precise spans;
3. generates the typed query surface (`Type::field()` handles, `get`/`query`,
   `SCHEMA_HASH`) that targets the `flusso-query` runtime.

## Companion derives

Three more derives ship alongside it, all re-exported through `flusso-query`:

- `FlussoValue` — a Rust enum or newtype standing in for a leaf field.
- `FlussoMap` — a newtype wrapper over a dynamic-key `map` field.
- `FlussoMultiDocument` — the combined-search union over several document types.

## Learn more

The query surface, the typed handles, and how the binding works are documented in the
[Querying guide](https://alias2k.github.io/flusso/guides/querying.html) and the
[`flusso-query` crate docs](https://docs.rs/flusso-query). flusso as a whole lives at
[github.com/alias2k/flusso](https://github.com/alias2k/flusso).
