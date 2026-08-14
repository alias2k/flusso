//! The `users` index: the full document (object + one-to-one + nested arrays,
//! three levels deep) and a filterable endpoint.

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_query::{
    Client, FlussoFragment, FlussoRoot, FlussoValue, OrderBy, SortBuilder, Sortable, multi_match,
};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;
use crate::shared::{LineItem, OrderStatus};

// `pub(crate)`: the cross-index endpoints in `global` reuse this document and
// its generated handles (same for `Product` and `Order`). Handles for every
// level — `account`, `profile`, `orders`, … — hang off `User` itself now.
#[derive(Debug, Serialize, Deserialize, FlussoRoot)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users")]
pub(crate) struct User {
    id: i32,
    email: String,
    full_name: Option<String>,
    // A group (object), always present.
    account: Account,
    // A one-to-one join → a nullable object.
    profile: Option<Profile>,
    // One-to-many joins → nested arrays.
    addresses: Vec<Address>,
    orders: Vec<UserOrder>,
    // Aggregates: count is non-null, sum/avg/max are nullable.
    order_count: i64,
    lifetime_value: Option<f64>,
    avg_order_value: Option<f64>,
    last_order_at: Option<String>,
    delivered_orders: i64,
}

#[derive(Debug, Serialize, Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Account {
    tier: AccountTier,
    country: Option<String>,
    created_at: String,
}

// A string enum stands in for the `keyword` at `account.tier`. `FlussoValue`
// with `#[flusso(keyword)]` implements `FlussoValue<kind::Keyword>` so
// `FlussoRoot` accepts it as the field type *and* `User::account().tier().eq(…)`
// accepts it as a query value; serde's `rename_all` controls the actual keyword
// strings (`"pro"`, …).
#[derive(Debug, Serialize, Deserialize, FlussoValue)]
#[serde(rename_all = "camelCase")]
#[flusso(keyword)]
enum AccountTier {
    Pro,
    Enterprise,
    Free,
}

#[derive(Debug, Serialize, Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Profile {
    bio: Option<String>,
    avatar_url: Option<String>,
    birth_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct Address {
    kind: String,
    line1: String,
    city: String,
    postal_code: Option<String>,
    country: Option<String>,
}

/// A user's nested order (distinct from the top-level `orders` index document).
#[derive(Debug, Serialize, Deserialize, FlussoFragment)]
#[serde(rename_all = "camelCase")]
struct UserOrder {
    status: OrderStatus,
    total: f64,
    placed_at: String,
    // The *shared* fragment — the same one the `orders` index embeds.
    items: Vec<LineItem>,
}

/// A sort direction from the request (`?sort_orders=desc`). The endpoint maps
/// this once, into an [`OrderBy`], and `SortBuilder` does the rest.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OrderDirection {
    Asc,
    Desc,
}

