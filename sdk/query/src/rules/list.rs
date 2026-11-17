//! A rule list: ordered `condition → outcome` arms, first match wins.
//!
//! [`Rules`] is what a rule function builds. A [`Plan`] is the same list ready
//! to use, and the one place both halves read it from: [`Plan::value`] decides
//! a document's value, [`Plan::select`] gives the condition under which the
//! rules yield a wanted value. Each arm is kept in the form it is sent in, so
//! the two cannot drift.

use serde::Serialize;
use serde_json::Value;

use super::condition::{Checked, Cond, Condition, Field, and, holds, not, or, positive, to_json};
use super::handles::{
    Array, Optional, Present, Required, RuleBool, RuleDate, RuleKeyword, RuleNumber,
};
use crate::handles::{FlussoValue, kind};

#[derive(Debug, Clone)]
pub(crate) enum Yield<T> {
    Value(T),
    Stored(Field),
}

/// What an arm yields: a fixed value, or whatever a stored field holds.
#[derive(Debug, Clone)]
pub struct Outcome<T>(pub(crate) Yield<T>);

/// Anything usable after `then` / `otherwise` in a [`Rules<T>`]: a `T`, or the
/// handle of a stored field that reads as a `T`.
///
/// A stored field stands for `T` when its kind does and its presence fits: a
/// [`Required`] field for `T` or `Option<T>`, an [`Optional`] one for
/// `Option<T>` only (or for `T` once it is taken as present with
/// `assume_present()`), an [`Array`] one for `Vec<T>` or `Option<Vec<T>>`.
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be what a `Rules<{T}>` yields",
    label = "not a `{T}`, and not a stored field that reads as one",
    note = "yield a `{T}`, or the handle of a stored field of the matching kind",
    note = "a field that can be missing reads as `Option<…>` (or as the plain type after `.assume_present()`), and an array field as `Vec<…>`"
)]
pub trait IntoOutcome<T> {
    /// The outcome this stands for.
    fn into_outcome(self) -> Outcome<T>;
}

impl<T: Serialize> IntoOutcome<T> for T {
    fn into_outcome(self) -> Outcome<T> {
        Outcome(Yield::Value(self))
    }
}

macro_rules! stored_outcome {
    ($handle:ident $(<$number:ty>)?, $kind:ty) => {
        stored_outcome!(@impl $handle $(<$number>)?, $kind, Required, V);
        stored_outcome!(@impl $handle $(<$number>)?, $kind, Required, Option<V>);
        stored_outcome!(@impl $handle $(<$number>)?, $kind, Optional, Option<V>);
        stored_outcome!(@impl $handle $(<$number>)?, $kind, Array, Vec<V>);
        stored_outcome!(@impl $handle $(<$number>)?, $kind, Array, Option<Vec<V>>);
    };
    (@impl $handle:ident $(<$number:ty>)?, $kind:ty, $presence:ty, $target:ty) => {
        impl<V: FlussoValue<$kind>> IntoOutcome<$target> for $handle<$($number,)? $presence> {
            fn into_outcome(self) -> Outcome<$target> {
                Outcome(Yield::Stored(self.field))
            }
        }
    };
}

stored_outcome!(RuleKeyword, kind::Keyword);
stored_outcome!(RuleBool, kind::Bool);
stored_outcome!(RuleDate, kind::Date);
stored_outcome!(RuleNumber<kind::Byte>, kind::Byte);
stored_outcome!(RuleNumber<kind::Short>, kind::Short);
stored_outcome!(RuleNumber<kind::Integer>, kind::Integer);
stored_outcome!(RuleNumber<kind::Long>, kind::Long);
stored_outcome!(RuleNumber<kind::Double>, kind::Double);
stored_outcome!(RuleNumber<kind::Decimal>, kind::Decimal);

