//! Sort keys: [`SortOrder`], [`SortMode`], and the [`Sort`] builder produced by
//! `.asc()` / `.desc()` on a sortable handle (or `Geo::distance_sort`,
//! [`Sort::score`], [`Sort::script`]).
//!
//! A [`Sort`] carries the key it sorts on (a field path, `_score`,
//! `_geo_distance`, or `_script`) plus its options (`missing`, `mode`,
//! `unmapped_type`, …); `.missing_first()` / `.mode(..)` chain onto it, and it
//! renders to one entry in the `sort` array. The typed handle is always the
//! entry point — there is no public string-path sort.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use super::{Geo, GeoPoint, NumericType, ScriptSortType};
use crate::query::{AsQuery, Root};
use crate::{FlussoDocument, nested_boundaries};

/// Sort direction.
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

impl SortOrder {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

/// How a multi-valued field collapses to one sort value.
#[derive(Debug, Clone, Copy)]
pub enum SortMode {
    /// Smallest value.
    Min,
    /// Largest value.
    Max,
    /// Arithmetic mean (numeric fields).
    Avg,
    /// Sum (numeric fields).
    Sum,
    /// Median (numeric fields).
    Median,
}

impl SortMode {
    fn as_str(self) -> &'static str {
        match self {
            SortMode::Min => "min",
            SortMode::Max => "max",
            SortMode::Avg => "avg",
            SortMode::Sum => "sum",
            SortMode::Median => "median",
        }
    }
}

/// A single sort key. Produced by `.asc()` / `.desc()` on a sortable handle, by
/// [`Sort::score`] / [`Sort::script`], or by `Geo::distance_sort`; chain the
/// option setters (`missing_first`, `mode`, `unmapped_type`, …) onto it.
#[derive(Debug, Clone)]
pub struct Sort {
    key: String,
    body: Map<String, Value>,
    /// What [`SortBuilder`] dedups on. Equals `key` for a normal sort; a
    /// `_script` map-key sort sets it to its field path so several still coexist.
    dedup_id: String,
    /// `Some` for a map-key `_script` sort — redirects `missing_*` into the
    /// script's `params.missing` (the `missing` body field is ignored on a
    /// `_script` sort) with a direction-correct sentinel for the value kind.
    script_kind: Option<MapSortValueKind>,
}

impl Sort {
    /// A field/order sort: `{ "<field>": { "order": "asc"|"desc" } }`.
    pub(crate) fn new(field: &str, order: SortOrder) -> Self {
        let mut body = Map::new();
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        Self::plain(field.to_string(), body)
    }

    /// A sort with no script-missing redirection, deduped on its render key.
    fn plain(key: String, body: Map<String, Value>) -> Self {
        Self {
            dedup_id: key.clone(),
            key,
            body,
            script_kind: None,
        }
    }

    /// Sort by relevance `_score` (descending by default).
    #[must_use]
    pub fn score() -> Self {
        let mut body = Map::new();
        body.insert("order".to_string(), Value::String("desc".to_string()));
        Self::plain("_score".to_string(), body)
    }

    /// Sort by a computed script value. `script_type` is the emitted value type
    /// ([`ScriptSortType::Number`] / [`ScriptSortType::String`]); `source` is
    /// the painless expression.
    #[must_use]
    pub fn script(
        script_type: ScriptSortType,
        source: impl Into<String>,
        order: SortOrder,
    ) -> Self {
        let mut script = Map::new();
        script.insert("source".to_string(), Value::String(source.into()));
        let mut body = Map::new();
        body.insert(
            "type".to_string(),
            Value::String(script_type.as_str().to_string()),
        );
        body.insert("script".to_string(), Value::Object(script));
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        Self::plain("_script".to_string(), body)
    }

    /// A pre-built sort clause (e.g. `_geo_distance`).
    pub(crate) fn from_parts(key: String, body: Map<String, Value>) -> Self {
        Self::plain(key, body)
    }

