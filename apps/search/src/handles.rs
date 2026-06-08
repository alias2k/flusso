//! Field handles. Each handle carries a field path **and a scope** `S`, and
//! exposes only the operators its mapping type supports; every operator builds a
//! [`Query`]`<S>`.
//!
//! Root fields and flattened object / `one_to_one` sub-fields are scope
//! [`Root`]; a `nested` array's element handles carry the element type as their
//! scope, so their queries must be lifted with [`Nested::any`]/[`Nested::all`]
//! before joining a parent query. (The derive picks the right scope; hand-written
//! handles default to `Root`.)

use std::marker::PhantomData;

use serde_json::{Map, Value};

use crate::query::{AsQuery, Query, Root};

// ---- shared leaf builders (generic over scope) -----------------------------

/// `{ "<wrapper>": { "<path>": <value> } }`.
fn single<S>(wrapper: &str, path: &str, value: Value) -> Query<S> {
    let mut inner = Map::new();
    inner.insert(path.to_string(), value);
    let mut outer = Map::new();
    outer.insert(wrapper.to_string(), Value::Object(inner));
    Query::leaf(Value::Object(outer))
}

/// `{ "exists": { "field": "<path>" } }`.
fn exists_q<S>(path: &str) -> Query<S> {
    let mut inner = Map::new();
    inner.insert("field".to_string(), Value::String(path.to_string()));
    let mut outer = Map::new();
    outer.insert("exists".to_string(), Value::Object(inner));
    Query::leaf(Value::Object(outer))
}

/// `{ "range": { "<path>": { <bounds…> } } }`.
fn range_q<S>(path: &str, bounds: Vec<(&str, Value)>) -> Query<S> {
    let mut body = Map::new();
    for (key, value) in bounds {
        body.insert(key.to_string(), value);
    }
    single("range", path, Value::Object(body))
}

/// `{ "nested": { "path": "<path>", "query": <query> } }`.
fn nested_value(path: &str, query: Value) -> Value {
    let mut body = Map::new();
    body.insert("path".to_string(), Value::String(path.to_string()));
    body.insert("query".to_string(), query);
    let mut outer = Map::new();
    outer.insert("nested".to_string(), Value::Object(body));
    Value::Object(outer)
}

/// `{ "bool": { "<clause>": [ … ] } }`.
fn bool_value(clause: &str, items: Vec<Value>) -> Value {
    let mut body = Map::new();
    body.insert(clause.to_string(), Value::Array(items));
    let mut outer = Map::new();
    outer.insert("bool".to_string(), Value::Object(body));
    Value::Object(outer)
}

/// `{ "match_all": {} }`.
pub(crate) fn match_all_value() -> Value {
    let mut outer = Map::new();
    outer.insert("match_all".to_string(), Value::Object(Map::new()));
    Value::Object(outer)
}

/// The keyword term for a value, taken from its serde serialization — so a
/// `#[derive(FlussoValue)]` enum/newtype matches exactly the string it stores
/// in the document. `String`/`&str` pass straight through; the non-string
/// fallback only fires for a hand-written [`trait@FlussoValue`] impl that breaks the
/// "serializes to a string" contract the derive enforces.
fn keyword_term(value: &impl serde::Serialize) -> Value {
    match serde_json::to_value(value) {
        Ok(Value::String(string)) => Value::String(string),
        Ok(other) => Value::String(other.to_string()),
        Err(_) => Value::String(String::new()),
    }
}

// ---- FlussoValue ------------------------------------------------------------

/// Field-category markers for [`trait@FlussoValue`]. Zero-size and uninhabited — they
/// exist only as the `K` type parameter, so one type can be a valid value for
/// several kinds (e.g. `String` is a [`kind::Keyword`], [`kind::Text`], and
/// [`kind::Date`] value).
pub mod kind {
    /// A `keyword` field — an exact string.
    #[derive(Debug)]
    pub enum Keyword {}
    /// A `text` field — an analyzed string.
    #[derive(Debug)]
    pub enum Text {}
    /// A numeric field (`byte`…`double`, `scaled_float`).
    #[derive(Debug)]
    pub enum Number {}
    /// A `date`/`timestamp` field — an ISO-8601 string.
    #[derive(Debug)]
    pub enum Date {}
}

