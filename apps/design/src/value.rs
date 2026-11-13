//! A value typed into the inspector (a `constant`, a column `default`, an
//! option) turned into the [`GenericValue`] its saved file reads back as.
//!
//! The inspector takes JSON. Each number keeps the text it was typed as and is
//! read the way the schema parser reads a YAML number:
//!
//! - integer form (`5`, `-12`, `+7`, `007`): an `i64` is a `BigInt`; anything
//!   larger is a `Decimal`;
//! - fraction or exponent form (`1.5`, `5.0`, `1e3`): a `Decimal`, as the parser
//!   reads a float.
//!
//! A number the file can't hold exactly is refused, never rounded:
//! [`ValueProblem::Inexact`] when it has more digits than the file keeps
//! (`0.1000000000000000000001`), [`ValueProblem::OutOfRange`] when it's too large
//! or too small to be a number in the file (`1.5e300`). Negative zero is zero.
//! Nesting deeper than [`MAX_DEPTH`] is [`ValueProblem::TooDeep`].
//!
//! Each [`ValueSlot`] has the parser's own rules:
//!
//! - a column `default` must be a scalar ([`ValueProblem::NotScalar`]);
//! - an array column's `default` must be a list ([`ValueProblem::NotAList`])
//!   with no null in it ([`ValueProblem::NullElement`]);
//! - a `constant` and an option take any value.
//!
//! The result is the value after one real round trip in its slot
//! (codegen, then the parser), so the preview and the content hash are computed
//! from what the file reads. [`read`] also says how the file spells it, when
//! that differs from what was typed (`1e3` is saved as `1000.0`, ` 5 ` as `5`).
//!
//! ```
//! use design::value::{ValueProblem, ValueSlot, read_typed};
//! use kernel::GenericValue;
//!
//! let constant = |text| read_typed(text, ValueSlot::Constant);
//! assert!(matches!(constant("9007199254740993"), Ok(GenericValue::BigInt(9_007_199_254_740_993))));
//! assert!(matches!(constant("1e3"), Ok(GenericValue::Decimal(d)) if d == 1000.into()));
//! assert!(matches!(constant("1.5e300"), Err(ValueProblem::OutOfRange { .. })));
//! assert_eq!(read_typed("[1]", ValueSlot::Default), Err(ValueProblem::NotScalar));
//! ```

use std::collections::BTreeMap;
use std::str::FromStr;

use config::yaml::SchemaYaml;
use kernel::{FieldSource, GenericValue, IndexSchema, ParseFrom};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::codegen;

/// How deep a typed value may nest. The JSON the editor posts to preview, diff
/// and save spells each list or object as two levels (`{"Array": [...]}`) and
/// sits inside the schema's own nesting, all under serde_json's limit of 128:
/// this cap leaves room for a value inside eleven nested joins (pinned at ten
/// by `the_deepest_accepted_value_previews_diffs_and_saves_ten_joins_deep`).
pub const MAX_DEPTH: usize = 32;

/// Where a typed value goes in the schema file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueSlot {
    /// A `constant` field's value: any value.
    #[default]
    Constant,
    /// A column's `default`: a scalar or `null`.
    Default,
    /// An array column's `default`: a list of non-null values.
    ArrayDefault,
    /// A field's `options` entry: any value.
    Option,
}

/// A typed value as its saved file reads it.
#[derive(Debug, Clone, PartialEq)]
pub struct Typed {
    /// The value the file reads back as.
    pub value: GenericValue,
    /// The value as the file spells it, when a number in it is written other
    /// than it was typed (`1e3` → `1000.0`); `None` when every number is
    /// written as typed.
    pub saved_as: Option<String>,
}

