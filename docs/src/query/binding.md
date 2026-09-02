# Binding to the schema

How the root derive finds `flusso.toml`, resolves the mapping with no database, names the index, and what it generates.

## The one input: the index name

```rust
#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "users")]
pub struct User { /* … */ }
```

At compile time the macro:

1. **Locates `flusso.toml`** by walking up from the consuming crate's `CARGO_MANIFEST_DIR`. Override with `#[flusso(index = "users", config = "…")]` or the `FLUSSO_CONFIG` env var.
2. **Selects the `[[index]]`** whose `name` matches.
3. **Loads that index's schema** (relative to `flusso.toml`) and **resolves the mapping** in-process, the same resolution `flusso build` performs. No database, no network.
4. **Tracks the config and every schema it read** as build inputs, so editing either retriggers compilation and a drifted struct fails the next build.

There is no `build.rs`, no generated file to `include!`, no committed mapping to keep in sync.

## Index name and schema hash

The resolved mapping's content hash is `SCHEMA_HASH`, the same hash `flusso build` writes into `flusso.lock` and the sink folds into the alias name. `User::INDEX` is the physical name `users_<hash>`, and `get`/`query` use it directly. A structural schema change rotates the hash and the mapping together, so the next `cargo build` regenerates the binding against the new index. Both consts are public for logging and admin.

The convenience alias (`users`) is for clients that don't recompile against the schema; a derived binding doesn't need it.

## Reading a prefixed deployment

When flusso runs with an index prefix (`FLUSSO_INDEX_PREFIX=dev_`, writing `dev_users_<hash>`), give the client the same prefix:

```rust
let client = Client::connect("https://localhost:9200")?
    .index_prefix(std::env::var("FLUSSO_INDEX_PREFIX").unwrap_or_default());
```

The prefix is applied at **runtime**, on the transport: the derive still bakes the unprefixed `INDEX`, and the client prepends the prefix to every request path (and strips it from `_index` when decoding combined search). One compiled binary serves every environment. It must match the writer's prefix exactly.

## What the derive expands to

For `User`, roughly:

```rust
impl User {
    pub fn get(client: &Client, id: i32) -> impl Future<Output = Result<Option<User>>>;
    pub fn query() -> Search<User>;

    // one handle per schema field, whether or not User projects it
    pub fn id() -> Number<kind::Integer>;
    pub fn email() -> Keyword;
    pub fn account() -> flusso_user_query::Account;             // object: chained namespace
    pub fn orders() -> Nested<Root, flusso_user_query::Orders>; // nested: own scope
}

impl FlussoRoot for User {
    const INDEX: &str = "users";
    const SCHEMA_HASH: &str = "3f2a1b9c…";
}
```

Plus **one namespace per container level** in a generated module `flusso_<root>_query`, so nothing lands in your namespace and a `struct UserOrders` of your own can't collide:

```rust
pub mod flusso_user_query {
    pub struct Account;                         // object: &self methods, Root scope
    impl Account {
        pub fn tier(&self) -> Enum<Root>;
    }

    pub struct Orders;                          // nested: associated fns, own scope
    impl FlussoScope for Orders {
        const PATH: &[Segment] = &[Segment { name: "orders", kind: SegmentKind::Nested }];
    }
    impl Orders {
        pub fn status() -> Enum<Orders>;
        pub fn items() -> Nested<Orders, OrdersItems>;
        pub fn shipping() -> OrdersShipping;    // object inside nested
    }
}
```

Rename a generated scope from the field, and everything under it follows: `#[flusso(scope = "Purchases")]` on `orders` gives `Purchases::total()` and `PurchasesItems::quantity()`. Only a root field can carry it. Rename the module with `#[flusso(index = "users", scope_mod = "user_queries")]`.

Finally the root bakes the resolved subtree into const data and drives the check into every fragment it embeds:

```rust
const _: () = {
    const __FLUSSO_LEVEL: &[FieldSpec] = &[ /* the resolved subtree */ ];
    Account::__flusso_check(children(__FLUSSO_LEVEL, "account"));
    Order::__flusso_check(children(__FLUSSO_LEVEL, "orders"));
};
```

## Scopes and paths

`FlussoScope` carries only `PATH`, the container chain from the root. `FlussoRoot: FlussoScope` adds the index identity and `query`/`get`, so a fragment physically can't start a search. The `Root` scope marker is shared by the root and every flattened object, which is what lets combined search and object handles compose.

## Related

- [Environment variables](../reference/environment.md#cli-flags) for `FLUSSO_CONFIG`.
- [flusso.toml top level](../reference/config-toml.md#prefix) for the write side of the prefix.
