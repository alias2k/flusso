//! The `orders` index document and a filterable endpoint.

use axum::extract::{Query, State};
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
    Router::new().route("/orders", get(list))
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
