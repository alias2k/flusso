//! Page cursors: resume an ordered result list right after a page's last hit.
//!
//! Offset paging (`from`/`size`) re-runs the query and skips `from` hits, so on
//! an index flusso keeps current from the WAL it drifts: an insert or delete
//! ahead of the window shifts every later page, and `from + size` stops at
//! `index.max_result_window`. A page cursor resumes *after a sort position*
//! instead (ADR 0008):
//!
//! - Each page searches with `search_after` the previous page's last sort
//!   values. The sort is made total by appending `_id` (and `_index` for a
//!   combined search) unless the caller's sort already holds it; an unsorted
//!   search keeps relevance order, with `_score` descending first.
//! - **Live** (the default): every page reads the index as it is now. Inserts
//!   and deletes ahead of the position never shift the walk, nothing is held
//!   open on the server, and nothing expires. A resumed union walk first
//!   points its `_index` values at the generation each hash alias serves now,
//!   so a reindex mid-walk repeats nothing.
//! - **Pinned** ([`point_in_time`](crate::CursorSearch::point_in_time)): the
//!   first page opens an OpenSearch point-in-time and every page reads it — no
//!   later write, no reindex — until the last page closes it or its keep-alive
//!   lapses ([`Error::CursorExpired`]).
//! - The [`PageCursor`] handed back in [`SearchResponse::next`] carries the
//!   sort values (and a pinned walk's point-in-time id) as an **opaque
//!   string**, safe to pass through an HTTP API and back. Its docs carry the
//!   walk example.
//!
//! [`SearchResponse::next`]: crate::SearchResponse::next

use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::Client;
use crate::error::{Error, Result};

/// The token version this crate writes and reads.
const VERSION: u8 = 1;

/// How long a pinned walk's point-in-time stays open between two pages,
/// unless [`keep_alive`](crate::CursorSearch::keep_alive) says otherwise.
pub(crate) const DEFAULT_KEEP_ALIVE: Duration = Duration::from_secs(60);

/// The page size OpenSearch uses when a search names none.
const DEFAULT_SIZE: u64 = 10;

/// An opaque token that resumes a cursor walk right after the page it came
/// with. Get one from [`SearchResponse::next`](crate::SearchResponse::next) and
/// hand it back to [`Search::cursor`](crate::Search::cursor).
///
/// It renders as a URL-safe string ([`Display`](fmt::Display) /
/// [`FromStr`], and serde as a plain string), so it can travel through an
/// HTTP API unchanged. It is bound to the query (and the prefixed indexes)
/// that produced it: resuming a different query fails with
/// [`Error::CursorMismatch`]. It is not signed — editing it only changes the
/// caller's own pages.
///
/// Start a walk with `None`, continue with the previous page's `next`:
///
/// ```no_run
/// # use flusso_query::{Client, FlussoRoot, FlussoScope, PageCursor, Result, Segment};
/// # #[derive(serde::Deserialize)] struct User { email: String }
/// # impl FlussoScope for User { const PATH: &'static [Segment] = &[]; }
/// # impl FlussoRoot for User {
/// #     const INDEX: &'static str = "users";
/// #     const SCHEMA_HASH: &'static str = "xxxxxx";
/// # }
/// # async fn walk(client: &Client) -> Result<()> {
/// let mut cursor: Option<PageCursor> = None;
/// loop {
///     let page = User::query().size(500).cursor(cursor).send(client).await?;
///     for hit in &page.hits {
///         println!("{}", hit.source.email);
///     }
///     match page.next {
///         Some(next) => cursor = Some(next),
///         None => break,
///     }
/// }
/// # Ok(())
/// # }
/// ```
///
/// A walk is **live** by default: each page reads the index as it is now, and
/// a cursor never expires. A document whose *sort value* changes mid-walk can
/// show twice or not at all. To read one fixed view instead, pin the walk with
/// [`point_in_time`](crate::CursorSearch::point_in_time) (OpenSearch ≥ 2.4): it
/// lives for the [`keep_alive`](crate::CursorSearch::keep_alive), renewed by
/// each page; once it lapses, or a reindex drops the generation it pinned, the
/// next page is [`Error::CursorExpired`]: start the walk again.
#[derive(Debug, Clone, PartialEq)]
pub struct PageCursor {
    /// The point-in-time a pinned walk reads; `None` for a live walk.
    pit: Option<String>,
    after: Vec<Value>,
    fingerprint: u64,
}

