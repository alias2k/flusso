//! The JSON response shape, and the physical index-name helper.

use flusso_query::{PageCursor, SearchResponse};
use serde::Serialize;

/// A page of hits, serialized as `{ "total": …, "hits": [ … ], "next": … }`.
/// `next` is the opaque page cursor to send back as `?cursor=` for the
/// following page — `null` on the last page (and for `/overview`'s sections,
/// which `_msearch` pages by offset).
#[derive(Serialize)]
pub(crate) struct Page<T> {
    total: u64,
    hits: Vec<Hit<T>>,
    next: Option<PageCursor>,
}

#[derive(Serialize)]
struct Hit<T> {
    id: String,
    score: f32,
    source: T,
}

impl<T> From<SearchResponse<T>> for Page<T> {
    fn from(response: SearchResponse<T>) -> Self {
        Page {
            total: response.total,
            next: response.next,
            hits: response
                .hits
                .into_iter()
                .map(|hit| Hit {
                    id: hit.id,
                    score: hit.score,
                    source: hit.source,
                })
                .collect(),
        }
    }
}
