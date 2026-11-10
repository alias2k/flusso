//! [`Walk`]: a whole cursor walk as one [`Stream`], page after page.
//!
//! A cursor search hands back one page and a [`PageCursor`] for the next. A
//! walk drives that loop for you: it fetches a page, yields its items, and
//! fetches the next page once they're consumed — no prefetch, so the caller
//! sets the pace.
//!
//! - Built by the cursor terminals: [`CursorSearch::pages`] / [`hits`] /
//!   [`id_stream`] and [`MultiCursorSearch::pages`] / [`hits`](crate::MultiCursorSearch::hits).
//!   It owns a clone of the [`Client`], so it is `'static + Send`: store it,
//!   return it, spawn it.
//! - A failed page yields its error, then the walk ends. [`Walk::cursor`]
//!   still names the failed page, so a new walk from it retries.
//! - A **live** walk holds nothing on the cluster. A **pinned** walk
//!   ([`point_in_time`]) holds a point-in-time until it ends: the last page
//!   closes it, [`Walk::close`] closes it now, dropping the walk closes it in
//!   the background, and [`Walk::into_cursor`] keeps it for a later resume.
//!
//! ```no_run
//! # use flusso_query::{Client, FlussoRoot, FlussoScope, Result, Segment};
//! # #[derive(serde::Deserialize)] struct User { email: String }
//! # impl FlussoScope for User { const PATH: &'static [Segment] = &[]; }
//! # impl FlussoRoot for User {
//! #     const INDEX: &'static str = "users";
//! #     const SCHEMA_HASH: &'static str = "xxxxxx";
//! # }
//! use futures::TryStreamExt;
//!
//! # async fn export(client: &Client) -> Result<()> {
//! let mut hits = User::query().size(500).cursor(None).hits(client);
//! while let Some(hit) = hits.try_next().await? {
//!     println!("{}", hit.source.email);
//! }
//! # Ok(())
//! # }
//! ```
//!
//! [`CursorSearch::pages`]: crate::CursorSearch::pages
//! [`hits`]: crate::CursorSearch::hits
//! [`id_stream`]: crate::CursorSearch::id_stream
//! [`MultiCursorSearch::pages`]: crate::MultiCursorSearch::pages
//! [`point_in_time`]: crate::CursorSearch::point_in_time

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;

use crate::Client;
use crate::cursor::PageCursor;
use crate::error::Result;

/// One fetched page, reduced to what a walk yields.
pub(crate) struct Fetched<I> {
    pub(crate) items: Vec<I>,
    pub(crate) next: Option<PageCursor>,
}

type PageFuture<I> = Pin<Box<dyn Future<Output = Result<Fetched<I>>> + Send>>;

/// Fetches the page after a cursor (`None`: the first page).
pub(crate) type FetchPage<I> =
    Box<dyn Fn(Client, Option<PageCursor>) -> PageFuture<I> + Send + Sync>;

/// A whole cursor walk as a [`Stream`] of `Result<I>`: pages
/// ([`SearchResponse`](crate::SearchResponse)), hits ([`Hit`](crate::Hit)), or
/// ids (`String`), depending on the terminal that built it.
///
/// Consume it with `futures::TryStreamExt` (`try_next`, `try_collect`, …).
/// Pages are fetched one at a time, as the previous page's items run out.
///
/// How it ends:
/// - **After the last page.** A pinned walk's point-in-time is closed then.
/// - **On a failed page.** The error is yielded, then the stream ends;
///   [`cursor`](Self::cursor) still names the failed page, so a new walk from it
///   retries.
/// - **[`close`](Self::close)**: now, reporting a failed close.
/// - **[`into_cursor`](Self::into_cursor)**: now, keeping a pinned walk's
///   point-in-time open for a later resume.
/// - **Dropped early** (`break`, `?`, a cancelled request): a pinned walk's
///   point-in-time is closed in the background on the current tokio runtime;
///   with none, it expires after its keep-alive. A first page dropped while its
///   request is still in flight may have opened one the walk never saw; that
///   one expires on its own too.
///
/// A live walk holds nothing on the cluster, so every ending is free for it.
pub struct Walk<I> {
    client: Client,
    fetch: FetchPage<I>,
    buffer: VecDeque<I>,
    /// Resumes after the last page fetched; `None` before the first page of a
    /// new walk and after the last page.
    cursor: Option<PageCursor>,
    in_flight: Option<PageFuture<I>>,
    finished: bool,
}

impl<I> Walk<I> {
    pub(crate) fn new(client: &Client, start: Option<PageCursor>, fetch: FetchPage<I>) -> Self {
        Self {
            client: client.clone(),
            fetch,
            buffer: VecDeque::new(),
            cursor: start,
            in_flight: None,
            finished: false,
        }
    }

    /// The checkpoint: the cursor that resumes right after the **last page
    /// fetched**. Hand it to a new `.cursor(..)` to continue later.
    ///
    /// Exact for [`pages`](crate::CursorSearch::pages). For hits and ids, the
    /// items of the current page not yet consumed are skipped on resume. `None`
    /// before the first page of a new walk, and once the walk is finished.
    #[must_use]
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }

    /// Whether the walk ended: its last page was fetched, or a page failed.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// End the walk now. A pinned walk closes its point-in-time and reports a
    /// failure to; a live walk holds nothing, so this is free.
    pub async fn close(mut self) -> Result<()> {
        self.in_flight = None;
        match self.cursor.take() {
            Some(cursor) => cursor.close(&self.client).await,
            None => Ok(()),
        }
    }

    /// End the walk but keep its position: the [`cursor`](Self::cursor),
    /// with a pinned walk's point-in-time left **open** (until its keep-alive
    /// lapses), so a later `.cursor(..)` resumes it.
    #[must_use]
    pub fn into_cursor(mut self) -> Option<PageCursor> {
        self.cursor.take()
    }
}

// Nothing in a walk is pinned in place: the in-flight page is its own
// `Pin<Box<_>>`, and items only ever move in and out of the buffer.
impl<I> Unpin for Walk<I> {}

impl<I> Stream for Walk<I> {
    type Item = Result<I>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if let Some(item) = this.buffer.pop_front() {
                return Poll::Ready(Some(Ok(item)));
            }
            if this.finished {
                return Poll::Ready(None);
            }
            let page = this
                .in_flight
                .get_or_insert_with(|| (this.fetch)(this.client.clone(), this.cursor.clone()));
            let fetched = match page.as_mut().poll(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(fetched) => fetched,
            };
            this.in_flight = None;
            match fetched {
                Ok(Fetched { items, next }) => {
                    this.finished = next.is_none();
                    this.cursor = next;
                    this.buffer.extend(items);
                }
                Err(error) => {
                    this.finished = true;
                    return Poll::Ready(Some(Err(error)));
                }
            }
        }
    }
}

impl<I> Drop for Walk<I> {
    /// A pinned walk dropped before its end closes its point-in-time in the
    /// background. `Drop` can't await, so the close is spawned on the current
    /// tokio runtime; with none (or one shutting down) it expires on its own.
    fn drop(&mut self) {
        let Some(cursor) = self.cursor.take().filter(PageCursor::is_pinned) else {
            return;
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let client = self.client.clone();
            runtime.spawn(async move {
                if let Err(error) = cursor.close(&client).await {
                    tracing::warn!(%error, "closing a dropped walk's point-in-time failed");
                }
            });
        }
    }
}

impl<I> fmt::Debug for Walk<I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Walk")
            .field("cursor", &self.cursor)
            .field("buffered", &self.buffer.len())
            .field("in_flight", &self.in_flight.is_some())
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}
