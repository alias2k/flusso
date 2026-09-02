# Migrating from path

`#[flusso(path = "…")]`, `#[derive(FlussoDocument)]`, and the `FlussoIndex`/`FlussoDocument` traits are removed. The compiler finds every site; the fixes are mechanical.

## The table

| Before | After |
| --- | --- |
| `#[derive(FlussoDocument)] #[flusso(index = "users")]` on the root | `#[derive(FlussoRoot)] #[flusso(index = "users")]` |
| `use flusso_query::FlussoIndex;` for `query`/`get` | `use flusso_query::FlussoRoot;` (one name for trait and derive) |
| `FlussoDocument` as a trait bound (the `PATH` carrier) | `FlussoScope` |
| `#[derive(FlussoDocument)] #[flusso(index = "users", path = "orders")]` | `#[derive(FlussoFragment)]`; drop the attribute |
| `Order::status()` (a child struct's handle) | `flusso_user_query::Orders::status()` (the root-generated namespace) |
| `Account::tier()` (an object child's handle) | `User::account().tier()` (chains from the parent) |

## Steps

1. Replace the root derive and import. The `path` attribute error names its replacement.
2. Strip `#[flusso(...)]` from every child and change its derive to `FlussoFragment`.
3. Fix the unresolved handle calls: nested handles move to the generated namespace, object handles chain from the parent.
4. `cargo check` between steps; each remaining error points at its line.

## What changed underneath

A child struct used to name its own location, so an object inside a nested array couldn't know its scope and was rejected. Now the root knows every path and generates every namespace, which is why `User::orders().any(Orders::shipping().carrier().eq("dhl"))` works and why a fragment can serve two indexes at once. See [Binding to the schema](binding.md).
