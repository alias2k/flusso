//! Deciding whether a root row counts as soft-deleted: a truthy marker
//! (boolean `true` or any present value) optionally narrowed by `when` filters,
//! which are evaluated against the already-fetched root row.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::str::FromStr;

use rust_decimal::Decimal;
use schema_core::{
    Filter, FilterOp, FilterValue, GenericValue, IndexSchema, NullOp, SoftDelete, ValueOpFilter,
};

use super::fields::field_column;

pub(super) fn is_soft_deleted(schema: &IndexSchema, root: &HashMap<String, GenericValue>) -> bool {
    let (marker, when) = match &schema.soft_delete {
        None => return false,
        Some(SoftDelete::Column(c)) => (root.get(c.column.as_ref()), c.when.as_deref()),
        Some(SoftDelete::Field(f)) => match field_column(&schema.fields, &f.field) {
            Some(column) => (root.get(column.as_ref()), f.when.as_deref()),
            None => return false,
        },
    };
    if !soft_truthy(marker) {
        return false;
    }
    match when {
        None => true,
        Some(filters) => when_matches(filters, root),
    }
}

/// A row counts as soft-deleted when the marker is a true boolean or a present
/// (non-null) value.
fn soft_truthy(value: Option<&GenericValue>) -> bool {
    match value {
        None | Some(GenericValue::Null) => false,
        Some(GenericValue::Bool(b)) => *b,
        Some(_) => true,
    }
}

/// Evaluate soft-delete `when` filters against the root row (an AND of all).
fn when_matches(filters: &[Filter], row: &HashMap<String, GenericValue>) -> bool {
    filters.iter().all(|filter| filter_matches(filter, row))
}

fn filter_matches(filter: &Filter, row: &HashMap<String, GenericValue>) -> bool {
    match filter {
        Filter::Raw(_) => {
            tracing::warn!("raw soft_delete `when` filters are not evaluated; ignoring");
            true
        }
        Filter::NullCheck(check) => {
            let is_null = matches!(row.get(check.column.as_ref()), None | Some(GenericValue::Null));
            match check.op {
                NullOp::IsNull => is_null,
                NullOp::IsNotNull => !is_null,
            }
        }
        Filter::ValueOp(op) => value_op_matches(op, row.get(op.column.as_ref())),
    }
}

fn value_op_matches(filter: &ValueOpFilter, value: Option<&GenericValue>) -> bool {
    let Some(text) = value.and_then(scalar_to_string) else {
        return false; // null or non-scalar never matches a value comparison
    };
    match (&filter.op, &filter.value) {
        (FilterOp::Eq, FilterValue::Single(v)) => text == *v,
        (FilterOp::Neq, FilterValue::Single(v)) => text != *v,
        (FilterOp::In, FilterValue::List(vs)) => vs.contains(&text),
        (FilterOp::NotIn, FilterValue::List(vs)) => !vs.contains(&text),
        (FilterOp::Lt, FilterValue::Single(v)) => compare(&text, v) == Ordering::Less,
        (FilterOp::Lte, FilterValue::Single(v)) => compare(&text, v) != Ordering::Greater,
        (FilterOp::Gt, FilterValue::Single(v)) => compare(&text, v) == Ordering::Greater,
        (FilterOp::Gte, FilterValue::Single(v)) => compare(&text, v) != Ordering::Less,
        (FilterOp::Between, FilterValue::Range(lo, hi)) => {
            compare(&text, lo) != Ordering::Less && compare(&text, hi) != Ordering::Greater
        }
        (FilterOp::Like, FilterValue::Single(v)) => like_match(&text, v, false),
        (FilterOp::Ilike, FilterValue::Single(v)) => like_match(&text, v, true),
        _ => false,
    }
}

/// Compare numerically when both sides parse as decimals, else lexically.
fn compare(a: &str, b: &str) -> Ordering {
    match (Decimal::from_str(a), Decimal::from_str(b)) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
    }
}

/// Approximate SQL `LIKE`: handles leading/trailing `%`; ignores `_` and
/// interior wildcards.
fn like_match(text: &str, pattern: &str, case_insensitive: bool) -> bool {
    let (text, pattern) = if case_insensitive {
        (text.to_lowercase(), pattern.to_lowercase())
    } else {
        (text.to_owned(), pattern.to_owned())
    };
    let core = pattern.trim_matches('%');
    match (pattern.starts_with('%'), pattern.ends_with('%')) {
        (true, true) => text.contains(core),
        (true, false) => text.ends_with(core),
        (false, true) => text.starts_with(core),
        (false, false) => text == core,
    }
}

