//! Decoding Postgres results into the schema's [`GenericValue`]: single named
//! columns (for reverse-resolution lookups) and JSON documents (assembled
//! server-side).

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fmt;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use kernel::{
    Aggregate, AggregateOp, BigDecimal, Column, Field, FieldSource, FlussoType, GenericValue,
    Relation,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde_json::value::RawValue;
use source::SourceError;
use source::document::BuildFailure;
use sqlx::postgres::{PgRow, PgValueFormat};
use sqlx::{Column as _, Row, TypeInfo, ValueRef as _};
use uuid::Uuid;

/// The SQL that selects `expr`, a key of base type `base_name`
/// (`pg_type.typname`), as [`decode_key`] reads it under `alias`: its
/// [`key_text`](crate::text_value::key_text), beside the value itself, which
/// is never decoded (a query's row description names its type, a domain by
/// its base type).
pub(crate) fn key_select(expr: &str, alias: &str, base_name: &str) -> String {
    format!(
        "{} AS \"{alias}\", {expr} AS \"{alias}{TYPED}\"",
        crate::text_value::key_text(expr, base_name)
    )
}

const TYPED: &str = ":typed";

/// The key [`key_select`] selected under `alias`, parsed by
/// [`read_back`](crate::text_value::read_back) into the very value a live
/// change carries for it.
/// [`GenericValue::Null`] for a null, a missing column, or a type that can't
/// be a key (warned).
pub(crate) fn decode_key(row: &PgRow, alias: &str) -> GenericValue {
    let typed = format!("{alias}{TYPED}");
    let Some(column) = row.columns().iter().find(|column| column.name() == typed) else {
        return GenericValue::Null;
    };
    if row
        .try_get_raw(column.ordinal())
        .map_or(true, |raw| raw.is_null())
    {
        return GenericValue::Null;
    }
    let info = column.type_info();
    let oid = info.oid().map_or(0, |oid| oid.0);
    if !crate::text_value::is_key_type(oid, info.name()) {
        tracing::warn!(column = %alias, r#type = %info.name(), "unsupported key type; treating as null");
        return GenericValue::Null;
    }
    match row.try_get::<String, _>(alias) {
        Ok(text) => crate::text_value::read_back(&text, oid),
        Err(error) => {
            tracing::warn!(column = %alias, %error, "could not read a key; treating as null");
            GenericValue::Null
        }
    }
}

/// The base types (`pg_type.typname`) a key may have: the ones
/// [`decode_key`] reads (`text_value::is_key_type` by OID). A key of any
/// other type reads as null: no document is ever written for it, which
/// `validate_indexes` reports (`DiagnosticKind::UnsupportedKeyType`).
pub(crate) const KEY_TYPES: &[&str] = &[
    "int2",
    "int4",
    "int8",
    "bool",
    "float4",
    "float8",
    "numeric",
    "text",
    "varchar",
    "bpchar",
    "name",
    "citext",
    "timestamp",
    "timestamptz",
    "date",
    "time",
    "uuid",
    "bytea",
];

/// Convert a JSON value into the value tree, untyped.
pub(super) fn json_to_generic(value: serde_json::Value) -> GenericValue {
    match value {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => match n.as_i64() {
            Some(i) => GenericValue::BigInt(i),
            None => n.as_f64().map_or(GenericValue::Null, float),
        },
        serde_json::Value::String(s) => GenericValue::String(s),
        serde_json::Value::Array(items) => {
            GenericValue::Array(items.into_iter().map(json_to_generic).collect())
        }
        serde_json::Value::Object(fields) => GenericValue::Map(
            fields
                .into_iter()
                .map(|(k, v)| (k, json_to_generic(v)))
                .collect(),
        ),
    }
}

/// Why a server-assembled document is not a body.
#[derive(Debug)]
pub(crate) enum DocumentError {
    /// A value its field can't hold: the document fails, for `on_error`.
    Failed(BuildFailure),
    /// The query's JSON did not parse.
    Malformed(serde_json::Error),
}

/// Read a server-assembled JSON document into the **typed** value tree, using
/// each field's declared type so the values a sink sees are canonical — a `date`
/// field becomes a [`Date`](GenericValue::Date), a `uuid` a [`Uuid`](GenericValue::Uuid),
/// not opaque strings. The document is built server-side as one JSON blob (so the
/// types are erased on the wire); this is where we put them back, guided by the
/// schema, in one pass over the text. Anything that fails to parse falls back to
/// the untyped JSON value rather than failing the document.
///
/// A `decimal` (a column, an array element, a `sum`/`min`/`max`, an `ids` key)
/// reads the exact number text Postgres wrote: a [`Decimal`](GenericValue::Decimal)
/// within its range, a [`BigDecimal`](GenericValue::BigDecimal) past it, never
/// rounded through `f64`.
///
/// Two values fail the document instead of falling back:
/// - an array column element that is not a scalar ([`coerce_array`]), since its
///   field cannot hold it;
/// - `NaN`, `Infinity` or `-Infinity` under a number field: JSON has no number
///   for them, and Postgres writes them as strings.
pub(crate) fn coerce_document(
    json: &[u8],
    fields: &[Field],
) -> Result<GenericValue, DocumentError> {
    let failure = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_slice(json);
    let read = Level {
        fields,
        at: At::Root,
        failure: &failure,
    }
    .deserialize(&mut deserializer)
    .and_then(|body| deserializer.end().map(|()| body));
    match (read, failure.into_inner()) {
        (_, Some(failure)) => Err(DocumentError::Failed(failure)),
        (Ok(body), None) => Ok(body),
        (Err(error), None) => Err(DocumentError::Malformed(error)),
    }
}

/// The document a query row holds in its `document` column, read by
/// [`coerce_document`]: the inner `Err` is a document that failed, the outer
/// one a row that holds no readable document.
pub(crate) fn read_document(
    row: &PgRow,
    fields: &[Field],
) -> source::Result<Result<GenericValue, BuildFailure>> {
    let raw = row.try_get_raw("document").map_err(super::query_err)?;
    let mut json = raw
        .as_bytes()
        .map_err(|error| SourceError::Query(format!("reading a document: {error}")))?;
    // A `jsonb` in the binary format leads with its format version.
    if raw.type_info().name() == "JSONB" && raw.format() == PgValueFormat::Binary {
        json = json.get(1..).unwrap_or_default();
    }
    match coerce_document(json, fields) {
        Ok(body) => Ok(Ok(body)),
        Err(DocumentError::Failed(failure)) => Ok(Err(failure)),
        Err(DocumentError::Malformed(error)) => Err(SourceError::Query(format!(
            "the document query returned malformed JSON: {error}"
        ))),
    }
}

/// Where a value sits in the document, rendered only for a failure.
#[derive(Clone, Copy)]
enum At<'a> {
    Root,
    Field(&'a At<'a>, &'a str),
    Index(&'a At<'a>, usize),
}

impl At<'_> {
    fn render(&self) -> String {
        let mut path = String::new();
        self.render_into(&mut path);
        path
    }

    fn render_into(&self, path: &mut String) {
        match self {
            At::Root => {}
            At::Field(parent, name) => {
                parent.render_into(path);
                if !path.is_empty() {
                    path.push('.');
                }
                path.push_str(name);
            }
            At::Index(parent, index) => {
                parent.render_into(path);
                path.push_str(&format!("[{index}]"));
            }
        }
    }

    fn failure(&self, message: String) -> BuildFailure {
        BuildFailure {
            field: self.render(),
            message,
            position: None,
        }
    }
}

/// The first failure of a document. Reading stops at it: the error the reader
/// returns only carries it out.
type Failure = Cell<Option<BuildFailure>>;

fn raise<E: de::Error>(failure: &Failure, cause: BuildFailure) -> E {
    failure.set(Some(cause));
    E::custom("the document failed")
}

/// The [`Visitor`] methods that read a value untyped, for a visitor typing
/// only some JSON shapes; `seq` leaves `visit_seq` out.
macro_rules! forward_untyped {
    () => {
        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<GenericValue, A::Error> {
            Untyped.visit_seq(seq)
        }
        forward_untyped!(seq);
    };
    (seq) => {
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<GenericValue, E> {
            Untyped.visit_bool(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<GenericValue, E> {
            Untyped.visit_i64(v)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<GenericValue, E> {
            Untyped.visit_u64(v)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<GenericValue, E> {
            Untyped.visit_f64(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<GenericValue, E> {
            Untyped.visit_str(v)
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<GenericValue, E> {
            Untyped.visit_string(v)
        }
        fn visit_unit<E: de::Error>(self) -> Result<GenericValue, E> {
            Untyped.visit_unit()
        }
        fn visit_none<E: de::Error>(self) -> Result<GenericValue, E> {
            Untyped.visit_none()
        }
    };
}

/// One object level of the document: each field typed by its declaration,
/// other keys skipped. Anything but an object is read untyped.
struct Level<'a> {
    fields: &'a [Field],
    at: At<'a>,
    failure: &'a Failure,
}

impl<'de> DeserializeSeed<'de> for Level<'_> {
    type Value = GenericValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<GenericValue, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Level<'_> {
    type Value = GenericValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a document object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GenericValue, A::Error> {
        let mut out = BTreeMap::new();
        // The query writes the keys in field order, so the next field is the
        // first guess.
        let mut next = 0;
        while let Some(found) = map.next_key_seed(FieldKey {
            fields: self.fields,
            next,
        })? {
            let Some(mut field) = found.and_then(|index| self.fields.get(index)) else {
                map.next_value::<IgnoredAny>()?;
                continue;
            };
            next = found.map_or(next, |index| index + 1);
            let name = field.field.as_ref();
            // Fields sharing a name: the last value is read through the first
            // declaration, as the mapping's namesake rule states.
            if out.contains_key(name)
                && let Some(first) = self.fields.iter().find(|f| f.field.as_ref() == name)
            {
                field = first;
            }
            let value = map.next_value_seed(FieldSeed {
                source: &field.source,
                at: At::Field(&self.at, name),
                failure: self.failure,
            })?;
            out.insert(name.to_owned(), value);
        }
        Ok(GenericValue::Map(out))
    }

    forward_untyped!();
}

/// A key of an object level: the index of the field it names, if any.
struct FieldKey<'a> {
    fields: &'a [Field],
    next: usize,
}

impl<'de> DeserializeSeed<'de> for FieldKey<'_> {
    type Value = Option<usize>;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Option<usize>, D::Error> {
        deserializer.deserialize_str(self)
    }
}

impl<'de> Visitor<'de> for FieldKey<'_> {
    type Value = Option<usize>;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a field name")
    }

    fn visit_str<E: de::Error>(self, key: &str) -> Result<Option<usize>, E> {
        let named = |field: &Field| field.field.as_ref() == key;
        if self.fields.get(self.next).is_some_and(named) {
            return Ok(Some(self.next));
        }
        Ok(self.fields.iter().position(named))
    }
}

/// One field's value, read by where it comes from.
struct FieldSeed<'a> {
    source: &'a FieldSource,
    at: At<'a>,
    failure: &'a Failure,
}

impl<'de> DeserializeSeed<'de> for FieldSeed<'_> {
    type Value = GenericValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<GenericValue, D::Error> {
        let Self {
            source,
            at,
            failure,
        } = self;
        let failed = |cause| raise(failure, cause);
        match source {
            FieldSource::Column(column) if column.array && column.ty == FlussoType::Decimal => {
                let raw = <&RawValue>::deserialize(deserializer)?;
                decimal_array(raw, column, &at).map_err(failed)
            }
            FieldSource::Column(column) if column.array => {
                let json = serde_json::Value::deserialize(deserializer)?;
                coerce_array(json, column, &at).map_err(failed)
            }
            FieldSource::Column(column) => scalar(deserializer, &column.ty, &at, failure),
            FieldSource::Group(fields) => Level {
                fields,
                at,
                failure,
            }
            .deserialize(deserializer),
            FieldSource::Relation(Relation::Join(join)) => deserializer.deserialize_any(Joined {
                fields: &join.fields,
                at,
                failure,
            }),
            FieldSource::Relation(Relation::Aggregate(aggregate)) => {
                aggregated(deserializer, aggregate, &at, failure)
            }
            // The query reads a constant back as untyped text, or null for an
            // object or a list; the schema holds it exactly.
            FieldSource::Constant(value) => {
                IgnoredAny::deserialize(deserializer)?;
                Ok(value.clone())
            }
            // A geo point ({lat,lon} / [lon,lat] / "lat,lon") carries no scalar
            // type to coerce to; keep its natural JSON shape. A script field is
            // a placeholder null, computed after the build.
            FieldSource::Geo(_) | FieldSource::Script(_) => Untyped.deserialize(deserializer),
        }
    }
}