impl<K, V: FlussoValue<K>> IntoOutcome<V> for Present<K> {
    fn into_outcome(self) -> Outcome<V> {
        Outcome(Yield::Stored(self.field))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Arm<T> {
    pub(crate) when: Condition,
    pub(crate) then: Outcome<T>,
}

/// An ordered rule list: the first arm whose condition holds decides the value.
///
/// One list gives a field its value on every fetched document, and a filter, a
/// sort and counts on that value, all answered by the index without a script.
/// A root declares it with `#[flusso(rules = function)]`, where the function
/// builds the list from the request's context.
///
/// ```
/// use flusso_query::rules::{Required, RuleDate, RuleKeyword, Rules};
///
/// let ends_at = RuleDate::<Required>::at("ends_at");
/// let state = RuleKeyword::<Required>::at("state");
/// let visible: Rules<String> = Rules::when(ends_at.lt("2026-11-16T12:00:00Z"))
///     .then("closed".to_owned())
///     .otherwise(state);
/// # let _ = visible;
/// ```
#[derive(Debug, Clone)]
pub struct Rules<T> {
    pub(crate) arms: Vec<Arm<T>>,
    pub(crate) fallback: Outcome<T>,
    pub(crate) order: Vec<T>,
}

/// A rule list still missing its `otherwise`.
#[derive(Debug)]
pub struct RulesBuilder<T> {
    arms: Vec<Arm<T>>,
}

/// A condition waiting for its `then`.
#[derive(Debug)]
pub struct When<T> {
    arms: Vec<Arm<T>>,
    when: Condition,
}

impl<T> Rules<T> {
    /// Start a rule list with its first condition.
    pub fn when(condition: Condition) -> When<T> {
        When {
            arms: Vec::new(),
            when: condition,
        }
    }

    /// The value order a sort and `counts` use, first to last. Without it the
    /// order is the overridden field's `variants:` in the schema, else the
    /// order the rules name their values in.
    #[must_use]
    pub fn order(mut self, order: impl IntoIterator<Item = T>) -> Self {
        self.order = order.into_iter().collect();
        self
    }
}

impl<T> When<T> {
    /// What the field is when the condition holds.
    pub fn then(mut self, outcome: impl IntoOutcome<T>) -> RulesBuilder<T> {
        self.arms.push(Arm {
            when: self.when,
            then: outcome.into_outcome(),
        });
        RulesBuilder { arms: self.arms }
    }
}

impl<T> RulesBuilder<T> {
    /// The next condition, tried when every earlier one failed.
    pub fn when(self, condition: Condition) -> When<T> {
        When {
            arms: self.arms,
            when: condition,
        }
    }

    /// What the field is when no condition holds.
    pub fn otherwise(self, outcome: impl IntoOutcome<T>) -> Rules<T> {
        Rules {
            arms: self.arms,
            fallback: outcome.into_outcome(),
            order: Vec::new(),
        }
    }
}

/// What an arm yields, with a fixed value already as JSON.
#[derive(Debug, Clone)]
pub(crate) enum Slot {
    Value(Value),
    Stored(Field),
}

/// One arm as it is sent and decided.
#[derive(Debug, Clone)]
pub(crate) struct PlanArm {
    /// The arm's condition, negations pushed down.
    pub(crate) when: Cond,
    /// The condition a later arm sends for "this arm did not hold".
    unless: Cond,
    pub(crate) slot: Slot,
}

/// A rule list ready to use. The last arm is the fallback, under a condition
/// that always holds.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    pub(crate) arms: Vec<PlanArm>,
}

/// Why [`Plan::value`] has no value for a document.
pub(crate) const UNDECIDED: &str = "no rule decides this document: it holds no value in a field \
     the schema declares always present, which the rules test";

impl Plan {
    pub(crate) fn new<T: Serialize>(rules: &Rules<T>) -> Checked<Self> {
        let arm = |when: Cond, outcome: &Outcome<T>| -> Checked<PlanArm> {
            Ok(PlanArm {
                unless: positive(&not(when.clone())),
                when: positive(&when),
                slot: match &outcome.0 {
                    Yield::Value(value) => Slot::Value(to_json(value)?),
                    Yield::Stored(field) => Slot::Stored(field.clone()),
                },
            })
        };
        let mut arms = rules
            .arms
            .iter()
            .map(|rule| arm(rule.when.0.clone()?, &rule.then))
            .collect::<Checked<Vec<_>>>()?;
        arms.push(arm(Cond::All, &rules.fallback)?);
        Ok(Self { arms })
    }

    /// The field's value on one document: the outcome of the first arm that
    /// holds.
    ///
    /// A filter says "no earlier arm held" with each earlier condition
    /// negated, and a negated range on a field the schema declares always
    /// present is sent as the opposite range. On a document that lacks such a
    /// field both ranges fail, so no filter selects it under any value; it gets
    /// no value here either, rather than one its own filter would not find.
    pub(crate) fn value(&self, source: &Value) -> Checked<Value> {
        let Some(at) = self.arms.iter().position(|arm| holds(&arm.when, source)) else {
            return Err(UNDECIDED.to_owned());
        };
        let mut earlier = self.arms.iter().take(at);
        if !earlier.all(|arm| holds(&arm.unless, source)) {
            return Err(UNDECIDED.to_owned());
        }
        Ok(match self.arms.get(at).map(|arm| &arm.slot) {
            Some(Slot::Value(value)) => value.clone(),
            Some(Slot::Stored(field)) => field.read(source),
            None => Value::Null,
        })
    }

    /// The documents whose value passes: `fixed` judges a fixed value, `stored`
    /// gives the test for a value read from a stored field.
    pub(crate) fn select(
        &self,
        fixed: impl Fn(&Value) -> bool,
        stored: impl Fn(&Field) -> Checked<Cond>,
    ) -> Checked<Cond> {
        let mut earlier: Vec<Cond> = Vec::new();
        let mut options = Vec::new();
        for arm in &self.arms {
            let passes = match &arm.slot {
                Slot::Value(value) if fixed(value) => Cond::All,
                Slot::Value(_) => Cond::Nothing,
                Slot::Stored(field) => stored(field)?,
            };
            let mut parts = earlier.clone();
            parts.push(arm.when.clone());
            parts.push(passes);
            options.push(and(parts));
            earlier.push(arm.unless.clone());
        }
        Ok(or(options))
    }
}
