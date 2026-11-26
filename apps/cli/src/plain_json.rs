//! JSON for a person to read: every flusso value as the plain JSON it holds.
//!
//! `kernel::GenericValue`'s own serde is externally tagged
//! (`{"String": "x"}`, `{"BigInt": 2}`) so a queue can round-trip it
//! losslessly, and the lock and other formats depend on that. A command whose
//! JSON people read (`flusso show … --format json`) renders through
//! [`to_value`] instead: the same document, with every `GenericValue`
//! written as `sink::to_json` writes it into a document (a string, a number,
//! a boolean, a list, an object, a date as its ISO text).
//!
//! It is a serializer over any `Serialize` type that hands every value to
//! `serde_json` except one serde names `GenericValue`, which it reads back
//! and renders plainly, so a type gaining a `GenericValue` field needs nothing
//! here.
//!
//! ```ignore
//! let value = plain_json::to_value(&config)?;
//! // `"format": "yyyy-MM-dd"`, never `"format": {"String": "yyyy-MM-dd"}`.
//! ```

use kernel::GenericValue;
use serde::ser::{self, Serialize};
use serde_json::{Map, Value};

/// `value` as JSON, every `GenericValue` in it as the plain JSON it holds.
pub(crate) fn to_value<T: Serialize + ?Sized>(value: &T) -> Result<Value, serde_json::Error> {
    value.serialize(Plain)
}

const GENERIC_VALUE: &str = "GenericValue";

/// The value serde gave as `GenericValue`'s `variant` holding `inner`, read
/// back and rendered as a document holds it.
fn generic<T: Serialize + ?Sized>(variant: &str, inner: &T) -> Result<Value, serde_json::Error> {
    let mut tagged = Map::new();
    tagged.insert(variant.to_owned(), serde_json::to_value(inner)?);
    let value: GenericValue = serde_json::from_value(Value::Object(tagged))?;
    Ok(sink::to_json(&value))
}

struct Plain;

impl ser::Serializer for Plain {
    type Ok = Value;
    type Error = serde_json::Error;
    type SerializeSeq = Seq;
    type SerializeTuple = Seq;
    type SerializeTupleStruct = Seq;
    type SerializeTupleVariant = Variant<Seq>;
    type SerializeMap = Object;
    type SerializeStruct = Object;
    type SerializeStructVariant = Variant<Object>;