/// A joined relation: an array of sub-documents (`has_many` / `many_to_many`), a
/// single sub-document (`belongs_to` / `has_one`), or null.
struct Joined<'a> {
    fields: &'a [Field],
    at: At<'a>,
    failure: &'a Failure,
}

impl<'de> Visitor<'de> for Joined<'_> {
    type Value = GenericValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a related document, a list of them, or null")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GenericValue, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element_seed(Level {
            fields: self.fields,
            at: At::Index(&self.at, items.len()),
            failure: self.failure,
        })? {
            items.push(item);
        }
        Ok(GenericValue::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<GenericValue, A::Error> {
        Level {
            fields: self.fields,
            at: self.at,
            failure: self.failure,
        }
        .visit_map(map)
    }

    forward_untyped!(seq);
}

/// A scalar of the declared type `ty`.
fn scalar<'de, D: Deserializer<'de>>(
    deserializer: D,
    ty: &FlussoType,
    at: &At<'_>,
    failure: &Failure,
) -> Result<GenericValue, D::Error> {
    if *ty == FlussoType::Decimal {
        let raw = <&RawValue>::deserialize(deserializer)?;
        return decimal(raw.get(), at).map_err(|cause| raise(failure, cause));
    }
    let json = serde_json::Value::deserialize(deserializer)?;
    typed_scalar(json, ty, at).map_err(|cause| raise(failure, cause))
}