// The single place this app states its missing-value policy: ascending keeps
// rows with no value last, descending pushes them first — so "no value" never
// masquerades as the largest. Every `.by(handle, dir)` inherits it.
impl From<OrderDirection> for OrderBy {
    fn from(direction: OrderDirection) -> Self {
        match direction {
            OrderDirection::Asc => OrderBy::asc().missing_last(),
            OrderDirection::Desc => OrderBy::desc().missing_first(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserFilter {
    // Free-text relevance query across the user's analyzed `text` fields.
    q: Option<String>,
    email: Option<String>,
    email_prefix: Option<String>,
    name: Option<String>,
    tier: Option<String>,
    bio: Option<String>,
    has_profile: Option<bool>,
    city: Option<String>,
    order_status: Option<String>,
    min_orders: Option<i64>,
    // Filter OF nested: trim each returned user's `orders` to the N most recent.
    recent_orders: Option<u64>,
    // Sorting: one optional direction per sortable column. An absent column is
    // skipped by `SortBuilder::by`, so the request shapes the `sort` array with
    // no per-field branching here.
    sort_name: Option<OrderDirection>,
    sort_orders: Option<OrderDirection>,
    sort_spend: Option<OrderDirection>,
    sort_joined: Option<OrderDirection>,
    // A field inside the `orders` nested array — sorted by the same `.by` call.
    sort_recent_order: Option<OrderDirection>,
    limit: Option<u64>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new()
        .route("/users", get(list))
        .route("/users/{id}", get(get_one))
}

/// `GET /users/{id}` — fetch one document by its root primary key, or `404`.
async fn get_one(
    State(client): State<Client>,
    Path(id): Path<i32>,
) -> Result<Json<User>, ApiError> {
    User::get(&client, id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound {
            resource: "users",
            id: id.to_string(),
        })
}

async fn list(
    State(client): State<Client>,
    Query(filter): Query<UserFilter>,
) -> Result<Json<Page<User>>, ApiError> {
    // One fluent chain replaces the hand-rolled field→handle→direction dispatch:
    // each `.by` skips an absent column, the nested field needs no special case,
    // relevance leads only when there's a query, and a fallback covers the rest.
    let sorts = SortBuilder::new()
        .score_if(filter.q.is_some())
        .by(User::full_name(), filter.sort_name)
        .by(User::order_count(), filter.sort_orders)
        .by(User::lifetime_value(), filter.sort_spend)
        .by(User::account().created_at(), filter.sort_joined)
        // A field inside the `orders` nested array — the *same* one-line `.by`.
        // The nested clause (`mode: max` → the user's most recent order) is
        // derived from the handle's scope; no hand-written `nested` wrapper.
        .by(UserOrders::placed_at(), filter.sort_recent_order)
        // Stable final key so rows with equal leading values page deterministically.
        .tiebreak(User::id())
        // Used only when the request named no sort: busiest customers first.
        .or_default(User::order_count().desc())
        .build();

    let mut search = User::query()
        // Free-text `q`: one scoring `multi_match` across the analyzed `text`
        // fields, root and flattened one-to-one alike (`fullName`, `profile.bio`).
        // It scores (drives relevance ranking) where the `filter(…)` clauses
        // below only narrow. Nested `text` (addresses, order items) needs a
        // nested query and so isn't part of this cross-field match.
        .query(
            filter
                .q
                .map(|q| multi_match(q, [User::full_name(), User::profile().bio()])),
        )
        .filter(filter.email.map(|v| User::email().eq(v)))
        .filter(filter.email_prefix.map(|v| User::email().prefix(v)))
        .query(filter.name.map(|v| User::full_name().matches(v)))
        // Object (group) and one-to-one fields are flattened — queried by dotted
        // path, no wrapper — so the child struct's generated handles work directly
        // in a filter: `User::account().tier()` is a `keyword` at `account.tier`,
        // `User::profile().bio()` a `text` at `profile.bio`.
        .filter(filter.tier.map(|v| User::account().tier().eq(v)))
        .query(filter.bio.map(|v| User::profile().bio().matches(v)))
        // The object/one-to-one parent handle: existence of the whole sub-object
        // (here, whether the user has a profile at all).
        .filter(filter.has_profile.map(|has| {
            let present = User::profile().exists();
            if has { present } else { present.not() }
        }))
        // Filter BY nested one-to-many fields: keep parents with a matching child.
        .filter(
            filter
                .city
                .map(|v| User::addresses().any(UserAddresses::city().eq(v))),
        )
        .filter(
            filter
                .order_status
                .map(|v| User::orders().any(UserOrders::status().eq(v))),
        )
        .filter(filter.min_orders.map(|v| User::order_count().gte(v)))
        .sorts(sorts)
        .size(filter.limit.unwrap_or(20));

    // Filter OF nested: shape the returned `orders` array (newest first, capped)
    // without changing which users match.
    if let Some(recent) = filter.recent_orders {
        search = search.filter_nested(
            User::orders()
                .project()
                .sort(UserOrders::placed_at().desc())
                .size(recent),
        );
    }

    let response = search.send(&client).await?;
    Ok(Json(Page::from(response)))
}
