---
description: Scaffold a typed Rust query struct (#[derive(FlussoRoot)]) for a flusso index.
argument-hint: <index-name> [StructName]
---

Generate a hand-written `#[derive(FlussoRoot)]` projection struct for the flusso index `$1` (Rust type name `$2`, defaulting to a PascalCase form of `$1`). Follow the **flusso-query** skill.

1. Locate `flusso.toml` and the `[[index]]` named `$1`; open its `schema:` file to read the field list (ask if it can't be found).
2. For each schema field, emit a struct field using the flusso-type → Rust-type table:
   - keyword/text/identifier → `String` (or a `#[derive(FlussoValue)]` newtype/enum); `enum` → `String` or a `#[derive(FlussoValue)]` enum; `uuid` → `String` or `uuid::Uuid` (`uuid` feature) — never `#[flusso(skip)]`; numbers → `i16/i32/i64/f32/f64`; `boolean` → `bool`; `date`/`timestamp` → a date leaf (`time`/`chrono` feature) or `String`; `json` → `serde_json::Value`; `geo` → `GeoPoint`.
   - `object`/`belongs_to`/`has_one` → a child struct (`Option<_>` for the to-one joins); `has_many`/`many_to_many` → `Vec<ChildStruct>`.
   - **Nullability:** non-null for primary keys, `required: true`, objects, `count`, and to-many joins; `Option<_>` for `required: false`, to-one joins, and `avg`/`sum`/`min`/`max`.
   - Add `#[serde(rename = "docKey")]` when the document key (case-preserved, often camelCase) differs from the snake_case Rust field.
3. Emit a child struct for every object/join, recursively — each a **`#[derive(FlussoFragment)]`** with **no** `#[flusso(…)]` attribute. A fragment names no location: `$2` validates it against whatever path it sits at, and recursion reaches its children. Never emit `path = "…"` (removed).
   - If two levels have the same shape (a line item in two indexes, a `billingAddress`/`shippingAddress` pair), write **one** fragment and embed it twice — it is checked at each site.
   - Handles for a child level come from `$2`, not from the child struct: an object chains (`$2::account().tier()`), a `nested` array is a generated namespace named `$2` + the path segments PascalCased (`$2_scope::Orders::status()`, `$2_scope::OrdersItems::quantity()`).
4. Remind the user this is a **projection** — they can omit fields they don't need; only declared fields are checked. It compiles against their `flusso.toml` (auto-discovered, or `FLUSSO_CONFIG`).

**If an equivalent document struct already exists** (a migration — the project already has this type): edit that struct **in place** instead of scaffolding a new one. Add `FlussoRoot` to its derive list and `#[flusso(index = "$1")]`, and **preserve all its existing fields, including the `id` / primary key** — a migration reproduces the current document exactly, it does not trim it. Don't create a parallel `$2`-v2 type alongside the original.

Skeleton:

```rust
use flusso_query::{FlussoFragment, FlussoRoot};

#[derive(Debug, Clone, serde::Deserialize, FlussoRoot)]
#[flusso(index = "$1")]           // the ONLY struct that names a location
pub struct $2 {
    // fields derived from the schema…
    pub orders: Vec<Order>,
}

#[derive(Debug, Clone, serde::Deserialize, FlussoFragment)]
pub struct Order {                // no index, no path — checked where embedded
    // …
}
```
