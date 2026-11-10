# Streaming a walk

A cursor search can hand back the whole walk as one `futures::Stream` of pages, hits, or ids, fetched one page at a time, instead of a loop over `page.next`.

## The terminals

```rust
use futures::TryStreamExt;

let mut hits = Order::query()
    .filter(Order::status().eq("delivered"))
    .size(500)
    .cursor(None)
    .hits(&client);
while let Some(hit) = hits.try_next().await? {
    export(hit.source);
}
```

| Terminal | Yields | On |
| --- | --- | --- |
| `.pages(&client)` | `SearchResponse<T>`, one per page | `CursorSearch`, `MultiCursorSearch` |
| `.hits(&client)` | `Hit<T>` | `CursorSearch`, `MultiCursorSearch` |
| `.id_stream(&client)` | `String`, the document id | `CursorSearch` |

Each returns a `Walk`, which implements `futures::Stream<Item = Result<…>>`. The terminal consumes the cursor search and keeps its own clone of the client, so a walk is `'static + Send`: return it from a function, hand it to an HTTP body, or spawn it.

- **It walks the cursor search as built.** `.cursor(None)` starts at the top, `.cursor(Some(c))` resumes after `c`, and `.point_in_time()` makes it a [pinned walk](paging.md#pinned-walks). Live by default, like a single page.
- **One page at a time.** The next page is fetched once the current page's items are consumed. Nothing is prefetched, so a slow consumer slows the walk down rather than piling pages up in memory.
- **`pages` keeps the per-page details** (`total`, `took`, the partial-result flags). Use it when those matter, or for exact checkpoints (below).

> **New to Rust streams?** A `Stream` is the async version of an `Iterator`. The methods that consume it (`try_next`, `try_collect`, `map_ok`, …) come from the `futures` crate's `TryStreamExt`/`StreamExt` traits, so add `futures = "0.3"` to the consuming crate and `use futures::TryStreamExt;`.

## Errors and retries

A failed page yields its error, then the stream ends: the next poll returns `None`.

- `walk.cursor()` still names the page that failed, so a new walk from it retries that page.
- After `Error::CursorMismatch` (the cursor belongs to a different query) or `Error::CursorExpired` (a pinned walk's point-in-time is gone), the walk keeps no cursor. There's nothing to retry; start a new walk from `None`.

## Checkpoints and resuming

`walk.cursor()` is the checkpoint: the cursor that resumes right after the **last page fetched**. Like any [page cursor](paging.md#cursor-paging), it's an opaque string, so it can be stored in a database or a job's state and parsed back later.

- **With `pages`, it's exact.** When a page arrives, `walk.cursor()` already points past it, so "process the page, then save the cursor" never repeats or skips a hit.
- **With `hits` and `id_stream`, it's per page, not per item.** Resuming skips whatever was left unconsumed of the current page.
- **A pinned walk keeps its point-in-time only through `walk.into_cursor()`.** Dropping the walk closes it (see below), which turns a saved pinned cursor into `CursorExpired`. A job that pauses for longer than the keep-alive should walk live, sorted on a stable field.

## Ending a walk

A live walk holds nothing on the cluster, so it can end any way it likes. A pinned walk holds a point-in-time until it ends:

| The walk… | Its point-in-time |
| --- | --- |
| runs to its last page | closed by that page |
| `walk.close().await` | closed now; a failed close is returned |
| `walk.into_cursor()` | left open, so the returned cursor resumes it, until the keep-alive lapses |
| is dropped early (`break`, `?`, a cancelled request, a disconnected client) | closed in the background on the current tokio runtime; outside one, it expires after the keep-alive |

Two edge cases expire on their own instead: a first page dropped while its request is still in flight (the walk hasn't learned the point-in-time's id yet), and a walk dropped outside any tokio runtime.

## The `Walk` methods

| Method | Does |
| --- | --- |
| `cursor()` | the checkpoint: resumes after the last page fetched. `None` before the first page and after the last |
| `is_finished()` | whether the walk ended: its last page arrived, or a page failed |
| `close().await` | ends the walk now; closes a pinned walk's point-in-time and returns a failed close |
| `into_cursor()` | ends the walk now and returns its cursor, leaving a pinned walk's point-in-time open |

## Recipe: an export over HTTP

Stream every match to the client as NDJSON, one line per document, without holding the result set in memory. With axum:

```rust
use axum::body::Body;
use axum::http::header;
use axum::response::IntoResponse;
use axum::BoxError;
use futures::StreamExt;

async fn export(State(client): State<Client>) -> impl IntoResponse {
    let walk = Order::query()
        .size(500)
        .cursor(None)
        .point_in_time() // one consistent copy, however long the download takes
        .hits(&client);
    let lines = walk.map(|hit| -> Result<Vec<u8>, BoxError> {
        let mut line = serde_json::to_vec(&hit?.source)?;
        line.push(b'\n');
        Ok(line)
    });
    ([(header::CONTENT_TYPE, "application/x-ndjson")], Body::from_stream(lines))
}
```

- **Pages follow the download.** The next page is fetched only as the client reads; a slow client means slow paging, not a growing buffer.
- **A disconnect cleans up.** axum drops the body, which drops the walk, which closes its point-in-time.
- **An error can only cut the body short.** The status and headers are already sent when the first page is read, so log the error (the `dev/search-api` example's `GET /orders/export` does) and let the client detect the truncated stream.

## Recipe: a resumable batch job

Walk every match in pages and save the checkpoint after each one, so a crash or a deploy resumes where it stopped:

```rust
use futures::TryStreamExt;

let start = load_checkpoint()?.map(|token| token.parse::<PageCursor>()).transpose()?;
let mut pages = User::query()
    .sort(User::created_at().asc()) // stable: a live walk must not sort on a value that changes
    .size(1000)
    .cursor(start)
    .pages(&client);
while let Some(page) = pages.try_next().await? {
    process(&page.hits).await?;
    match pages.cursor() {
        Some(next) => save_checkpoint(&next.to_string())?,
        None => mark_done()?, // the last page: a re-run must not start over
    }
}
```

- **Walk live for a job that can pause.** A pinned cursor dies with its keep-alive; a live cursor never expires.
- **Saving after processing is at-least-once.** A crash between `process` and `save_checkpoint` re-runs that one page, so make `process` idempotent.

## Related

- [Paging](paging.md) for offset vs live vs pinned cursors, and what each sees of concurrent writes.
- [Results and the escape hatch](results-and-escape-hatch.md) for `SearchResponse` and `Hit`.
- [Several indexes](several-indexes.md) for the blended list a `MultiCursorSearch` walks.
