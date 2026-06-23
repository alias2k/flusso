//! The `products` index document and a filterable endpoint.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use flusso_query::{Client, FlussoDocument, FlussoIndex, multi_match};
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;

// `pub(crate)`: reused by the cross-index endpoints in `global`.
#[derive(Debug, Serialize, Deserialize, FlussoDocument)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "products")]
pub(crate) struct Product {
    id: i32,
    sku: String,
    name: String,
    description: Option<String>,
    // A `map` field: localized titles keyed by language code, deserialized as a
    // `HashMap` (no derive needed — `HashMap<String, String>` is a `text` map).
    title: HashMap<String, String>,
    in_stock: bool,
    review_count: i64,
    avg_rating: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ProductFilter {
    // Free-text relevance query across `name` + `description`.
    q: Option<String>,
    sku: Option<String>,
    name: Option<String>,
    // Full-text search across every localized `title`, optionally preferring
    // the caller's `locale` (e.g. `?title=tastiera&locale=it`).
    title: Option<String>,
    locale: Option<String>,
    in_stock: Option<bool>,
    min_reviews: Option<i64>,
    min_rating: Option<f64>,
    limit: Option<u64>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new()
        .route("/products", get(list))
        .route("/products/{id}", get(get_one))
}

/// `GET /products/{id}` — fetch one document by its root primary key, or `404`.
async fn get_one(
    State(client): State<Client>,
    Path(id): Path<i32>,
) -> Result<Json<Product>, ApiError> {
    Product::get(&client, id)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::NotFound {
            resource: "products",
            id: id.to_string(),
        })
}

async fn list(
    State(client): State<Client>,
    Query(filter): Query<ProductFilter>,
) -> Result<Json<Page<Product>>, ApiError> {
    let response = Product::query()
        // Free-text `q`: a scoring cross-field match over the two analyzed
        // `text` fields. `name` (a single field) keeps its own narrower filter.
        .query(
            filter
                .q
                .map(|q| multi_match(q, [Product::name(), Product::description()])),
        )
        .filter(filter.sku.map(|v| Product::sku().eq(v)))
        .query(filter.name.map(|v| Product::name().matches(v)))
        // Cross-language title search; prefer the caller's locale when given.
        .query(filter.title.map(|v| {
            let search = Product::title().search(v);
            match filter.locale {
                Some(locale) => search.prefer(locale, 3.0),
                None => search,
            }
        }))
        .filter(filter.in_stock.map(|v| Product::in_stock().eq(v)))
        .filter(filter.min_reviews.map(|v| Product::review_count().gte(v)))
        .filter(filter.min_rating.map(|v| Product::avg_rating().gte(v)))
        .sort(Product::review_count().desc())
        .size(filter.limit.unwrap_or(20))
        .send(&client)
        .await?;
    Ok(Json(Page::from(response)))
}