/// Why typed text can't become a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ValueProblem {
    /// Not JSON; `position` is the 1-based character it stopped at.
    Syntax { position: usize },
    /// A number with more digits than the saved file keeps.
    Inexact { number: String },
    /// A number too large or too small to be a number in the saved file.
    OutOfRange { number: String },
    /// Nested deeper than `limit` lists or objects.
    TooDeep { limit: usize },
    /// A list or object where the file takes only a scalar (a `default`).
    NotScalar,
    /// Anything but a list where the file takes a list (an array column's
    /// `default`).
    NotAList,
    /// A `null` inside an array column's `default`.
    NullElement,
    /// The value would read back as something else (a codegen defect).
    ReadsBackDifferently,
}

/// Typed JSON → the value its saved file reads back as in `slot`, or why it
/// can't be one.
pub fn read_typed(text: &str, slot: ValueSlot) -> Result<GenericValue, ValueProblem> {
    read(text, slot).map(|typed| typed.value)
}

/// Typed JSON → the value its saved file reads back as in `slot`, and how the
/// file spells it when that differs from what was typed.
pub fn read(text: &str, slot: ValueSlot) -> Result<Typed, ValueProblem> {
    let mut reader = Reader {
        chars: text.chars().collect(),
        at: 0,
        depth: 0,
        numbers: Vec::new(),
    };
    let value = reader.value()?;
    reader.blank();
    if reader.at < reader.chars.len() {
        return Err(reader.syntax());
    }
    let value = read_back(&value, slot)?;
    let respelled = text.trim() != text
        || reader.numbers.iter().any(|(typed, number)| {
            codegen::scalar_text(number).map_or(true, |saved| saved != *typed)
        });
    let saved_as = if respelled {
        Some(spelled(&value).ok_or(ValueProblem::ReadsBackDifferently)?)
    } else {
        None
    };
    Ok(Typed { value, saved_as })
}

/// `value` as typed JSON, each scalar spelled as the saved file writes it.
fn spelled(value: &GenericValue) -> Option<String> {
    Some(match value {
        GenericValue::String(s) => serde_json::to_string(s).ok()?,
        GenericValue::Array(items) => {
            let items: Option<Vec<String>> = items.iter().map(spelled).collect();
            format!("[{}]", items?.join(", "))
        }
        GenericValue::Map(map) => {
            let entries: Option<Vec<String>> = map
                .iter()
                .map(|(k, v)| {
                    Some(format!(
                        "{}: {}",
                        serde_json::to_string(k).ok()?,
                        spelled(v)?
                    ))
                })
                .collect();
            format!("{{{}}}", entries?.join(", "))
        }
        scalar => codegen::scalar_text(scalar).ok()?,
    })
}

/// `value` written in `slot` and parsed back; refused if the parser rejects it
/// there or it changes.
fn read_back(value: &GenericValue, slot: ValueSlot) -> Result<GenericValue, ValueProblem> {
    if slot == ValueSlot::Default && matches!(value, GenericValue::Array(_) | GenericValue::Map(_))
    {
        return Err(ValueProblem::NotScalar);
    }
    if slot == ValueSlot::ArrayDefault {
        let GenericValue::Array(items) = value else {
            return Err(ValueProblem::NotAList);
        };
        if items.iter().any(|item| matches!(item, GenericValue::Null)) {
            return Err(ValueProblem::NullElement);
        }
    }
    let mut schema = carrier(slot).ok_or(ValueProblem::ReadsBackDifferently)?;
    let field = schema
        .fields
        .first_mut()
        .ok_or(ValueProblem::ReadsBackDifferently)?;
    put(field, slot, value.clone());
    let yaml = codegen::schema_to_yaml(&schema).map_err(|_| ValueProblem::ReadsBackDifferently)?;
    let back = SchemaYaml::try_parse(&yaml)
        .ok()
        .and_then(|entity| IndexSchema::try_from(entity).ok())
        .and_then(|reparsed| reparsed.fields.into_iter().next())
        .and_then(|field| get(field, slot));
    match back {
        Some(back) if same(&back, value) => Ok(back),
        _ => Err(ValueProblem::ReadsBackDifferently),
    }
}

