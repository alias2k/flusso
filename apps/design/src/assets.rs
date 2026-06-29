//! Serving the embedded SPA.
//!
//! The designer's frontend is built (`apps/design/frontend/` → `dist/`) and
//! embedded into the binary at compile time via `rust-embed`, so a published
//! `flusso` needs no external files to serve the UI. Unknown paths fall back to
//! `index.html` for client-side routing.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dist"]
struct Assets;

/// Serve an embedded asset by request path, falling back to `index.html`.
pub(crate) async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
            file.data,
        )
            .into_response();
    }

    match Assets::get("index.html") {
        Some(file) => ([(header::CONTENT_TYPE, "text/html".to_owned())], file.data).into_response(),
        None => (StatusCode::NOT_FOUND, "asset not found").into_response(),
    }
}