    fn serialize_bool(self, v: bool) -> Result<Value, Self::Error> {
        Ok(Value::Bool(v))
    }
    fn serialize_i8(self, v: i8) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_i128(self, v: i128) -> Result<Value, Self::Error> {
        serde_json::to_value(v)
    }
    fn serialize_u8(self, v: u8) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<Value, Self::Error> {
        Ok(v.into())
    }
    fn serialize_u128(self, v: u128) -> Result<Value, Self::Error> {
        serde_json::to_value(v)
    }
    fn serialize_f32(self, v: f32) -> Result<Value, Self::Error> {
        serde_json::to_value(v)
    }
    fn serialize_f64(self, v: f64) -> Result<Value, Self::Error> {
        serde_json::to_value(v)
    }
    fn serialize_char(self, v: char) -> Result<Value, Self::Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_str(self, v: &str) -> Result<Value, Self::Error> {
        Ok(Value::String(v.into()))
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<Value, Self::Error> {
        serde_json::to_value(v)
    }
    fn serialize_none(self) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value, Self::Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<Value, Self::Error> {
        Ok(Value::Null)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
    ) -> Result<Value, Self::Error> {
        if name == GENERIC_VALUE {
            return generic(variant, &());
        }
        Ok(Value::String(variant.into()))
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        value: &T,
    ) -> Result<Value, Self::Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Value, Self::Error> {
        if name == GENERIC_VALUE {
            return generic(variant, value);
        }
        let mut object = Map::new();
        object.insert(variant.into(), value.serialize(Plain)?);
        Ok(Value::Object(object))
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Seq, Self::Error> {
        Ok(Seq(Vec::with_capacity(len.unwrap_or_default())))
    }
    fn serialize_tuple(self, len: usize) -> Result<Seq, Self::Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_struct(self, _: &'static str, len: usize) -> Result<Seq, Self::Error> {
        self.serialize_seq(Some(len))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Variant<Seq>, Self::Error> {
        Ok(Variant(variant, Seq(Vec::with_capacity(len))))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Object, Self::Error> {
        Ok(Object::default())
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Object, Self::Error> {
        Ok(Object::default())
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Variant<Object>, Self::Error> {
        Ok(Variant(variant, Object::default()))
    }
}

struct Seq(Vec<Value>);

impl ser::SerializeSeq for Seq {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.0.push(value.serialize(Plain)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Value::Array(self.0))
    }
}

impl ser::SerializeTuple for Seq {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for Seq {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        ser::SerializeSeq::serialize_element(self, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        ser::SerializeSeq::end(self)
    }
}

#[derive(Default)]
struct Object {
    map: Map<String, Value>,
    key: Option<String>,
}

impl ser::SerializeMap for Object {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        self.key = Some(match key.serialize(Plain)? {
            Value::String(key) => key,
            other => other.to_string(),
        });
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        let key = self.key.take().unwrap_or_default();
        self.map.insert(key, value.serialize(Plain)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Value::Object(self.map))
    }
}

impl ser::SerializeStruct for Object {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.map.insert(key.into(), value.serialize(Plain)?);
        Ok(())
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Value::Object(self.map))
    }
}

/// A tuple or struct variant: `{"variant": <its fields>}`, as serde_json writes it.
struct Variant<T>(&'static str, T);

impl<T> Variant<T> {
    fn wrap(variant: &str, inner: Value) -> Value {
        let mut object = Map::new();
        object.insert(variant.into(), inner);
        Value::Object(object)
    }
}

impl ser::SerializeTupleVariant for Variant<Seq> {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        ser::SerializeSeq::serialize_element(&mut self.1, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Self::wrap(self.0, Value::Array(self.1.0)))
    }
}

impl ser::SerializeStructVariant for Variant<Object> {
    type Ok = Value;
    type Error = serde_json::Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        ser::SerializeStruct::serialize_field(&mut self.1, key, value)
    }
    fn end(self) -> Result<Value, Self::Error> {
        Ok(Self::wrap(self.0, Value::Object(self.1.map)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::collections::BTreeMap;

    use kernel::{GenericValue, OptionValue, Options};
    use serde::Serialize;
    use serde_json::json;

    use super::to_value;

    #[derive(Serialize)]
    struct Holder {
        value: GenericValue,
        list: Vec<GenericValue>,
        maybe: Option<GenericValue>,
        keyed: BTreeMap<String, GenericValue>,
        #[serde(flatten)]
        flat: BTreeMap<String, GenericValue>,
        tagged: Tagged,
        options: Options,
    }

    #[derive(Serialize)]
    #[serde(rename_all = "snake_case")]
    enum Tagged {
        Constant(GenericValue),
    }

    fn holding(value: GenericValue) -> serde_json::Value {
        to_value(&Holder {
            value: value.clone(),
            list: vec![value.clone()],
            maybe: Some(value.clone()),
            keyed: BTreeMap::from([("k".to_owned(), value.clone())]),
            flat: BTreeMap::from([("flat".to_owned(), value.clone())]),
            tagged: Tagged::Constant(value),
            options: Options::default(),
        })
        .unwrap()
    }

    /// Every `GenericValue` variant, wherever serde meets it (a field, a list,
    /// an `Option`, a map, a flattened map, inside another enum), is the JSON
    /// a document holds.
    #[test]
    fn every_generic_value_is_its_plain_json_wherever_it_sits() {
        let tagged =
            |tagged: serde_json::Value| -> GenericValue { serde_json::from_value(tagged).unwrap() };
        for (value, plain) in [
            (GenericValue::Null, json!(null)),
            (GenericValue::Bool(true), json!(true)),
            (GenericValue::SmallInt(-3), json!(-3)),
            (GenericValue::Int(7), json!(7)),
            (
                GenericValue::BigInt(9_007_199_254_740_993),
                json!(9_007_199_254_740_993_i64),
            ),
            (GenericValue::Float(1.5), json!(1.5)),
            (GenericValue::Double(2.25), json!(2.25)),
            (tagged(json!({ "Decimal": "12.5" })), json!(12.5)),
            (
                GenericValue::String("yyyy-MM-dd".into()),
                json!("yyyy-MM-dd"),
            ),
            (
                tagged(json!({ "Uuid": "67e55044-10b1-426f-9247-bb680e5fe0c8" })),
                json!("67e55044-10b1-426f-9247-bb680e5fe0c8"),
            ),
            (tagged(json!({ "Date": "2024-01-02" })), json!("2024-01-02")),
            (
                tagged(json!({ "Time": "03:04:05.123" })),
                json!("03:04:05.123"),
            ),
            (
                tagged(json!({ "Timestamp": "2024-01-02T03:04:05.123" })),
                json!("2024-01-02T03:04:05.123"),
            ),
            (
                tagged(json!({ "TimestampTz": "2024-01-02T03:04:05.123Z" })),
                json!("2024-01-02T03:04:05.123+00:00"),
            ),
            (GenericValue::Bytes(vec![1, 2, 3]), json!("AQID")),
            (
                GenericValue::Array(vec![GenericValue::String("a".into()), GenericValue::Int(1)]),
                json!(["a", 1]),
            ),
            (
                GenericValue::Map(BTreeMap::from([(
                    "inner".to_owned(),
                    GenericValue::Map(BTreeMap::from([(
                        "on".to_owned(),
                        GenericValue::Bool(false),
                    )])),
                )])),
                json!({ "inner": { "on": false } }),
            ),
            (tagged(json!({ "BigDecimal": "0.5" })), json!(0.5)),
        ] {
            assert_eq!(
                holding(value.clone()),
                json!({
                    "value": plain,
                    "list": [plain],
                    "maybe": plain,
                    "keyed": { "k": plain },
                    "flat": plain,
                    "tagged": { "constant": plain },
                    "options": {},
                }),
                "{value:?}"
            );
        }
    }

    /// Every other type serializes as `serde_json` writes it, an adapter's
    /// options among them: a plain value for each `OptionValue` variant.
    #[test]
    fn every_other_value_is_written_as_serde_json_writes_it() {
        let options = Options::from_iter([
            ("null".to_owned(), OptionValue::Null),
            ("on".to_owned(), OptionValue::Bool(true)),
            ("count".to_owned(), OptionValue::Integer(3)),
            ("ratio".to_owned(), OptionValue::Float(0.5)),
            ("name".to_owned(), OptionValue::String("x".into())),
            (
                "list".to_owned(),
                OptionValue::Array(vec![
                    OptionValue::Integer(1),
                    OptionValue::String("y".into()),
                ]),
            ),
            (
                "table".to_owned(),
                OptionValue::Map(BTreeMap::from([("k".to_owned(), OptionValue::Bool(false))])),
            ),
        ]);
        assert_eq!(
            to_value(&options).unwrap(),
            serde_json::to_value(&options).unwrap()
        );
        assert_eq!(
            to_value(&options).unwrap(),
            json!({
                "null": null, "on": true, "count": 3, "ratio": 0.5, "name": "x",
                "list": [1, "y"], "table": { "k": false },
            })
        );
        #[derive(Serialize)]
        enum Shape {
            Unit,
            Tuple(u8, char),
            Struct { a: (i8, f32) },
        }
        let shapes = vec![
            Shape::Unit,
            Shape::Tuple(1, 'c'),
            Shape::Struct { a: (-1, 0.25) },
        ];
        assert_eq!(
            to_value(&shapes).unwrap(),
            serde_json::to_value(&shapes).unwrap()
        );
    }
}
