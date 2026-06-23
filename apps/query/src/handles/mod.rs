//! Field handles. Each handle carries a field path **and a scope** `S`, and
//! exposes only the operators its mapping type supports; every operator builds a
//! [`Query`]`<S>`.
//!
//! Root fields and flattened object / to-one-join sub-fields are scope
//! [`Root`](crate::Root); a `nested` array's element handles carry the element type as their
//! scope, so their queries must be lifted with [`Nested::any`]/[`Nested::all`]
//! before joining a parent query. (The derive picks the right scope; hand-written
//! handles default to `Root`.)
//!
//! The handles are grouped by field family:
//!
//! - `string` — [`Keyword`], [`Text`], and the cross-field [`multi_match`].
//! - `scalar` — [`Bool`], [`Number`], [`Date`] (exact/range value fields).
//! - `map` — [`TextMap`]/[`KeywordMap`] dynamic-key objects and [`MapSearch`].
//! - `nested` — [`Nested`] arrays and their [`NestedProjection`].
//! - `object` — [`Object`] sub-documents and the opaque [`Binary`]/[`Json`] fields.
//! - `geo` — [`Geo`] points and [`GeoPoint`].
//! - `sort` — [`Sort`]/[`SortOrder`].
//!
//! This module holds the pieces they share: the leaf-query builders and the
//! [`trait@FlussoValue`] type-kind machinery.

use serde_json::{Map, Value};

use crate::query::Query;

mod compound;
mod extra;
mod geo;
mod map;
mod nested;
mod object;
mod params;
mod scalar;
mod sort;
mod string;

pub use compound::{
    BoostingQuery, ConstantScoreQuery, DisMaxQuery, FunctionScoreQuery, boosting, constant_score,
    dis_max, function_score,
};
pub use extra::{
    CombinedFieldsQuery, DistanceFeatureQuery, IdsQuery, MoreLikeThisQuery, QueryStringQuery,
    RankFeatureQuery, ScriptQuery, ScriptScoreQuery, SimpleQueryStringQuery, combined_fields,
    distance_feature, ids, more_like_this, query_string, rank_feature, script, script_score,
    simple_query_string,
};
pub use geo::{Distance, DistanceUnit, Geo, GeoDistanceQuery, GeoPoint};
pub use map::{DateMap, KeywordMap, MapSearch, NumberMap, TextMap};
pub use nested::{Nested, NestedProjection, NestedQuery};
pub use object::{Binary, Json, Object};
pub use params::{
    BoostMode, DistanceType, Fuzziness, MinimumShouldMatch, MultiMatchType, NestedScoreMode,
    NumericType, Operator, RangeRelation, ScoreMode, ScriptSortType, ValidationMethod,
    ZeroTermsQuery,
};
pub use scalar::{Bool, Date, EqQuery, Number, RangeQuery, TermsQuery};
pub use sort::{MaybeOrderBy, Missing, OrderBy, Sort, SortBuilder, SortMode, SortOrder, Sortable};
pub use string::{
    FuzzyQuery, Keyword, MatchQuery, MultiMatchQuery, NoSubfields, PrefixQuery, RegexpQuery,
    TermQuery, Text, WildcardQuery, WithSubfields, multi_match,
};

/// `{ "<wrapper>": { "<path>": <value> } }`.
fn single<S>(wrapper: &str, path: &str, value: Value) -> Query<S> {
    let mut inner = Map::new();
    inner.insert(path.to_string(), value);
    wrap(wrapper, inner)
}

/// `{ "<wrapper>": { <body> } }`.
fn wrap<S>(wrapper: &str, body: Map<String, Value>) -> Query<S> {
    let mut outer = Map::new();
    outer.insert(wrapper.to_string(), Value::Object(body));
    Query::leaf(Value::Object(outer))
}

/// The universal leaf-query modifiers every builder carries: `boost` (a
/// relevance multiplier) and `name` (`_name`, surfaced in a hit's
/// `matched_queries`). Builders embed one and call [`Common::write`] when
/// rendering.
#[derive(Debug, Clone, Default)]
pub(crate) struct Common {
    boost: Option<f32>,
    name: Option<String>,
}

impl Common {
    pub(crate) fn set_boost(&mut self, boost: f32) {
        self.boost = Some(boost);
    }

    pub(crate) fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    /// Whether neither modifier is set (so a builder may render the shorthand).
    pub(crate) fn is_empty(&self) -> bool {
        self.boost.is_none() && self.name.is_none()
    }

    /// Write `boost` / `_name` into an option map, if set.
    pub(crate) fn write(&self, map: &mut Map<String, Value>) {
        if let Some(boost) = self.boost {
            map.insert("boost".to_string(), Value::from(boost));
        }
        if let Some(name) = &self.name {
            map.insert("_name".to_string(), Value::String(name.clone()));
        }
    }
}

/// Emit the universal `boost` / `name` setters on a builder whose [`Common`]
/// lives in `self.$field`. Keeps the two methods identical across every builder.
macro_rules! common_opts {
    ($field:ident) => {
        /// Multiply this clause's relevance score by `boost`.
        #[must_use]
        pub fn boost(mut self, boost: f32) -> Self {
            self.$field.set_boost(boost);
            self
        }

        /// Tag this clause with `_name`, surfaced in a hit's `matched_queries`.
        #[must_use]
        pub fn name(mut self, name: impl Into<String>) -> Self {
            self.$field.set_name(name.into());
            self
        }
    };
}
pub(crate) use common_opts;

