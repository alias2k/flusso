//! The JSON response shape, and the physical index-name helper.

use flusso_search::SearchResponse;
use serde::Serialize;

/// A page of hits, serialized as `{ "total": …, "hits": [ … ] }`.
#[derive(Serialize)]
pub(crate) struct Page<T> {
    total: u64,
    hits: Vec<Hit<T>>,
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