    /// A map-key `_script` sort: deduped on `dedup_id` (the field path, so two
    /// map sorts coexist) with `missing_*` redirecting into `params.missing`.
    pub(crate) fn map_script(
        dedup_id: String,
        body: Map<String, Value>,
        kind: MapSortValueKind,
    ) -> Self {
        Self {
            key: "_script".to_string(),
            body,
            dedup_id,
            script_kind: Some(kind),
        }
    }

    /// A field sort that is **nesting-aware**: it reads the scope `S`'s path and,
    /// when the field sits inside one or more `nested` arrays, wraps the sort in
    /// the matching `nested` chain (and defaults `mode` from the direction —
    /// `asc → min`, `desc → max`). A root or flattened-object field (empty path)
    /// renders a plain sort. Backs every [`Sortable`] handle.
    pub(crate) fn field<S: FlussoDocument>(path: &str, order: SortOrder) -> Self {
        let mut sort = Sort::new(path, order);
        let boundaries = nested_boundaries(S::PATH);
        if let Some(nested) = nested_clause(&boundaries) {
            sort.body.insert("nested".to_string(), nested);
            sort.body.insert(
                "mode".to_string(),
                Value::String(default_mode(order).to_string()),
            );
        }
        sort
    }

    /// Sort ascending.
    #[must_use]
    pub fn asc(mut self) -> Self {
        self.body
            .insert("order".to_string(), Value::String("asc".to_string()));
        self
    }

    /// Sort descending.
    #[must_use]
    pub fn desc(mut self) -> Self {
        self.body
            .insert("order".to_string(), Value::String("desc".to_string()));
        self
    }

    /// Place documents missing this field first. On a map-key sort this resolves
    /// to a direction-correct sentinel in `params.missing` (a `_script` sort
    /// ignores the `missing` body field).
    #[must_use]
    pub fn missing_first(mut self) -> Self {
        match self.script_kind {
            Some(kind) => {
                let value = missing_sentinel(false, self.current_order(), kind);
                self.set_script_missing(value);
            }
            None => {
                self.body
                    .insert("missing".to_string(), Value::String("_first".to_string()));
            }
        }
        self
    }

    /// Place documents missing this field last. On a map-key sort this resolves
    /// to a direction-correct sentinel in `params.missing`.
    #[must_use]
    pub fn missing_last(mut self) -> Self {
        match self.script_kind {
            Some(kind) => {
                let value = missing_sentinel(true, self.current_order(), kind);
                self.set_script_missing(value);
            }
            None => {
                self.body
                    .insert("missing".to_string(), Value::String("_last".to_string()));
            }
        }
        self
    }

    /// Substitute a literal value for documents missing this field. On a map-key
    /// sort this becomes the `params.missing` fallback value.
    #[must_use]
    pub fn missing(mut self, value: impl Into<Value>) -> Self {
        let value = value.into();
        match self.script_kind {
            Some(_) => self.set_script_missing(value),
            None => {
                self.body.insert("missing".to_string(), value);
            }
        }
        self
    }

    /// This sort's current direction, read back from the rendered body.
    fn current_order(&self) -> SortOrder {
        match self.body.get("order").and_then(Value::as_str) {
            Some("desc") => SortOrder::Desc,
            _ => SortOrder::Asc,
        }
    }

    /// Write `params.missing` for a `_script` sort (no-op if the shape is unexpected).
    fn set_script_missing(&mut self, value: Value) {
        if let Some(params) = self
            .body
            .get_mut("script")
            .and_then(Value::as_object_mut)
            .and_then(|script| script.get_mut("params"))
            .and_then(Value::as_object_mut)
        {
            params.insert("missing".to_string(), value);
        }
    }

    /// How a multi-valued field reduces to one sort value.
    #[must_use]
    pub fn mode(mut self, mode: SortMode) -> Self {
        self.body
            .insert("mode".to_string(), Value::String(mode.as_str().to_string()));
        self
    }

    /// Type to assume when the field is unmapped on some shard (instead of
    /// failing the search), e.g. `"long"`. A no-op on a map-key `_script` sort —
    /// it's a field-sort option with no meaning there.
    #[must_use]
    pub fn unmapped_type(mut self, unmapped_type: impl Into<String>) -> Self {
        if self.script_kind.is_none() {
            self.body.insert(
                "unmapped_type".to_string(),
                Value::String(unmapped_type.into()),
            );
        }
        self
    }