fn scalar_to_string(value: &GenericValue) -> Option<String> {
    match value {
        GenericValue::Bool(b) => Some(b.to_string()),
        GenericValue::Int(i) => Some(i.to_string()),
        GenericValue::Decimal(d) => Some(d.to_string()),
        GenericValue::String(s) => Some(s.clone()),
        GenericValue::Null | GenericValue::Array(_) | GenericValue::Map(_) => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{compare, like_match, soft_truthy, value_op_matches, when_matches};
    use schema_core::{
        ColumnName, Filter, FilterOp, FilterValue, GenericValue, NullCheckFilter, NullOp,
        ValueOpFilter,
    };
    use std::collections::HashMap;

    fn col(name: &str) -> ColumnName {
        ColumnName::try_new(name).unwrap()
    }

    fn row(pairs: &[(&str, GenericValue)]) -> HashMap<String, GenericValue> {
        pairs.iter().map(|(k, v)| ((*k).to_owned(), v.clone())).collect()
    }

    fn value_op(column: &str, op: FilterOp, value: FilterValue) -> ValueOpFilter {
        ValueOpFilter {
            column: col(column),
            op,
            value,
        }
    }

    #[test]
    fn compare_is_numeric_then_lexical() {
        assert_eq!(compare("9", "10"), std::cmp::Ordering::Less); // numeric, not "9" > "1…"
        assert_eq!(compare("b", "a"), std::cmp::Ordering::Greater); // lexical fallback
    }

    #[test]
    fn like_match_handles_anchors_and_case() {
        assert!(like_match("hello world", "%world", false));
        assert!(like_match("hello", "hel%", false));
        assert!(like_match("HELLO", "%ell%", true));
        assert!(!like_match("hello", "bye", false));
    }

    #[test]
    fn value_op_eq_in_and_between() {
        let active = GenericValue::String("active".into());
        assert!(value_op_matches(
            &value_op("status", FilterOp::Eq, FilterValue::Single("active".into())),
            Some(&active),
        ));
        assert!(value_op_matches(
            &value_op(
                "status",
                FilterOp::In,
                FilterValue::List(vec!["a".into(), "active".into()]),
            ),
            Some(&active),
        ));
        let between = value_op(
            "n",
            FilterOp::Between,
            FilterValue::Range("1".into(), "10".into()),
        );
        assert!(value_op_matches(&between, Some(&GenericValue::Int(5))));
        assert!(!value_op_matches(&between, Some(&GenericValue::Int(20))));
    }

    #[test]
    fn value_op_null_never_matches() {
        let eq = value_op("x", FilterOp::Eq, FilterValue::Single("v".into()));
        assert!(!value_op_matches(&eq, None));
        assert!(!value_op_matches(&eq, Some(&GenericValue::Null)));
    }

    #[test]
    fn when_matches_is_conjunction() {
        let r = row(&[
            ("status", GenericValue::String("deleted".into())),
            ("n", GenericValue::Int(3)),
        ]);
        let filters = vec![
            Filter::ValueOp(value_op(
                "status",
                FilterOp::Eq,
                FilterValue::Single("deleted".into()),
            )),
            Filter::ValueOp(value_op("n", FilterOp::Gte, FilterValue::Single("2".into()))),
        ];
        assert!(when_matches(&filters, &r));

        let null_check = vec![Filter::NullCheck(NullCheckFilter {
            column: col("missing"),
            op: NullOp::IsNull,
        })];
        assert!(when_matches(&null_check, &r));
    }

    #[test]
    fn soft_truthy_treats_present_or_true_as_deleted() {
        assert!(!soft_truthy(None));
        assert!(!soft_truthy(Some(&GenericValue::Null)));
        assert!(!soft_truthy(Some(&GenericValue::Bool(false))));
        assert!(soft_truthy(Some(&GenericValue::Bool(true))));
        // e.g. a non-null deleted_at timestamp decoded as text
        assert!(soft_truthy(Some(&GenericValue::String("2024-01-01T00:00:00Z".into()))));
    }
}
