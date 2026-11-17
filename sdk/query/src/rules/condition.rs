//! A condition over stored fields, in the one form both sides read: it renders
//! as a query for the index and it is decided on a document's `_source` in
//! Rust. Every test here has both, and they agree by construction: the tests
//! in this module pin the Rust side, the `read_time` live test pins it against
//! OpenSearch.
//!
//! What "agree" rests on, per kind:
//!
//! - **missing** fails every test but `missing()`;
//! - **an array** passes when any element does;
//! - **keyword** compares exactly, the value as the document spells it;
//! - **numbers** compare as numbers, a `double` as the `f64` nearest its text;
//! - **dates** compare at the millisecond (see [`super::date`]).

use std::cmp::Ordering;

use serde::Serialize;
use serde_json::{Map, Value, json};

use super::date;
use crate::query::{AsQuery, Query, Root};
use crate::{Error, Result};

/// A value or the reason it cannot be used, kept until the request is sent.
pub(crate) type Checked<T> = std::result::Result<T, String>;

/// How a field's values compare, taken from the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Keyword,
    Bool,
    Whole,
    Double,
    Date,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Keyword => "keyword",
            Kind::Bool => "boolean",
            Kind::Whole => "whole-number",
            Kind::Double => "number",
            Kind::Date => "date",
        }
    }
}

/// One stored field a rule reads: where it is and how its values compare.
#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) path: String,
    pub(crate) kind: Kind,
    /// The schema says every document holds exactly one value here.
    pub(crate) total: bool,
}

/// Which side of a value a comparison keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Side {
    Below,
    AtMost,
    Above,
    AtLeast,
}

/// One comparable value. A date is `Whole` epoch milliseconds.
#[derive(Debug, Clone, PartialEq)]
enum Scalar {
    Text(String),
    Bool(bool),
    Whole(i64),
    Float(f64),
}

#[derive(Debug, Clone)]
struct Bound {
    value: Scalar,
    inclusive: bool,
}

