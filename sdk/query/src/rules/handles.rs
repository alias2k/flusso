//! The rule handles: one per stored field a rule can read, offering exactly
//! the tests a rule can make on it.
//!
//! `#[derive(FlussoRoot)]` generates them behind `Root::doc()`, only for the
//! fields whose tests Rust decides exactly as the index does. Each handle also
//! says, in its type, whether the field always holds one value
//! ([`Required`]), can be missing ([`Optional`]) or holds several ([`Array`]),
//! which decides what a rule may yield it as (see
//! [`IntoOutcome`](super::IntoOutcome)).

use std::marker::PhantomData;

use super::condition::{Checked, Cond, Condition, Field, Kind, Side, not, to_json};
use crate::handles::{FlussoValue, kind};

mod sealed {
    #[allow(unreachable_pub)]
    pub trait Sealed {}
}

/// How many values a stored field holds, as the schema declares it.
pub trait Presence: sealed::Sealed {
    /// Every document holds exactly one value.
    #[doc(hidden)]
    const TOTAL: bool;
}

/// Every document holds exactly one value here.
#[derive(Debug, Clone, Copy)]
pub enum Required {}

/// A document may hold no value here: the field, or an object above it, is
/// nullable.
#[derive(Debug, Clone, Copy)]
pub enum Optional {}

/// A document holds any number of values here: the field, or an object above
/// it, is an array.
#[derive(Debug, Clone, Copy)]
pub enum Array {}

impl sealed::Sealed for Required {}
impl sealed::Sealed for Optional {}
impl sealed::Sealed for Array {}

impl Presence for Required {
    const TOTAL: bool = true;
}
impl Presence for Optional {
    const TOTAL: bool = false;
}
impl Presence for Array {
    const TOTAL: bool = false;
}

/// A numeric kind a rule can compare: the whole-number kinds, `double` and
/// `decimal`. A 32-bit `float` is not one.
pub trait RuleNumberKind: sealed::Sealed {
    /// The index holds whole numbers for this kind.
    #[doc(hidden)]
    const WHOLE: bool;
}

macro_rules! number_kinds {
    ($($kind:ident: $whole:literal),+ $(,)?) => {$(
        impl sealed::Sealed for kind::$kind {}
        impl RuleNumberKind for kind::$kind {
            const WHOLE: bool = $whole;
        }
    )+};
}
number_kinds!(Byte: true, Short: true, Integer: true, Long: true, Double: false, Decimal: false);

fn condition(cond: Checked<Cond>) -> Condition {
    Condition(cond)
}