    /// Numeric type to sort as ([`NumericType`]), for cross-index type coercion.
    /// A no-op on a map-key `_script` sort — a field-sort-only option.
    #[must_use]
    pub fn numeric_type(mut self, numeric_type: NumericType) -> Self {
        if self.script_kind.is_none() {
            self.body.insert(
                "numeric_type".to_string(),
                Value::String(numeric_type.as_str().to_string()),
            );
        }
        self
    }

    /// Date `format` for a `date` field sort. A no-op on a map-key `_script`
    /// sort — a field-sort-only option.
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        if self.script_kind.is_none() {
            self.body
                .insert("format".to_string(), Value::String(format.into()));
        }
        self
    }

    /// Sort by a field inside a `nested` array scoped to `path`, considering
    /// only elements matching `filter`. An escape hatch for the rare
    /// filter-scoped nested sort; ordinary nested sorts come from a
    /// [`Sortable`] handle, which derives the (possibly multi-level) `nested`
    /// chain from the field's scope automatically.
    #[must_use]
    pub fn nested_filtered<S>(mut self, path: impl Into<String>, filter: impl AsQuery<S>) -> Self {
        let mut nested = Map::new();
        nested.insert("path".to_string(), Value::String(path.into()));
        if let Some(query) = filter.into_query() {
            nested.insert("filter".to_string(), query.to_value());
        }
        self.body
            .insert("nested".to_string(), Value::Object(nested));
        self
    }

    pub(crate) fn to_value(&self) -> Value {
        let mut outer = Map::new();
        outer.insert(self.key.clone(), Value::Object(self.body.clone()));
        Value::Object(outer)
    }

    /// What [`SortBuilder`] dedups on — the render key for a normal sort, or the
    /// field path for a map-key `_script` sort (so several still coexist).
    pub(crate) fn dedup_id(&self) -> &str {
        &self.dedup_id
    }

    /// Drop the `nested` chain (and its companion `mode`) for use inside
    /// `inner_hits`: there the sort already runs within the nested document, so
    /// the field path is relative and no wrapper applies. A plain or `_score`
    /// sort is unchanged.
    pub(crate) fn without_nested_context(mut self) -> Self {
        if self.body.remove("nested").is_some() {
            self.body.remove("mode");
        }
        self
    }
}

/// Wrap a plain sort in the `nested` chain for `boundaries` (cumulative dotted
/// paths, outermost first). `None` when there is no nesting.
fn nested_clause(boundaries: &[String]) -> Option<Value> {
    let (path, rest) = boundaries.split_first()?;
    let mut clause = Map::new();
    clause.insert("path".to_string(), Value::String(path.clone()));
    if let Some(inner) = nested_clause(rest) {
        clause.insert("nested".to_string(), inner);
    }
    Some(Value::Object(clause))
}

/// The `mode` a nested sort defaults to for `order`: the smallest element value
/// when ascending, the largest when descending (so the chosen element is the one
/// that sorts the parent extremally).
fn default_mode(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Asc => "min",
        SortOrder::Desc => "max",
    }
}

/// A handle that can produce a [`Sort`]. The compile-time gate for
/// [`SortBuilder::by`] / [`tiebreak`](SortBuilder::tiebreak): implemented for the
/// orderable leaf handles (`Keyword`, `Text`, `Number<K>`, `Date`, `Bool`) and
/// for a [`MapKeySort`] (`Type::field().sort_key(..)`), but **not** for a bare
/// `Geo` / `Object` / map handle — so `by(geo_handle, …)` or `by(map_handle, …)`
/// fails to compile (geo sorts go through [`SortBuilder::near`] /
/// [`raw`](SortBuilder::raw); a map sorts via its `sort_key`).
///
/// `.asc()` / `.desc()` are nesting-aware: a field (or map) inside one or more
/// `nested` arrays renders the matching `nested` chain automatically, from the
/// scope.
pub trait Sortable {
    /// Sort ascending.
    fn asc(&self) -> Sort;
    /// Sort descending.
    fn desc(&self) -> Sort;
}

