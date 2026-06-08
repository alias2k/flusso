//! A small but production-shaped HTTP API over a flusso-maintained search index.
//!
//! It uses `flusso-search` + `#[derive(FlussoDocument)]` against the **same**
//! `../flusso.toml` the engine builds from (auto-discovered at compile time), so
//! the document types are checked against the schema and the query surface is
//! generated. Each index gets a filterable list endpoint plus a fetch-by-id
//! endpoint (`Type::get`, returning `404` when absent); `users`/`products` also
//! take a free-text `q` (a scoring `multi_match` over their analyzed fields):
//!
//! ```text
//! GET /users?q=ada&min_orders=5&tier=pro&city=London
//! GET /users/1
//! GET /products?q=keyboard&in_stock=true&min_rating=4
//! GET /products/3
//! GET /orders?status=delivered&user_id=2&min_total=100
//! GET /orders/10
//! GET /health
//! ```
//!
//! Run the dev stack (`docker compose up`, then `cargo run -- run --config
//! dev/flusso.toml` to populate), then `cargo run -p flusso-dev-search-api`.
//!
//! Layout: one module per index ([`users`], [`products`], [`orders`]) holding
//! its document types, filter, and handler; [`response`] and [`error`] are
//! shared. Each index module exposes a `routes()` the router merges.

use axum::Router;
use axum::routing::get;
use flusso_search::Client;
use tracing_subscriber::EnvFilter;

mod error;
mod orders;
mod products;
mod response;
mod users;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,flusso_search=debug")),
        )
        .with_writer(std::io::stderr)
        .init();

    let url =
        std::env::var("OPENSEARCH_URL").unwrap_or_else(|_| "http://localhost:9200".to_owned());
    let mut client = Client::connect(&url)?;
    if let (Ok(user), Ok(password)) = (
        std::env::var("OPENSEARCH_USER"),
        std::env::var("OPENSEARCH_PASSWORD"),
    ) {
        client = client.basic_auth(user, password);
    }

    let app = Router::new()
        .route("/health", get(health))
        .merge(users::routes())
        .merge(products::routes())
        .merge(orders::routes())
        .with_state(client);

    let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_owned());
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    tracing::info!(%bind, %url, "search-api listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> &'static str {
    "ok"
}
