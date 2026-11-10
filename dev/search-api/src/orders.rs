//! The `orders` index document, a filterable endpoint, and a streaming export.

// Note: the `orders` document has no analyzed `text` field (status is a
// `keyword`, the rest numeric/date), so there's no free-text `q` here — unlike
// users/products. Filter it by its exact and range fields below.
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{BoxError, Json, Router};
use flusso_query::{Client, Decimal, FlussoRoot, PageCursor, Sortable};
use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;
use crate::response::Page;
use crate::shared::{LineItem, OrderStatus};

// `pub(crate)`: reused by the cross-index endpoints in `global`.
#[derive(Debug, Serialize, Deserialize, FlussoRoot)]
#[serde(rename_all = "camelCase")]
#[flusso(index = "orders")]
pub(crate) struct Order {
    id: i32,
    user_id: i32,
    status: OrderStatus,
    // A `decimal` column → a `Decimal` handle, queried with `Decimal` (no `f64`
    // cast). Needs the `decimal` feature on `flusso-query`.
    total: Decimal,
    item_count: i64,
    units_sold: Option<i32>,
    // The *shared* fragment — the same one `users` embeds three levels deep.
    items: Vec<LineItem>,
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
    /// The previous page's `next`, to continue a cursor walk; absent for page one.
    cursor: Option<PageCursor>,
}

pub(crate) fn routes() -> Router<Client> {
    Router::new()
        .route("/orders", get(list))
        .route("/orders/export", get(export))
        .route("/orders/{id}", get(get_one))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ExportFilter {
    user_id: Option<i32>,
    status: Option<String>,
}

/// `GET /orders/export?status=delivered` — every matching order as NDJSON,
/// streamed as it's read.
///
/// A pinned walk: the export is one consistent copy of the index, however long
/// it takes to download. Each page is fetched only once the client has taken
/// the previous one, and if the client disconnects, axum drops the body, the
/// walk with it, and the walk closes its point-in-time. The status and headers
/// go out before the first page is read, so an error mid-export can only cut
/// the body short; it is logged here.
async fn export(
    State(client): State<Client>,
    Query(filter): Query<ExportFilter>,
) -> impl IntoResponse {
    let walk = Order::query()
        .filter(filter.user_id.map(|v| Order::user_id().eq(v)))
        .filter(filter.status.map(|v| Order::status().eq(v)))
        .size(500)
        .cursor(None)
        .point_in_time()
        .hits(&client);
    let lines = walk.map(|hit| -> Result<Vec<u8>, BoxError> {
        let hit = hit.inspect_err(|error| tracing::warn!(%error, "order export cut short"))?;
        let mut line = serde_json::to_vec(&hit.source)?;
        line.push(b'\n');
        Ok(line)
    });
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(lines),
    )
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
    let response = query.cursor(filter.cursor).send(&client).await?;
    Ok(Json(Page::from(response)))
}