/// Where to place documents missing the sorted field (a field-sort `missing`).
#[derive(Debug, Clone)]
pub enum Missing {
    /// Missing values sort first (`_first`).
    First,
    /// Missing values sort last (`_last`).
    Last,
    /// Missing values take this substitute value.
    Value(Value),
}

/// The doc-values shape of a `map`'s values, which decides how
/// [`MapKeySort`] reads a key: the exact `.keyword` subfield + lowercasing for a
/// string map, the bare numeric/date field otherwise.
#[derive(Debug, Clone, Copy)]
pub(crate) enum MapSortValueKind {
    /// `text`/`keyword` values — sort on the dynamic `.keyword` subfield,
    /// lowercased for case-insensitive order (parity with scalar string sort).
    String,
    /// Numeric values — sort on the bare key field.
    Number,
    /// Date values — sort on the bare key field, by epoch millis.
    Date,
}

/// `for (def f : params.fields) { if present → return … }` then the `missing`
/// fallback. Walking `params.fields` in order is the whole point: it's the
/// **key fallback** (sort by the first preferred key a document actually has).
const STRING_SOURCE: &str = "for (def f : params.fields) { if (doc.containsKey(f) && doc[f].size() > 0) { return doc[f].value.toLowerCase(); } } return params.missing;";
const NUMBER_SOURCE: &str = "for (def f : params.fields) { if (doc.containsKey(f) && doc[f].size() > 0) { return doc[f].value; } } return params.missing;";
const DATE_SOURCE: &str = "for (def f : params.fields) { if (doc.containsKey(f) && doc[f].size() > 0) { return doc[f].value.toInstant().toEpochMilli(); } } return params.missing;";

/// The `params.missing` sentinel that places key-less documents first or last on
/// a map-key `_script` sort, given the direction and value kind. The extreme
/// flips with direction so the rule holds either way: missing-last is a high
/// value under `asc`, a low value under `desc`.
fn missing_sentinel(last: bool, order: SortOrder, kind: MapSortValueKind) -> Value {
    let high = last == matches!(order, SortOrder::Asc);
    match kind {
        MapSortValueKind::String => Value::String(if high {
            "\u{10ffff}".to_string()
        } else {
            String::new()
        }),
        MapSortValueKind::Number | MapSortValueKind::Date => {
            Value::from(if high { i64::MAX } else { i64::MIN })
        }
    }
}

/// A sort over a dynamic-key `map` field by an **ordered fallback of keys** —
/// "sort by `it`, else `en`, …". Built with `*Map::sort_key("it").or("en")` and
/// used like any other sortable handle: `SortBuilder::by(handle, dir)`, or
/// `.asc()` / `.desc()` for a bare [`Sort`].
///
/// It renders a `_script` sort whose painless source walks the keys in order and
/// sorts by the value of the **first one a document has** — true fallback, so a
/// row with only `en` still orders by `en` (not lexicographic tiers). String
/// maps sort case-insensitively on the dynamic `.keyword` subfield; numeric/date
/// maps on the bare key (epoch millis for dates). Nesting-aware via scope `S`,
/// exactly like a field sort.
///
/// Documents with **none** of the keys sort first under `.asc()` / last under
/// `.desc()` by default; place them explicitly with `.missing_first()` /
/// `.missing_last()` / `.missing(value)` on the produced [`Sort`] (or via the
/// [`OrderBy`] passed to [`SortBuilder::by`]) — these redirect into the script's
/// fallback value, so they work despite a `_script` sort ignoring `missing`.
///
/// ```
/// use flusso_query::{Root, SortBuilder, SortOrder, TextMap};
///
/// // Italian, falling back to English — through the normal `by`.
/// let sorts = SortBuilder::new()
///     .by(TextMap::<Root>::at("name").sort_key("it").or("en"), SortOrder::Desc)
///     .build();
/// assert_eq!(sorts.len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct MapKeySort<S = Root> {
    path: String,
    keys: Vec<String>,
    kind: MapSortValueKind,
    _scope: PhantomData<fn() -> S>,
}