/// The key a typed option is written under to read it back.
const OPTION_KEY: &str = "option";

/// A schema of one field that holds a value in `slot`.
fn carrier(slot: ValueSlot) -> Option<IndexSchema> {
    let field = match slot {
        ValueSlot::Constant => "- constant: c\n  value: null\n",
        ValueSlot::Default | ValueSlot::Option => "- boolean: c\n  required: false\n",
        ValueSlot::ArrayDefault => "- json: c\n  required: false\n  array: true\n",
    };
    let yaml = format!("version: 1\ntable: t\nprimary_key: id\nfields:\n{field}");
    IndexSchema::try_from(SchemaYaml::try_parse(&yaml).ok()?).ok()
}

fn put(field: &mut kernel::Field, slot: ValueSlot, value: GenericValue) {
    match (slot, &mut field.source) {
        (ValueSlot::Constant, source) => *source = FieldSource::Constant(value),
        (ValueSlot::Default | ValueSlot::ArrayDefault, FieldSource::Column(column)) => {
            column.default = Some(value);
        }
        (ValueSlot::Default | ValueSlot::ArrayDefault, _) => {}
        (ValueSlot::Option, _) => {
            field.options.insert(OPTION_KEY.to_owned(), value);
        }
    }
}

fn get(mut field: kernel::Field, slot: ValueSlot) -> Option<GenericValue> {
    match (slot, field.source) {
        (ValueSlot::Constant, FieldSource::Constant(value)) => Some(value),
        (ValueSlot::Default | ValueSlot::ArrayDefault, FieldSource::Column(column)) => {
            Some(column.default.unwrap_or(GenericValue::Null))
        }
        (ValueSlot::Option, _) => field.options.remove(OPTION_KEY),
        _ => None,
    }
}

fn same(a: &GenericValue, b: &GenericValue) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

struct Reader {
    chars: Vec<char>,
    at: usize,
    depth: usize,
    /// Each number as typed, with the value it was read as.
    numbers: Vec<(String, GenericValue)>,
}

impl Reader {
    fn syntax(&self) -> ValueProblem {
        ValueProblem::Syntax {
            position: self.at + 1,
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn blank(&mut self) {
        while self
            .peek()
            .is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r'))
        {
            self.at += 1;
        }
    }

    fn expect(&mut self, c: char) -> Result<(), ValueProblem> {
        self.blank();
        if self.peek() == Some(c) {
            self.at += 1;
            Ok(())
        } else {
            Err(self.syntax())
        }
    }

    fn word(&mut self, word: &str, value: GenericValue) -> Result<GenericValue, ValueProblem> {
        let end = self.at + word.chars().count();
        if self
            .chars
            .get(self.at..end)
            .is_some_and(|s| s.iter().copied().eq(word.chars()))
        {
            self.at = end;
            Ok(value)
        } else {
            Err(self.syntax())
        }
    }

    fn value(&mut self) -> Result<GenericValue, ValueProblem> {
        self.blank();
        match self.peek() {
            Some('"') => self.string().map(GenericValue::String),
            Some('[') => self.nested(Self::array),
            Some('{') => self.nested(Self::map),
            Some('t') => self.word("true", GenericValue::Bool(true)),
            Some('f') => self.word("false", GenericValue::Bool(false)),
            Some('n') => self.word("null", GenericValue::Null),
            Some(c) if c.is_ascii_digit() || matches!(c, '-' | '+' | '.') => self.number(),
            _ => Err(self.syntax()),
        }
    }

    fn nested(
        &mut self,
        read: fn(&mut Self) -> Result<GenericValue, ValueProblem>,
    ) -> Result<GenericValue, ValueProblem> {
        if self.depth == MAX_DEPTH {
            return Err(ValueProblem::TooDeep { limit: MAX_DEPTH });
        }
        self.depth += 1;
        let value = read(self);
        self.depth -= 1;
        value
    }

    fn string(&mut self) -> Result<String, ValueProblem> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.peek() {
                None => return Err(self.syntax()),
                Some('\\') => self.at += 2,
                Some('"') => break,
                Some(_) => self.at += 1,
            }
        }
        self.at += 1;
        let raw: String = self
            .chars
            .get(start..self.at)
            .unwrap_or_default()
            .iter()
            .collect();
        serde_json::from_str(&raw).map_err(|_| ValueProblem::Syntax {
            position: start + 1,
        })
    }

