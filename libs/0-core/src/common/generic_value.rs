use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The canonical value vocabulary every layer trades in — the **middle type**
/// between a source and a sink.
///
/// A source maps its native types *into* these variants; a sink maps them *out*
/// to its own representation. The set is deliberately fine-grained — numerics are
/// split by width, temporals are split into date/time/timestamp/timestamptz — so
/// no semantic information is lost in transit: a `date` arrives at a sink as a
/// [`Date`](Self::Date), not an opaque string a future sink would have to guess
/// at. Text-family Postgres types (`text`/`varchar`/`citext`/enum) share the one
/// [`String`](Self::String) shape because they don't differ *as values*; their
/// indexing differs, and that lives in the field's `FlussoType`.
///
/// Serde is **derived and format-agnostic** on purpose. The derive is externally
/// tagged (`{"Date":"2024-01-01"}`, `{"BigInt":5}`), so serialize → deserialize
/// is a lossless identity — a `Date` round-trips back a `Date`, never collapsing
/// to a string. That lets a queue persist or transport a value in whatever format
/// it likes and hand it back unchanged: it goes in as a `GenericValue` and comes
/// out as the same `GenericValue`. Core picks no format (no JSON here); the
/// concrete encoding is the consumer's choice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenericValue {
    Null,
    Bool(bool),
    /// `smallint` / `int2`.
    SmallInt(i16),
    /// `integer` / `int4`.
    Int(i32),
    /// `bigint` / `int8`.
    BigInt(i64),
    /// `real` / `float4`.
    Float(f32),
    /// `double precision` / `float8`.
    Double(f64),
    /// `numeric` / `decimal` — exact.
    Decimal(Decimal),
    /// Any text-family value (`text`/`varchar`/`citext`/enum).
    String(String),
    /// `uuid`.
    Uuid(Uuid),
    /// `date` — no time, no zone.
    Date(NaiveDate),
    /// `time` — no date, no zone.
    Time(NaiveTime),
    /// `timestamp` — date + time, no zone.
    Timestamp(NaiveDateTime),
    /// `timestamptz` — an instant, normalized to UTC.
    TimestampTz(DateTime<Utc>),
    /// `bytea`.
    Bytes(Vec<u8>),
    Array(Vec<GenericValue>),
    Map(BTreeMap<String, GenericValue>),
}

impl GenericValue {
    /// Whether this value can stand as a single SQL parameter, key, or literal:
    /// true for every scalar variant, false for `Null` and the composite
    /// `Array`/`Map`. The one home for that rule — the Postgres source applies
    /// it when binding params, building keys, and inlining literals. Written as
    /// an exhaustive match so a new variant cannot be added without classifying
    /// it here.
    pub fn is_bindable_scalar(&self) -> bool {
        match self {
            GenericValue::Bool(_)
            | GenericValue::SmallInt(_)
            | GenericValue::Int(_)
            | GenericValue::BigInt(_)
            | GenericValue::Float(_)
            | GenericValue::Double(_)
            | GenericValue::Decimal(_)
            | GenericValue::String(_)
            | GenericValue::Uuid(_)
            | GenericValue::Date(_)
            | GenericValue::Time(_)
            | GenericValue::Timestamp(_)
            | GenericValue::TimestampTz(_)
            | GenericValue::Bytes(_) => true,
            GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => false,
        }
    }
}

// `f32`/`f64` are not `Eq`/`Hash`, so the derives won't do; compare and hash the
// float variants by their bit pattern (a total equivalence — the rare float key
// is pathological anyway, and this keeps `Eq`/`Hash` consistent). Every other
// variant defers to its payload's own impls.
impl PartialEq for GenericValue {
    fn eq(&self, other: &Self) -> bool {
        use GenericValue::*;
        match (self, other) {
            (Null, Null) => true,
            (Bool(a), Bool(b)) => a == b,
            (SmallInt(a), SmallInt(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (BigInt(a), BigInt(b)) => a == b,
            (Float(a), Float(b)) => a.to_bits() == b.to_bits(),
            (Double(a), Double(b)) => a.to_bits() == b.to_bits(),
            (Decimal(a), Decimal(b)) => a == b,
            (String(a), String(b)) => a == b,
            (Uuid(a), Uuid(b)) => a == b,
            (Date(a), Date(b)) => a == b,
            (Time(a), Time(b)) => a == b,
            (Timestamp(a), Timestamp(b)) => a == b,
            (TimestampTz(a), TimestampTz(b)) => a == b,
            (Bytes(a), Bytes(b)) => a == b,
            (Array(a), Array(b)) => a == b,
            (Map(a), Map(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for GenericValue {}

impl Hash for GenericValue {
    fn hash<H: Hasher>(&self, state: &mut H) {
        use GenericValue::*;
        std::mem::discriminant(self).hash(state);
        match self {
            Null => {}
            Bool(v) => v.hash(state),
            SmallInt(v) => v.hash(state),
            Int(v) => v.hash(state),
            BigInt(v) => v.hash(state),
            Float(v) => v.to_bits().hash(state),
            Double(v) => v.to_bits().hash(state),
            Decimal(v) => v.hash(state),
            String(v) => v.hash(state),
            Uuid(v) => v.hash(state),
            Date(v) => v.hash(state),
            Time(v) => v.hash(state),
            Timestamp(v) => v.hash(state),
            TimestampTz(v) => v.hash(state),
            Bytes(v) => v.hash(state),
            Array(v) => v.hash(state),
            Map(v) => v.hash(state),
        }
    }
}
