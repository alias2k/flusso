# flusso-dev-search-api

A small HTTP API over the dev indexes, built with `flusso-search` +
`#[derive(FlussoDocument)]`. It's the read side of the dev stack: the engine
keeps OpenSearch in sync from Postgres, and this serves typed, filterable
queries over the result.

The document structs here are **projections** of the schemas in
[`../flusso.toml`](../flusso.toml) — the derive discovers that config at compile
time (walking up from this crate) and checks each struct against it, so a schema
change that breaks a field fails `cargo build`.

## Run it

```sh
# 1. Bring up Postgres + OpenSearch.
docker compose up -d                       # from the repo root

# 2. Build the artifact and populate the indexes from Postgres.
cargo run -- run --config dev/flusso.toml   # the engine; leave it running

# 3. Serve the read API.
cargo run -p flusso-dev-search-api
#    OPENSEARCH_URL   (default http://localhost:9200)
#    OPENSEARCH_USER / OPENSEARCH_PASSWORD  (optional basic auth)
#    BIND             (default 0.0.0.0:8080)
```

## Endpoints

Each index has a **list** endpoint (filterable via query params; absent params
are simply not applied — the `Option<Query>` optional-filter primitive) and a
**fetch-by-id** endpoint (`GET /<index>/{id}`, backed by `Type::get`, returning
`404` when the document doesn't exist).

`users` and `products` also accept a free-text **`q`** — a single scoring
`multi_match` across that document's analyzed `text` fields, so it drives
relevance ranking while the other params only narrow. (`orders` has no `text`
field, so it has no `q`.)

| Endpoint | Filters |
| -------- | ------- |
| `GET /users` | `q` (full-text over `fullName` + `profile.bio`), `email`, `email_prefix`, `name` (full-text on `fullName`), `tier` (`account.tier`), `bio` (full-text on `profile.bio`), `has_profile` (one-to-one exists), `city` (matches an address), `order_status` (matches an order), `min_orders`, `recent_orders=N` (trim each user's returned `orders` to the N newest), `limit` |
| `GET /users/{id}` | — (fetch one) |
| `GET /products` | `q` (full-text over `name` + `description`), `sku`, `name` (full-text), `in_stock`, `min_reviews`, `min_rating`, `limit` |
| `GET /products/{id}` | — (fetch one) |
| `GET /orders` | `user_id`, `status`, `min_total`, `min_items`, `limit` |
| `GET /orders/{id}` | — (fetch one) |
| `GET /health` | — |

```sh
# Free-text, then narrow: users matching "ada" with ≥5 orders, in London, pro tier,
# returning only their 3 newest orders:
curl 'localhost:8080/users?q=ada&min_orders=5&tier=pro&city=London&recent_orders=3'
curl 'localhost:8080/products?q=keyboard&in_stock=true&min_rating=4'
curl 'localhost:8080/orders?status=delivered&min_total=100'

# Fetch one document by id (404 if absent):
curl 'localhost:8080/users/1'
curl 'localhost:8080/products/3'
curl 'localhost:8080/orders/10'
```

The `users` document is the full nested shape — the derive validates each level
against the schema and `_source` deserializes into typed structs:

```json
{
  "total": 42,
  "hits": [{
    "id": "1", "score": 1.0,
    "source": {
      "id": 1, "email": "ada@example.com", "fullName": "Ada Lovelace",
      "account": { "tier": "gold", "country": "GB", "createdAt": "…" },
      "profile": { "bio": "…", "avatarUrl": "…", "birthDate": "…" },
      "addresses": [{ "kind": "home", "line1": "…", "city": "Boston", "postalCode": "…", "country": "GB" }],
      "orders": [{ "status": "delivered", "total": 42.0, "placedAt": "…",
                   "items": [{ "productId": 7, "quantity": 2, "unitPrice": 21.0 }] }],
      "orderCount": 9, "lifetimeValue": 380.0, "avgOrderValue": 42.2,
      "lastOrderAt": "…", "deliveredOrders": 7
    }
  }]
}
```

Note on the complex fields:

- `account` (object) and `profile` (one-to-one) are **flattened** in OpenSearch,
  so they're queried by dotted path with no wrapper. The child struct's generated
  handles work directly in a filter — `Account::tier()` (`account.tier`),
  `Profile::bio()` (`profile.bio`) — which is how the `tier` / `bio` filters work.
- `addresses` / `orders` are **nested**, so a child query is wrapped through the
  parent handle: `User::addresses().any(Address::city().eq(…))` — the `city` /
  `order_status` filters.

The only thing not generated for object/one-to-one fields is a parent entry
handle (`User::account()` / `User::profile()`) — useful mainly for an existence
check on the object itself; for now `Keyword::at("profile").exists()` covers it.

## A note on index names

The engine writes to a **physical** index named `<logical>_<hash>` (e.g.
`users_3f2a1b9c`) and there's no read alias yet. You don't deal with that here:
the derive bakes the physical name into `T::search`/`T::get` (it knows the hash
at compile time, the same one the sink appends), so handlers just call
`User::search(&client)` — the hash stays hidden. A structural schema change
rotates the hash *and* forces this crate to recompile, so the binding always
targets the right index. (`T::SCHEMA_HASH` is still exposed if you need it.)