impl<S> MapKeySort<S> {
    pub(crate) fn new(path: String, key: impl Into<String>, kind: MapSortValueKind) -> Self {
        Self {
            path,
            keys: vec![key.into()],
            kind,
            _scope: PhantomData,
        }
    }

    /// Add the next fallback key, tried only for documents missing every key so
    /// far. Chain several for a longer preference order (`it` → `en` → `de`).
    #[must_use]
    pub fn or(mut self, key: impl Into<String>) -> Self {
        self.keys.push(key.into());
        self
    }

    fn leaf_field(&self, key: &str) -> String {
        match self.kind {
            MapSortValueKind::String => format!("{}.{key}.keyword", self.path),
            MapSortValueKind::Number | MapSortValueKind::Date => format!("{}.{key}", self.path),
        }
    }
}

impl<S: FlussoDocument> MapKeySort<S> {
    fn build(&self, order: SortOrder) -> Sort {
        let fields: Vec<Value> = self
            .keys
            .iter()
            .map(|key| Value::String(self.leaf_field(key)))
            .collect();

        let (sort_type, source, default_missing) = match self.kind {
            MapSortValueKind::String => ("string", STRING_SOURCE, Value::String(String::new())),
            MapSortValueKind::Number => ("number", NUMBER_SOURCE, Value::from(0)),
            MapSortValueKind::Date => ("number", DATE_SOURCE, Value::from(0)),
        };

        let mut params = Map::new();
        params.insert("fields".to_string(), Value::Array(fields));
        params.insert("missing".to_string(), default_missing);

        let mut script = Map::new();
        script.insert("source".to_string(), Value::String(source.to_string()));
        script.insert("params".to_string(), Value::Object(params));

        let mut body = Map::new();
        body.insert("type".to_string(), Value::String(sort_type.to_string()));
        body.insert("script".to_string(), Value::Object(script));
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );

        let boundaries = nested_boundaries(S::PATH);
        if let Some(nested) = nested_clause(&boundaries) {
            body.insert("nested".to_string(), nested);
            body.insert(
                "mode".to_string(),
                Value::String(default_mode(order).to_string()),
            );
        }

        Sort::map_script(self.path.clone(), body, self.kind)
    }
}

/// `.asc()` / `.desc()` build the `_script` sort; a bare value defaults to `asc`.
impl<S: FlussoDocument> Sortable for MapKeySort<S> {
    fn asc(&self) -> Sort {
        self.build(SortOrder::Asc)
    }
    fn desc(&self) -> Sort {
        self.build(SortOrder::Desc)
    }
}

impl<S: FlussoDocument> From<MapKeySort<S>> for Sort {
    fn from(map_sort: MapKeySort<S>) -> Self {
        map_sort.build(SortOrder::Asc)
    }
}

impl<S: FlussoDocument> From<MapKeySort<S>> for Option<Sort> {
    fn from(map_sort: MapKeySort<S>) -> Self {
        Some(map_sort.into())
    }
}

/// A field sort minus the field — a direction plus the field-sort modifiers,
/// ready to attach to whatever handle [`SortBuilder::by`] is given.
///
/// This is what a consumer converts its own request enum into, once
/// (`impl From<MyDir> for OrderBy`). It carries full parity with [`Sort`]'s
/// field-sort options; everything but the direction defaults to unset.
#[derive(Debug, Clone)]
pub struct OrderBy {
    order: SortOrder,
    missing: Option<Missing>,
    mode: Option<SortMode>,
    numeric_type: Option<NumericType>,
    unmapped_type: Option<String>,
    format: Option<String>,
}

impl OrderBy {
    /// Ascending, no modifiers.
    #[must_use]
    pub fn asc() -> Self {
        Self::new(SortOrder::Asc)
    }

    /// Descending, no modifiers.
    #[must_use]
    pub fn desc() -> Self {
        Self::new(SortOrder::Desc)
    }

    fn new(order: SortOrder) -> Self {
        Self {
            order,
            missing: None,
            mode: None,
            numeric_type: None,
            unmapped_type: None,
            format: None,
        }
    }

