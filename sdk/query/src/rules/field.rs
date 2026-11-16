//! A rule field bound to a context: the filter, sort and counts on its value.

use std::cmp::Ordering;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use super::condition::{Checked, Cond, Kind, Side, and, clause, invalid, not, render};
use super::list::{Plan, Rules, Slot, Yield};
use crate::handles::{Sort, SortOrder, Sortable};
use crate::query::{Query, Root};
use crate::search::Search;
use crate::{Client, Result};

/// A rule field bound to a context: what a root's `state(&cx)` returns.
///
/// Its queries select on the value the rules decide, each a plain query over
/// the stored fields the rules read, with no script:
///
/// ```
/// use flusso_query::rules::{Required, RuleDate, RuleField, Rules};
/// use flusso_query::AsQuery;
/// use serde_json::json;
///
/// let ends_at = RuleDate::<Required>::at("ends_at");
/// let rules: Rules<&str> = Rules::when(ends_at.lt(1_794_830_400_000_i64.to_string()))
///     .then("closed")
///     .otherwise("open");
/// let state = RuleField::new(rules, &[]);
///
/// assert_eq!(
///     state.eq("open").to_value(),
///     json!({ "range": { "ends_at": { "gte": 1_794_830_400_000_i64, "format": "epoch_millis" } } }),
/// );
/// ```
#[derive(Debug, Clone)]
pub struct RuleField<T> {
    rules: Rules<T>,
    plan: Checked<Plan>,
    schema_order: &'static [&'static str],
}

