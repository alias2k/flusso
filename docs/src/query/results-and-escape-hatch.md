# Results and the escape hatch

The terminals a search can end in, the response shape, the two ways to page, and the `raw` hatch for DSL the typed builder doesn't cover.

## Terminals

| Call | Returns | Notes |
| --- | --- | --- |
| `.send(&client)` | `SearchResponse<T>` | a typed page |
| `.ids(&client)` | `Vec<String>` | matching document ids (root primary keys, stringified) in order, with `_source: false`; sort and `from`/`size` apply, `filter_nested` is dropped; after `.cursor(..)` it returns an `IdsPage` |
| `.count(&client)` | `u64` | the same query to `_count`; sort, pagination, and projections are ignored |
| `Type::get(&client, id)` | `Option<T>` | one document by id; `id` is typed as the root primary key |

`ids` is the cheap way to search in OpenSearch and load full rows from Postgres.

## The response

```rust
pub struct SearchResponse<T> {
    pub total: u64,             // total matches, not the page size
    pub max_score: Option<f32>,
    pub hits: Vec<Hit<T>>,
    pub took: std::time::Duration,
    pub timed_out: bool,        // with `shards`: `is_partial()` flags an incomplete page
    pub shards: ShardStats,
    pub next: Option<PageCursor>, // cursor paging only; see below
}

pub struct Hit<T> {
    pub id: String,
    pub score: f32,
    pub sort: Vec<serde_json::Value>, // the hit's sort values; empty with no sort
    pub source: T,              // your struct
}
```

There is no `serde_json::Value` in the common path.

## Paging

Two ways, both on `Search` and on combined search (`MultiSearch`):

| | Offset: `.from(n).size(k)` | Cursor: `.size(k).cursor(c)` |
| --- | --- | --- |
| Each page reads | the index as it is now | one snapshot, taken by the first page |
| Writes during the walk | shift later pages: duplicates, skips | invisible |
| A reindex during the walk | pages straddle two generations | invisible until the old generation is dropped, then `CursorExpired` |
| Depth | `from + size` ≤ `index.max_result_window` (10 000 by default) | unbounded |
| Jump to page N | yes | no, forward only |
| Server cost | none | one point-in-time per walk, until the last page or the keep-alive |
| Backend | OpenSearch and Elasticsearch 7.x | OpenSearch ≥ 2.4 |

Offset paging is the default and stays as it is. Use a cursor for infinite scroll, "load more", and exports.

### Cursor paging

`.cursor(None)` starts a walk; `.cursor(page.next)` continues it. The last page's `next` is `None`.

```rust
let mut cursor: Option<PageCursor> = None;
loop {
    let page = User::query()
        .filter(User::active().eq(true))
        .sort(User::created_at().desc())
        .size(500)
        .cursor(cursor)
        .send(&client)
        .await?;
    export(&page.hits);
    match page.next {
        Some(next) => cursor = Some(next),
        None => break,
    }
}
```

- **The token is opaque.** `PageCursor` is a URL-safe string (`Display`/`FromStr`, serde as a string), so an HTTP API hands it out as `next` and takes it back as `?cursor=`. The `dev/search-api` example does exactly that.
- **Rebuild the same query for every page.** The cursor is bound to the query, sort, and indexes that made it; resuming a different one is `Error::CursorMismatch`. The page size may change between pages.
- **The sort is made total.** A cursor search appends `_id` ascending (combined search: `_index`, then `_id`) unless the sort already holds it, so equal leading keys never skip or repeat a hit. With no sort at all, the walk is in `_id` order.
- **`.cursor(..)` fixes the query.** It returns a `CursorSearch` with only `keep_alive`, `send`, and `ids` (`MultiCursorSearch`: `keep_alive`, `send`). A `from` or `search_after` set before it is `Error::Cursor`.
- **`ids` pages too.** `CursorSearch::ids` returns an `IdsPage { ids, next }`.
- **`total` is the snapshot's total**, the same on every page. `track_total_hits(false)` after the first page skips counting it again.
- **The snapshot expires.** `.keep_alive(Duration)` sets how long it stays open between two pages (default one minute); every page renews it. After it lapses, or once a reindex drops the generation it pinned, the next page is `Error::CursorExpired`: start the walk again. The last page closes the snapshot; `next.close(&client)` closes an abandoned walk early. OpenSearch caps open snapshots per node (`search.max_open_pit_context`, 300 by default).
- **Nested projections** (`filter_nested`) are evaluated inside the same snapshot. Their elements still page by `from`/`size` within each parent: `inner_hits` takes no cursor.

How it works, and why: [ADR 0008](https://github.com/alias2k/flusso/blob/main/docs/adr/0008-page-cursors-are-pit-plus-search-after.md).

## The escape hatch

`raw` takes OpenSearch query DSL verbatim and still deserializes into the typed struct:

```rust
let page: SearchResponse<User> = User::query()
    .raw(serde_json::json!({
        "knn": { "embedding": { "vector": [/* … */], "k": 10 } }
    }))
    .send(&client)
    .await?;
```

It's the valve for the few types with no flusso field (`knn`, `geo_shape`, span and parent/child queries, percolators). `function_score`, `script`, `constant_score`, `query_string`, `search_after`, and the rest are in the typed surface; see [Composing queries and options](composing.md).

## Out of scope

- **Aggregations** (facets, histograms, cardinality). They need their own typed result tree; `raw` covers them meanwhile.
- **Writes.** flusso owns the index; the client never upserts or deletes.
- **Correlating hits across indexes.** Both multi-index shapes ship; joining their results is the caller's job.
- **Scroll.** Cursor paging covers both list UIs and full exports.
- **Cursors on Elasticsearch.** Its point-in-time API differs; tracked in [#157](https://github.com/alias2k/flusso/issues/157).
- **Generating the struct.** By design; the developer owns it.