    /// Place documents missing this field first.
    #[must_use]
    pub fn missing_first(mut self) -> Self {
        self.missing = Some(Missing::First);
        self
    }

    /// Place documents missing this field last.
    #[must_use]
    pub fn missing_last(mut self) -> Self {
        self.missing = Some(Missing::Last);
        self
    }

    /// Substitute a literal value for documents missing this field.
    #[must_use]
    pub fn missing(mut self, value: impl Into<Value>) -> Self {
        self.missing = Some(Missing::Value(value.into()));
        self
    }

    /// How a multi-valued field reduces to one sort value.
    #[must_use]
    pub fn mode(mut self, mode: SortMode) -> Self {
        self.mode = Some(mode);
        self
    }

    /// Numeric type to sort as, for cross-index type coercion.
    #[must_use]
    pub fn numeric_type(mut self, numeric_type: NumericType) -> Self {
        self.numeric_type = Some(numeric_type);
        self
    }

    /// Type to assume when the field is unmapped on some shard.
    #[must_use]
    pub fn unmapped_type(mut self, unmapped_type: impl Into<String>) -> Self {
        self.unmapped_type = Some(unmapped_type.into());
        self
    }

    /// Date `format` for a `date` field sort.
    #[must_use]
    pub fn format(mut self, format: impl Into<String>) -> Self {
        self.format = Some(format.into());
        self
    }

    /// Build the field [`Sort`] for `handle`, in this order with these modifiers.
    fn into_sort<H: Sortable>(self, handle: &H) -> Sort {
        let mut sort = match self.order {
            SortOrder::Asc => handle.asc(),
            SortOrder::Desc => handle.desc(),
        };
        sort = match self.missing {
            Some(Missing::First) => sort.missing_first(),
            Some(Missing::Last) => sort.missing_last(),
            Some(Missing::Value(value)) => sort.missing(value),
            None => sort,
        };
        if let Some(mode) = self.mode {
            sort = sort.mode(mode);
        }
        if let Some(numeric_type) = self.numeric_type {
            sort = sort.numeric_type(numeric_type);
        }
        if let Some(unmapped_type) = self.unmapped_type {
            sort = sort.unmapped_type(unmapped_type);
        }
        if let Some(format) = self.format {
            sort = sort.format(format);
        }
        sort
    }
}

impl From<SortOrder> for OrderBy {
    fn from(order: SortOrder) -> Self {
        Self::new(order)
    }
}

/// The optionality carrier for [`SortBuilder::by`]: an absent order skips the
/// field. A local newtype because coherence forbids `impl From<…> for Option<_>`.
///
/// `SortOrder`, `OrderBy`, and — via the umbrella impl — `Option<T: Into<OrderBy>>`
/// all flow in, so a consumer's `Option<MyDir>` self-skips on `None` after one
/// `impl From<MyDir> for OrderBy`.
#[derive(Debug, Clone)]
pub struct MaybeOrderBy(Option<OrderBy>);

impl From<OrderBy> for MaybeOrderBy {
    fn from(order: OrderBy) -> Self {
        Self(Some(order))
    }
}

impl From<SortOrder> for MaybeOrderBy {
    fn from(order: SortOrder) -> Self {
        Self(Some(order.into()))
    }
}

impl<T: Into<OrderBy>> From<Option<T>> for MaybeOrderBy {
    fn from(order: Option<T>) -> Self {
        Self(order.map(Into::into))
    }
}

/// Builds the `sort` array, one fluent verb per concern — each absorbing its own
/// optionality so a request maps straight through with no per-field `if let`.
///
/// `by`/`near`/`tiebreak`/`or_default` **dedup** by sort key (first wins), so a
/// field added twice — or an explicit sort that a tiebreak/default would repeat —
/// appears once; `raw` is exempt. `or_default` only contributes when the builder
/// would otherwise be empty.
///
/// ```
/// use flusso_query::{SortBuilder, SortOrder, OrderBy};
/// # use flusso_query::{Keyword, Number, kind, Root};
/// # fn keyword(p: &str) -> Keyword<Root> { Keyword::at(p) }
/// # fn count() -> Number<kind::Long, Root> { Number::at("orderCount") }
/// let sorts = SortBuilder::new()
///     .score_if(true)
///     .by(count(), SortOrder::Desc)
///     .by(keyword("city"), None::<OrderBy>)   // skipped
///     .tiebreak(keyword("id"))
///     .build();
/// assert_eq!(sorts.len(), 3);                 // _score, orderCount, id
/// ```
#[derive(Debug, Default)]
pub struct SortBuilder {
    sorts: Vec<Sort>,
    fallback: Option<Sort>,
}