#[derive(Serialize, Deserialize)]
struct Token {
    v: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pit: Option<String>,
    after: Vec<Value>,
    fp: String,
}

impl PageCursor {
    pub(crate) fn new(pit: Option<String>, after: Vec<Value>, fingerprint: u64) -> Self {
        Self {
            pit,
            after,
            fingerprint,
        }
    }

    /// End a [pinned](crate::CursorSearch::point_in_time) walk now: close its
    /// point-in-time instead of letting it expire. The last page closes it on
    /// its own; call this when a pinned walk is abandoned early. A live walk
    /// holds nothing, so this is a no-op; closing an expired point-in-time is
    /// not an error either.
    pub async fn close(&self, client: &Client) -> Result<()> {
        match &self.pit {
            Some(pit) => client.close_pit(pit).await,
            None => Ok(()),
        }
    }

    /// Whether this walk is pinned to a point-in-time.
    #[must_use]
    pub fn is_pinned(&self) -> bool {
        self.pit.is_some()
    }

    /// Reject a cursor handed back with a different query than the one that
    /// produced it.
    pub(crate) fn check(&self, fingerprint: u64) -> Result<()> {
        if self.fingerprint == fingerprint {
            Ok(())
        } else {
            Err(Error::CursorMismatch)
        }
    }
}

impl fmt::Display for PageCursor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let token = Token {
            v: VERSION,
            pit: self.pit.clone(),
            after: self.after.clone(),
            fp: format!("{:016x}", self.fingerprint),
        };
        let json = serde_json::to_vec(&token).map_err(|_| fmt::Error)?;
        f.write_str(&URL_SAFE_NO_PAD.encode(json))
    }
}

impl FromStr for PageCursor {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let malformed = |reason: &str| Error::Cursor(format!("malformed page cursor: {reason}"));
        let bytes = URL_SAFE_NO_PAD
            .decode(s.trim())
            .map_err(|_| malformed("not base64url"))?;
        let token: Token = serde_json::from_slice(&bytes).map_err(|_| malformed("not a token"))?;
        if token.v != VERSION {
            return Err(malformed("unknown version"));
        }
        let fingerprint =
            u64::from_str_radix(&token.fp, 16).map_err(|_| malformed("bad fingerprint"))?;
        Ok(Self::new(token.pit, token.after, fingerprint))
    }
}

impl Serialize for PageCursor {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for PageCursor {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        raw.parse().map_err(serde::de::Error::custom)
    }
}

/// A stable 64-bit FNV-1a hash of what a cursor is bound to: the request body
/// (query, total sort, filters — everything but the page size and position)
/// and the unprefixed index path. Hand-rolled because std's
/// hasher may change between Rust releases, and a token outlives a deploy.
pub(crate) fn fingerprint(bound: &Value, path: &str) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    bound
        .to_string()
        .bytes()
        .chain(path.bytes())
        .fold(OFFSET, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        })
}

/// One cursor page's request, independent of how its hits decode: what
/// [`CursorSearch`](crate::CursorSearch) and
/// [`MultiCursorSearch`](crate::MultiCursorSearch) both hand to [`fetch_page`].
pub(crate) struct PageRequest<'a> {
    /// The unprefixed physical path: one index or a comma-joined list.
    pub(crate) path: &'a str,
    /// The search's own body (query, sort, size, …) without `from`/`search_after`.
    pub(crate) body: Map<String, Value>,
    /// Sort keys appended, in order, when the caller's sort lacks them.
    pub(crate) tiebreak: &'static [&'static str],
    /// The unprefixed physical index of every target the path addresses
    /// (`{INDEX}_{SCHEMA_HASH}`), for rewriting `_index` sort values to the
    /// current generation on a live resume.
    pub(crate) targets: Vec<String>,
    pub(crate) cursor: Option<&'a PageCursor>,
    /// Open a point-in-time on the first page. A resumed walk follows its
    /// cursor instead.
    pub(crate) pinned: bool,
    pub(crate) keep_alive: Duration,
}

