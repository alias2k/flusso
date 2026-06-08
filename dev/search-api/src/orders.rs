//! The `orders` index document and a filterable endpoint.

// Note: the `orders` document has no analyzed `text` field (status is a
// `keyword`, the rest numeric/date), so there's no free-text `q` here — unlike
// users/products. Filter it by its exact and range fields below.
use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_search::{Client, FlussoDocument};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "orders")]
struct Order {
    id: i32,
    user_id: i32,
    status: String,
    total: f64,
    item_count: i64,
    units_sold: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OrderFilter {
    user_id: Option<i32>,
    status: Option<String>,
    min_total: Option<f64>,
    min_items: Option<i64>,
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
    let response = Order::search(&client)
        .filter(filter.user_id.map(|v| Order::user_id().eq(v)))
        .filter(filter.status.map(|v| Order::status().eq(v)))
        .filter(filter.min_total.map(|v| Order::total().gte(v)))
        .filter(filter.min_items.map(|v| Order::item_count().gte(v)))
        .sort(Order::total().desc())
        .size(filter.limit.unwrap_or(20))
        .send()
        .await?;
    Ok(Json(Page::from(response)))
}