impl SortBuilder {
    /// An empty builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Push `sort` unless its dedup id is already present (first wins).
    fn push_unique(&mut self, sort: Sort) {
        if !self
            .sorts
            .iter()
            .any(|existing| existing.dedup_id() == sort.dedup_id())
        {
            self.sorts.push(sort);
        }
    }

    /// Sort by a field — or by a map key with fallback
    /// (`Type::field().sort_key("it").or("en")`). `dir` accepts a [`SortOrder`],
    /// an [`OrderBy`], or an `Option` of either (a `None` skips it), so a
    /// request's `Option<dir>` flows straight in. Nesting-aware: a field (or map)
    /// inside `nested` arrays renders the right `nested` chain from its scope.
    ///
    /// Map-key sorts render a `_script` sort but dedup on the field path, so
    /// several still coexist; an [`OrderBy`]'s `missing_first`/`missing_last`
    /// resolves to a direction-correct fallback value (a `_script` sort can't use
    /// the `missing` field). `numeric_type`/`unmapped_type`/`format` are dropped
    /// (field-sort-only); `mode` is kept (it's valid on a `_script` sort).
    #[must_use]
    pub fn by<H: Sortable>(mut self, handle: H, dir: impl Into<MaybeOrderBy>) -> Self {
        if let Some(order) = dir.into().0 {
            self.push_unique(order.into_sort(&handle));
        }
        self
    }

    /// Sort by distance from `center` (`_geo_distance`, nearest first). A `None`
    /// center skips it. Pass a unit / script geo sort through [`raw`](Self::raw).
    #[must_use]
    pub fn near<S>(mut self, handle: Geo<S>, center: impl Into<Option<GeoPoint>>) -> Self {
        if let Some(center) = center.into() {
            self.push_unique(handle.distance_from(center));
        }
        self
    }

    /// Sort by relevance `_score` (descending).
    #[must_use]
    pub fn score(mut self) -> Self {
        self.push_unique(Sort::score());
        self
    }

    /// Sort by `_score` only when `cond` holds (e.g. a free-text query is present).
    #[must_use]
    pub fn score_if(self, cond: bool) -> Self {
        if cond { self.score() } else { self }
    }

    /// Append a pre-built [`Sort`] verbatim — the escape hatch for sorts the
    /// typed verbs don't cover (`_script`, a geo sort with options). A `None`
    /// adds nothing. **Not** deduped.
    #[must_use]
    pub fn raw(mut self, sort: impl Into<Option<Sort>>) -> Self {
        if let Some(sort) = sort.into() {
            self.sorts.push(sort);
        }
        self
    }

    /// A stable final sort key (ascending) — append a unique field so equal
    /// leading keys still page deterministically.
    #[must_use]
    pub fn tiebreak<H: Sortable>(mut self, handle: H) -> Self {
        self.push_unique(handle.asc());
        self
    }

    /// A fallback used only if nothing else lands in the builder.
    #[must_use]
    pub fn or_default(mut self, sort: impl Into<Sort>) -> Self {
        if self.fallback.is_none() {
            self.fallback = Some(sort.into());
        }
        self
    }

    /// Finish: the `sort` array (the fallback, if set, when otherwise empty).
    #[must_use]
    pub fn build(mut self) -> Vec<Sort> {
        if self.sorts.is_empty()
            && let Some(fallback) = self.fallback
        {
            self.sorts.push(fallback);
        }
        self.sorts
    }
}

impl IntoIterator for SortBuilder {
    type Item = Sort;
    type IntoIter = std::vec::IntoIter<Sort>;

    fn into_iter(self) -> Self::IntoIter {
        self.build().into_iter()
    }
}