/// A fetched cursor page: the raw `_search` response and the cursor that
/// resumes after it (`None` on the last page).
pub(crate) struct FetchedPage {
    pub(crate) response: Value,
    pub(crate) next: Option<PageCursor>,
}

/// Append each `tiebreak` key the sort doesn't already hold, ascending — so the
/// sort is total and `search_after` never skips or repeats a hit with equal
/// leading keys.
pub(crate) fn total_sort(mut sort: Vec<Value>, tiebreak: &[&str]) -> Vec<Value> {
    for key in tiebreak {
        let present = sort.iter().any(|entry| sort_key(entry) == Some(key));
        if !present {
            let mut order = Map::new();
            order.insert("order".to_string(), Value::String("asc".to_string()));
            let mut entry = Map::new();
            entry.insert((*key).to_string(), Value::Object(order));
            sort.push(Value::Object(entry));
        }
    }
    sort
}

/// The field a rendered sort entry sorts on: `{ "<key>": {…} }` or `"<key>"`.
fn sort_key(entry: &Value) -> Option<&str> {
    match entry {
        Value::String(key) => Some(key),
        Value::Object(map) => map.keys().next().map(String::as_str),
        _ => None,
    }
}

/// The keys that shape how a page is *delivered*, not which hits it holds or
/// their order — left out of the fingerprint, so a walk may change its page
/// size, and a cursor from `send` resumes `ids` of the same search.
const UNBOUND_KEYS: &[&str] = &[
    "size",
    "_source",
    "highlight",
    "track_total_hits",
    "track_scores",
];

/// A cursor page's request body and the fingerprint its cursor is bound to.
pub(crate) struct PageBody {
    pub(crate) body: Value,
    pub(crate) fingerprint: u64,
}

/// The sort a cursor page runs with: the search's own, made total with the
/// tiebreak. With no sort, the walk keeps relevance order: `_score`
/// descending leads, exactly what an unsorted search ranks by.
pub(crate) fn effective_sort(request: &PageRequest<'_>) -> Vec<Value> {
    let sort = match request.body.get("sort") {
        Some(Value::Array(sort)) if !sort.is_empty() => sort.clone(),
        _ => vec![crate::handles::Sort::score().to_value()],
    };
    total_sort(sort, request.tiebreak)
}

/// The body a cursor page POSTs: the search's body with the total `sort`,
/// the point-in-time when pinned, and `search_after` when resuming.
/// `bound_path` is the physical path as requested (prefix applied), so a
/// cursor can't cross deployments.
pub(crate) fn page_body(
    request: &PageRequest<'_>,
    pit: Option<&str>,
    bound_path: &str,
    after: Option<Vec<Value>>,
) -> PageBody {
    let mut body = request.body.clone();
    body.insert("sort".to_string(), Value::Array(effective_sort(request)));
    let mut bound = body.clone();
    for key in UNBOUND_KEYS {
        bound.remove(*key);
    }
    let fingerprint = fingerprint(&Value::Object(bound), bound_path);

    if let Some(pit) = pit {
        let mut pit_clause = Map::new();
        pit_clause.insert("id".to_string(), Value::String(pit.to_owned()));
        pit_clause.insert(
            "keep_alive".to_string(),
            Value::String(keep_alive_param(request.keep_alive)),
        );
        body.insert("pit".to_string(), Value::Object(pit_clause));
    }
    if let Some(after) = after {
        body.insert("search_after".to_string(), Value::Array(after));
    }
    PageBody {
        body: Value::Object(body),
        fingerprint,
    }
}

