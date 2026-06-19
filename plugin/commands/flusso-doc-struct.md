---
description: Scaffold a typed Rust query struct (#[derive(FlussoDocument)]) for a flusso index.
argument-hint: <index-name> [StructName]
---

Generate a hand-written `#[derive(FlussoDocument)]` projection struct for the flusso index `$1` (Rust type name `$2`, defaulting to a PascalCase form of `$1`). Follow the **flusso-query** skill.

1. Locate `flusso.toml` and the `[[index]]` named `$1`; open its `schema:` file to read the field list (ask if it can't be found).
2. For each schema field, emit a struct field using the flusso-type → Rust-type table:
   - keyword/enum/text/identifier/uuid → `String`; numbers → `i16/i32/i64/f32/f64`; `boolean` → `bool`; `date`/`timestamp` → a date leaf (`time`/`chrono` feature) or `String`; `json` → `serde_json::Value`; `geo` → `GeoPoint`.
   - `object`/`belongs_to`/`has_one` → a child struct (`Option<_>` for the to-one joins); `has_many`/`many_to_many` → `Vec<ChildStruct>`.
   - **Nullability:** non-null for primary keys, `required: true`, objects, `count`, and to-many joins; `Option<_>` for `required: false`, to-one joins, and `avg`/`sum`/`min`/`max`.
   - Add `#[serde(rename = "docKey")]` when the document key (case-preserved, often camelCase) differs from the snake_case Rust field.
3. Emit a child struct with `#[flusso(index = "$1", path = "<dotted.path>")]` for every object/join, recursively.
4. Remind the user this is a **projection** — they can omit fields they don't need; only declared fields are checked. It compiles against their `flusso.toml` (auto-discovered, or `FLUSSO_CONFIG`).

**If an equivalent document struct already exists** (a migration — the project already has this type): edit that struct **in place** instead of scaffolding a new one. Add `FlussoDocument` to its derive list and `#[flusso(index = "$1")]`, and **preserve all its existing fields, including the `id` / primary key** — a migration reproduces the current document exactly, it does not trim it. Don't create a parallel `$2`-v2 type alongside the original.

Skeleton:

```rust
use flusso_query::FlussoDocument;

#[derive(Debug, Clone, serde::Deserialize, FlussoDocument)]
#[flusso(index = "$1")]
pub struct $2 {
    // fields derived from the schema…
}
```