/// Render `{ wrapper: { path: <value> } }` (the DSL shorthand) when `opts` is
/// empty, else `{ wrapper: { path: { <key>: <value>, ...opts } } }`. The shared
/// shape for the value-bearing leaf queries (`term`/`prefix`/`wildcard`/… with
/// `key = "value"`; `match`/`match_phrase`/… with `key = "query"`).
fn keyed_value_query<S>(
    wrapper: &str,
    path: &str,
    key: &str,
    value: Value,
    mut opts: Map<String, Value>,
) -> Query<S> {
    if opts.is_empty() {
        single(wrapper, path, value)
    } else {
        opts.insert(key.to_string(), value);
        single(wrapper, path, Value::Object(opts))
    }
}

/// `{ "exists": { "field": "<path>" } }`.
fn exists_q<S>(path: &str) -> Query<S> {
    let mut inner = Map::new();
    inner.insert("field".to_string(), Value::String(path.to_string()));
    let mut outer = Map::new();
    outer.insert("exists".to_string(), Value::Object(inner));
    Query::leaf(Value::Object(outer))
}

/// `{ "match_all": {} }`.
pub(crate) fn match_all_value() -> Value {
    let mut outer = Map::new();
    outer.insert("match_all".to_string(), Value::Object(Map::new()));
    Value::Object(outer)
}

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
    /// A `boolean` field.
    #[derive(Debug)]
    pub enum Bool {}
    /// A `byte` field (`i8`).
    #[derive(Debug)]
    pub enum Byte {}
    /// A `short` field (`i16`).
    #[derive(Debug)]
    pub enum Short {}
    /// An `integer` field (`i32`).
    #[derive(Debug)]
    pub enum Integer {}
    /// A `long` field (`i64`).
    #[derive(Debug)]
    pub enum Long {}
    /// A `float` / `half_float` field (`f32`).
    #[derive(Debug)]
    pub enum Float {}
    /// A `double` field (`f64`).
    #[derive(Debug)]
    pub enum Double {}
    /// A `decimal` / `scaled_float` field (`rust_decimal::Decimal`).
    #[derive(Debug)]
    pub enum Decimal {}
    /// A `date`/`timestamp` field — an ISO-8601 string.
    #[derive(Debug)]
    pub enum Date {}
}

/// A Rust type usable where a field of kind `K` is expected: as the field type
/// in a `#[derive(FlussoDocument)]` struct, and as a query value — for
/// [`kind::Keyword`] on [`Keyword::eq`]/[`Keyword::any_of`], and for
/// [`kind::Date`] on [`Date::eq`]/[`Date::gte`]/… (`String`/`&str`, or the
/// `chrono` date types behind the `chrono` feature).
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
pub trait FlussoValue<K>: serde::Serialize {}

impl FlussoValue<kind::Keyword> for String {}
impl FlussoValue<kind::Keyword> for &str {}
#[cfg(feature = "uuid")]
impl FlussoValue<kind::Keyword> for uuid::Uuid {}
#[cfg(feature = "uuid")]
impl FlussoValue<kind::Keyword> for &uuid::Uuid {}

impl FlussoValue<kind::Text> for String {}
impl FlussoValue<kind::Text> for &str {}

impl FlussoValue<kind::Bool> for bool {}

// A numeric type is a valid value for a numeric kind iff it widens into that
// kind's Rust leaf **without loss** — so `Order::age().eq(5)` works on an `i64`
// field, an `f64` field rejects an `i64` (precision), and an integer field
// rejects a float. (`i32`→`f64` and any int→`Decimal` are lossless, so bare
// literals work on `double`/`decimal`; `byte`/`short` need a typed literal.)
macro_rules! number_values {
    ($kind:path: $($ty:ty),+ $(,)?) => { $(impl FlussoValue<$kind> for $ty {})+ };
}
number_values!(kind::Byte: i8);
number_values!(kind::Short: i8, i16);
number_values!(kind::Integer: i8, i16, i32);
number_values!(kind::Long: i8, i16, i32, i64);
number_values!(kind::Float: i8, i16, f32);
number_values!(kind::Double: i8, i16, i32, f32, f64);
number_values!(kind::Decimal: i8, i16, i32, i64);
#[cfg(feature = "decimal")]
impl FlussoValue<kind::Decimal> for crate::Decimal {}

impl FlussoValue<kind::Date> for String {}
impl FlussoValue<kind::Date> for &str {}
#[cfg(feature = "chrono")]
impl FlussoValue<kind::Date> for chrono::NaiveDate {}
#[cfg(feature = "chrono")]
impl FlussoValue<kind::Date> for chrono::NaiveDateTime {}
#[cfg(feature = "chrono")]
impl FlussoValue<kind::Date> for chrono::DateTime<chrono::Utc> {}

/// A Rust type usable as the **document type** of a `map` field of value kind
/// `K`: a dynamic-key object whose values are all of kind `K`.
///
/// The canonical map type — `HashMap<String, V>` where `V` is a `K` value — is
/// pre-implemented via a blanket impl, so `HashMap<String, String>` is a valid
/// `text`/`keyword` map and `HashMap<String, i64>` a valid `long` map with no
/// extra code. A whole-map newtype wrapper (`struct Translations(HashMap<…>)`)
/// opts in with `#[derive(FlussoMap)]`. `FlussoDocument` emits a deferred bound
/// on this trait for a `map` field, so the document only compiles when its type
/// genuinely fits the declared value kind.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a valid map for a `{K}` field",
    label = "unsupported map type",
    note = "use `HashMap<String, V>` with a `{K}` value type, or add `#[derive(FlussoMap)]` (with the matching kind) to `{Self}`"
)]
pub trait FlussoMap<K> {}

impl<K, V: FlussoValue<K>> FlussoMap<K> for std::collections::HashMap<String, V> {}
