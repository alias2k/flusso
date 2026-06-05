//! Field handles. Each handle carries a field path and exposes only the
//! operators its mapping type supports; every operator builds a [`Query`].
//!
//! Until the `#[derive(FlussoDocument)]` macro exists, callers construct these
//! by hand with [`Keyword::at`] and friends, passing the field's document path
//! (dotted for sub-objects and nested fields, e.g. `"account.tier"`,
//! `"orders.status"`). The derive will generate exactly these constructors.

use std::marker::PhantomData;

use serde_json::{Map, Value};

use crate::query::{BoolQuery, Query};

// ---- shared leaf builders --------------------------------------------------

/// `{ "<wrapper>": { "<path>": <value> } }`.
fn single(wrapper: &str, path: &str, value: Value) -> Query {
    let mut inner = Map::new();
    inner.insert(path.to_string(), value);
    let mut outer = Map::new();
    outer.insert(wrapper.to_string(), Value::Object(inner));
    Query::leaf(Value::Object(outer))
}

/// `{ "exists": { "field": "<path>" } }`.
fn exists_q(path: &str) -> Query {
    let mut inner = Map::new();
    inner.insert("field".to_string(), Value::String(path.to_string()));
    let mut outer = Map::new();
    outer.insert("exists".to_string(), Value::Object(inner));
    Query::leaf(Value::Object(outer))
}

/// `{ "range": { "<path>": { <bounds…> } } }`.
fn range_q(path: &str, bounds: Vec<(&str, Value)>) -> Query {
    let mut body = Map::new();
    for (key, value) in bounds {
        body.insert(key.to_string(), value);
    }
    single("range", path, Value::Object(body))
}