#[derive(Debug, Clone)]
enum Op {
    Exists,
    In(Vec<Scalar>),
    Range {
        lower: Option<Bound>,
        upper: Option<Bound>,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct Test {
    field: Field,
    op: Op,
}

/// A condition that renders as a query and is decided on a document.
#[derive(Debug, Clone)]
pub(crate) enum Cond {
    All,
    Nothing,
    Test(Test),
    And(Vec<Cond>),
    Or(Vec<Cond>),
    Not(Box<Cond>),
}

pub(crate) fn and(parts: impl IntoIterator<Item = Cond>) -> Cond {
    let mut kept = Vec::new();
    for part in parts {
        match part {
            Cond::All => {}
            Cond::Nothing => return Cond::Nothing,
            Cond::And(inner) => kept.extend(inner),
            other => kept.push(other),
        }
    }
    match kept.len() {
        0 => Cond::All,
        1 => kept.pop().unwrap_or(Cond::All),
        _ => Cond::And(kept),
    }
}

pub(crate) fn or(parts: impl IntoIterator<Item = Cond>) -> Cond {
    let mut kept = Vec::new();
    for part in parts {
        match part {
            Cond::Nothing => {}
            Cond::All => return Cond::All,
            Cond::Or(inner) => kept.extend(inner),
            other => kept.push(other),
        }
    }
    match kept.len() {
        0 => Cond::Nothing,
        1 => kept.pop().unwrap_or(Cond::Nothing),
        _ => Cond::Or(kept),
    }
}

pub(crate) fn not(cond: Cond) -> Cond {
    match cond {
        Cond::All => Cond::Nothing,
        Cond::Nothing => Cond::All,
        Cond::Not(inner) => *inner,
        other => Cond::Not(Box::new(other)),
    }
}

pub(crate) fn to_json(value: &impl Serialize) -> Checked<Value> {
    serde_json::to_value(value).map_err(|error| format!("a value does not serialize: {error}"))
}

/// Read one JSON value as `kind`. `None` for a value the index would not hold
/// under that mapping.
fn scalar(kind: Kind, value: &Value) -> Option<Scalar> {
    match (kind, value) {
        (Kind::Keyword, Value::String(text)) => Some(Scalar::Text(text.clone())),
        (Kind::Keyword, Value::Number(number)) => Some(Scalar::Text(number.to_string())),
        (Kind::Keyword, Value::Bool(flag)) => Some(Scalar::Text(flag.to_string())),
        (Kind::Bool, Value::Bool(flag)) => Some(Scalar::Bool(*flag)),
        (Kind::Bool, Value::String(text)) => match text.as_str() {
            "true" => Some(Scalar::Bool(true)),
            "false" | "" => Some(Scalar::Bool(false)),
            _ => None,
        },
        (Kind::Whole, Value::Number(number)) => number
            .as_i64()
            .or_else(|| number.as_f64().and_then(whole_part))
            .map(Scalar::Whole),
        (Kind::Whole, Value::String(text)) => text
            .parse()
            .ok()
            .or_else(|| text.parse().ok().and_then(whole_part))
            .map(Scalar::Whole),
        (Kind::Double, Value::Number(number)) => number.as_f64().map(Scalar::Float),
        (Kind::Double, Value::String(text)) => text.parse().ok().map(Scalar::Float),
        (Kind::Date, value) => date::span(value).map(|span| Scalar::Whole(span.first)),
        _ => None,
    }
    .filter(|scalar| !matches!(scalar, Scalar::Float(number) if !number.is_finite()))
}

/// The whole number the index stores for a fraction sent to a whole-number
/// field: the fraction dropped, towards zero.
fn whole_part(number: f64) -> Option<i64> {
    let whole = number.trunc();
    // Both bounds are exact as `f64`; `i64::MAX` itself is not.
    (-9_223_372_036_854_775_808.0..9_223_372_036_854_775_808.0)
        .contains(&whole)
        .then_some(whole as i64)
}

fn compare(left: &Scalar, right: &Scalar) -> Option<Ordering> {
    match (left, right) {
        (Scalar::Text(left), Scalar::Text(right)) => Some(left.cmp(right)),
        (Scalar::Bool(left), Scalar::Bool(right)) => Some(left.cmp(right)),
        (Scalar::Whole(left), Scalar::Whole(right)) => Some(left.cmp(right)),
        (Scalar::Float(left), Scalar::Float(right)) => left.partial_cmp(right),
        _ => None,
    }
}

impl Field {
    pub(crate) fn new(path: &str, kind: Kind, total: bool) -> Self {
        Self {
            path: path.to_owned(),
            kind,
            total,
        }
    }

    fn test(&self, op: Op) -> Cond {
        Cond::Test(Test {
            field: self.clone(),
            op,
        })
    }

    fn constant(&self, value: &Value) -> Checked<Scalar> {
        scalar(self.kind, value).ok_or_else(|| {
            format!(
                "`{value}` is not a {} value, so it cannot be compared with `{}`",
                self.kind.name(),
                self.path
            )
        })
    }

    fn span(&self, value: &Value) -> Checked<date::Span> {
        date::span(value).ok_or_else(|| {
            format!(
                "`{value}` is not a date, so it cannot be compared with `{}`; write an ISO-8601 \
                 date or timestamp, or epoch milliseconds",
                self.path
            )
        })
    }

    fn range(&self, lower: Option<Bound>, upper: Option<Bound>) -> Cond {
        self.test(Op::Range { lower, upper })
    }

    /// The document holds a value here.
    pub(crate) fn exists(&self) -> Cond {
        self.test(Op::Exists)
    }

    /// The field is `value`. A date constant is the stretch it covers.
    pub(crate) fn equals(&self, value: &Value) -> Checked<Cond> {
        if self.kind != Kind::Date {
            return Ok(self.test(Op::In(vec![self.constant(value)?])));
        }
        let span = self.span(value)?;
        Ok(self.range(
            Some(Bound {
                value: Scalar::Whole(span.first),
                inclusive: true,
            }),
            Some(Bound {
                value: Scalar::Whole(span.last),
                inclusive: true,
            }),
        ))
    }

    /// The field is one of `values`.
    pub(crate) fn one_of(&self, values: &[Value]) -> Checked<Cond> {
        if self.kind == Kind::Date {
            return values
                .iter()
                .map(|value| self.equals(value))
                .collect::<Checked<Vec<_>>>()
                .map(or);
        }
        if values.is_empty() {
            return Ok(Cond::Nothing);
        }
        values
            .iter()
            .map(|value| self.constant(value))
            .collect::<Checked<Vec<_>>>()
            .map(|values| self.test(Op::In(values)))
    }

    fn bound(&self, side: Side, value: &Value) -> Checked<Bound> {
        let inclusive = matches!(side, Side::AtMost | Side::AtLeast);
        // A date constant covers a stretch: "after" and "up to" measure from
        // its end, "from" and "before" from its start, as the index does.
        let value = if self.kind == Kind::Date {
            let span = self.span(value)?;
            Scalar::Whole(match side {
                Side::Above | Side::AtMost => span.last,
                Side::Below | Side::AtLeast => span.first,
            })
        } else {
            self.constant(value)?
        };
        Ok(Bound { value, inclusive })
    }

    /// The field is on `side` of `value`.
    pub(crate) fn compares(&self, side: Side, value: &Value) -> Checked<Cond> {
        let bound = Some(self.bound(side, value)?);
        Ok(match side {
            Side::Above | Side::AtLeast => self.range(bound, None),
            Side::Below | Side::AtMost => self.range(None, bound),
        })
    }

    /// The field is from `low` to `high`, both included.
    pub(crate) fn between(&self, low: &Value, high: &Value) -> Checked<Cond> {
        Ok(self.range(
            Some(self.bound(Side::AtLeast, low)?),
            Some(self.bound(Side::AtMost, high)?),
        ))
    }

    /// What the document holds here: the value, `null` when missing, and an
    /// array when the path runs through one.
    pub(crate) fn read(&self, source: &Value) -> Value {
        read(source, &self.path)
    }
}

fn read(value: &Value, path: &str) -> Value {
    match value {
        Value::Array(items) => {
            let mut found = Vec::new();
            for item in items {
                match read(item, path) {
                    Value::Null => {}
                    Value::Array(inner) => {
                        found.extend(inner.into_iter().filter(|value| !value.is_null()));
                    }
                    other => found.push(other),
                }
            }
            Value::Array(found)
        }
        Value::Object(map) => match path.split_once('.') {
            Some((head, rest)) => map.get(head).map_or(Value::Null, |child| read(child, rest)),
            None => map.get(path).cloned().unwrap_or(Value::Null),
        },
        _ => Value::Null,
    }
}

/// `cond` with its negations pushed down to the tests. A negated range on a
/// field that always holds one value becomes the opposite range, which the
/// index answers from its sorted structure instead of excluding a whole set.
pub(crate) fn positive(cond: &Cond) -> Cond {
    match cond {
        Cond::Not(inner) => negative(inner),
        Cond::And(parts) => and(parts.iter().map(positive)),
        Cond::Or(parts) => or(parts.iter().map(positive)),
        other => other.clone(),
    }
}

fn negative(cond: &Cond) -> Cond {
    match cond {
        Cond::All => Cond::Nothing,
        Cond::Nothing => Cond::All,
        Cond::Not(inner) => positive(inner),
        Cond::And(parts) => or(parts.iter().map(negative)),
        Cond::Or(parts) => and(parts.iter().map(negative)),
        Cond::Test(test) => match &test.op {
            Op::Range { lower, upper } if test.field.total => {
                let flipped = |bound: &Bound| {
                    Some(Bound {
                        value: bound.value.clone(),
                        inclusive: !bound.inclusive,
                    })
                };
                let below = lower
                    .as_ref()
                    .map(|bound| test.field.range(None, flipped(bound)));
                let above = upper
                    .as_ref()
                    .map(|bound| test.field.range(flipped(bound), None));
                or(below.into_iter().chain(above))
            }
            _ => Cond::Not(Box::new(cond.clone())),
        },
    }
}

fn scalar_value(scalar: &Scalar) -> Value {
    match scalar {
        Scalar::Text(text) => Value::String(text.clone()),
        Scalar::Bool(flag) => Value::Bool(*flag),
        Scalar::Whole(number) => Value::from(*number),
        Scalar::Float(number) => Value::from(*number),
    }
}

impl Test {
    fn range(&self, lower: Option<&Bound>, upper: Option<&Bound>) -> Value {
        let mut bounds = Map::new();
        if let Some(bound) = lower {
            let key = if bound.inclusive { "gte" } else { "gt" };
            bounds.insert(key.to_owned(), scalar_value(&bound.value));
        }
        if let Some(bound) = upper {
            let key = if bound.inclusive { "lte" } else { "lt" };
            bounds.insert(key.to_owned(), scalar_value(&bound.value));
        }
        if self.field.kind == Kind::Date {
            bounds.insert("format".to_owned(), Value::from("epoch_millis"));
        }
        json!({ "range": { self.field.path.as_str(): bounds } })
    }

    fn render(&self) -> Value {
        match &self.op {
            Op::Exists => json!({ "exists": { "field": self.field.path } }),
            Op::In(values) => {
                let values: Vec<Value> = values.iter().map(scalar_value).collect();
                json!({ "terms": { self.field.path.as_str(): values } })
            }
            Op::Range { lower, upper } => self.range(lower.as_ref(), upper.as_ref()),
        }
    }

    fn holds(&self, source: &Value) -> bool {
        let mut found = Vec::new();
        leaves(source, &self.field.path, &mut found);
        let mut values = found
            .into_iter()
            .filter_map(|value| scalar(self.field.kind, value));
        match &self.op {
            Op::Exists => values.next().is_some(),
            Op::In(wanted) => values.any(|value| {
                wanted
                    .iter()
                    .any(|wanted| compare(&value, wanted) == Some(Ordering::Equal))
            }),
            Op::Range { lower, upper } => values.any(|value| {
                let above =
                    lower
                        .as_ref()
                        .is_none_or(|bound| match compare(&value, &bound.value) {
                            Some(Ordering::Greater) => true,
                            Some(Ordering::Equal) => bound.inclusive,
                            _ => false,
                        });
                let below =
                    upper
                        .as_ref()
                        .is_none_or(|bound| match compare(&value, &bound.value) {
                            Some(Ordering::Less) => true,
                            Some(Ordering::Equal) => bound.inclusive,
                            _ => false,
                        });
                above && below
            }),
        }
    }
}

/// Every non-null value at `path`, looking through arrays at any level: what
/// the index sees for a field that is not `nested`.
fn leaves<'a>(value: &'a Value, path: &str, out: &mut Vec<&'a Value>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| leaves(item, path, out)),
        Value::Object(map) => {
            let (head, rest) = match path.split_once('.') {
                Some((head, rest)) => (head, Some(rest)),
                None => (path, None),
            };
            match (map.get(head), rest) {
                (Some(child), Some(rest)) => leaves(child, rest, out),
                (Some(child), None) => flatten(child, out),
                (None, _) => {}
            }
        }
        _ => {}
    }
}