/// An aggregate's result, typed by its op: a count is a [`BigInt`](GenericValue::BigInt),
/// an average a [`Double`](GenericValue::Double), `sum`/`min`/`max` follow the
/// declared `value_type`, `ids` is an array of the element type, and `exists`
/// is a [`Bool`](GenericValue::Bool).
fn aggregated<'de, D: Deserializer<'de>>(
    deserializer: D,
    aggregate: &Aggregate,
    at: &At<'_>,
    failure: &Failure,
) -> Result<GenericValue, D::Error> {
    match &aggregate.op {
        AggregateOp::Count => scalar(deserializer, &FlussoType::Long, at, failure),
        AggregateOp::Exists => scalar(deserializer, &FlussoType::Boolean, at, failure),
        AggregateOp::Avg(_) => scalar(deserializer, &FlussoType::Double, at, failure),
        AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
            match &aggregate.value_type {
                Some(ty) => scalar(deserializer, ty, at, failure),
                None => Untyped.deserialize(deserializer),
            }
        }
        AggregateOp::Ids { element_type } if *element_type == FlussoType::Decimal => {
            let raw = <&RawValue>::deserialize(deserializer)?;
            decimal_ids(raw.get(), at).map_err(|cause| raise(failure, cause))
        }
        AggregateOp::Ids { element_type } => match serde_json::Value::deserialize(deserializer)? {
            serde_json::Value::Array(items) => items
                .into_iter()
                .enumerate()
                .map(|(index, item)| typed_scalar(item, element_type, &At::Index(at, index)))
                .collect::<Result<_, _>>()
                .map(GenericValue::Array)
                .map_err(|cause| raise(failure, cause)),
            other => Ok(json_to_generic(other)),
        },
    }
}

