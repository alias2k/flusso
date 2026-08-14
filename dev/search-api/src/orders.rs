//! The `orders` index document and a filterable endpoint.

// Note: the `orders` document has no analyzed `text` field (status is a
// `keyword`, the rest numeric/date), so there's no free-text `q` here — unlike
// users/products. Filter it by its exact and range fields below.
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_query::{Client, Decimal, FlussoDocument, FlussoRoot, Sortable};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;

// `pub(crate)`: reused by the cross-index endpoints in `global`.
#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "orders")]
pub(crate) struct Order {
    id: i32,
    user_id: i32,
    status: String,
    // A `decimal` column → a `Decimal` handle, queried with `Decimal` (no `f64`
    // cast). Needs the `decimal` feature on `flusso-query`.
    total: Decimal,
    item_count: i64,
    units_sold: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OrderFilter {
    user_id: Option<i32>,
    status: Option<String>,
    min_total: Option<Decimal>,
    min_items: Option<i64>,
    // `status` sorts by the declared enum lifecycle (pending → … → cancelled);
    // anything else (or absent) sorts by `total` descending.
    sort: Option<String>,
    limit: Option<u64>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new()
        .route("/orders", get(list))
        .route("/orders/{id}", get(get_one))
}

/// `GET /orders/{id}` — fetch one document by its root primary key, or `404`.
async fn get_one(
    State(client): State<Client>,
    Path(id): Path<i32>,
) -> Result<Json<Order>, ApiError> {
    Order::get(&client, id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound {
            resource: "orders",
            id: id.to_string(),
        })
}

async fn list(
    State(client): State<Client>,
    Query(filter): Query<OrderFilter>,
) -> Result<Json<Page<Order>>, ApiError> {
    let query = Order::query()
        .filter(filter.user_id.map(|v| Order::user_id().eq(v)))
        .filter(filter.status.map(|v| Order::status().eq(v)))
        .filter(filter.min_total.map(|v| Order::total().gte(v)))
        .filter(filter.min_items.map(|v| Order::item_count().gte(v)))
        .size(filter.limit.unwrap_or(20));
    // `sort=status` sorts by the enum's declared order (pending → paid → shipped
    // → delivered → cancelled) — an ordered enum, so `.asc()` uses that rank, not
    // alphabetical. Any other value falls back to biggest-total-first.
    let query = if filter.sort.as_deref() == Some("status") {
        query.sort(Order::status().asc())
    } else {
        query.sort(Order::total().desc())
    };
    let response = query.send(&client).await?;
    Ok(Json(Page::from(response)))
}
