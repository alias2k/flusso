//! Cross-index endpoints — the two multi-index shapes side by side:
//!
//! - `GET /search` — **blended**: one query over `users` + `products`, hits
//!   ranked together in a single list (a [`FlussoMultiDocument`] union, each
//!   hit decoded into its variant by the index it came from).
//! - `GET /overview` — **sectioned**: three independent typed searches (own
//!   query, sort, and type per section) sharing one `_msearch` round-trip.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_query::{Client, FlussoIndex, FlussoMultiDocument, multi_match};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::orders::Order;
use crate::products::Product;
use crate::response::Page;
use crate::users::{Profile, User};

/// One item in the blended `/search` result list — the union of the indexes a
/// global search box should reach (`orders` stays out: it has no analyzed
/// text to match a search box against). The derive lists each variant's index
/// and decodes every hit into the variant matching its physical `_index`; the
/// serde `tag` labels each hit in the JSON: `{ "type": "user", … }` /
/// `{ "type": "product", … }`.
#[derive(Debug, Serialize, FlussoMultiDocument)]
#[serde(tag = "type", rename_all = "camelCase")]
enum SearchItem {
    User(User),
    Product(Product),
}

#[derive(Debug, Deserialize)]
struct SearchFilter {
    /// The search-box text (required — this endpoint *is* the search box).
    q: String,
    limit: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OverviewFilter {
    /// Optional search-box text, applied to the users and products sections.
    q: Option<String>,
    /// Narrow the orders section to one status (`pending`, `shipped`, …).
    status: Option<String>,
    /// Per-section page size.
    limit: Option<u64>,
}

/// The `/overview` response: one independently-shaped section per index.
#[derive(Serialize)]
struct Overview {
    users: Page<User>,
    products: Page<Product>,
    orders: Page<Order>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new()
        .route("/search", get(search))
        .route("/overview", get(overview))
}

/// `GET /search?q=ada` — one relevance-ranked list across users and products.
///
/// A single `multi_match` spans both documents' analyzed fields: a field
/// unmapped in one index simply doesn't match there, so the clause is shared.
/// No sort — the blended list is ordered by relevance, which is always valid
/// across indexes (a field sort would have to exist in every index).
async fn search(
    State(client): State<Client>,
    Query(filter): Query<SearchFilter>,
) -> Result<Json<Page<SearchItem>>, ApiError> {
    let response = SearchItem::query()
        .query(multi_match(
            filter.q,
            [
                User::full_name(),
                Profile::bio(),
                Product::name(),
                Product::description(),
            ],
        ))
        .size(filter.limit.unwrap_or(20))
        .send(&client)
        .await?;
    Ok(Json(Page::from(response)))
}

/// `GET /overview?q=ada&status=delivered` — a dashboard in one round-trip.
///
/// Each section keeps its own query, sort, and document type — that's what
/// `_msearch` is for; the queries are plain values built without the client,
/// which appears once in the `msearch` call.
async fn overview(
    State(client): State<Client>,
    Query(filter): Query<OverviewFilter>,
) -> Result<Json<Overview>, ApiError> {
    let limit = filter.limit.unwrap_or(5);

    let users = User::query()
        .query(
            filter
                .q
                .clone()
                .map(|q| multi_match(q, [User::full_name(), Profile::bio()])),
        )
        .sort(User::order_count().desc())
        .size(limit);
    let products = Product::query()
        .query(
            filter
                .q
                .map(|q| multi_match(q, [Product::name(), Product::description()])),
        )
        .sort(Product::review_count().desc())
        .size(limit);
    let orders = Order::query()
        .filter(filter.status.map(|v| Order::status().eq(v)))
        .sort(Order::total().desc())
        .size(limit);

    let (users, products, orders) = client.msearch((&users, &products, &orders)).await?;
    Ok(Json(Overview {
        users: Page::from(users),
        products: Page::from(products),
        orders: Page::from(orders),
    }))
}
