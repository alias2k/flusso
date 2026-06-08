//! The `users` index: the full document (object + one-to-one + nested arrays,
//! three levels deep) and a filterable endpoint.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_search::{Client, FlussoDocument, FlussoValue};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users")]
struct User {
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

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", path = "account")]
struct Account {
    tier: AccountTier,
    country: Option<String>,
    created_at: String,
}

// A string enum stands in for the `keyword` at `account.tier`. `FlussoValue`
// with `#[flusso(keyword)]` implements `FlussoValue<kind::Keyword>` so
// `FlussoDocument` accepts it as the field type *and* `Account::tier().eq(…)`
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

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", path = "profile")]
struct Profile {
    bio: Option<String>,
    avatar_url: Option<String>,
    birth_date: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", path = "addresses")]
struct Address {
    kind: String,
    line1: String,
    city: String,
    postal_code: Option<String>,
    country: Option<String>,
}

/// A user's nested order (distinct from the top-level `orders` index document).
#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", path = "orders")]
struct UserOrder {
    status: String,
    total: f64,
    placed_at: String,
    items: Vec<OrderLine>,
}

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "users", path = "orders.items")]
struct OrderLine {
    product_id: i32,
    quantity: i32,
    unit_price: f64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UserFilter {
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
    limit: Option<u64>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new().route("/users", get(list))
}

async fn list(
    State(client): State<Client>,
    Query(filter): Query<UserFilter>,
) -> Result<Json<Page<User>>, ApiError> {
    let mut search = User::search(&client)
        .filter(filter.email.map(|v| User::email().eq(v)))
        .filter(filter.email_prefix.map(|v| User::email().prefix(v)))
        .query(filter.name.map(|v| User::full_name().matches(v)))
        // Object (group) and one-to-one fields are flattened — queried by dotted
        // path, no wrapper — so the child struct's generated handles work directly
        // in a filter: `Account::tier()` is a `keyword` at `account.tier`,
        // `Profile::bio()` a `text` at `profile.bio`.
        .filter(filter.tier.map(|v| Account::tier().eq(v)))
        .query(filter.bio.map(|v| Profile::bio().matches(v)))
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
                .map(|v| User::addresses().any(Address::city().eq(v))),
        )
        .filter(
            filter
                .order_status
                .map(|v| User::orders().any(UserOrder::status().eq(v))),
        )
        .filter(filter.min_orders.map(|v| User::order_count().gte(v)))
        .sort(User::order_count().desc())
        .size(filter.limit.unwrap_or(20));

    // Filter OF nested: shape the returned `orders` array (newest first, capped)
    // without changing which users match.
    if let Some(recent) = filter.recent_orders {
        search = search.filter_nested(
            User::orders()
                .project()
                .sort(UserOrder::placed_at().desc())
                .size(recent),
        );
    }

    let response = search.send().await?;
    Ok(Json(Page::from(response)))
}
