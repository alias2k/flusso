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
mod geo;
mod nested;
mod object;
mod scalar;
mod sort;
mod string;

pub use compound::{
    BoostingQuery, ConstantScoreQuery, DisMaxQuery, FunctionScoreQuery, boosting, constant_score,
    dis_max, function_score,
};
pub use geo::{Geo, GeoDistanceQuery, GeoPoint};
pub use nested::{Nested, NestedProjection, NestedQuery};
pub use object::{Binary, Json, Object};
pub use scalar::{Bool, Date, EqQuery, Number, RangeQuery, TermsQuery};
pub use sort::{Sort, SortMode, SortOrder};
pub use string::{
    FuzzyQuery, Keyword, MatchQuery, MultiMatchQuery, PrefixQuery, RegexpQuery, TermQuery, Text,
    WildcardQuery, multi_match,
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
