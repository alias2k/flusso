# flusso-query-derive — `#[derive(FlussoRoot)]` / `#[derive(FlussoFragment)]`

The proc-macro behind [`flusso-query`](https://crates.io/crates/flusso-query): it validates
document structs against the index mapping at compile time and generates the typed query
surface. Don't depend on this crate directly — pull the derives through `flusso-query`'s
`derive` feature:

```rust,ignore
use flusso_query::{FlussoFragment, FlussoRoot};
```

## The derives

| Derive | Stands for |
| --- | --- |
| `FlussoRoot` | The **index-bound** document. The only type that reads the schema. |
| `FlussoFragment` | A **location-free** shape, validated wherever a root embeds it. |
| `FlussoValue` | A Rust enum or newtype standing in for a leaf field. |
| `FlussoMap` | A newtype wrapper over a dynamic-key `map` field. |
| `FlussoMultiDocument` | The combined-search union over several document types. |

`FlussoDocument` and `#[flusso(path = "…")]` are **removed** — a clean break at the major bump. See [Migrating from path](https://alias2k.github.io/flusso/query/migrating-from-path.html).

## What `FlussoRoot` does

It does **not** generate the document struct. You write the struct; the derive, at compile
time and **with no database**:

1. discovers `flusso.toml` (walking up from `CARGO_MANIFEST_DIR`, or via a
   `#[flusso(config = "…")]` attribute / the `FLUSSO_CONFIG` env var) and resolves the
   named index's mapping;
2. validates each declared field against that mapping — exists, type matches,
   nullability matches — reporting every problem at once with precise spans;
3. generates the typed query surface for the **whole index**: a handle for every field at
   every level, through one generated namespace per container (`User::account().tier()`
   for an object, `flusso_user_query::Orders::total()` for a `nested` array), plus `get`/`query` and
   `SCHEMA_HASH`;
4. bakes the resolved mapping into const data and drives the check into every fragment it
   embeds — recursively, once per embedding site.

A schema change that breaks a field fails the build — that's the safety net.

## What `FlussoFragment` does

A fragment names no index and no path, so one declaration covers every place its shape
appears: two paths in the same index, two different indexes, or a shared crate. It has no
schema of its own, so it can't check itself — the root hands it the resolved level for the
path it landed on, and it holds one assertion per declared field with the message baked in
at macro time.

The error's primary span is the embedding, with a note chain down to the offending field,
so a mismatch three levels deep still names both the use site and the field.

Because a fragment carries no location, embedding it twice checks it twice.

## Learn more

The query surface, the typed handles, and how the binding works are documented in the
[Query part of the manual](https://alias2k.github.io/flusso/query/overview.html) and the
[`flusso-query` crate docs](https://docs.rs/flusso-query). flusso as a whole lives at
[github.com/alias2k/flusso](https://github.com/alias2k/flusso).