/// An `ids` aggregate of `decimal` keys, each from its exact number text.
fn decimal_ids(raw: &str, at: &At<'_>) -> Result<GenericValue, BuildFailure> {
    if !raw.starts_with('[') {
        return Ok(untyped_raw(raw));
    }
    let items: Vec<&RawValue> = serde_json::from_str(raw)
        .map_err(|error| at.failure(format!("the ids could not be read: {error}")))?;
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| decimal(item.get(), &At::Index(at, index)))
        .collect::<Result<_, _>>()
        .map(GenericValue::Array)
}

/// A JSON value read without a declared type: a whole number is a
/// [`BigInt`](GenericValue::BigInt), any other number goes through [`float`].
struct Untyped;

impl<'de> DeserializeSeed<'de> for Untyped {
    type Value = GenericValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<GenericValue, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Untyped {
    type Value = GenericValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<GenericValue, E> {
        Ok(GenericValue::Bool(v))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<GenericValue, E> {
        Ok(GenericValue::BigInt(v))
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<GenericValue, E> {
        Ok(i64::try_from(v).map_or_else(|_| float(v as f64), GenericValue::BigInt))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<GenericValue, E> {
        Ok(float(v))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<GenericValue, E> {
        Ok(GenericValue::String(v.to_owned()))
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<GenericValue, E> {
        Ok(GenericValue::String(v))
    }

    fn visit_unit<E: de::Error>(self) -> Result<GenericValue, E> {
        Ok(GenericValue::Null)
    }

    fn visit_none<E: de::Error>(self) -> Result<GenericValue, E> {
        Ok(GenericValue::Null)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GenericValue, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element_seed(Untyped)? {
            items.push(item);
        }
        Ok(GenericValue::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GenericValue, A::Error> {
        let mut out = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(Untyped)?;
            out.insert(key, value);
        }
        Ok(GenericValue::Map(out))
    }
}

/// `raw`, valid JSON, read untyped.
fn untyped_raw(raw: &str) -> GenericValue {
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    Untyped
        .deserialize(&mut deserializer)
        .unwrap_or(GenericValue::Null)
}

/// An array column's value: null becomes the column's `default` list (or stays
/// null), a value that is not an array is wrapped, and null elements are
/// dropped. Each remaining element is coerced to the declared type; for any
/// type but `json`, an object or array element fails the document.
fn coerce_array(
    json: serde_json::Value,
    column: &Column,
    at: &At<'_>,
) -> Result<GenericValue, BuildFailure> {
    let (items, wrapped) = match json {
        serde_json::Value::Null => return Ok(default_list(column)),
        serde_json::Value::Array(items) => (items, false),
        other => (vec![other], true),
    };
    if column.ty != FlussoType::Json {
        let kinds = items.iter().map(|item| match item {
            serde_json::Value::Object(_) => Some("an object"),
            serde_json::Value::Array(_) => Some("an array"),
            _ => None,
        });
        if let Some(failure) = structured_element(kinds, wrapped, column, at) {
            return Err(failure);
        }
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(_, item)| !item.is_null())
        .map(|(index, item)| typed_scalar(item, &column.ty, &At::Index(at, index)))
        .collect::<Result<_, _>>()
        .map(GenericValue::Array)
}

/// A `decimal` array column's value, as [`coerce_array`] reads any other, each
/// element from the exact number text Postgres wrote.
fn decimal_array(
    raw: &RawValue,
    column: &Column,
    at: &At<'_>,
) -> Result<GenericValue, BuildFailure> {
    let raw = raw.get();
    let (items, wrapped): (Vec<&RawValue>, bool) = match raw.as_bytes().first() {
        Some(b'n') => return Ok(default_list(column)),
        Some(b'[') => (
            serde_json::from_str(raw)
                .map_err(|error| at.failure(format!("the column could not be read: {error}")))?,
            false,
        ),
        _ => {
            (
                vec![serde_json::from_str(raw).map_err(|error| {
                    at.failure(format!("the column could not be read: {error}"))
                })?],
                true,
            )
        }
    };
    let kinds = items
        .iter()
        .map(|item| match item.get().as_bytes().first() {
            Some(b'{') => Some("an object"),
            Some(b'[') => Some("an array"),
            _ => None,
        });
    if let Some(failure) = structured_element(kinds, wrapped, column, at) {
        return Err(failure);
    }
    items
        .into_iter()
        .enumerate()
        .filter(|(_, item)| item.get() != "null")
        .map(|(index, item)| decimal(item.get(), &At::Index(at, index)))
        .collect::<Result<_, _>>()
        .map(GenericValue::Array)
}

/// The failure for the first element `kinds` names as structured (an object
/// or an array), which a scalar array field cannot hold.
fn structured_element(
    kinds: impl Iterator<Item = Option<&'static str>>,
    wrapped: bool,
    column: &Column,
    at: &At<'_>,
) -> Option<BuildFailure> {
    let (index, what) = kinds
        .enumerate()
        .find_map(|(index, kind)| kind.map(|what| (index, what)))?;
    let position = if wrapped {
        String::new()
    } else {
        format!(" at [{index}]")
    };
    Some(at.failure(format!(
        "the column holds {what}{position}, but an `array: true` field of `{}` takes \
         scalar elements",
        type_name(&column.ty)
    )))
}

/// A null array column's `default` list (or null), each element coerced to the
/// column's type. A decimal element is kept as the schema holds it.
fn default_list(column: &Column) -> GenericValue {
    let Some(default) = &column.default else {
        return GenericValue::Null;
    };
    let items = match default {
        GenericValue::Array(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    GenericValue::Array(
        items
            .iter()
            .filter(|item| **item != GenericValue::Null)
            .map(|item| match (&column.ty, item) {
                (FlussoType::Decimal, GenericValue::Decimal(_) | GenericValue::BigDecimal(_)) => {
                    item.clone()
                }
                (ty, item) => coerce_scalar(generic_to_json(item), ty),
            })
            .collect(),
    )
}

fn generic_to_json(value: &GenericValue) -> serde_json::Value {
    match value {
        GenericValue::Null | GenericValue::Bytes(_) => serde_json::Value::Null,
        GenericValue::Bool(b) => serde_json::Value::Bool(*b),
        GenericValue::SmallInt(i) => (*i).into(),
        GenericValue::Int(i) => (*i).into(),
        GenericValue::BigInt(i) => (*i).into(),
        GenericValue::Float(f) => serde_json::json!(f),
        GenericValue::Double(f) => serde_json::json!(f),
        GenericValue::Decimal(d) => serde_json::from_str(&d.to_string())
            .unwrap_or_else(|_| serde_json::Value::String(d.to_string())),
        GenericValue::BigDecimal(d) => serde_json::from_str(d.as_str())
            .unwrap_or_else(|_| serde_json::Value::String(d.to_string())),
        GenericValue::String(s) => serde_json::Value::String(s.clone()),
        GenericValue::Uuid(u) => serde_json::Value::String(u.to_string()),
        GenericValue::Date(d) => serde_json::Value::String(d.to_string()),
        GenericValue::Time(t) => serde_json::Value::String(t.to_string()),
        GenericValue::Timestamp(ts) => {
            serde_json::Value::String(ts.format("%Y-%m-%dT%H:%M:%S%.f").to_string())
        }
        GenericValue::TimestampTz(ts) => serde_json::Value::String(ts.to_rfc3339()),
        GenericValue::Array(items) => items.iter().map(generic_to_json).collect(),
        GenericValue::Map(map) => serde_json::Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), generic_to_json(value)))
                .collect(),
        ),
    }
}

/// The type as a schema writes it, for a failure message.
fn type_name(ty: &FlussoType) -> String {
    match serde_json::to_value(ty) {
        Ok(serde_json::Value::String(name)) => name,
        _ => "custom".to_owned(),
    }
}

/// `NaN`, `Infinity` and `-Infinity`, as Postgres writes a `numeric`,
/// `float4` or `float8` it has no JSON number for.
fn is_non_finite(text: &str) -> bool {
    matches!(text, "NaN" | "Infinity" | "-Infinity")
}

fn non_finite(text: &str, ty: &FlussoType, at: &At<'_>) -> BuildFailure {
    at.failure(format!(
        "the value is `{text}`, which a `{}` field cannot hold: JSON has no number for it",
        type_name(ty)
    ))
}

/// [`coerce_scalar`], failing on a non-finite number under a number type.
fn typed_scalar(
    json: serde_json::Value,
    ty: &FlussoType,
    at: &At<'_>,
) -> Result<GenericValue, BuildFailure> {
    if let (
        FlussoType::Short
        | FlussoType::Integer
        | FlussoType::Long
        | FlussoType::Float
        | FlussoType::Double
        | FlussoType::Decimal,
        Some(text),
    ) = (ty, json.as_str())
        && is_non_finite(text)
    {
        return Err(non_finite(text, ty, at));
    }
    Ok(coerce_scalar(json, ty))
}

/// A `decimal` from `raw`, the JSON text Postgres wrote: a number keeps every
/// digit ([`Decimal`] within its range, [`BigDecimal`] past it), and so does a
/// string that holds one. Other text stays a string, a non-finite one fails.
fn decimal(raw: &str, at: &At<'_>) -> Result<GenericValue, BuildFailure> {
    match raw.as_bytes().first() {
        Some(b'-' | b'0'..=b'9') => Ok(exact_decimal(raw).unwrap_or_else(|| untyped_raw(raw))),
        Some(b'"') => {
            let text: Cow<'_, str> = match serde_json::from_str::<&str>(raw) {
                Ok(text) => Cow::Borrowed(text),
                Err(_) => Cow::Owned(serde_json::from_str::<String>(raw).unwrap_or_default()),
            };
            if is_non_finite(&text) {
                return Err(non_finite(&text, &FlussoType::Decimal, at));
            }
            Ok(exact_decimal(&text).unwrap_or_else(|| GenericValue::String(text.into_owned())))
        }
        _ => Ok(untyped_raw(raw)),
    }
}

/// `text` as an exact decimal, `None` when it is not a decimal number.
fn exact_decimal(text: &str) -> Option<GenericValue> {
    match Decimal::from_str_exact(text) {
        Ok(d) => Some(GenericValue::Decimal(d)),
        Err(_) => BigDecimal::parse(text).map(GenericValue::BigDecimal),
    }
}

/// Coerce a JSON scalar to the canonical variant its declared [`FlussoType`]
/// implies. A null stays null; a value that doesn't fit the declared type falls
/// back to its untyped JSON shape (never an error).
fn coerce_scalar(json: serde_json::Value, ty: &FlussoType) -> GenericValue {
    if json.is_null() {
        return GenericValue::Null;
    }
    match ty {
        FlussoType::Boolean => match json.as_bool() {
            Some(b) => GenericValue::Bool(b),
            None => json_to_generic(json),
        },
        FlussoType::Short => match json.as_i64().and_then(|i| i16::try_from(i).ok()) {
            Some(i) => GenericValue::SmallInt(i),
            None => json_to_generic(json),
        },
        FlussoType::Integer => match json.as_i64().and_then(|i| i32::try_from(i).ok()) {
            Some(i) => GenericValue::Int(i),
            None => json_to_generic(json),
        },
        FlussoType::Long => match json.as_i64() {
            Some(i) => GenericValue::BigInt(i),
            None => json_to_generic(json),
        },
        FlussoType::Float => match json.as_f64() {
            Some(f) => GenericValue::Float(f as f32),
            None => json_to_generic(json),
        },
        FlussoType::Double => match json.as_f64() {
            Some(f) => GenericValue::Double(f),
            None => json_to_generic(json),
        },
        FlussoType::Decimal => match &json {
            serde_json::Value::String(text) => exact_decimal(text),
            serde_json::Value::Number(n) => exact_decimal(&n.to_string()),
            _ => None,
        }
        .unwrap_or_else(|| json_to_generic(json)),
        FlussoType::Uuid => match json.as_str().and_then(|s| Uuid::parse_str(s).ok()) {
            Some(u) => GenericValue::Uuid(u),
            None => json_to_generic(json),
        },
        FlussoType::Date => match json.as_str().and_then(|s| s.parse::<NaiveDate>().ok()) {
            Some(d) => GenericValue::Date(d),
            None => json_to_generic(json),
        },
        // `timestamp` / `timestamptz` / `time` all declare `Timestamp`; recover
        // the precise variant from the value's shape.
        FlussoType::Timestamp => json
            .as_str()
            .and_then(parse_temporal)
            .unwrap_or_else(|| json_to_generic(json)),
        FlussoType::Binary => match json.as_str().and_then(crate::text_value::postgres_hex) {
            Some(bytes) => GenericValue::Bytes(bytes),
            None => json_to_generic(json),
        },
        // Text-family and the structured / escape-hatch types keep their natural
        // JSON shape.
        FlussoType::Text
        | FlussoType::Identifier
        | FlussoType::Keyword
        | FlussoType::Enum
        | FlussoType::Json
        | FlussoType::Map { .. }
        | FlussoType::GeoPoint
        | FlussoType::Custom { .. } => json_to_generic(json),
    }
}

/// Parse a `Timestamp`-declared string into the tightest temporal variant: an
/// offset instant → [`TimestampTz`](GenericValue::TimestampTz), else a naive
/// datetime, a time, or a date. `None` if it's none of those.
fn parse_temporal(s: &str) -> Option<GenericValue> {
    if let Ok(instant) = DateTime::parse_from_rfc3339(s) {
        return Some(GenericValue::TimestampTz(instant.with_timezone(&Utc)));
    }
    if let Ok(naive) = s.parse::<NaiveDateTime>() {
        return Some(GenericValue::Timestamp(naive));
    }
    if let Ok(time) = s.parse::<NaiveTime>() {
        return Some(GenericValue::Time(time));
    }
    s.parse::<NaiveDate>().ok().map(GenericValue::Date)
}

/// A float with no declared type: a decimal where it fits one exactly, else a
/// double. Either is written as a JSON number.
fn float(v: f64) -> GenericValue {
    Decimal::try_from(v).map_or(GenericValue::Double(v), GenericValue::Decimal)
}

#[cfg(test)]
mod tests;