impl<T: Serialize> RuleField<T> {
    /// Bind `rules`. `schema_order` is the `variants:` order of the stored
    /// field these rules override, empty when there is none.
    #[must_use]
    pub fn new(rules: Rules<T>, schema_order: &'static [&'static str]) -> Self {
        let plan = Plan::new(&rules);
        Self {
            rules,
            plan,
            schema_order,
        }
    }

    fn query(cond: &Checked<Cond>) -> Query<Root> {
        Query::leaf(clause(cond))
    }

    fn json(value: &T) -> Checked<Value> {
        super::condition::to_json(value)
    }

    fn one_of(&self, values: &[Value]) -> Checked<Cond> {
        self.plan
            .as_ref()
            .map_err(Clone::clone)?
            .select(|value| values.contains(value), |field| field.one_of(values))
    }

    /// The field is exactly `value`.
    #[must_use]
    pub fn eq(&self, value: T) -> Query<Root> {
        Self::query(&Self::json(&value).and_then(|value| self.one_of(&[value])))
    }

    /// The field is anything but `value`.
    #[must_use]
    pub fn ne(&self, value: T) -> Query<Root> {
        Self::query(
            &Self::json(&value)
                .and_then(|value| self.one_of(&[value]))
                .map(not),
        )
    }

    /// The field is one of `values`.
    #[must_use]
    pub fn any_of(&self, values: impl IntoIterator<Item = T>) -> Query<Root> {
        let values: Checked<Vec<Value>> =
            values.into_iter().map(|value| Self::json(&value)).collect();
        Self::query(&values.and_then(|values| self.one_of(&values)))
    }

    /// The documents whose value is on `side` of `value` by `T`'s own
    /// ordering. A number or date read from a stored field is ordered by the
    /// index, which orders those as `T` does. Text is not: the index orders it
    /// alphabetically, so a stored keyword is matched against the known values
    /// that pass.
    fn compares(&self, side: Side, value: &T) -> Checked<Cond>
    where
        T: PartialOrd + DeserializeOwned,
    {
        let json = Self::json(value)?;
        let passes = |candidate: &Value| {
            let Ok(candidate) = serde_json::from_value::<T>(candidate.clone()) else {
                return false;
            };
            match candidate.partial_cmp(value) {
                Some(Ordering::Less) => matches!(side, Side::Below | Side::AtMost),
                Some(Ordering::Equal) => matches!(side, Side::AtMost | Side::AtLeast),
                Some(Ordering::Greater) => matches!(side, Side::Above | Side::AtLeast),
                None => false,
            }
        };
        self.plan
            .as_ref()
            .map_err(Clone::clone)?
            .select(passes, |field| match field.kind {
                Kind::Keyword | Kind::Bool => {
                    let known = self.ordered()?;
                    if known.is_empty() {
                        return Err(format!(
                            "an ordered comparison reaches the stored field `{}`, whose values \
                             the index orders alphabetically; list the values in order with \
                             `Rules::order`",
                            field.path
                        ));
                    }
                    let passing: Vec<Value> = known.into_iter().filter(passes).collect();
                    field.one_of(&passing)
                }
                Kind::Whole | Kind::Double | Kind::Date => field.compares(side, &json),
            })
    }

    /// The field is below `value`, by `T`'s own ordering.
    #[must_use]
    pub fn lt(&self, value: T) -> Query<Root>
    where
        T: PartialOrd + DeserializeOwned,
    {
        Self::query(&self.compares(Side::Below, &value))
    }

    /// The field is `value` or below.
    #[must_use]
    pub fn lte(&self, value: T) -> Query<Root>
    where
        T: PartialOrd + DeserializeOwned,
    {
        Self::query(&self.compares(Side::AtMost, &value))
    }

    /// The field is above `value`.
    #[must_use]
    pub fn gt(&self, value: T) -> Query<Root>
    where
        T: PartialOrd + DeserializeOwned,
    {
        Self::query(&self.compares(Side::Above, &value))
    }

    /// The field is `value` or above.
    #[must_use]
    pub fn gte(&self, value: T) -> Query<Root>
    where
        T: PartialOrd + DeserializeOwned,
    {
        Self::query(&self.compares(Side::AtLeast, &value))
    }

    /// The field is from `low` to `high`, both included.
    #[must_use]
    pub fn between(&self, low: T, high: T) -> Query<Root>
    where
        T: PartialOrd + DeserializeOwned,
    {
        let both = self
            .compares(Side::AtLeast, &low)
            .and_then(|lower| Ok(and([lower, self.compares(Side::AtMost, &high)?])));
        Self::query(&both)
    }

    /// The values in sort order: the rules' own `order`, else the schema's
    /// declared variants, else the fixed values in the order the rules name
    /// them.
    fn ordered(&self) -> Checked<Vec<Value>> {
        if !self.rules.order.is_empty() {
            return self.rules.order.iter().map(Self::json).collect();
        }
        if !self.schema_order.is_empty() {
            return Ok(self
                .schema_order
                .iter()
                .map(|&name| Value::from(name))
                .collect());
        }
        let mut seen = Vec::new();
        let outcomes = self
            .rules
            .arms
            .iter()
            .map(|arm| &arm.then)
            .chain(std::iter::once(&self.rules.fallback));
        for outcome in outcomes {
            if let Yield::Value(value) = &outcome.0 {
                let value = Self::json(value)?;
                if !seen.contains(&value) {
                    seen.push(value);
                }
            }
        }
        Ok(seen)
    }

    /// One `function_score` function per way of reaching a value. Under
    /// `score_mode: first` the score is the rank of the value the first
    /// matching arm yields; a value outside the order ranks after every other.
    fn rank_functions(&self) -> Checked<(Vec<Value>, u64)> {
        let plan = self.plan.as_ref().map_err(Clone::clone)?;
        let order = self.ordered()?;
        let unknown = order.len() as u64 + 1;
        let rank = |value: &Value| {
            order
                .iter()
                .position(|known| known == value)
                .map_or(unknown, |at| at as u64 + 1)
        };
        let mut functions = Vec::new();
        for arm in &plan.arms {
            match &arm.slot {
                Slot::Value(value) => {
                    functions.push(json!({ "filter": render(&arm.when), "weight": rank(value) }));
                }
                Slot::Stored(field) => {
                    for value in &order {
                        let Ok(holds) = field.equals(value) else {
                            continue;
                        };
                        let filter = render(&and([arm.when.clone(), holds]));
                        functions.push(json!({ "filter": filter, "weight": rank(value) }));
                    }
                    functions.push(json!({ "filter": render(&arm.when), "weight": unknown }));
                }
            }
        }
        Ok((functions, unknown))
    }

    fn sort(&self, order: SortOrder) -> Sort {
        match self.rank_functions() {
            Ok((functions, top)) => Sort::ranked(order, functions, top),
            Err(reason) => Sort::ranked(order, vec![json!({ "filter": invalid(&reason) })], 1),
        }
    }

    /// Sort by the field's value order, first value first.
    ///
    /// The order travels in the score, so the request's text relevance is
    /// replaced and `min_score` cannot be combined with it.
    #[must_use]
    pub fn asc(&self) -> Sort {
        self.sort(SortOrder::Asc)
    }

    /// Sort by the field's value order, last value first. See
    /// [`asc`](Self::asc).
    #[must_use]
    pub fn desc(&self) -> Sort {
        self.sort(SortOrder::Desc)
    }

    /// How many of `search`'s documents have each value, in sort order, in one
    /// `_msearch` round trip.
    ///
    /// The values counted are the ones [`order`](Rules::order) lists, else the
    /// schema's variants, else the fixed values the rules name; a value a
    /// stored field yields outside of those is not counted.
    pub async fn counts<R>(&self, search: &Search<R>, client: &Client) -> Result<Vec<(T, u64)>>
    where
        R: DeserializeOwned + Clone,
        T: DeserializeOwned,
    {
        let order = self.ordered().map_err(crate::Error::ReadTimeField)?;
        let mut values = Vec::with_capacity(order.len());
        let mut searches = Vec::with_capacity(order.len());
        for value in order {
            let only = Self::query(&self.one_of(std::slice::from_ref(&value)));
            searches.push(search.clone().filter(only).size(0).track_total_hits(true));
            values.push(serde_json::from_value::<T>(value)?);
        }
        let pages = client.msearch_all(&searches).await?;
        Ok(values
            .into_iter()
            .zip(pages)
            .map(|(value, page)| (value, page.total))
            .collect())
    }
}

impl<T: Serialize> Sortable for RuleField<T> {
    fn asc(&self) -> Sort {
        self.sort(SortOrder::Asc)
    }

    fn desc(&self) -> Sort {
        self.sort(SortOrder::Desc)
    }
}