fn flatten<'a>(value: &'a Value, out: &mut Vec<&'a Value>) {
    match value {
        Value::Null => {}
        Value::Array(items) => items.iter().for_each(|item| flatten(item, out)),
        other => out.push(other),
    }
}

/// `cond` as a query clause. Expects [`positive`] form for the cheapest query;
/// any form renders correctly.
pub(crate) fn render(cond: &Cond) -> Value {
    match cond {
        Cond::All => json!({ "match_all": {} }),
        Cond::Nothing => json!({ "match_none": {} }),
        Cond::Test(test) => test.render(),
        Cond::Not(inner) => json!({ "bool": { "must_not": [render(inner)] } }),
        Cond::Or(parts) => {
            let should: Vec<Value> = parts.iter().map(render).collect();
            json!({ "bool": { "should": should, "minimum_should_match": 1 } })
        }
        Cond::And(parts) => {
            let mut filter = Vec::new();
            let mut must_not = Vec::new();
            for part in parts {
                match part {
                    Cond::Not(inner) => must_not.push(render(inner)),
                    other => filter.push(render(other)),
                }
            }
            let mut body = Map::new();
            if !filter.is_empty() {
                body.insert("filter".to_owned(), Value::Array(filter));
            }
            if !must_not.is_empty() {
                body.insert("must_not".to_owned(), Value::Array(must_not));
            }
            json!({ "bool": body })
        }
    }
}