    fn array(&mut self) -> Result<GenericValue, ValueProblem> {
        self.at += 1;
        let mut items = Vec::new();
        self.blank();
        if self.peek() == Some(']') {
            self.at += 1;
            return Ok(GenericValue::Array(items));
        }
        loop {
            items.push(self.value()?);
            self.blank();
            match self.peek() {
                Some(',') => self.at += 1,
                Some(']') => {
                    self.at += 1;
                    return Ok(GenericValue::Array(items));
                }
                _ => return Err(self.syntax()),
            }
        }
    }

    fn map(&mut self) -> Result<GenericValue, ValueProblem> {
        self.at += 1;
        let mut entries = BTreeMap::new();
        self.blank();
        if self.peek() == Some('}') {
            self.at += 1;
            return Ok(GenericValue::Map(entries));
        }
        loop {
            self.blank();
            if self.peek() != Some('"') {
                return Err(self.syntax());
            }
            let key = self.string()?;
            self.expect(':')?;
            entries.insert(key, self.value()?);
            self.blank();
            match self.peek() {
                Some(',') => self.at += 1,
                Some('}') => {
                    self.at += 1;
                    return Ok(GenericValue::Map(entries));
                }
                _ => return Err(self.syntax()),
            }
        }
    }

    fn number(&mut self) -> Result<GenericValue, ValueProblem> {
        let start = self.at;
        let mut text = String::new();
        if let Some(sign @ ('-' | '+')) = self.peek() {
            text.push(sign);
            self.at += 1;
        }
        let digits = |r: &mut Self, text: &mut String| {
            let mut n = 0;
            while let Some(c) = r.peek().filter(char::is_ascii_digit) {
                text.push(c);
                r.at += 1;
                n += 1;
            }
            n
        };
        let mut mantissa = digits(self, &mut text);
        let mut integer = true;
        if self.peek() == Some('.') {
            integer = false;
            text.push('.');
            self.at += 1;
            mantissa += digits(self, &mut text);
        }
        if mantissa == 0 {
            return Err(ValueProblem::Syntax {
                position: start + 1,
            });
        }
        if let Some(e @ ('e' | 'E')) = self.peek() {
            integer = false;
            text.push(e);
            self.at += 1;
            if let Some(sign @ ('-' | '+')) = self.peek() {
                text.push(sign);
                self.at += 1;
            }
            if digits(self, &mut text) == 0 {
                return Err(self.syntax());
            }
        }
        let value = number_value(&text, integer)?;
        self.numbers.push((text, value.clone()));
        Ok(value)
    }
}

/// A number token as the parser reads its form (see the module docs), refused
/// unless that value is exactly the one typed.
fn number_value(text: &str, integer: bool) -> Result<GenericValue, ValueProblem> {
    let typed = Exact::of(text).ok_or_else(|| ValueProblem::OutOfRange {
        number: text.to_owned(),
    })?;
    let read = if integer {
        let plain = typed.integer().unwrap_or_default();
        if let Ok(n) = plain.parse::<i64>() {
            return Ok(GenericValue::BigInt(n));
        }
        match plain.parse::<u64>() {
            Ok(n) => Some(Decimal::from(n)),
            Err(_) => float_decimal(text),
        }
    } else {
        float_decimal(text)
    };
    let Some(read) = read else {
        return Err(ValueProblem::OutOfRange {
            number: text.to_owned(),
        });
    };
    let number = text.to_owned();
    if Exact::of(&read.to_string()) == Some(typed) {
        Ok(GenericValue::Decimal(read))
    } else if read.is_zero() {
        Err(ValueProblem::OutOfRange { number })
    } else {
        Err(ValueProblem::Inexact { number })
    }
}