/// A Rust type usable where a field of kind `K` is expected: as the field type
/// in a `#[derive(FlussoDocument)]` struct, and (for [`kind::Keyword`]) as a
/// query value on [`Keyword::eq`]/[`Keyword::in_`].
///
/// Built-in leaf types are pre-implemented (`String`/`&str` for keyword, the
/// numeric primitives for number, …). Custom enums and newtype wrappers opt in
/// with `#[derive(FlussoValue)]` (e.g. a `Pro`/`Enterprise`/`Free` tier enum →
/// `Account::tier().eq(AccountTier::Pro)`, matched against its serde string).
/// `FlussoDocument` emits a deferred bound on this trait for any non-primitive
/// field type, so a document only compiles when the type genuinely fits.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid value for a `{K}` field",
    label = "unsupported field type",
    note = "use a built-in leaf type, or add `#[derive(FlussoValue)]` (with the matching kind) to `{Self}`"
)]
pub trait FlussoValue<K> {}

impl FlussoValue<kind::Keyword> for String {}
impl FlussoValue<kind::Keyword> for &str {}

impl FlussoValue<kind::Text> for String {}
impl FlussoValue<kind::Text> for &str {}

impl FlussoValue<kind::Number> for i8 {}
impl FlussoValue<kind::Number> for i16 {}
impl FlussoValue<kind::Number> for i32 {}
impl FlussoValue<kind::Number> for i64 {}
impl FlussoValue<kind::Number> for f32 {}
impl FlussoValue<kind::Number> for f64 {}
#[cfg(feature = "decimal")]
impl FlussoValue<kind::Number> for crate::Decimal {}

impl FlussoValue<kind::Date> for String {}

// ---- Keyword ---------------------------------------------------------------

