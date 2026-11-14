//! [`BigDecimal`]: an exact decimal number past [`Decimal`]'s range, held as
//! its text.
//!
//! A [`Decimal`] holds a 96-bit mantissa and at most 28 fractional digits
//! (about ±7.9e28). A Postgres `numeric` holds far more. A value past
//! [`Decimal`] keeps the exact text the source wrote, and a sink writes that
//! text as a JSON number: JSON numbers have no precision limit.
//!
//! The text is always a JSON number literal (`-?digits[.digits][e±digits]`),
//! so it can be written into a JSON document as is. Two values are equal when
//! their text is: `1.0` and `1.00` are different values, as they are different
//! texts in a document.
//!
//! ```
//! use kernel::BigDecimal;
//!
//! let big = BigDecimal::parse("1000000000000000000000000000000").unwrap();
//! assert_eq!(big.as_str(), "1000000000000000000000000000000");
//! assert_eq!(big.to_f64(), 1e30);
//!
//! assert!(BigDecimal::parse("NaN").is_none());
//! assert!(BigDecimal::parse("1.").is_none());
//! ```
//!
//! [`Decimal`]: rust_decimal::Decimal

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An exact decimal number too large or too precise for
/// [`Decimal`](rust_decimal::Decimal), held as its JSON number text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BigDecimal(Box<str>);

impl BigDecimal {
    /// `text` as a [`BigDecimal`] when it is a JSON number literal, `None`
    /// otherwise (`NaN`, `Infinity`, a leading `+`, `1.`, `.5`, …).
    pub fn parse(text: &str) -> Option<Self> {
        is_json_number(text).then(|| Self(text.into()))
    }

    /// The exact text, a JSON number literal.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The nearest `f64` (an infinity past `f64`'s range).
    pub fn to_f64(&self) -> f64 {
        self.0.parse().unwrap_or(f64::NAN)
    }
}

impl fmt::Display for BigDecimal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Serialize for BigDecimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for BigDecimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        if is_json_number(&text) {
            Ok(Self(text.into_boxed_str()))
        } else {
            Err(serde::de::Error::custom(format!(
                "`{text}` is not a decimal number"
            )))
        }
    }
}

fn is_json_number(text: &str) -> bool {
    let bytes = text.as_bytes();
    let bytes = bytes.strip_prefix(b"-").unwrap_or(bytes);
    let bytes = match bytes {
        [b'0', rest @ ..] => rest,
        [b'1'..=b'9', ..] => skip_digits(bytes),
        _ => return false,
    };
    let bytes = match bytes.strip_prefix(b".") {
        Some(fraction) => match at_least_one_digit(fraction) {
            Some(rest) => rest,
            None => return false,
        },
        None => bytes,
    };
    let bytes = match bytes {
        [b'e' | b'E', exponent @ ..] => {
            let exponent = match exponent {
                [b'+' | b'-', rest @ ..] => rest,
                rest => rest,
            };
            match at_least_one_digit(exponent) {
                Some(rest) => rest,
                None => return false,
            }
        }
        rest => rest,
    };
    bytes.is_empty()
}

fn skip_digits(bytes: &[u8]) -> &[u8] {
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    bytes.get(digits..).unwrap_or_default()
}

fn at_least_one_digit(bytes: &[u8]) -> Option<&[u8]> {
    let rest = skip_digits(bytes);
    (rest.len() < bytes.len()).then_some(rest)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn accepts_every_json_number_and_nothing_else() {
        for text in [
            "0",
            "-0",
            "42",
            "-1000000000000000000000000000000.5",
            "0.123456789012345678901234567890",
            "1e30",
            "1E+30",
            "1.234e-30",
        ] {
            assert_eq!(BigDecimal::parse(text).unwrap().as_str(), text);
        }
        for text in [
            "",
            "-",
            "+1",
            "01",
            "1.",
            ".5",
            "1e",
            "1e+",
            "NaN",
            "Infinity",
            "-Infinity",
            "1 ",
            " 1",
            "0x10",
            "1_000",
        ] {
            assert!(BigDecimal::parse(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn round_trips_through_serde_as_its_text() {
        let big = BigDecimal::parse("1000000000000000000000000000000").unwrap();
        let json = serde_json::to_string(&big).unwrap();
        assert_eq!(json, "\"1000000000000000000000000000000\"");
        assert_eq!(serde_json::from_str::<BigDecimal>(&json).unwrap(), big);
        assert!(serde_json::from_str::<BigDecimal>("\"NaN\"").is_err());
    }
}