/// Point each `_index` value of a live cursor's `search_after` at the
/// generation its hash alias serves **now**.
///
/// A hit's `_index` is the concrete generation (`users_x_1`). After a reindex
/// the alias serves `users_x_2`, and resuming after `users_x_1` would repeat
/// every hit tied on the leading keys. `current` maps each prefixed hash alias
/// to its concrete index; a value whose target is unknown, or whose alias
/// isn't listed (an un-aliased legacy index), is left alone.
pub(crate) fn rewrite_generations(
    after: &mut [Value],
    sort: &[Value],
    targets: &[String],
    prefix: &str,
    current: &HashMap<String, String>,
) {
    let positions = sort
        .iter()
        .enumerate()
        .filter(|(_, entry)| sort_key(entry) == Some("_index"))
        .map(|(position, _)| position);
    for position in positions {
        let Some(Value::String(concrete)) = after.get_mut(position) else {
            continue;
        };
        let stripped = concrete.strip_prefix(prefix).unwrap_or(concrete);
        let target = targets.iter().find(|target| {
            stripped == target.as_str() || crate::multi::is_generation_of(stripped, target)
        });
        if let Some(now) = target.and_then(|target| current.get(&format!("{prefix}{target}"))) {
            concrete.clone_from(now);
        }
    }
}

/// A keep-alive as an OpenSearch time value, in whole milliseconds (at least 1).
pub(crate) fn keep_alive_param(keep_alive: Duration) -> String {
    format!("{}ms", keep_alive.as_millis().max(1))
}

/// Run one cursor page.
///
/// - **Live** (the default): a plain `search_after` search on the path. A
///   resumed union walk first rewrites its `_index` values to the current
///   generations ([`rewrite_generations`]).
/// - **Pinned**: open the point-in-time (first page) or follow the cursor's,
///   search it, and close it after the last page — or after a first page that
///   fails, which would otherwise hold it until the keep-alive lapses.
pub(crate) async fn fetch_page(client: &Client, request: PageRequest<'_>) -> Result<FetchedPage> {
    let bound_path = client.prefixed(request.path);
    let (pit, opened) = match request.cursor {
        Some(cursor) => (cursor.pit.clone(), false),
        None if request.pinned => (
            Some(client.open_pit(request.path, request.keep_alive).await?),
            true,
        ),
        None => (None, false),
    };
    let after = match request.cursor {
        None => None,
        Some(cursor) => {
            let mut after = cursor.after.clone();
            let sort = effective_sort(&request);
            let sorts_on_index = sort.iter().any(|entry| sort_key(entry) == Some("_index"));
            if cursor.pit.is_none() && sorts_on_index {
                let current = client.current_generations(request.path).await?;
                rewrite_generations(
                    &mut after,
                    &sort,
                    &request.targets,
                    &client.index_prefix,
                    &current,
                );
            }
            Some(after)
        }
    };
    let PageBody { body, fingerprint } = page_body(&request, pit.as_deref(), &bound_path, after);
    if let Some(cursor) = request.cursor {
        cursor.check(fingerprint)?;
    }
    let size = body
        .get("size")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_SIZE);
    let searched = match &pit {
        Some(_) => client.search_pit(&body).await,
        None => client.search_at(request.path, &body).await,
    };
    let response = match searched {
        Ok(response) => response,
        Err(error) => {
            if let (true, Some(pit)) = (opened, &pit) {
                close_quietly(client, pit).await;
            }
            return Err(error);
        }
    };
    let pit = pit.map(|pit| {
        response
            .get("pit_id")
            .and_then(Value::as_str)
            .map_or(pit, str::to_owned)
    });
    let next = match next_after(&response, size) {
        Some(after) => Some(PageCursor::new(pit, after, fingerprint)),
        None => {
            if let Some(pit) = &pit {
                close_quietly(client, pit).await;
            }
            None
        }
    };
    Ok(FetchedPage { response, next })
}

/// Close a point-in-time best-effort: a failure only costs a context until its
/// keep-alive lapses, so it is logged, not returned.
async fn close_quietly(client: &Client, pit: &str) {
    if let Err(error) = client.close_pit(pit).await {
        tracing::warn!(%error, "closing a pinned cursor walk's point-in-time failed");
    }
}

/// The last hit's sort values when the page came back full (more may follow);
/// `None` for a short or empty page — the end of the walk.
pub(crate) fn next_after(response: &Value, size: u64) -> Option<Vec<Value>> {
    let hits = response.get("hits")?.get("hits")?.as_array()?;
    if size == 0 || (hits.len() as u64) < size {
        return None;
    }
    hits.last()?.get("sort")?.as_array().cloned()
}