/// Whether `cond` holds on a document's `_source`.
pub(crate) fn holds(cond: &Cond, source: &Value) -> bool {
    match cond {
        Cond::All => true,
        Cond::Nothing => false,
        Cond::Test(test) => test.holds(source),
        Cond::Not(inner) => !holds(inner, source),
        Cond::And(parts) => parts.iter().all(|part| holds(part, source)),
        Cond::Or(parts) => parts.iter().any(|part| holds(part, source)),
    }
}

/// The key of the clause that stands in for a rule that cannot be evaluated.
/// It is no query the index knows, and [`refuse_invalid`] stops the request
/// before it leaves.
const INVALID: &str = "flusso_invalid_rule";

/// The clause standing in for a rule that cannot be evaluated, carrying why.
pub(crate) fn invalid(reason: &str) -> Value {
    json!({ INVALID: { "reason": reason } })
}

/// `cond` as a clause: the query, or the stand-in carrying the reason.
pub(crate) fn clause(cond: &Checked<Cond>) -> Value {
    match cond {
        Ok(cond) => render(&positive(cond)),
        Err(reason) => invalid(reason),
    }
}

/// The reason inside the first invalid-rule clause anywhere in `body`.
fn invalid_reason(body: &Value) -> Option<&str> {
    match body {
        Value::Object(map) => match map.get(INVALID) {
            Some(marker) => Some(
                marker
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("a rule cannot be evaluated"),
            ),
            None => map.values().find_map(invalid_reason),
        },
        Value::Array(items) => items.iter().find_map(invalid_reason),
        _ => None,
    }
}