/// What every handle shares: the field, and the presence tests.
macro_rules! handle {
    ($(#[$doc:meta])* $name:ident $(<$kind:ident: $bound:ident>)?, $field:expr) => {
        $(#[$doc])*
        #[derive(Debug, Clone)]
        pub struct $name<$($kind,)? P = Required> {
            pub(crate) field: Field,
            marker: PhantomData<fn() -> ($($kind,)? P,)>,
        }

        impl<$($kind: $bound,)? P: Presence> $name<$($kind,)? P> {
            /// The field at the dotted `path` from the document root.
            #[must_use]
            pub fn at(path: &str) -> Self {
                let kind: Kind = $field;
                Self {
                    field: Field::new(path, kind, P::TOTAL),
                    marker: PhantomData,
                }
            }

            /// The document holds a value here.
            #[must_use]
            pub fn exists(&self) -> Condition {
                condition(Ok(self.field.exists()))
            }

            /// The document holds no value here.
            #[must_use]
            pub fn missing(&self) -> Condition {
                condition(Ok(not(self.field.exists())))
            }
        }
    };
}

/// The exact-match tests, for a value of kind `$kind`.
macro_rules! exact {
    ($kind:ty) => {
        /// Exactly `value`.
        #[must_use]
        pub fn eq(&self, value: impl FlussoValue<$kind>) -> Condition {
            condition(to_json(&value).and_then(|value| self.field.equals(&value)))
        }

        /// Anything but `value`, a missing value included.
        #[must_use]
        pub fn ne(&self, value: impl FlussoValue<$kind>) -> Condition {
            self.eq(value).not()
        }
    };
}

/// `any_of`, for a value of kind `$kind`.
macro_rules! any_of {
    ($kind:ty) => {
        /// One of `values`.
        #[must_use]
        pub fn any_of(
            &self,
            values: impl IntoIterator<Item = impl FlussoValue<$kind>>,
        ) -> Condition {
            let values: Checked<Vec<_>> = values.into_iter().map(|value| to_json(&value)).collect();
            condition(values.and_then(|values| self.field.one_of(&values)))
        }
    };
}

/// The ordered tests, for a value of kind `$kind`.
macro_rules! ordered {
    ($kind:ty) => {
        /// Below `value`.
        #[must_use]
        pub fn lt(&self, value: impl FlussoValue<$kind>) -> Condition {
            self.compares(Side::Below, &value)
        }

        /// `value` or below.
        #[must_use]
        pub fn lte(&self, value: impl FlussoValue<$kind>) -> Condition {
            self.compares(Side::AtMost, &value)
        }

        /// Above `value`.
        #[must_use]
        pub fn gt(&self, value: impl FlussoValue<$kind>) -> Condition {
            self.compares(Side::Above, &value)
        }

        /// `value` or above.
        #[must_use]
        pub fn gte(&self, value: impl FlussoValue<$kind>) -> Condition {
            self.compares(Side::AtLeast, &value)
        }

        /// From `low` to `high`, both included.
        #[must_use]
        pub fn between(
            &self,
            low: impl FlussoValue<$kind>,
            high: impl FlussoValue<$kind>,
        ) -> Condition {
            condition(to_json(&low).and_then(|low| {
                let high = to_json(&high)?;
                self.field.between(&low, &high)
            }))
        }

        fn compares(&self, side: Side, value: &impl FlussoValue<$kind>) -> Condition {
            condition(to_json(value).and_then(|value| self.field.compares(side, &value)))
        }
    };
}

/// A stored field that can be missing, taken as present: what
/// `assume_present()` on an [`Optional`] handle returns. It is only good for
/// yielding, where it reads as the plain type instead of an `Option`.
#[derive(Debug, Clone)]
pub struct Present<K> {
    pub(crate) field: Field,
    marker: PhantomData<fn() -> K>,
}

/// `assume_present`, for the handle of a field of kind `$kind`.
macro_rules! assume_present {
    ($kind:ty) => {
        /// Yield this field as a plain value instead of an `Option`, on the
        /// rule's word that it is there whenever its arm holds: test
        /// [`exists`](Self::exists) in the arm's condition.
        ///
        /// A document where the arm holds and the field is missing then fails
        /// to decode, as a stored field declared non-`Option` does.
        #[must_use]
        pub fn assume_present(self) -> Present<$kind> {
            Present {
                field: self.field,
                marker: PhantomData,
            }
        }
    };
}

handle!(
    /// A stored `keyword`, `enum` or `uuid` field, as a rule reads it: compared
    /// exactly, as the document spells it.
    RuleKeyword,
    Kind::Keyword
);

impl<P: Presence> RuleKeyword<P> {
    exact!(kind::Keyword);
    any_of!(kind::Keyword);
}

handle!(
    /// A stored `boolean` field, as a rule reads it.
    RuleBool,
    Kind::Bool
);

impl<P: Presence> RuleBool<P> {
    exact!(kind::Bool);
}

impl RuleKeyword<Optional> {
    assume_present!(kind::Keyword);
}

impl RuleBool<Optional> {
    assume_present!(kind::Bool);
}

handle!(
    /// A stored whole-number, `double` or `decimal` field, as a rule reads it.
    /// `K` is the field's kind, so a value of another numeric type does not
    /// compile. A `decimal` compares as the index holds it: as the nearest
    /// `double`.
    RuleNumber<K: RuleNumberKind>,
    if K::WHOLE { Kind::Whole } else { Kind::Double }
);

impl<K: RuleNumberKind, P: Presence> RuleNumber<K, P> {
    exact!(K);
    any_of!(K);
    ordered!(K);
}

impl<K: RuleNumberKind> RuleNumber<K, Optional> {
    assume_present!(K);
}

handle!(
    /// A stored `date` or `timestamp` field, as a rule reads it.
    ///
    /// Dates compare at the millisecond, the precision the index keeps. A
    /// timestamp written to the second is that one instant; a bare date stands
    /// for its whole day, so `lt("2026-12-01")` is "before the 1st" and
    /// `lte("2026-12-01")` is "up to the end of the 1st".
    RuleDate,
    Kind::Date
);

impl<P: Presence> RuleDate<P> {
    exact!(kind::Date);
    any_of!(kind::Date);
    ordered!(kind::Date);
}

impl RuleDate<Optional> {
    assume_present!(kind::Date);
}