/// An exact, aggregatable string field (`keyword`, `enum`, `uuid`).
#[derive(Debug, Clone)]
pub struct Keyword<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Keyword<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match. Accepts a `String`/`&str`, or any `#[derive(FlussoValue)]`
    /// keyword enum/newtype — matched against its serde string form
    /// (`Account::tier().eq(AccountTier::Pro)`).
    pub fn eq(&self, value: impl FlussoValue<kind::Keyword> + serde::Serialize) -> Query<S> {
        single("term", &self.path, keyword_term(&value))
    }

    /// Match any of the given values (`String`/`&str` or keyword `FlussoValue` types).
    pub fn in_(
        &self,
        values: impl IntoIterator<Item = impl FlussoValue<kind::Keyword> + serde::Serialize>,
    ) -> Query<S> {
        let array = values.into_iter().map(|v| keyword_term(&v)).collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Prefix match.
    pub fn prefix(&self, value: impl Into<String>) -> Query<S> {
        single("prefix", &self.path, Value::String(value.into()))
    }

    /// Wildcard match — `?` matches one character, `*` matches any run.
    pub fn wildcard(&self, pattern: impl Into<String>) -> Query<S> {
        single("wildcard", &self.path, Value::String(pattern.into()))
    }

    /// Regular-expression match (Lucene regex syntax, anchored to the whole term).
    pub fn regexp(&self, pattern: impl Into<String>) -> Query<S> {
        single("regexp", &self.path, Value::String(pattern.into()))
    }

    /// Fuzzy term match — tolerates typos within the default `AUTO` distance.
    pub fn fuzzy(&self, value: impl Into<String>) -> Query<S> {
        single("fuzzy", &self.path, Value::String(value.into()))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort ascending on this field.
    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    /// Sort descending on this field.
    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}

// ---- Text ------------------------------------------------------------------

/// An analyzed full-text field (`text`, `identifier`). No exact `eq`.
#[derive(Debug, Clone)]
pub struct Text<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Text<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Analyzed match.
    pub fn matches(&self, value: impl Into<String>) -> Query<S> {
        single("match", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase match (terms in order).
    pub fn match_phrase(&self, value: impl Into<String>) -> Query<S> {
        single("match_phrase", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase-prefix match (search-as-you-type).
    pub fn match_phrase_prefix(&self, value: impl Into<String>) -> Query<S> {
        single(
            "match_phrase_prefix",
            &self.path,
            Value::String(value.into()),
        )
    }

    /// Analyzed match tolerant of typos — a `match` with `fuzziness: AUTO`.
    pub fn matches_fuzzy(&self, value: impl Into<String>) -> Query<S> {
        let mut params = Map::new();
        params.insert("query".to_string(), Value::String(value.into()));
        params.insert("fuzziness".to_string(), Value::String("AUTO".to_string()));
        single("match", &self.path, Value::Object(params))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

/// A cross-field full-text query over several [`Text`] fields in the same scope.
pub fn multi_match<S>(
    query: impl Into<String>,
    fields: impl IntoIterator<Item = Text<S>>,
) -> Query<S> {
    let paths = fields
        .into_iter()
        .map(|field| Value::String(field.path))
        .collect();
    let mut body = Map::new();
    body.insert("query".to_string(), Value::String(query.into()));
    body.insert("fields".to_string(), Value::Array(paths));
    let mut outer = Map::new();
    outer.insert("multi_match".to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}

// ---- Bool ------------------------------------------------------------------

/// A boolean field.
#[derive(Debug, Clone)]
pub struct Bool<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Bool<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: bool) -> Query<S> {
        single("term", &self.path, Value::Bool(value))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

// ---- Number ----------------------------------------------------------------

/// A numeric field. `T` is the Rust scalar; `S` is the scope (defaults to
/// [`Root`], so `Number<i64>` is a root-scope handle).
#[derive(Debug, Clone)]
pub struct Number<T, S = Root> {
    path: String,
    _marker: PhantomData<fn() -> (T, S)>,
}

impl<T, S> Number<T, S>
where
    T: Into<Value> + Copy,
{
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: T) -> Query<S> {
        single("term", &self.path, value.into())
    }

    /// Match any of the given values.
    pub fn in_(&self, values: impl IntoIterator<Item = T>) -> Query<S> {
        let array = values.into_iter().map(Into::into).collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Strictly less than `value`.
    pub fn lt(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("lt", value.into())])
    }

    /// Less than or equal to `value`.
    pub fn lte(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("lte", value.into())])
    }

    /// Strictly greater than `value`.
    pub fn gt(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("gt", value.into())])
    }

    /// Greater than or equal to `value`.
    pub fn gte(&self, value: T) -> Query<S> {
        range_q(&self.path, vec![("gte", value.into())])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: T, high: T) -> Query<S> {
        range_q(&self.path, vec![("gte", low.into()), ("lte", high.into())])
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort ascending on this field.
    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    /// Sort descending on this field.
    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}

// ---- Date ------------------------------------------------------------------

/// A `date`/`timestamp` field. Bounds are ISO-8601 strings.
#[derive(Debug, Clone)]
pub struct Date<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Date<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Exact match.
    pub fn eq(&self, value: impl Into<String>) -> Query<S> {
        single("term", &self.path, Value::String(value.into()))
    }

    /// Strictly before `value`.
    pub fn lt(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("lt", Value::String(value.into()))])
    }

    /// At or before `value`.
    pub fn lte(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("lte", Value::String(value.into()))])
    }

    /// Strictly after `value`.
    pub fn gt(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("gt", Value::String(value.into()))])
    }

    /// At or after `value`.
    pub fn gte(&self, value: impl Into<String>) -> Query<S> {
        range_q(&self.path, vec![("gte", Value::String(value.into()))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: impl Into<String>, high: impl Into<String>) -> Query<S> {
        range_q(
            &self.path,
            vec![
                ("gte", Value::String(low.into())),
                ("lte", Value::String(high.into())),
            ],
        )
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort ascending on this field.
    pub fn asc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Asc)
    }

    /// Sort descending on this field.
    pub fn desc(&self) -> Sort {
        Sort::new(&self.path, SortOrder::Desc)
    }
}

// ---- Nested ----------------------------------------------------------------

/// A `nested` array of objects. `E` is the **enclosing** scope (where queries
/// over this array land — `Root` at the top level, the parent element type when
/// deeper); `C` is the **child** scope (the element type). Lifting a child query
/// (`Query<C>`) through `any`/`all` produces a `Query<E>`.
#[derive(Debug, Clone)]
pub struct Nested<E = Root, C = serde_json::Value> {
    path: String,
    _marker: PhantomData<fn() -> (E, C)>,
}

impl<E, C> Nested<E, C> {
    /// Build a handle for the nested array at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Parents with **at least one** element matching `query`.
    pub fn any(&self, query: impl AsQuery<C>) -> Query<E> {
        let inner = query
            .into_query()
            .map_or_else(match_all_value, |q| q.to_value());
        Query::leaf(nested_value(&self.path, inner))
    }

