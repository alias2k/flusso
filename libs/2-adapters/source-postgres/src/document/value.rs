//! Decoding Postgres results into the schema's [`GenericValue`]: single named
//! columns (for reverse-resolution lookups) and JSON documents (assembled
//! server-side).

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use kernel::{
    Aggregate, AggregateOp, BigDecimal, Column, Field, FieldSource, FlussoType, GenericValue,
    Relation,
};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::de::{self, DeserializeSeed, Deserializer, Error as _, MapAccess, SeqAccess, Visitor};
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
///
/// The value is cast to `key_type`, the column's base type with no modifier
/// (`PgDocumentBuilder::key_type`), so the statement's result types follow
/// from its text alone. Postgres refuses to run a prepared statement whose
/// result type changed under it ("cached plan must not change result type"),
/// and sqlx keeps one per text on each pooled connection: without the cast, a
/// key column that changed type (`int` → `bigint`) would fail every lookup
/// on every connection that ran it before, with the same text after.
pub(crate) fn key_select(expr: &str, alias: &str, key_type: &str, base_name: &str) -> String {
    format!(
        "{} AS \"{alias}\", {expr}::{key_type} AS \"{alias}{TYPED}\"",
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

/// Convert a JSON value (e.g. a server-assembled document) into the value tree.
pub(super) fn json_to_generic(value: serde_json::Value) -> GenericValue {
    match value {
        serde_json::Value::Null => GenericValue::Null,
        serde_json::Value::Bool(b) => GenericValue::Bool(b),
        serde_json::Value::Number(n) => kernel::untyped_json_number(&n),
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

/// How the document of one index is read, built once per builder: at each
/// object level, the first field declared under each name, so a key resolves
/// in one lookup whatever order the query wrote the keys in.
#[derive(Debug, Default)]
pub(crate) struct DocumentPlan {
    first: HashMap<Box<str>, usize>,
    is_first: Vec<bool>,
    children: Vec<DocumentPlan>,
}

impl DocumentPlan {
    /// The plan of a level declaring `fields`, and of every level below it.
    pub(crate) fn new(fields: &[Field]) -> Self {
        let mut first = HashMap::with_capacity(fields.len());
        let is_first = fields
            .iter()
            .enumerate()
            .map(
                |(index, field)| match first.entry(Box::from(field.field.as_ref())) {
                    Entry::Vacant(slot) => {
                        slot.insert(index);
                        true
                    }
                    Entry::Occupied(_) => false,
                },
            )
            .collect();
        let children = fields
            .iter()
            .map(|field| match &field.source {
                FieldSource::Group(fields) => Self::new(fields),
                FieldSource::Relation(Relation::Join(join)) => Self::new(&join.fields),
                _ => Self::default(),
            })
            .collect();
        Self {
            first,
            is_first,
            children,
        }
    }

    /// The field `key` names: the first declared under that name. The query
    /// writes keys in field order up to 50 fields, so `next` is tried first.
    fn field(&self, fields: &[Field], key: &str, next: usize) -> Option<usize> {
        let guessed = fields
            .get(next)
            .is_some_and(|field| field.field.as_ref() == key)
            && self.is_first.get(next) == Some(&true);
        if guessed {
            return Some(next);
        }
        self.first.get(key).copied()
    }

    fn child(&self, index: usize) -> &DocumentPlan {
        static LEAF: std::sync::OnceLock<DocumentPlan> = std::sync::OnceLock::new();
        self.children
            .get(index)
            .unwrap_or_else(|| LEAF.get_or_init(DocumentPlan::default))
    }
}

/// Read a server-assembled JSON document into the **typed** value tree, using
/// each field's declared type so the values a sink sees are canonical — a `date`
/// field becomes a [`Date`](GenericValue::Date), a `uuid` a [`Uuid`](GenericValue::Uuid),
/// not opaque strings. The document is built server-side as one JSON blob (so the
/// types are erased on the wire); this is where we put them back, guided by the
/// schema and its [`DocumentPlan`], in one pass over the text. Anything that
/// fails to parse falls back to the untyped JSON value rather than failing the
/// document.
///
/// A declared `decimal` (a column, an array element, a `sum`/`min`/`max`, an
/// `ids` key, a `map` value) reads the exact number text Postgres wrote, as a
/// [`BigDecimal`](GenericValue::BigDecimal), never rounded through `f64`. Every
/// other value reads as it always did, a number with no declared type through
/// `f64` ([`json_to_generic`]).
///
/// An array column element that is not a scalar fails the document
/// ([`coerce_array`]), since its field cannot hold it; a document that doesn't
/// parse, or nests deeper than `serde_json`'s 128 levels, is `Malformed`, and
/// that wins over a failure, as when the whole document was parsed first.
pub(crate) fn coerce_document(
    json: &[u8],
    fields: &[Field],
    plan: &DocumentPlan,
) -> Result<GenericValue, DocumentError> {
    let failures = Failures::default();
    let mut deserializer = serde_json::Deserializer::from_slice(json);
    let read = Level {
        fields,
        plan,
        at: At::Root,
        failures: &failures,
    }
    .deserialize(&mut deserializer)
    .and_then(|body| deserializer.end().map(|()| body));
    match read {
        Err(error) => Err(DocumentError::Malformed(error)),
        Ok(body) => match failures.first() {
            Some(failure) => Err(DocumentError::Failed(failure)),
            None => Ok(body),
        },
    }
}

/// Read each document for `fields`; `None` for one that failed.
#[cfg(feature = "bench")]
pub(crate) fn bench_reader(fields: Vec<Field>) -> impl Fn(&[u8]) -> Option<GenericValue> {
    let plan = DocumentPlan::new(&fields);
    move |json| coerce_document(json, &fields, &plan).ok()
}

/// The document a query row holds in its `document` column, read by
/// [`coerce_document`]: the inner `Err` is a document that failed, the outer
/// one a row that holds no readable document.
pub(crate) fn read_document(
    row: &PgRow,
    fields: &[Field],
    plan: &DocumentPlan,
) -> source::Result<Result<GenericValue, BuildFailure>> {
    let raw = row.try_get_raw("document").map_err(super::query_err)?;
    let mut json = raw
        .as_bytes()
        .map_err(|error| SourceError::Query(format!("reading a document: {error}")))?;
    // A `jsonb` in the binary format leads with its format version.
    if raw.type_info().name() == "JSONB" && raw.format() == PgValueFormat::Binary {
        json = json.get(1..).unwrap_or_default();
    }
    match coerce_document(json, fields, plan) {
        Ok(body) => Ok(Ok(body)),
        Err(DocumentError::Failed(failure)) => Ok(Err(failure)),
        Err(DocumentError::Malformed(error)) => Err(SourceError::Query(format!(
            "the document query returned malformed JSON: {error}"
        ))),
    }
}

/// Where a value sits in the document: rendered only for a failure, and
/// ordered as the fields are declared, so the failure reported is the one a
/// walk of the fields in declaration order meets first.
#[derive(Clone, Copy)]
enum At<'a> {
    Root,
    Field(&'a At<'a>, &'a str, usize),
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
            At::Field(parent, name, _) => {
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

    fn order(&self) -> Vec<usize> {
        match self {
            At::Root => Vec::new(),
            At::Field(parent, _, index) | At::Index(parent, index) => {
                let mut order = parent.order();
                order.push(*index);
                order
            }
        }
    }

    /// How many objects and arrays enclose the value here.
    fn containers(&self) -> usize {
        match self {
            At::Root => 0,
            At::Field(parent, ..) | At::Index(parent, _) => parent.containers() + 1,
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

/// The failures met while reading, each with where it sits. Reading goes on
/// past one, so a malformed document is reported as malformed wherever it is.
#[derive(Default)]
struct Failures(RefCell<Vec<(Vec<usize>, BuildFailure)>>);

impl Failures {
    fn record(&self, at: &At<'_>, failure: BuildFailure) {
        self.0.borrow_mut().push((at.order(), failure));
    }

    /// Forget what was met under `at`: a repeated key replaces its value.
    fn forget_under(&self, at: &At<'_>) {
        let prefix = at.order();
        self.0
            .borrow_mut()
            .retain(|(order, _)| !order.starts_with(&prefix));
    }

    fn first(self) -> Option<BuildFailure> {
        self.0
            .into_inner()
            .into_iter()
            .min_by(|(a, _), (b, _)| a.cmp(b))
            .map(|(_, failure)| failure)
    }
}

/// The deepest `serde_json` reads: a 128th enclosing object or array fails.
const DEPTH_LIMIT: usize = 128;

/// The [`Visitor`] methods that read a value untyped, for a visitor typing
/// only some JSON shapes; `seq` leaves `visit_seq` out.
macro_rules! forward_untyped {
    () => {
        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<GenericValue, A::Error> {
            Untyped::IN_PLACE.visit_seq(seq)
        }
        forward_untyped!(seq);
    };
    (seq) => {
        fn visit_bool<E: de::Error>(self, v: bool) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_bool(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_i64(v)
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_u64(v)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_f64(v)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_str(v)
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_string(v)
        }
        fn visit_unit<E: de::Error>(self) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_unit()
        }
        fn visit_none<E: de::Error>(self) -> Result<GenericValue, E> {
            Untyped::IN_PLACE.visit_none()
        }
    };
}

/// One object level of the document: each field typed by its declaration,
/// other keys skipped. Anything but an object is read untyped.
struct Level<'a> {
    fields: &'a [Field],
    plan: &'a DocumentPlan,
    at: At<'a>,
    failures: &'a Failures,
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
        let mut next = 0;
        while let Some(found) = map.next_key_seed(FieldKey {
            fields: self.fields,
            plan: self.plan,
            next,
        })? {
            let Some((index, field)) =
                found.and_then(|index| Some((index, self.fields.get(index)?)))
            else {
                map.next_value::<Skip>()?;
                continue;
            };
            next = index + 1;
            let name = field.field.as_ref();
            let at = At::Field(&self.at, name, index);
            if out.contains_key(name) {
                self.failures.forget_under(&at);
            }
            let value = map.next_value_seed(FieldSeed {
                source: &field.source,
                plan: self.plan.child(index),
                at,
                failures: self.failures,
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
    plan: &'a DocumentPlan,
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
        Ok(self.plan.field(self.fields, key, self.next))
    }
}

/// One field's value, read by where it comes from.
struct FieldSeed<'a> {
    source: &'a FieldSource,
    plan: &'a DocumentPlan,
    at: At<'a>,
    failures: &'a Failures,
}

impl<'de> DeserializeSeed<'de> for FieldSeed<'_> {
    type Value = GenericValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<GenericValue, D::Error> {
        let Self {
            source,
            plan,
            at,
            failures,
        } = self;
        match source {
            FieldSource::Column(column)
                if column.array && matches!(column.ty, FlussoType::Decimal | FlussoType::Float) =>
            {
                let element: fn(&str, &At<'_>) -> Result<GenericValue, String> = match column.ty {
                    FlussoType::Decimal => decimal,
                    _ => single,
                };
                let raw = <&RawValue>::deserialize(deserializer)?;
                let read = raw_array(raw.get(), column, &at, element).map_err(D::Error::custom)?;
                Ok(failed_as_null(read, &at, failures))
            }
            FieldSource::Column(column) if column.array => {
                let json = serde_json::Value::deserialize(deserializer)?;
                Ok(failed_as_null(
                    coerce_array(json, column, &at),
                    &at,
                    failures,
                ))
            }
            FieldSource::Column(column) => scalar(deserializer, &column.ty, &at),
            FieldSource::Group(fields) => Level {
                fields,
                plan,
                at,
                failures,
            }
            .deserialize(deserializer),
            FieldSource::Relation(Relation::Join(join)) => deserializer.deserialize_any(Joined {
                fields: &join.fields,
                plan,
                at,
                failures,
            }),
            FieldSource::Relation(Relation::Aggregate(aggregate)) => {
                aggregated(deserializer, aggregate, &at)
            }
            // The query reads a constant back as untyped text, or null for an
            // object or a list; the schema holds it exactly.
            FieldSource::Constant(value) => {
                Skip::deserialize(deserializer)?;
                Ok(value.clone())
            }
            // A geo point ({lat,lon} / [lon,lat] / "lat,lon") carries no scalar
            // type to coerce to; keep its natural JSON shape. A script field is
            // a placeholder null, computed after the build.
            FieldSource::Geo(_) | FieldSource::Script(_) => {
                Untyped::IN_PLACE.deserialize(deserializer)
            }
        }
    }
}

/// `read`'s body, or `null` with its failure recorded.
fn failed_as_null(
    read: Result<GenericValue, BuildFailure>,
    at: &At<'_>,
    failures: &Failures,
) -> GenericValue {
    read.unwrap_or_else(|failure| {
        failures.record(at, failure);
        GenericValue::Null
    })
}

/// A joined relation: an array of sub-documents (`has_many` / `many_to_many`), a
/// single sub-document (`belongs_to` / `has_one`), or null.
struct Joined<'a> {
    fields: &'a [Field],
    plan: &'a DocumentPlan,
    at: At<'a>,
    failures: &'a Failures,
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
            plan: self.plan,
            at: At::Index(&self.at, items.len()),
            failures: self.failures,
        })? {
            items.push(item);
        }
        Ok(GenericValue::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<GenericValue, A::Error> {
        Level {
            fields: self.fields,
            plan: self.plan,
            at: self.at,
            failures: self.failures,
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
) -> Result<GenericValue, D::Error> {
    match ty {
        FlussoType::Decimal => {
            let raw = <&RawValue>::deserialize(deserializer)?;
            decimal(raw.get(), at).map_err(D::Error::custom)
        }
        FlussoType::Float => {
            let raw = <&RawValue>::deserialize(deserializer)?;
            single(raw.get(), at).map_err(D::Error::custom)
        }
        FlussoType::Map { values } if **values == FlussoType::Decimal => {
            deserializer.deserialize_any(DecimalMap { at: *at })
        }
        _ => Ok(coerce_scalar(
            serde_json::Value::deserialize(deserializer)?,
            ty,
        )),
    }
}

/// An aggregate's result, typed by its op: a count is a [`BigInt`](GenericValue::BigInt),
/// an average a [`Double`](GenericValue::Double), `sum`/`min`/`max` follow the
/// declared `value_type`, `ids` is an array of the element type, and `exists`
/// is a [`Bool`](GenericValue::Bool).
fn aggregated<'de, D: Deserializer<'de>>(
    deserializer: D,
    aggregate: &Aggregate,
    at: &At<'_>,
) -> Result<GenericValue, D::Error> {
    match &aggregate.op {
        AggregateOp::Count => scalar(deserializer, &FlussoType::Long, at),
        AggregateOp::Exists => scalar(deserializer, &FlussoType::Boolean, at),
        AggregateOp::Avg(_) => scalar(deserializer, &FlussoType::Double, at),
        AggregateOp::Sum(_) | AggregateOp::Min(_) | AggregateOp::Max(_) => {
            match &aggregate.value_type {
                Some(ty) => scalar(deserializer, ty, at),
                None => Untyped::IN_PLACE.deserialize(deserializer),
            }
        }
        AggregateOp::Ids { element_type }
            if matches!(element_type, FlussoType::Decimal | FlussoType::Float) =>
        {
            let element: fn(&str, &At<'_>) -> Result<GenericValue, String> = match element_type {
                FlussoType::Decimal => decimal,
                _ => single,
            };
            let raw = <&RawValue>::deserialize(deserializer)?;
            raw_ids(raw.get(), at, element).map_err(D::Error::custom)
        }
        AggregateOp::Ids { element_type } => {
            Ok(match serde_json::Value::deserialize(deserializer)? {
                serde_json::Value::Array(items) => GenericValue::Array(
                    items
                        .into_iter()
                        .map(|item| coerce_scalar(item, element_type))
                        .collect(),
                ),
                other => json_to_generic(other),
            })
        }
    }
}

/// A `map` with `values: decimal`: each value read as a declared `decimal`.
struct DecimalMap<'a> {
    at: At<'a>,
}

impl<'de> Visitor<'de> for DecimalMap<'_> {
    type Value = GenericValue;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("an object of decimals")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GenericValue, A::Error> {
        let mut out = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let raw = map.next_value::<&RawValue>()?;
            let value = decimal(raw.get(), &At::Field(&self.at, &key, out.len()))
                .map_err(A::Error::custom)?;
            out.insert(key, value);
        }
        Ok(GenericValue::Map(out))
    }

    forward_untyped!();
}

/// A value read only to be checked, as a whole document read would check it:
/// a number past `f64`'s range fails, as it did when the document was read
/// into a `serde_json::Value`.
struct Skip;

impl<'de> Deserialize<'de> for Skip {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Skip, D::Error> {
        deserializer.deserialize_any(Skip)
    }
}

impl<'de> Visitor<'de> for Skip {
    type Value = Skip;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Skip, E> {
        Ok(Skip)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Skip, A::Error> {
        while seq.next_element::<Skip>()?.is_some() {}
        Ok(Skip)
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Skip, A::Error> {
        while map.next_key::<Skip>()?.is_some() {
            map.next_value::<Skip>()?;
        }
        Ok(Skip)
    }
}

/// A JSON value read without a declared type, as [`json_to_generic`] reads
/// it: a number by [`kernel::untyped_json_number`] (a whole number in `i64` a
/// [`BigInt`](GenericValue::BigInt), past it the exact
/// [`Decimal`](GenericValue::Decimal), a fraction [`kernel::untyped_number`]). `remaining` counts the objects and arrays
/// it may still open before `serde_json`'s depth limit, for a value read again
/// from its raw text.
#[derive(Clone, Copy)]
struct Untyped {
    remaining: usize,
}

impl Untyped {
    const IN_PLACE: Self = Self {
        remaining: usize::MAX,
    };

    fn enter<E: de::Error>(self) -> Result<Self, E> {
        match self.remaining.checked_sub(1) {
            Some(remaining) if remaining > 0 => Ok(Self { remaining }),
            _ => Err(E::custom("recursion limit exceeded")),
        }
    }
}

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

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<GenericValue, E> {
        Ok(i64::try_from(v).map_or_else(
            |_| GenericValue::Decimal(Decimal::from(v)),
            GenericValue::BigInt,
        ))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<GenericValue, E> {
        Ok(kernel::untyped_number(v))
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
        let inner = self.enter()?;
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(item) = seq.next_element_seed(inner)? {
            items.push(item);
        }
        Ok(GenericValue::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<GenericValue, A::Error> {
        let inner = self.enter()?;
        let mut out = BTreeMap::new();
        while let Some(key) = map.next_key::<String>()? {
            let value = map.next_value_seed(inner)?;
            out.insert(key, value);
        }
        Ok(GenericValue::Map(out))
    }
}

/// `raw`, valid JSON captured at `at`, read untyped again. It may open no more
/// objects and arrays than the document had left there, so it fails exactly
/// where reading the whole document at once would have.
fn untyped_raw(raw: &str, at: &At<'_>) -> Result<GenericValue, String> {
    let reread = Untyped {
        remaining: DEPTH_LIMIT.saturating_sub(at.containers()),
    };
    let mut deserializer = serde_json::Deserializer::from_str(raw);
    reread
        .deserialize(&mut deserializer)
        .map_err(|error| error.to_string())
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
    Ok(coerce_elements(items, &column.ty))
}

/// A `decimal` or `float` array column's value, as [`coerce_array`] reads any
/// other, each element read by `element` from the exact number text Postgres
/// wrote. The outer `Err` is a value nested past the depth limit.
fn raw_array(
    raw: &str,
    column: &Column,
    at: &At<'_>,
    element: fn(&str, &At<'_>) -> Result<GenericValue, String>,
) -> Result<Result<GenericValue, BuildFailure>, String> {
    let (items, wrapped): (Vec<&str>, bool) = match raw.as_bytes().first() {
        Some(b'n') => return Ok(Ok(default_list(column))),
        Some(b'[') => (
            serde_json::from_str::<Vec<&RawValue>>(raw)
                .map_err(|error| error.to_string())?
                .into_iter()
                .map(RawValue::get)
                .collect(),
            false,
        ),
        _ => (vec![raw], true),
    };
    let structured = |item: &&str| match item.as_bytes().first() {
        Some(b'{') => Some("an object"),
        Some(b'[') => Some("an array"),
        _ => None,
    };
    for (index, item) in items.iter().enumerate() {
        if structured(item).is_some() {
            let element = At::Index(at, index);
            untyped_raw(item, if wrapped { at } else { &element })?;
        }
    }
    if let Some(failure) = structured_element(items.iter().map(structured), wrapped, column, at) {
        return Ok(Err(failure));
    }
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| **item != "null")
        .map(|(index, item)| element(item, &At::Index(at, index)))
        .collect::<Result<_, _>>()
        .map(|items| Ok(GenericValue::Array(items)))
}

/// An `ids` aggregate of `decimal` or `float` keys, each read by `element`
/// from its exact number text.
fn raw_ids(
    raw: &str,
    at: &At<'_>,
    element: fn(&str, &At<'_>) -> Result<GenericValue, String>,
) -> Result<GenericValue, String> {
    if !raw.starts_with('[') {
        return untyped_raw(raw, at);
    }
    serde_json::from_str::<Vec<&RawValue>>(raw)
        .map_err(|error| error.to_string())?
        .into_iter()
        .enumerate()
        .map(|(index, item)| element(item.get(), &At::Index(at, index)))
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

/// A null array column's `default` list (or null), each element coerced to
/// the column's type.
fn default_list(column: &Column) -> GenericValue {
    column
        .default
        .as_ref()
        .map_or(GenericValue::Null, |default| {
            coerce_elements(default_to_json(default), &column.ty)
        })
}

/// A declared `decimal` from `raw`, the JSON text Postgres wrote: a number
/// keeps that text, and so does a string holding a decimal number. Any other
/// value reads as it did before (issue #327): a string `Decimal` parses,
/// other text, `null`, a boolean or a structure untyped. `Err` is a value
/// nested past the depth limit.
fn decimal(raw: &str, at: &At<'_>) -> Result<GenericValue, String> {
    match raw.as_bytes().first() {
        Some(b'-' | b'0'..=b'9') => Ok(BigDecimal::parse(raw).map_or_else(
            || GenericValue::String(raw.to_owned()),
            GenericValue::BigDecimal,
        )),
        Some(b'"') => {
            let text: Cow<'_, str> = match serde_json::from_str::<&str>(raw) {
                Ok(text) => Cow::Borrowed(text),
                Err(_) => Cow::Owned(
                    serde_json::from_str::<String>(raw).map_err(|error| error.to_string())?,
                ),
            };
            Ok(match BigDecimal::parse(&text) {
                Some(exact) => GenericValue::BigDecimal(exact),
                None => Decimal::from_str(&text).map_or_else(
                    |_| GenericValue::String(text.into_owned()),
                    GenericValue::Decimal,
                ),
            })
        }
        _ => untyped_raw(raw, at),
    }
}

/// A declared `float` from `raw`, the JSON text Postgres wrote: a number is
/// what Postgres's own cast to `float4` gives, read by how many significant
/// digits the text has (issue #366):
///
/// - up to 9, the `f32` nearest the text: a `float4` column's own value (a
///   correctly rounded `f64` then `as f32` rounds twice: `7.038531e-26`), and
///   `numeric::float4`, which parses the text;
/// - 10 to 17, the `f64` the text names, then `as f32`: a `float8` column's
///   shortest text, and `float8::float4`, which rounds the double;
/// - 18 or more, the `f32` nearest the text: only a `numeric` writes that many.
///
/// A number past `f32`'s range and within `f64`'s is infinite, which the sink
/// writes as `null`, as 0.17 did (Postgres's cast would refuse it; refusing it
/// here would fail documents 0.17 indexed). Past `f64`'s range the read fails,
/// as 0.17's did. Any other value reads as before. `Err` is a value nested past
/// the depth limit, or out of range.
fn single(raw: &str, at: &At<'_>) -> Result<GenericValue, String> {
    match raw.as_bytes().first() {
        Some(b'-' | b'0'..=b'9') => {
            let wide: f64 = raw.parse().map_err(|error| format!("{error}"))?;
            if wide.is_infinite() {
                return Err(format!("number out of range: {raw}"));
            }
            #[allow(clippy::cast_possible_truncation)]
            if (10..=17).contains(&significant_digits(raw)) {
                return Ok(GenericValue::Float(wide as f32));
            }
            raw.parse::<f32>()
                .map(GenericValue::Float)
                .map_err(|error| error.to_string())
        }
        Some(b'{' | b'[') => untyped_raw(raw, at),
        _ => serde_json::from_str(raw)
            .map(|json| coerce_scalar(json, &FlussoType::Float))
            .map_err(|error| error.to_string()),
    }
}

/// The significant digits of `number`, a JSON number: those of its mantissa
/// from the first non-zero one.
fn significant_digits(number: &str) -> usize {
    number
        .bytes()
        .take_while(|b| b | 0x20 != b'e')
        .filter(u8::is_ascii_digit)
        .skip_while(|&b| b == b'0')
        .count()
}

/// Coerce each non-null element to `ty`, dropping the nulls.
fn coerce_elements(items: Vec<serde_json::Value>, ty: &FlussoType) -> GenericValue {
    GenericValue::Array(
        items
            .into_iter()
            .filter(|item| !item.is_null())
            .map(|item| coerce_scalar(item, ty))
            .collect(),
    )
}

/// An array `default` (as the config parsed it) back as JSON elements, so it
/// takes the same coercion as a value read from the column.
fn default_to_json(default: &GenericValue) -> Vec<serde_json::Value> {
    match default {
        GenericValue::Array(items) => items.iter().map(generic_to_json).collect(),
        other => vec![generic_to_json(other)],
    }
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
        FlussoType::Decimal => coerce_decimal(json),
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

/// A `numeric`-declared value, kept exact: parse from the JSON number's text (so
/// precision isn't lost through `f64`) or a string, else fall back untyped.
fn coerce_decimal(json: serde_json::Value) -> GenericValue {
    let parsed = match &json {
        serde_json::Value::Number(n) => Decimal::from_str(&n.to_string()).ok(),
        serde_json::Value::String(s) => Decimal::from_str(s).ok(),
        _ => None,
    };
    parsed.map_or_else(|| json_to_generic(json), GenericValue::Decimal)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod differential;