/// The decimal the parser reads a YAML float as: the shortest text of its
/// `f64`, when that is a decimal at all.
fn float_decimal(text: &str) -> Option<Decimal> {
    let float = text.trim_start_matches('+').parse::<f64>().ok()?;
    if !float.is_finite() {
        return None;
    }
    Decimal::from_str(&serde_yaml::Number::from(float).to_string()).ok()
}

/// A decimal number's exact value: `digits × 10^exponent`, without leading or
/// trailing zeros, so two spellings of one value compare equal. Zero has no
/// sign.
#[derive(Debug, PartialEq, Eq)]
struct Exact {
    negative: bool,
    digits: String,
    exponent: i64,
}

impl Exact {
    fn of(text: &str) -> Option<Self> {
        let (negative, rest) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text.strip_prefix('+').unwrap_or(text)),
        };
        let (mantissa, exponent) = match rest.split_once(['e', 'E']) {
            Some((m, e)) => (m, e.trim_start_matches('+').parse::<i64>().ok()?),
            None => (rest, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        let all: String = whole.chars().chain(fraction.chars()).collect();
        let mut exponent = exponent.checked_sub(i64::try_from(fraction.len()).ok()?)?;
        let leading = all.trim_start_matches('0');
        let digits = leading.trim_end_matches('0');
        exponent = exponent.checked_add(i64::try_from(leading.len() - digits.len()).ok()?)?;
        if digits.is_empty() {
            return Some(Self {
                negative: false,
                digits: String::new(),
                exponent: 0,
            });
        }
        Some(Self {
            negative,
            digits: digits.to_owned(),
            exponent,
        })
    }

    /// The value as integer text, when it is an integer short enough to spell.
    fn integer(&self) -> Option<String> {
        if self.digits.is_empty() {
            return Some(String::from("0"));
        }
        let zeros = usize::try_from(self.exponent).ok().filter(|&n| n <= 40)?;
        let sign = if self.negative { "-" } else { "" };
        Some(format!("{sign}{}{}", self.digits, "0".repeat(zeros)))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn read_typed(text: &str) -> Result<GenericValue, ValueProblem> {
        super::read_typed(text, ValueSlot::Constant)
    }

    fn read_constant(text: &str) -> Result<Typed, ValueProblem> {
        super::read(text, ValueSlot::Constant)
    }

    fn decimal(text: &str) -> GenericValue {
        GenericValue::Decimal(text.parse().unwrap())
    }

    fn read(text: &str) -> serde_json::Value {
        serde_json::to_value(read_typed(text).unwrap()).unwrap()
    }

    fn json(value: &GenericValue) -> serde_json::Value {
        serde_json::to_value(value).unwrap()
    }

    #[test]
    fn an_exponent_form_is_the_decimal_the_parser_reads() {
        assert_eq!(read("1e3"), json(&decimal("1000.0")));
        assert_eq!(read("1e-7"), json(&decimal("0.0000001")));
        assert_eq!(read("1e21"), json(&decimal("1000000000000000000000")));
        assert_eq!(read("-2.5E+2"), json(&decimal("-250.0")));
    }

    #[test]
    fn an_integer_past_2_53_keeps_its_digits() {
        assert_eq!(
            read("9007199254740993"),
            json(&GenericValue::BigInt(9_007_199_254_740_993))
        );
        assert_eq!(
            read("9223372036854775808"),
            json(&decimal("9223372036854775808"))
        );
        assert_eq!(
            read("18446744073709551615"),
            json(&decimal("18446744073709551615"))
        );
    }

    #[test]
    fn an_integer_past_u64_is_a_decimal_when_it_is_exact_and_refused_otherwise() {
        assert_eq!(
            read("100000000000000000000000"),
            json(&decimal("100000000000000000000000"))
        );
        assert_eq!(
            read_typed("18446744073709551617"),
            Err(ValueProblem::Inexact {
                number: "18446744073709551617".into()
            })
        );
    }

    #[test]
    fn negative_zero_is_zero() {
        assert_eq!(read("-0"), json(&GenericValue::BigInt(0)));
        assert_eq!(read("-0.0"), json(&decimal("0.0")));
    }

    #[test]
    fn a_number_with_more_digits_than_the_file_keeps_is_refused() {
        assert_eq!(
            read_typed("0.1000000000000000000001"),
            Err(ValueProblem::Inexact {
                number: "0.1000000000000000000001".into()
            })
        );
        assert_eq!(
            read_typed("[1, 3.14159265358979323846]"),
            Err(ValueProblem::Inexact {
                number: "3.14159265358979323846".into()
            })
        );
    }

    #[test]
    fn a_number_too_large_or_small_for_the_file_is_refused() {
        for text in ["1.5e300", "1e400", "-1e400", "1e-30"] {
            assert!(
                matches!(read_typed(text), Err(ValueProblem::OutOfRange { .. })),
                "{text}: {:?}",
                read_typed(text)
            );
        }
    }

    #[test]
    fn a_leading_plus_leading_zeros_and_whitespace_keep_the_value() {
        assert_eq!(read("+5"), json(&GenericValue::BigInt(5)));
        assert_eq!(read("007"), json(&GenericValue::BigInt(7)));
        assert_eq!(read("  42 \n"), json(&GenericValue::BigInt(42)));
        assert_eq!(read("+.5"), json(&decimal("0.5")));
        assert_eq!(read("00.5"), json(&decimal("0.5")));
    }

    #[test]
    fn strings_and_structures_read_as_json() {
        assert_eq!(
            read(r#""a\"b""#),
            json(&GenericValue::String("a\"b".into()))
        );
        assert_eq!(read(r#""007""#), json(&GenericValue::String("007".into())));
        assert_eq!(
            read(r#"{"k": [true, null, 1.5]}"#),
            json(&GenericValue::Map(BTreeMap::from([(
                "k".to_owned(),
                GenericValue::Array(vec![
                    GenericValue::Bool(true),
                    GenericValue::Null,
                    decimal("1.5")
                ])
            )])))
        );
    }

    #[test]
    fn nesting_past_the_cap_is_refused_without_overflowing_the_stack() {
        let deep = "[".repeat(100_000) + "]".repeat(100_000).as_str();
        assert_eq!(
            read_typed(&deep),
            Err(ValueProblem::TooDeep { limit: MAX_DEPTH })
        );
        let objects = r#"{"a":"#.repeat(100_000);
        assert_eq!(
            read_typed(&objects),
            Err(ValueProblem::TooDeep { limit: MAX_DEPTH })
        );
    }

    #[test]
    fn nesting_up_to_the_cap_reads_back() {
        let text = "[".repeat(MAX_DEPTH) + "1" + "]".repeat(MAX_DEPTH).as_str();
        let mut expected = GenericValue::BigInt(1);
        for _ in 0..MAX_DEPTH {
            expected = GenericValue::Array(vec![expected]);
        }
        assert_eq!(read_constant(&text).unwrap().value, expected);
        let text = r#"{"a":"#.repeat(MAX_DEPTH) + "1" + "}".repeat(MAX_DEPTH).as_str();
        assert!(read_constant(&text).is_ok());
    }

    #[test]
    fn saved_as_is_the_files_spelling_when_a_number_is_respelled() {
        let saved = |text: &str| read_constant(text).unwrap().saved_as;
        assert_eq!(saved("1e3").as_deref(), Some("1000.0"));
        assert_eq!(saved("+5").as_deref(), Some("5"));
        assert_eq!(saved("[1, 1e3]").as_deref(), Some("[1, 1000.0]"));
        assert_eq!(
            saved(r#"{"b": 1E3, "a": "x"}"#).as_deref(),
            Some(r#"{"a": "x", "b": 1000.0}"#)
        );
        assert_eq!(saved(" 5 ").as_deref(), Some("5"));
        assert_eq!(saved(" [1,2] ").as_deref(), Some("[1, 2]"));
        for text in ["5", "1.5", "-12", r#""1e3""#, r#"{"a": [1, 2.5]}"#, "true"] {
            assert_eq!(saved(text), None, "{text}");
        }
    }

    #[test]
    fn saved_as_matches_the_generated_file() {
        for text in ["1e-7", "1e21", "1e3", "5.0", "+.5", "007"] {
            let typed = read_constant(text).unwrap();
            let schema = IndexSchema::try_from(
                SchemaYaml::try_parse(
                    "version: 1\ntable: t\nprimary_key: id\nfields:\n- constant: c\n  value: null\n",
                )
                .unwrap(),
            )
            .unwrap();
            let mut schema = schema;
            schema.fields[0].source = FieldSource::Constant(typed.value.clone());
            let yaml = codegen::schema_to_yaml(&schema).unwrap();
            let line = yaml
                .lines()
                .find_map(|l| l.trim().strip_prefix("value: "))
                .unwrap()
                .to_owned();
            assert_eq!(
                typed.saved_as.unwrap_or_else(|| text.to_owned()),
                line,
                "{text}"
            );
        }
    }

    #[test]
    fn text_that_is_not_json_is_a_syntax_problem() {
        for text in [
            "", "abc", "[1,", "{1: 2}", "1 2", ".", "-", "1e", "\"x", "tru",
        ] {
            assert!(
                matches!(read_typed(text), Err(ValueProblem::Syntax { .. })),
                "{text:?}: {:?}",
                read_typed(text)
            );
        }
    }

    #[test]
    fn an_array_default_takes_a_list_without_nulls() {
        let default = |text: &str| super::read_typed(text, ValueSlot::ArrayDefault);
        assert_eq!(default("5"), Err(ValueProblem::NotAList));
        assert_eq!(default(r#"{"a": 1}"#), Err(ValueProblem::NotAList));
        assert_eq!(default(r#"["a", null]"#), Err(ValueProblem::NullElement));
        assert_eq!(
            default(r#"["a", 1]"#),
            Ok(GenericValue::Array(vec![
                GenericValue::String("a".to_owned()),
                GenericValue::BigInt(1)
            ]))
        );
        assert_eq!(default("[]"), Ok(GenericValue::Array(Vec::new())));
    }

    #[test]
    fn a_default_takes_only_a_scalar() {
        let default = |text: &str| super::read_typed(text, ValueSlot::Default);
        assert_eq!(default("[1]"), Err(ValueProblem::NotScalar));
        assert_eq!(default(r#"{"a": 1}"#), Err(ValueProblem::NotScalar));
        assert_eq!(default("5"), Ok(GenericValue::BigInt(5)));
        assert_eq!(default("1e3"), Ok(decimal("1000.0")));
        assert_eq!(default(r#""x""#), Ok(GenericValue::String("x".into())));
        assert_eq!(default("null"), Ok(GenericValue::Null));
    }

    #[test]
    fn an_option_and_a_constant_take_any_value() {
        for slot in [ValueSlot::Option, ValueSlot::Constant] {
            for text in ["5", "1e3", r#"[1, "a"]"#, r#"{"a": {"b": [true]}}"#, "null"] {
                assert!(super::read_typed(text, slot).is_ok(), "{slot:?} {text}");
            }
        }
    }
}
