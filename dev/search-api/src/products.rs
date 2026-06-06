//! The `products` index document and a filterable endpoint.

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_search::{Client, FlussoDocument};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;

#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "products")]
struct Product {
    id: i32,
    sku: String,
    name: String,
    in_stock: bool,
    review_count: i64,
    avg_rating: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProductFilter {
    sku: Option<String>,
    name: Option<String>,
    in_stock: Option<bool>,
    min_reviews: Option<i64>,
    min_rating: Option<f64>,
    limit: Option<u64>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new().route("/products", get(list))
}

async fn list(
    State(client): State<Client>,
    Query(filter): Query<ProductFilter>,
) -> Result<Json<Page<Product>>, ApiError> {
    let response = Product::search(&client)
        .filter(filter.sku.map(|v| Product::sku().eq(v)))
        .query(filter.name.map(|v| Product::name().matches(v)))
        .filter(filter.in_stock.map(|v| Product::in_stock().eq(v)))
        .filter(filter.min_reviews.map(|v| Product::review_count().gte(v)))
        .filter(filter.min_rating.map(|v| Product::avg_rating().gte(v)))
        .sort(Product::review_count().desc())
        .size(filter.limit.unwrap_or(20))
        .send()
        .await?;
    Ok(Json(Page::from(response)))
}