/// `{ "nested": { "path": "<path>", "query": <inner> } }`.
fn nested_q(path: &str, inner: Query) -> Query {
    let mut body = Map::new();
    body.insert("path".to_string(), Value::String(path.to_string()));
    body.insert("query".to_string(), inner.to_value());
    let mut outer = Map::new();
    outer.insert("nested".to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}

/// `{ "<name>": { <body> } }` — for clauses whose body isn't a single
/// `path: value` pair.
fn wrap(name: &str, body: Map<String, Value>) -> Query {
    let mut outer = Map::new();
    outer.insert(name.to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}

// ---- Keyword ---------------------------------------------------------------

/// An exact, aggregatable string field (`keyword`, `enum`, `uuid`).
#[derive(Debug, Clone)]
pub struct Keyword {
    path: String,
}

impl Keyword {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Exact match.
    pub fn eq(&self, value: impl Into<String>) -> Query {
        single("term", &self.path, Value::String(value.into()))
    }

    /// Match any of the given values.
    pub fn in_(&self, values: impl IntoIterator<Item = impl Into<String>>) -> Query {
        let array = values
            .into_iter()
            .map(|v| Value::String(v.into()))
            .collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Prefix match.
    pub fn prefix(&self, value: impl Into<String>) -> Query {
        single("prefix", &self.path, Value::String(value.into()))
    }

    /// Wildcard match — `?` matches one character, `*` matches any run.
    pub fn wildcard(&self, pattern: impl Into<String>) -> Query {
        single("wildcard", &self.path, Value::String(pattern.into()))
    }

    /// Regular-expression match (Lucene regex syntax, anchored to the whole
    /// term).
    pub fn regexp(&self, pattern: impl Into<String>) -> Query {
        single("regexp", &self.path, Value::String(pattern.into()))
    }

    /// Fuzzy term match — tolerates typos within the default `AUTO` edit
    /// distance.
    pub fn fuzzy(&self, value: impl Into<String>) -> Query {
        single("fuzzy", &self.path, Value::String(value.into()))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
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
pub struct Text {
    path: String,
}

impl Text {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Analyzed match.
    pub fn matches(&self, value: impl Into<String>) -> Query {
        single("match", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase match (terms in order).
    pub fn match_phrase(&self, value: impl Into<String>) -> Query {
        single("match_phrase", &self.path, Value::String(value.into()))
    }

    /// Analyzed phrase-prefix match — the last term is treated as a prefix
    /// (search-as-you-type).
    pub fn match_phrase_prefix(&self, value: impl Into<String>) -> Query {
        single("match_phrase_prefix", &self.path, Value::String(value.into()))
    }

    /// Analyzed match tolerant of typos — a `match` with `fuzziness: AUTO`.
    pub fn matches_fuzzy(&self, value: impl Into<String>) -> Query {
        let mut params = Map::new();
        params.insert("query".to_string(), Value::String(value.into()));
        params.insert("fuzziness".to_string(), Value::String("AUTO".to_string()));
        single("match", &self.path, Value::Object(params))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }
}

/// A cross-field full-text query over several [`Text`] fields.
///
/// `{ "multi_match": { "query": "<query>", "fields": [ <paths…> ] } }`, using
/// OpenSearch's default `best_fields` scoring. For per-field boosts, a phrase
/// type, or analyzer overrides, drop to a `raw` clause.
pub fn multi_match(query: impl Into<String>, fields: impl IntoIterator<Item = Text>) -> Query {
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
pub struct Bool {
    path: String,
}

impl Bool {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Exact match.
    pub fn eq(&self, value: bool) -> Query {
        single("term", &self.path, Value::Bool(value))
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }
}

// ---- Number ----------------------------------------------------------------

/// A numeric field. `T` is the Rust scalar (`i16`/`i32`/`i64`/`f32`/`f64`) the
/// schema's type maps to; operator arguments take that type.
#[derive(Debug, Clone)]
pub struct Number<T> {
    path: String,
    // `fn() -> T` keeps `Number<T>` `Send`/`Sync` and `Debug` without bounding
    // `T`, while still pinning the operator argument type.
    _marker: PhantomData<fn() -> T>,
}

impl<T> Number<T>
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
    pub fn eq(&self, value: T) -> Query {
        single("term", &self.path, value.into())
    }

    /// Match any of the given values.
    pub fn in_(&self, values: impl IntoIterator<Item = T>) -> Query {
        let array = values.into_iter().map(Into::into).collect();
        single("terms", &self.path, Value::Array(array))
    }

    /// Strictly less than `value`.
    pub fn lt(&self, value: T) -> Query {
        range_q(&self.path, vec![("lt", value.into())])
    }

    /// Less than or equal to `value`.
    pub fn lte(&self, value: T) -> Query {
        range_q(&self.path, vec![("lte", value.into())])
    }

    /// Strictly greater than `value`.
    pub fn gt(&self, value: T) -> Query {
        range_q(&self.path, vec![("gt", value.into())])
    }

    /// Greater than or equal to `value`.
    pub fn gte(&self, value: T) -> Query {
        range_q(&self.path, vec![("gte", value.into())])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: T, high: T) -> Query {
        range_q(&self.path, vec![("gte", low.into()), ("lte", high.into())])
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
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

/// A `date`/`timestamp` field. Bounds are ISO-8601 strings (or anything the
/// index's date format accepts).
#[derive(Debug, Clone)]
pub struct Date {
    path: String,
}

impl Date {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Exact match.
    pub fn eq(&self, value: impl Into<String>) -> Query {
        single("term", &self.path, Value::String(value.into()))
    }

    /// Strictly before `value`.
    pub fn lt(&self, value: impl Into<String>) -> Query {
        range_q(&self.path, vec![("lt", Value::String(value.into()))])
    }

    /// At or before `value`.
    pub fn lte(&self, value: impl Into<String>) -> Query {
        range_q(&self.path, vec![("lte", Value::String(value.into()))])
    }

    /// Strictly after `value`.
    pub fn gt(&self, value: impl Into<String>) -> Query {
        range_q(&self.path, vec![("gt", Value::String(value.into()))])
    }

    /// At or after `value`.
    pub fn gte(&self, value: impl Into<String>) -> Query {
        range_q(&self.path, vec![("gte", Value::String(value.into()))])
    }

    /// Inclusive range `[low, high]`.
    pub fn between(&self, low: impl Into<String>, high: impl Into<String>) -> Query {
        range_q(
            &self.path,
            vec![
                ("gte", Value::String(low.into())),
                ("lte", Value::String(high.into())),
            ],
        )
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
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

/// A `nested` array of objects. `T` is the element document type (the struct
/// the array deserializes into). Queries over the element fields are built with
/// handles whose paths are dotted under this one (e.g. `"orders.status"`).
#[derive(Debug, Clone)]
pub struct Nested<T> {
    path: String,
    _marker: PhantomData<fn() -> T>,
}

impl<T> Nested<T> {
    /// Build a handle for the nested array at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    /// Parents with **at least one** element matching `query`.
    pub fn any(&self, query: Query) -> Query {
        nested_q(&self.path, query)
    }

    /// Parents where **every** element matches `query` (including those with no
    /// elements). Expressed as "no element fails `query`".
    pub fn all(&self, query: Query) -> Query {
        let fails = Query::from_bool(BoolQuery {
            must_not: vec![query],
            ..BoolQuery::default()
        });
        Query::from_bool(BoolQuery {
            must_not: vec![nested_q(&self.path, fails)],
            ..BoolQuery::default()
        })
    }

    /// The nested array has at least one element.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }
}

// ---- Binary ----------------------------------------------------------------

/// A `binary` field — base64-encoded, not searchable. Only existence.
#[derive(Debug, Clone)]
pub struct Binary {
    path: String,
}

impl Binary {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }
}

// ---- Json ------------------------------------------------------------------

/// An untyped `object`/`json` field. The escape hatch: existence, or a raw
/// clause spliced in verbatim.
#[derive(Debug, Clone)]
pub struct Json {
    path: String,
}

impl Json {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// The field has a non-null value.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }

    /// Splice a raw OpenSearch query clause in verbatim.
    pub fn raw(&self, clause: Value) -> Query {
        Query::leaf(clause)
    }
}

// ---- Geo -------------------------------------------------------------------

/// A geographic point — latitude/longitude in degrees.
///
/// The argument type for the [`Geo`] operators, and a deserialization target
/// for a `geo_point` field stored as `{ "lat": …, "lon": … }`.
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
pub struct Geo {
    path: String,
}

impl Geo {
    /// Build a handle for the field at `path`.
    pub fn at(path: impl Into<String>) -> Self {
        Self { path: path.into() }
    }

    /// Points within `distance` (e.g. `"12km"`, `"5mi"`) of `center`.
    pub fn within(&self, distance: impl Into<String>, center: GeoPoint) -> Query {
        let mut body = Map::new();
        body.insert("distance".to_string(), Value::String(distance.into()));
        body.insert(self.path.clone(), center.to_value());
        wrap("geo_distance", body)
    }

    /// Points inside the axis-aligned box with the given corners.
    pub fn in_bounding_box(&self, top_left: GeoPoint, bottom_right: GeoPoint) -> Query {
        let mut corners = Map::new();
        corners.insert("top_left".to_string(), top_left.to_value());
        corners.insert("bottom_right".to_string(), bottom_right.to_value());
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(corners));
        wrap("geo_bounding_box", body)
    }

    /// Points inside the polygon described by `points` (three or more vertices).
    pub fn in_polygon(&self, points: impl IntoIterator<Item = GeoPoint>) -> Query {
        let vertices = points.into_iter().map(GeoPoint::to_value).collect();
        let mut inner = Map::new();
        inner.insert("points".to_string(), Value::Array(vertices));
        let mut body = Map::new();
        body.insert(self.path.clone(), Value::Object(inner));
        wrap("geo_polygon", body)
    }

    /// The field has a value.
    pub fn exists(&self) -> Query {
        exists_q(&self.path)
    }

    /// Sort by distance from `center`, measured in `unit` (e.g. `"km"`, `"mi"`,
    /// `"m"`).
    pub fn distance_sort(
        &self,
        center: GeoPoint,
        order: SortOrder,
        unit: impl Into<String>,
    ) -> Sort {
        let mut body = Map::new();
        body.insert(self.path.clone(), center.to_value());
        body.insert("order".to_string(), Value::String(order.as_str().to_string()));
        body.insert("unit".to_string(), Value::String(unit.into()));
        let mut outer = Map::new();
        outer.insert("_geo_distance".to_string(), Value::Object(body));
        Sort::raw(Value::Object(outer))
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
/// `Geo::distance_sort`). Carries its already-built clause.
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

    /// A pre-built sort clause (used for shapes that aren't field/order, like
    /// `_geo_distance`).
    fn raw(value: Value) -> Self {
        Self { value }
    }

    pub(crate) fn to_value(&self) -> Value {
        self.value.clone()
    }
}
