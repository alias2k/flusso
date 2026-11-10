# Paging

Two ways through a long result list: by offset, re-running the query each page, or by page cursor, walking one snapshot of the index.

## Offset or cursor

Both work on `Search` and on combined search (`MultiSearch`):

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

## Cursor paging

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
- **Rebuild the same query for every page.** The cursor is bound to the query, sort, and (prefixed) indexes that made it; resuming a different one is `Error::CursorMismatch`. The page size may change between pages, and a cursor from `send` also resumes `ids`.
- **The sort is made total.** A cursor search appends `_id` ascending (combined search: `_index`, then `_id`) unless the sort already holds it, so equal leading keys never skip or repeat a hit. With no sort at all, the walk keeps relevance order: `_score` descending, then the tiebreak.
- **`.cursor(..)` fixes the query.** It returns a `CursorSearch` with only `keep_alive`, `send`, and `ids` (`MultiCursorSearch`: `keep_alive`, `send`). A `from` or `search_after` set before it is `Error::Cursor`.
- **`ids` pages too.** `CursorSearch::ids` returns an `IdsPage { ids, next }`.
- **`total` is the snapshot's total**, the same on every page. `track_total_hits(false)` after the first page skips counting it again.
- **The snapshot expires.** `.keep_alive(Duration)` sets how long it stays open between two pages (default one minute); every page renews it. After it lapses, or once a reindex drops the generation it pinned, the next page is `Error::CursorExpired`: start the walk again. The last page closes the snapshot; `next.close(&client)` closes an abandoned walk early. OpenSearch caps open snapshots per node (`search.max_open_pit_context`, 300 by default).
- **Nested projections** (`filter_nested`) are evaluated inside the same snapshot. Their elements still page by `from`/`size` within each parent: `inner_hits` takes no cursor.

How it works, and why: [ADR 0008](https://github.com/alias2k/flusso/blob/main/docs/adr/0008-page-cursors-are-pit-plus-search-after.md).

## Related

- [Results and the escape hatch](results-and-escape-hatch.md) for the response types.
- [Several indexes](several-indexes.md) for the blended list a `MultiSearch` cursor walks.
