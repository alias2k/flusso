# Paging

Three ways through a long result list: by offset, re-running the query each page; by live cursor, resuming after the last hit; or by pinned cursor, walking one fixed view of the index.

## Offset or cursor

All three work on `Search` and on combined search (`MultiSearch`):

| | Offset: `.from(n).size(k)` | Live cursor: `.cursor(c)` | Pinned cursor: `.cursor(c).point_in_time()` |
| --- | --- | --- | --- |
| Each page reads | the index now | the index now | the index as of page one |
| Insert or delete before the position (already paged past) | shifts later pages: duplicates, skips | no effect | no effect |
| Insert after the position | shows up | shows up | never shows up |
| Updated content | fresh | fresh | as of page one |
| A document whose sort value changes mid-walk | may repeat or vanish | may repeat or vanish | no effect |
| A reindex mid-walk | pages straddle two generations | no effect | no effect until the old generation is dropped, then `CursorExpired` |
| `total` | recounted per page | recounted per page | fixed |
| Depth | `from + size` ≤ `index.max_result_window` (10 000 by default) | unlimited | unlimited |
| Jump to page N | yes | no, forward only | no, forward only |
| Server cost | none | none | one point-in-time per walk, until the last page or the keep-alive |
| Expires | never | never | after the keep-alive, or when its generation is dropped |
| Backend | OpenSearch, Elasticsearch 7.x | OpenSearch, Elasticsearch 7.x | OpenSearch ≥ 2.4 |

- **Offset** stays the default `Search` behavior: pages a user jumps between, a short list.
- **Live cursor** fits infinite scroll and "load more": fresh, free, and immune to inserts and deletes.
- **Pinned cursor** fits exports and reports that need one exact, consistent copy, and accept that it's frozen at page one.

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
- **Sort on something stable** for a live walk. A sort on a value that changes while the walk runs (`order_count`, `updated_at`) can show a moved document twice or skip it. Relevance near-ties can also swap between pages.
- **`.cursor(..)` fixes the query.** It returns a `CursorSearch` with only `point_in_time`, `keep_alive`, `send`, and `ids` (`MultiCursorSearch`: no `ids`). A `from` or `search_after` set before it is `Error::Cursor`.
- **`ids` pages too.** `CursorSearch::ids` returns an `IdsPage { ids, next }`.
- **A live walk follows a reindex.** Single-index walks continue on the new generation as is. A combined walk's `_index` sort value names a concrete generation, so each resumed page first asks which generation every hash alias serves now (one extra `_alias` request) and resumes from there.

## Pinned walks

`.point_in_time()` pins the walk: the first page opens an OpenSearch point-in-time, and every page reads it.

- **The token decides the mode on resume.** A pinned walk's `next` carries its point-in-time; resuming it needs no `.point_in_time()` (`PageCursor::is_pinned` tells which).
- **It expires.** `.keep_alive(Duration)` sets how long it stays open between two pages (default one minute, implies `point_in_time`); every page renews it. After it lapses, or once a reindex drops the generation it pinned, the next page is `Error::CursorExpired`: start again. The last page closes it; `next.close(&client)` closes an abandoned walk early.
- **It costs the cluster.** Each open walk holds a search context (capped by `search.max_open_pit_context`, 300 per node by default) and keeps merged-away segments on disk until it closes. Don't pin a walk that most users abandon after page one.
- **Nested projections** (`filter_nested`) read the same point-in-time. Their elements still page by `from`/`size` within each parent: `inner_hits` takes no cursor.

How it works, and why: [ADR 0008](https://github.com/alias2k/flusso/blob/main/docs/adr/0008-page-cursors-are-keyset-with-an-opt-in-point-in-time.md).

## Related

- [Results and the escape hatch](results-and-escape-hatch.md) for the response types.
- [Several indexes](several-indexes.md) for the blended list a `MultiSearch` cursor walks.
