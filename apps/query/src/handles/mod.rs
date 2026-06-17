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

mod geo;
mod nested;
mod object;
mod scalar;
mod sort;
mod string;

pub use geo::{Geo, GeoPoint};
pub use nested::{Nested, NestedProjection};
pub use object::{Binary, Json, Object};
pub use scalar::{Bool, Date, Number};
pub use sort::{Sort, SortOrder};
pub use string::{Keyword, Text, multi_match};

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

/// `{ "match_all": {} }`.
pub(crate) fn match_all_value() -> Value {
    let mut outer = Map::new();
    outer.insert("match_all".to_string(), Value::Object(Map::new()));
    Value::Object(outer)
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