/// Fail a request whose body holds a rule that cannot be evaluated, before it
/// is sent. Called by every transport that sends a query body.
pub(crate) fn refuse_invalid(body: &Value) -> Result<()> {
    match invalid_reason(body) {
        Some(reason) => Err(Error::ReadTimeField(reason.to_owned())),
        None => Ok(()),
    }
}

/// A test a rule can make on stored fields.
///
/// Built from the rule handles behind a root's `doc()` and combined with
/// [`and`](Self::and) / [`or`](Self::or) / [`not`](Self::not). It is also an
/// ordinary filter: `Search::filter` takes it as it takes any query.
///
/// ```
/// use flusso_query::rules::{RuleBool, RuleDate};
/// use serde_json::json;
///
/// let ends_at = RuleDate::<flusso_query::rules::Required>::at("ends_at");
/// let auto_renew = RuleBool::<flusso_query::rules::Required>::at("auto_renew");
/// let lapsing = ends_at.lt("2026-12-01").and(auto_renew.eq(false));
///
/// assert!(lapsing.holds(&json!({ "ends_at": "2026-11-20T09:00:00Z", "auto_renew": false })));
/// assert!(!lapsing.holds(&json!({ "ends_at": "2026-11-20T09:00:00Z", "auto_renew": true })));
/// ```
#[derive(Debug, Clone)]
pub struct Condition(pub(crate) Checked<Cond>);

impl Condition {
    fn join(self, other: Condition, join: impl FnOnce([Cond; 2]) -> Cond) -> Condition {
        Condition(match (self.0, other.0) {
            (Ok(left), Ok(right)) => Ok(join([left, right])),
            (Err(reason), _) | (_, Err(reason)) => Err(reason),
        })
    }

    /// Both hold.
    #[must_use]
    pub fn and(self, other: Condition) -> Condition {
        self.join(other, and)
    }

    /// Either holds.
    #[must_use]
    pub fn or(self, other: Condition) -> Condition {
        self.join(other, or)
    }

    /// This does not hold.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Condition {
        Condition(self.0.map(not))
    }

    /// Whether the condition holds on a document's `_source`, decided as the
    /// index decides it. A condition that cannot be evaluated holds nowhere.
    #[must_use]
    pub fn holds(&self, source: &Value) -> bool {
        self.0
            .as_ref()
            .is_ok_and(|cond| holds(&positive(cond), source))
    }
}

impl AsQuery<Root> for Condition {
    fn into_query(self) -> Option<Query<Root>> {
        Some(Query::leaf(clause(&self.0)))
    }
}
