# Read-side migrations

Two different jobs share one rule: **a migration reproduces the current documents, it does not
redesign them.**

- **Adopting flusso** in a project that doesn't use it yet — the project's existing document type
  is the spec.
- **Moving off the removed form** (`FlussoDocument`, `#[flusso(path = "…")]`) — mechanical, and
  nothing compiles until it is done.

Reached from the `flusso-query` skill, the `/flusso-migrate-query` command, and the `flusso-expert`
agent. This file is the only copy.

## Adopting flusso onto an existing struct

When the task is "migrate this to flusso" or "switch the existing implementation over," the
existing struct is the spec, not a starting suggestion.

**Edit it in place.** Add `FlussoRoot` to the derive list and `#[flusso(index = "…")]` to the
*existing* struct. Child shapes get `FlussoFragment` and no attribute. Keep the name, module, and
visibility. A second "v2" struct beside the original leaves two document types and breaks every
consumer.

**Preserve every field, above all the `id` / primary key.** Match each existing field to a schema
field. If a leaf Rust type or `Option` shape disagrees with the schema, fix the *schema* or surface
the mismatch. Deleting a field to make it compile silently changes the document contract.

If the existing primary-key field isn't in the schema yet, add it there (`- <type>: id` plus
`primary_key: id`) rather than removing it from the struct.

Keep existing `#[serde(rename = …)]`s and field ordering. The derive validates by leaf identifier
and `Option` shape, so a faithful copy compiles and a `cargo check` failure names the field that
drifted.

## Moving off the removed form

`#[derive(FlussoDocument)]`, `#[flusso(path = "…")]`, and the `FlussoIndex` / `FlussoDocument`
traits were removed at the major bump. Nothing compiles until the migration finishes, which makes
the compiler the checklist.

Find the work:

```sh
rg -t rust 'FlussoDocument|FlussoIndex|flusso\(.*\bpath\s*=' -l
```

Empty output means the project is already current. Say so and stop.

Go in this order. Steps 1 and 2 are find-and-replace; only then does `cargo check` surface step 3,
because before that it drowns in unresolved imports.

### 1. Roots: rename the derive

A struct with `#[flusso(index = "…")]` and **no** `path` is a root.

```rust
-#[derive(serde::Deserialize, FlussoDocument)]        // use flusso_query::FlussoDocument
+#[derive(serde::Deserialize, FlussoRoot)]            // use flusso_query::FlussoRoot
 #[flusso(index = "users")]                           // attribute unchanged
```

The `FlussoIndex` **trait** import (for `.query()` / `.get()`) also becomes `FlussoRoot`, now the
same name as the derive, so one import covers both. A `FlussoDocument` trait *bound* (the `PATH`
carrier, rare) becomes `FlussoScope`.

### 2. Children: make them fragments

A struct with `#[flusso(index = "…", path = "…")]` is a child.

```rust
-#[derive(serde::Deserialize, FlussoDocument)]
-#[flusso(index = "users", path = "orders")]          // delete the whole line
+#[derive(serde::Deserialize, FlussoFragment)]
 pub struct Order { /* unchanged */ }
```

Two structs that differed only by path collapse into **one** fragment embedded twice (a line item in
two indexes, a `billingAddress`/`shippingAddress` pair). Collapse them; that is the point of the
change.

Then run `cargo check`. What remains is exactly step 3.

### 3. Call sites: handles move to the root

The only part needing thought. A child struct no longer carries handles; the root generates one
namespace per level.

| Old | New | Rule |
| --- | --- | --- |
| `Account::tier()` | `User::account().tier()` | an object flattens, so it chains from its parent |
| `Order::status()` | `flusso_user_query::Orders::status()` | a `nested` array is a named namespace |
| `Item::quantity()` | `flusso_user_query::OrdersItems::quantity()` | same rule, one level deeper |

Generated names live in a `flusso_<root>_query` module, snake-cased from the root (`User` →
`flusso_user_query`), each type named for its level (`Orders`, `OrdersItems`, `BillingAddress`).
They never enter the caller's namespace, so a struct already named after a level is fine. Import
what you use: `use flusso_user_query::Orders;`.

Scope types in signatures move too: `Query<Order>` → `Query<flusso_user_query::Orders>`.

A generated name that collides with an existing type gets renamed at the root field rather than
worked around:

```rust
#[flusso(scope = "Purchases")]
orders: Vec<Order>,      // → Purchases::status(), PurchasesItems::quantity()
```

Rename the module itself with `#[flusso(scope_mod = "user_queries")]` if you already have one by
that name.

### 4. Newly-checked embeds

Embedding is checked by default now, so a plain un-derived struct in an `object`/`nested` field is
an error naming the fix. Prefer `#[derive(FlussoFragment)]`, which gets validated, over
`#[flusso(opaque)]`, which only silences the check.

### 5. Verify and report

```sh
cargo check --workspace --all-targets     # zero errors, zero warnings
```

Report how many roots and fragments changed, which structs collapsed into one, and **every
`#[flusso(opaque)]` added** — each marks a spot that is no longer validated, so the user decides
whether to keep it.

Do not redesign en route. Names, modules, visibility, field sets, and `#[serde(rename)]`s stay.
