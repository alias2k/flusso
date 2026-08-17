---
description: Migrate read-side Rust off the removed FlussoDocument / path = "…" form onto FlussoRoot + FlussoFragment.
argument-hint: [path-or-crate]
---

Migrate this project's `flusso-query` code from the old derive form to the current one. Scope: `$1` if given, else the whole workspace. Follow the **flusso-query** skill.

The old form was **removed** at the major bump, so nothing compiles until this is finished — which makes the compiler the checklist. **Do it in the order below**: steps 1–2 are pure find-and-replace, and only then does `cargo check` surface step 3's call sites (before that they're drowned out by unresolved imports).

## 0. Find the work

```sh
rg -t rust 'FlussoDocument|FlussoIndex|flusso\(.*\bpath\s*=' -l
```

If that's empty, the project is already on the new form — say so and stop.

## 1. Roots: rename the derive

A struct with `#[flusso(index = "…")]` and **no** `path` is a root.

- `#[derive(…, FlussoDocument)]` → `#[derive(…, FlussoRoot)]`
- `use flusso_query::FlussoDocument;` → `use flusso_query::FlussoRoot;`
- The `FlussoIndex` **trait** import (needed for `.query()` / `.get()`) → `FlussoRoot`. It is now the same name as the derive, so one import covers both. A `FlussoDocument` trait *bound* (the `PATH` carrier, rare) → `FlussoScope`.

Leave the `#[flusso(index = "…")]` attribute exactly as it is.

## 2. Children: make them fragments

A struct with `#[flusso(index = "…", path = "…")]` is a child.

- Replace its derive with `FlussoFragment`.
- **Delete the whole `#[flusso(…)]` line** — a fragment names no index and no path.

Two shapes that were separate structs only because they sat at different paths can now be **one** fragment embedded twice (a line item in two indexes, a `billingAddress`/`shippingAddress` pair). Collapse them if the project has any; that's the point of the change.

Then run `cargo check`. With the imports and derives fixed, what remains is exactly step 3.

## 3. Call sites: handles move to the root

This is the only breaking part. A child struct no longer carries handles — the root generates them for every level. Rewrite by the level's kind:

| Old | New | Rule |
| --- | --- | --- |
| `Account::tier()` (object) | `User::account().tier()` | an object **flattens**, so it chains from its parent |
| `Order::status()` (nested) | `flusso_user_query::Orders::status()` | a `nested` array is a **named namespace** |
| `Item::quantity()` (deeper) | `flusso_user_query::OrdersItems::quantity()` | same rule, one level down |

**Names live in a module: `flusso_<root>_query`, snake_cased** — `User` → `flusso_user_query`, and the type is named for its level (`Orders`, `OrdersItems`, `BillingAddress`). Generated types never enter your namespace, so a struct of your own named after a level is fine. Import what you use: `use flusso_user_query::Orders;`. They are never in your namespace, so nothing you already named can clash.

Scope types in signatures move too: `Query<Order>` → `Query<flusso_user_query::Orders>`.

If a generated name collides with a type the project already has, rename it on the root field rather than working around it:

```rust
#[flusso(scope = "Purchases")]
orders: Vec<Order>,      // → Purchases::status(), PurchasesItems::quantity()
```

## 4. Newly-checked embeds

Embedding is checked by default now, so a plain un-derived struct in an `object`/`nested` field is an error naming the fix. Either give it `#[derive(FlussoFragment)]` (preferred — it gets validated) or mark the field `#[flusso(opaque)]` to keep it unchecked deliberately.

## 5. Verify

```sh
cargo check --workspace --all-targets     # clean
```

Report what changed: how many roots, how many fragments, any structs you collapsed into one, and any `#[flusso(opaque)]` you added (each is a spot that is now *not* validated — call them out so the user can decide).

Do **not** redesign while migrating: keep every struct's name, module, visibility, field set, and `#[serde(rename)]`s. A migration reproduces the current documents exactly.
