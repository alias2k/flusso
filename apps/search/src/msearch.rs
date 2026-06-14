//! [`Client::msearch`] — several typed searches in one `_msearch` round-trip.
//!
//! Each search keeps its own index, body, and document type; OpenSearch
//! answers per slot, in order, and each slot decodes with its own type. The
//! bundle is a tuple of `&Search<T>` (arity 1–8, types may differ per slot),
//! so the searches survive the call and stay reusable. For many searches of
//! *one* type, see [`Client::msearch_all`].

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::search::merge_inner_hits;
use crate::{Client, Search, SearchResponse};

impl Client {
    /// Run several typed searches in one `_msearch` request, returning one
    /// typed [`SearchResponse`] per search, in slot order. The bundle is a
    /// tuple of `&Search<T>` whose document types may differ:
    ///
    /// ```no_run
    /// # use flusso_search::{Client, Search};
    /// # #[derive(serde::Deserialize)] struct User { email: String }
    /// # #[derive(serde::Deserialize)] struct Order { status: String }
    /// # async fn run() -> flusso_search::Result<()> {
    /// # let client = Client::connect("http://localhost:9200")?;
    /// # let users_query = Search::<User>::new("users", "xxxxxx");
    /// # let orders_query = Search::<Order>::new("orders", "yyyyyy");
    /// let (users, orders) = client.msearch((&users_query, &orders_query)).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// A slot-level failure fails the whole call with [`Error::Msearch`]
    /// naming the slot — there are no partial results.
    #[tracing::instrument(
        name = "search.msearch",
        skip_all,
        fields(searches = B::LEN),
        err,
    )]
    pub async fn msearch<B: MsearchBundle>(&self, bundle: B) -> Result<B::Output> {
        let envelope = self.msearch_raw(bundle.ndjson()?).await?;
        let raw: RawMsearchResponse = serde_json::from_value(envelope)?;
        bundle.decode(raw.responses)
    }

    /// Run many searches of **one** document type in a single `_msearch`
    /// request, returning one [`SearchResponse`] per search, in order. The
    /// heterogeneous (mixed-type) form is [`Client::msearch`].
    #[tracing::instrument(
        name = "search.msearch_all",
        skip_all,
        fields(searches = searches.len()),
        err,
    )]
    pub async fn msearch_all<T>(&self, searches: &[Search<T>]) -> Result<Vec<SearchResponse<T>>>
    where
        T: DeserializeOwned,
    {
        if searches.is_empty() {
            return Ok(Vec::new());
        }
        let mut lines = String::new();
        for search in searches {
            append_lines(search, &mut lines)?;
        }
        let envelope = self.msearch_raw(lines).await?;
        let raw: RawMsearchResponse = serde_json::from_value(envelope)?;
        let mut entries = raw.responses.into_iter();
        searches
            .iter()
            .enumerate()
            .map(|(slot, search)| decode_slot(search, slot, entries.next()))
            .collect()
    }
}

/// A bundle of searches runnable in one [`Client::msearch`] request —
/// implemented for tuples of `&Search<T>` up to arity 8, each slot with its
/// own document type. You don't implement this; you pass tuples.
pub trait MsearchBundle {
    /// One typed [`SearchResponse`] per slot, in order.
    type Output;

    /// How many searches the bundle holds.
    const LEN: usize;

    /// Render the bundle as `_msearch` NDJSON: a `{"index": …}` header line
    /// and a body line per slot.
    fn ndjson(&self) -> Result<String>;

    /// Decode the envelope's `responses` entries, in slot order.
    fn decode(&self, responses: Vec<Value>) -> Result<Self::Output>;
}

/// Append one search's two NDJSON lines: the `{"index": …}` header (the
/// physical index, exactly what the sink writes) and the `_search` body.
fn append_lines<T>(search: &Search<T>, ndjson: &mut String) -> Result<()> {
    let header = serde_json::to_string(&json!({ "index": search.physical_index() }))?;
    let body = serde_json::to_string(&search.body())?;
    ndjson.push_str(&header);
    ndjson.push('\n');
    ndjson.push_str(&body);
    ndjson.push('\n');
    Ok(())
}

/// Decode one slot: surface its per-slot error if present, merge inner hits
/// for the slot's own nested projections, then decode the typed page.
fn decode_slot<T>(
    search: &Search<T>,
    slot: usize,
    entry: Option<Value>,
) -> Result<SearchResponse<T>>
where
    T: DeserializeOwned,
{
    let mut entry = entry.ok_or_else(|| Error::Msearch {
        slot,
        status: 0,
        body: "missing response slot".to_owned(),
    })?;
    if let Some(error) = entry.get("error") {
        let status = entry
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .unwrap_or(0);
        return Err(Error::Msearch {
            slot,
            status,
            body: error.to_string(),
        });
    }
    let paths = search.nested_paths();
    if !paths.is_empty() {
        merge_inner_hits(&mut entry, &paths);
    }
    SearchResponse::from_value(entry)
}

/// The `_msearch` response envelope.
#[derive(Deserialize)]
struct RawMsearchResponse {
    responses: Vec<Value>,
}

/// Implement [`MsearchBundle`] for one tuple arity of `&Search<T>`.
macro_rules! impl_msearch_bundle {
    ($len:expr => $( $T:ident . $idx:tt ),+) => {
        impl<$($T),+> MsearchBundle for ($(&Search<$T>,)+)
        where
            $($T: DeserializeOwned,)+
        {
            type Output = ($(SearchResponse<$T>,)+);

            const LEN: usize = $len;

            fn ndjson(&self) -> Result<String> {
                let mut lines = String::new();
                $( append_lines(self.$idx, &mut lines)?; )+
                Ok(lines)
            }

            fn decode(&self, responses: Vec<Value>) -> Result<Self::Output> {
                let mut entries = responses.into_iter();
                Ok(( $( decode_slot(self.$idx, $idx, entries.next())?, )+ ))
            }
        }
    };
}

impl_msearch_bundle!(1 => T0.0);
impl_msearch_bundle!(2 => T0.0, T1.1);
impl_msearch_bundle!(3 => T0.0, T1.1, T2.2);
impl_msearch_bundle!(4 => T0.0, T1.1, T2.2, T3.3);
impl_msearch_bundle!(5 => T0.0, T1.1, T2.2, T3.3, T4.4);
impl_msearch_bundle!(6 => T0.0, T1.1, T2.2, T3.3, T4.4, T5.5);
impl_msearch_bundle!(7 => T0.0, T1.1, T2.2, T3.3, T4.4, T5.5, T6.6);
impl_msearch_bundle!(8 => T0.0, T1.1, T2.2, T3.3, T4.4, T5.5, T6.6, T7.7);