    /// Parents where **every** element matches `query` ("no element fails it").
    pub fn all(&self, query: impl AsQuery<C>) -> Query<E> {
        let inner = query
            .into_query()
            .map_or_else(match_all_value, |q| q.to_value());
        let fails = bool_value("must_not", vec![inner]);
        let nested = nested_value(&self.path, fails);
        Query::leaf(bool_value("must_not", vec![nested]))
    }

    /// The nested array has at least one element.
    pub fn exists(&self) -> Query<E> {
        exists_q(&self.path)
    }

    /// Shape the **returned** array: keep elements matching `query` (with the
    /// builder's sort/size). Pass to [`crate::Search::filter_nested`].
    pub fn matching(&self, query: impl AsQuery<C>) -> NestedProjection {
        NestedProjection {
            path: self.path.clone(),
            query: query.into_query().map(|q| q.to_value()),
            sort: Vec::new(),
            size: None,
            from: None,
        }
    }

    /// Like [`matching`](Self::matching) with no predicate — every element.
    pub fn project(&self) -> NestedProjection {
        NestedProjection {
            path: self.path.clone(),
            query: None,
            sort: Vec::new(),
            size: None,
            from: None,
        }
    }
}

/// A request to shape one nested array in the results (via `inner_hits`).
#[derive(Debug, Clone)]
pub struct NestedProjection {
    path: String,
    query: Option<Value>,
    sort: Vec<Sort>,
    size: Option<u64>,
    from: Option<u64>,
}

impl NestedProjection {
    /// Order the returned elements.
    #[must_use]
    pub fn sort(mut self, sort: Sort) -> Self {
        self.sort.push(sort);
        self
    }

    /// Cap how many elements are returned per parent.
    #[must_use]
    pub fn size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    /// Offset within each parent's matching elements.
    #[must_use]
    pub fn from(mut self, from: u64) -> Self {
        self.from = Some(from);
        self
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    /// The `{ "nested": { path, query, inner_hits } }` clause (inner_hits named
    /// after the path, for retrieval).
    pub(crate) fn to_value(&self) -> Value {
        let query = self.query.clone().unwrap_or_else(match_all_value);
        let mut inner_hits = Map::new();
        inner_hits.insert("name".to_string(), Value::String(self.path.clone()));
        if let Some(size) = self.size {
            inner_hits.insert("size".to_string(), Value::from(size));
        }
        if let Some(from) = self.from {
            inner_hits.insert("from".to_string(), Value::from(from));
        }
        if !self.sort.is_empty() {
            let keys = self.sort.iter().map(Sort::to_value).collect();
            inner_hits.insert("sort".to_string(), Value::Array(keys));
        }
        let mut nested = Map::new();
        nested.insert("path".to_string(), Value::String(self.path.clone()));
        nested.insert("query".to_string(), query);
        nested.insert("inner_hits".to_string(), Value::Object(inner_hits));
        let mut outer = Map::new();
        outer.insert("nested".to_string(), Value::Object(nested));
        Value::Object(outer)
    }
}

// ---- Object ----------------------------------------------------------------

/// An `object` sub-document — a `group` or a `one_to_one` join. Objects are
/// **flattened**, so their sub-fields are queried by their own scope-`S`
/// dotted-path handles directly (`Account::tier()`); this handle is for the
/// object itself. `S` is the enclosing scope (`Root` at the top level).
#[derive(Debug, Clone)]
pub struct Object<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Object<S> {
    /// Build a handle for the object at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The object is present — most useful on a nullable `one_to_one`.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

// ---- Binary ----------------------------------------------------------------

/// A `binary` field — base64-encoded, not searchable. Only existence.
#[derive(Debug, Clone)]
pub struct Binary<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Binary<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }
}

// ---- Json ------------------------------------------------------------------

/// An untyped `object`/`json` field. The escape hatch: existence, or a raw
/// clause spliced in verbatim.
#[derive(Debug, Clone)]
pub struct Json<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Json<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Splice a raw OpenSearch query clause in verbatim.
    pub fn raw(&self, clause: Value) -> Query<S> {
        Query::leaf(clause)
    }
}

// ---- Sort ------------------------------------------------------------------

