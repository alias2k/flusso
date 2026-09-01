---
description: Scaffold a typed Rust query struct (#[derive(FlussoRoot)]) for a flusso index.
argument-hint: <index-name> [StructName]
---

Generate a `#[derive(FlussoRoot)]` projection struct for the flusso index `$1` (Rust type name `$2`,
defaulting to a PascalCase form of `$1`).

**Follow the flusso-query skill.** It owns the flusso-type → Rust-type table, the nullability rules,
and how handles work for child levels. Don't guess any of those here.

1. Locate `flusso.toml` and the `[[index]]` named `$1`, then open its `schema:` file for the field
   list. Ask if it can't be found.
2. Emit one struct field per schema field, using the skill's type table and nullability rules. Add
   `#[serde(rename = "docKey")]` where the document key differs from the snake_case Rust field.
3. Emit a child struct for every object and join, recursively — each a **`#[derive(FlussoFragment)]`**
   with **no** `#[flusso(…)]` attribute. Where two levels have the same shape, write **one** fragment
   and embed it twice.
4. Tell the user this is a **projection**: they can omit fields they don't need, and only declared
   fields are checked.

**If an equivalent struct already exists**, this is a migration. Read
`${CLAUDE_PLUGIN_ROOT}/skills/flusso-query/migration.md` and edit that struct in place rather than
scaffolding a parallel one.

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