/// Sort direction.
#[derive(Debug, Clone, Copy)]
pub enum SortOrder {
    /// Ascending.
    Asc,
    /// Descending.
    Desc,
}

impl SortOrder {
    fn as_str(self) -> &'static str {
        match self {
            SortOrder::Asc => "asc",
            SortOrder::Desc => "desc",
        }
    }
}

/// A single sort key, produced by `.asc()` / `.desc()` on a sortable handle (or
/// `Geo::distance_sort`). Scope-free.
#[derive(Debug, Clone)]
pub struct Sort {
    value: Value,
}

impl Sort {
    /// A field/order sort: `{ "<field>": { "order": "asc"|"desc" } }`.
    fn new(field: &str, order: SortOrder) -> Self {
        let mut order_body = Map::new();
        order_body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        let mut outer = Map::new();
        outer.insert(field.to_string(), Value::Object(order_body));
        Self {
            value: Value::Object(outer),
        }
    }

    /// A pre-built sort clause (e.g. `_geo_distance`).
    fn raw(value: Value) -> Self {
        Self { value }
    }

    pub(crate) fn to_value(&self) -> Value {
        self.value.clone()
    }
}

// ---- Geo -------------------------------------------------------------------

/// A geographic point — latitude/longitude in degrees.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct GeoPoint {
    /// Latitude in degrees.
    pub lat: f64,
    /// Longitude in degrees.
    pub lon: f64,
}

impl GeoPoint {
    /// A point at `lat`/`lon` degrees.
    pub fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }

    /// `{ "lat": …, "lon": … }`.
    fn to_value(self) -> Value {
        let mut point = Map::new();
        point.insert("lat".to_string(), Value::from(self.lat));
        point.insert("lon".to_string(), Value::from(self.lon));
        Value::Object(point)
    }
}

/// A `geo_point` field — distance, bounding-box, and polygon queries, plus
/// sort-by-distance.
#[derive(Debug, Clone)]
pub struct Geo<S = Root> {
    path: String,
    _scope: PhantomData<fn() -> S>,
}

impl<S> Geo<S> {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _scope: PhantomData,
        }
    }

    /// Points within `distance` (e.g. `"12km"`, `"5mi"`) of `center`.
    pub fn within(&self, distance: impl Into<String>, center: GeoPoint) -> Query<S> {
        let mut body = Map::new();
        body.insert("distance".to_string(), Value::String(distance.into()));
        body.insert(self.path.clone(), center.to_value());
        wrap_object("geo_distance", body)
    }

    /// Points inside the axis-aligned box with the given corners.
    pub fn in_bounding_box(&self, top_left: GeoPoint, bottom_right: GeoPoint) -> Query<S> {
        let mut corners = Map::new();
        corners.insert("top_left".to_string(), top_left.to_value());
        corners.insert("bottom_right".to_string(), bottom_right.to_value());
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(corners));
        wrap_object("geo_bounding_box", body)
    }

    /// Points inside the polygon described by `points` (three or more vertices).
    pub fn in_polygon(&self, points: impl IntoIterator<Item = GeoPoint>) -> Query<S> {
        let vertices = points.into_iter().map(GeoPoint::to_value).collect();
        let mut inner = Map::new();
        inner.insert("points".to_string(), Value::Array(vertices));
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(inner));
        wrap_object("geo_polygon", body)
    }

    /// The field has a value.
    pub fn exists(&self) -> Query<S> {
        exists_q(&self.path)
    }

    /// Sort by distance from `center`, measured in `unit` (e.g. `"km"`).
    pub fn distance_sort(
        &self,
        center: GeoPoint,
        order: SortOrder,
        unit: impl Into<String>,
    ) -> Sort {
        let mut body = Map::new();
        body.insert(self.path.clone(), center.to_value());
        body.insert(
            "order".to_string(),
            Value::String(order.as_str().to_string()),
        );
        body.insert("unit".to_string(), Value::String(unit.into()));
        let mut outer = Map::new();
        outer.insert("_geo_distance".to_string(), Value::Object(body));
        Sort::raw(Value::Object(outer))
    }
}

/// `{ "<name>": { <body> } }` as a scope-`S` query.
fn wrap_object<S>(name: &str, body: Map<String, Value>) -> Query<S> {
    let mut outer = Map::new();
    outer.insert(name.to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}
