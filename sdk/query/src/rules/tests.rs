//! The Rust half of every rule: what a condition renders, what it decides on
//! a document, and that the two forms of one condition (as written, and with
//! its negations pushed down for the index) decide the same. The `read_time`
//! live test in `dev/query-e2e` pins the same cases against OpenSearch.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::condition::{Cond, holds, positive, refuse_invalid, render};
use super::{
    Array, Computation, Condition, Optional, Required, RuleBool, RuleDate, RuleField, RuleKeyword,
    RuleNumber, Rules,
};
use crate::{
    AsQuery, Error, FlussoRoot, FlussoScope, Search, Segment, Sort, SortOrder, Sortable, kind,
};

const NOON: i64 = 1_794_830_400_000; // 2026-11-16T12:00:00Z
const NOON_TEXT: &str = "2026-11-16T12:00:00Z";

fn ends_at() -> RuleDate {
    RuleDate::at("ends_at")
}

fn signed_at() -> RuleDate<Optional> {
    RuleDate::at("signed_at")
}

fn state() -> RuleKeyword {
    RuleKeyword::at("state")
}

fn fee() -> RuleNumber<kind::Double> {
    RuleNumber::at("fee")
}

fn seats() -> RuleNumber<kind::Long, Optional> {
    RuleNumber::at("seats")
}

fn tags() -> RuleKeyword<Array> {
    RuleKeyword::at("tags")
}

fn query(condition: Condition) -> Value {
    condition.to_value()
}

mod conditions {
    use super::*;

    #[test]
    fn a_missing_value_fails_every_test_but_missing() {
        let empty = json!({ "seats": null });
        assert!(!seats().eq(3).holds(&empty));
        assert!(!seats().lt(3).holds(&empty));
        assert!(!seats().gte(3).holds(&empty));
        assert!(!seats().exists().holds(&empty));
        assert!(seats().missing().holds(&empty));
        assert!(seats().missing().holds(&json!({})));
        assert!(seats().ne(3).holds(&empty));
    }

    #[test]
    fn an_array_passes_when_any_element_does() {
        let doc = json!({ "tags": ["a", "b"], "lines": [{ "sku": "x" }, { "sku": ["y", null] }] });
        assert!(tags().eq("b").holds(&doc));
        assert!(!tags().eq("c").holds(&doc));
        assert!(tags().any_of(["c", "a"]).holds(&doc));
        // `ne` is "no element is it", as `must_not` reads an array.
        assert!(!tags().ne("a").holds(&doc));
        let sku = RuleKeyword::<Array>::at("lines.sku");
        assert!(sku.eq("y").holds(&doc));
        assert!(!sku.eq("z").holds(&doc));
        assert!(!tags().exists().holds(&json!({ "tags": [] })));
    }

    #[test]
    fn a_keyword_compares_exactly_as_the_document_spells_it() {
        assert!(state().eq("Active").holds(&json!({ "state": "Active" })));
        assert!(!state().eq("active").holds(&json!({ "state": "Active" })));
        // The index holds a number or a boolean sent to a keyword field as its text.
        assert!(state().eq("7").holds(&json!({ "state": 7 })));
        assert!(state().eq("true").holds(&json!({ "state": true })));
        assert_eq!(
            query(state().any_of(["a", "b"])),
            json!({ "terms": { "state": ["a", "b"] } })
        );
        assert_eq!(query(state().any_of([""; 0])), json!({ "match_none": {} }));
    }

    #[test]
    fn numbers_compare_as_numbers() {
        let doc = json!({ "fee": 600.0, "seats": "12" });
        assert!(fee().lte(600.0).holds(&doc));
        assert!(!fee().lt(600.0).holds(&doc));
        assert!(fee().between(599.5, 600.0).holds(&doc));
        assert!(fee().eq(600).holds(&doc));
        // A number spelled as text is read as the index coerces it.
        assert!(seats().gt(11).holds(&doc));
        assert!(fee().eq(600.5).holds(&json!({ "fee": "600.5" })));
        assert!(
            seats()
                .eq(i64::MIN)
                .holds(&json!({ "seats": -9_223_372_036_854_775_808.0 }))
        );
        assert_eq!(
            query(fee().between(1.5, 2)),
            json!({ "range": { "fee": { "gte": 1.5, "lte": 2.0 } } })
        );
    }

    #[test]
    fn a_value_the_index_coerces_is_read_the_same_way() {
        // A whole-number field stores a fraction without it, towards zero,
        // however the number is spelled.
        for (stored, whole) in [
            (json!(2.5), 2),
            (json!(-2.5), -2),
            (json!("7.9"), 7),
            (json!("-7.9"), -7),
            (json!("1e1"), 10),
            (json!(3.0), 3),
        ] {
            let doc = json!({ "seats": stored });
            assert!(seats().eq(whole).holds(&doc), "{doc}");
            assert!(seats().between(whole, whole).holds(&doc), "{doc}");
            assert!(!seats().gt(whole).holds(&doc), "{doc}");
            assert!(!seats().lt(whole).holds(&doc), "{doc}");
        }
        // A boolean field reads its two words, and an empty string as `false`.
        let renews = RuleBool::<Required>::at("auto_renew");
        for (stored, flag) in [
            (json!(true), true),
            (json!("true"), true),
            (json!(false), false),
            (json!("false"), false),
            (json!(""), false),
        ] {
            let doc = json!({ "auto_renew": stored });
            assert!(renews.eq(flag).holds(&doc), "{doc}");
            assert!(!renews.eq(!flag).holds(&doc), "{doc}");
        }
        // What the mapping would refuse is no value at all.
        for doc in [
            json!({ "seats": "many" }),
            json!({ "seats": 1e30 }),
            // One past the largest `long`.
            json!({ "seats": 9_223_372_036_854_775_808.0 }),
            json!({ "seats": true }),
            json!({ "fee": "NaN" }),
            json!({ "fee": [] }),
            json!({ "auto_renew": "yes" }),
            json!({ "auto_renew": 1 }),
        ] {
            assert!(!seats().exists().holds(&doc), "{doc}");
            assert!(!seats().gte(i64::MIN).holds(&doc), "{doc}");
            assert!(!fee().exists().holds(&doc), "{doc}");
            assert!(!renews.exists().holds(&doc), "{doc}");
        }
    }

    #[test]
    fn a_path_is_read_through_objects_only() {
        let country = RuleKeyword::<Optional>::at("billing.country");
        assert!(
            country
                .eq("IT")
                .holds(&json!({ "billing": { "country": "IT" } }))
        );
        assert!(country.missing().holds(&json!({ "billing": "IT" })));
        assert!(country.missing().holds(&json!({ "billing": null })));
        assert!(country.missing().holds(&json!("IT")));
    }

    #[test]
    fn a_number_that_is_no_number_is_refused_before_the_request_is_sent() {
        for broken in [fee().gt(f64::NAN), fee().eq(f64::INFINITY)] {
            assert!(!broken.holds(&json!({ "fee": 1.0 })));
            let Err(Error::ReadTimeField(reason)) = refuse_invalid(&query(broken)) else {
                panic!("the request must be refused");
            };
            assert!(
                reason.contains("is not a number value, so it cannot be compared with `fee`"),
                "{reason}"
            );
        }
    }

    #[test]
    fn a_timestamp_to_the_second_is_one_instant() {
        let at_noon = json!({ "ends_at": NOON_TEXT });
        let just_after = json!({ "ends_at": "2026-11-16T12:00:00.400Z" });
        assert!(ends_at().eq(NOON_TEXT).holds(&at_noon));
        assert!(!ends_at().eq(NOON_TEXT).holds(&just_after));
        assert!(ends_at().gt(NOON_TEXT).holds(&just_after));
        assert!(!ends_at().lte(NOON_TEXT).holds(&just_after));
        assert_eq!(
            query(ends_at().gt(NOON_TEXT)),
            json!({ "range": { "ends_at": { "gt": NOON, "format": "epoch_millis" } } })
        );
        assert_eq!(
            query(ends_at().eq(NOON_TEXT)),
            json!({ "range": { "ends_at": { "gte": NOON, "lte": NOON, "format": "epoch_millis" } } })
        );
    }

    #[test]
    fn a_bare_date_stands_for_its_whole_day() {
        let day_start = NOON - 12 * 3_600_000;
        let day_end = day_start + 86_400_000 - 1;
        let evening = json!({ "ends_at": "2026-11-16T21:30:00Z" });
        assert!(ends_at().eq("2026-11-16").holds(&evening));
        assert!(ends_at().lte("2026-11-16").holds(&evening));
        assert!(!ends_at().gt("2026-11-16").holds(&evening));
        assert!(!ends_at().lt("2026-11-16").holds(&evening));
        assert!(ends_at().gte("2026-11-16").holds(&evening));
        assert_eq!(
            query(ends_at().gt("2026-11-16")),
            json!({ "range": { "ends_at": { "gt": day_end, "format": "epoch_millis" } } })
        );
        assert_eq!(
            query(ends_at().lt("2026-11-16")),
            json!({ "range": { "ends_at": { "lt": day_start, "format": "epoch_millis" } } })
        );
        assert_eq!(
            query(ends_at().any_of(["2026-11-16", NOON_TEXT])),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "range": { "ends_at": { "gte": day_start, "lte": day_end, "format": "epoch_millis" } } },
                { "range": { "ends_at": { "gte": NOON, "lte": NOON, "format": "epoch_millis" } } },
            ] } })
        );
    }

    #[test]
    fn a_negated_range_on_a_required_field_is_the_opposite_range() {
        assert_eq!(
            query(ends_at().lt(NOON_TEXT).not()),
            json!({ "range": { "ends_at": { "gte": NOON, "format": "epoch_millis" } } })
        );
        assert_eq!(
            query(fee().between(1, 2).not()),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "range": { "fee": { "lt": 1.0 } } },
                { "range": { "fee": { "gt": 2.0 } } },
            ] } })
        );
    }

    #[test]
    fn a_negated_range_on_a_field_that_can_be_missing_stays_a_negation() {
        assert_eq!(
            query(signed_at().lt(NOON_TEXT).not()),
            json!({ "bool": { "must_not": [
                { "range": { "signed_at": { "lt": NOON, "format": "epoch_millis" } } },
            ] } })
        );
        // Missing is "not before noon", which the opposite range would lose.
        assert!(signed_at().lt(NOON_TEXT).not().holds(&json!({})));
    }

    #[test]
    fn and_or_not_combine_and_simplify() {
        let both = state().eq("a").and(fee().gt(1)).and(signed_at().missing());
        assert_eq!(
            query(both),
            json!({ "bool": {
                "filter": [{ "terms": { "state": ["a"] } }, { "range": { "fee": { "gt": 1.0 } } }],
                "must_not": [{ "exists": { "field": "signed_at" } }],
            } })
        );
        let either = state().eq("a").or(state().eq("b")).not();
        assert_eq!(
            query(either),
            json!({ "bool": { "must_not": [
                { "terms": { "state": ["a"] } },
                { "terms": { "state": ["b"] } },
            ] } })
        );
        assert_eq!(
            query(state().eq("a").not().not()),
            json!({ "terms": { "state": ["a"] } })
        );
    }

    #[test]
    fn no_values_match_nothing_and_the_negation_of_that_everything() {
        let nothing = || state().any_of([""; 0]);
        let everything = || nothing().not();
        let is_a = || state().eq("a");
        let doc = json!({ "state": "a" });
        assert_eq!(query(everything()), json!({ "match_all": {} }));
        assert!(!nothing().holds(&doc));
        assert!(everything().holds(&doc));
        assert!(everything().holds(&json!({})));
        // Either one disappears from a group it cannot change, and decides a
        // group it settles.
        assert_eq!(query(is_a().and(everything())), query(is_a()));
        assert_eq!(query(is_a().or(nothing())), query(is_a()));
        assert_eq!(query(is_a().and(nothing())), json!({ "match_none": {} }));
        assert_eq!(query(is_a().or(everything())), json!({ "match_all": {} }));
        assert_eq!(
            query(everything().and(everything())),
            json!({ "match_all": {} })
        );
        assert_eq!(query(nothing().or(nothing())), json!({ "match_none": {} }));
        assert!(is_a().or(everything()).holds(&json!({ "state": "b" })));
        assert!(!is_a().and(nothing()).holds(&doc));
    }

    #[test]
    fn text_that_is_not_a_date_is_refused_before_the_request_is_sent() {
        let broken = state().eq("a").and(ends_at().lt("next tuesday"));
        assert!(!broken.holds(&json!({ "state": "a", "ends_at": NOON_TEXT })));
        let body = json!({ "query": { "bool": { "filter": [query(broken)] } } });
        let Err(Error::ReadTimeField(reason)) = refuse_invalid(&body) else {
            panic!("the request must be refused");
        };
        assert!(
            reason.contains("`\"next tuesday\"` is not a date"),
            "{reason}"
        );
        assert!(reason.contains("`ends_at`"), "{reason}");
        assert!(refuse_invalid(&json!({ "query": { "match_all": {} } })).is_ok());
    }

    /// A small deterministic generator: the cases stay the same run to run.
    struct Dice(u64);

    impl Dice {
        fn roll(&mut self, sides: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % sides
        }
    }

    fn random_condition(dice: &mut Dice, depth: u32) -> Condition {
        if depth > 0 && dice.roll(3) == 0 {
            let left = random_condition(dice, depth - 1);
            let right = random_condition(dice, depth - 1);
            return match dice.roll(3) {
                0 => left.and(right),
                1 => left.or(right),
                _ => left.not(),
            };
        }
        let day = format!("2026-11-{:02}", 14 + dice.roll(5));
        let number = dice.roll(5) as i32;
        match dice.roll(10) {
            0 => ends_at().lt(day),
            1 => ends_at().gte(day),
            2 => ends_at().between("2026-11-15", day),
            3 => fee().lte(number),
            4 => fee().gt(number),
            5 => state().eq(["a", "b", "c"][dice.roll(3) as usize]),
            6 => signed_at().lt(day),
            7 => signed_at().missing(),
            8 => seats().between(1, number),
            _ => tags().any_of(["a", "b"]),
        }
    }

    fn random_document(dice: &mut Dice) -> Value {
        let state = ["a", "b", "c", "d"][dice.roll(4) as usize];
        let mut doc = json!({
            "ends_at": format!("2026-11-{:02}T{:02}:00:00Z", 13 + dice.roll(7), dice.roll(24)),
            "fee": dice.roll(6) as f64 - 0.5 * dice.roll(2) as f64,
            "state": state,
        });
        if dice.roll(2) == 0 {
            doc["signed_at"] = json!(format!("2026-11-{:02}", 13 + dice.roll(7)));
        }
        if dice.roll(3) > 0 {
            doc["seats"] = json!(dice.roll(6));
        }
        match dice.roll(3) {
            0 => {}
            1 => doc["tags"] = json!([]),
            _ => doc["tags"] = json!([if dice.roll(2) == 0 { "a" } else { "c" }, "z"]),
        }
        doc
    }

    #[test]
    fn pushing_negations_down_never_changes_what_a_condition_decides() {
        let mut dice = Dice(7);
        for _ in 0..2_000 {
            let condition = random_condition(&mut dice, 3);
            let written = condition.0.clone().unwrap();
            let sent = positive(&written);
            for _ in 0..20 {
                let doc = random_document(&mut dice);
                assert_eq!(
                    holds(&written, &doc),
                    holds(&sent, &doc),
                    "{} on {doc}",
                    render(&sent)
                );
            }
        }
    }

    #[test]
    fn the_sent_form_keeps_no_negated_group() {
        fn only_tests_are_negated(cond: &Cond) -> bool {
            match cond {
                Cond::Not(inner) => matches!(**inner, Cond::Test(_)),
                Cond::And(parts) | Cond::Or(parts) => parts.iter().all(only_tests_are_negated),
                _ => true,
            }
        }
        let mut dice = Dice(11);
        for _ in 0..500 {
            let written = random_condition(&mut dice, 4).0.unwrap();
            assert!(only_tests_are_negated(&positive(&written)));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Draft,
    Closed,
    Scheduled,
    Active,
    Suspended,
}

impl crate::FlussoValue<kind::Keyword> for State {}

struct Viewing {
    now: &'static str,
}

/// Draft until signed, closed once ended or terminated, else whatever is stored.
fn visible_state(cx: &Viewing) -> Rules<State> {
    Rules::when(signed_at().missing())
        .then(State::Draft)
        .when(state().eq("terminated").or(ends_at().lt(cx.now)))
        .then(State::Closed)
        .otherwise(state())
}

const CX: Viewing = Viewing { now: NOON_TEXT };

fn before_noon() -> Value {
    json!({ "range": { "ends_at": { "lt": NOON, "format": "epoch_millis" } } })
}

fn from_noon() -> Value {
    json!({ "range": { "ends_at": { "gte": NOON, "format": "epoch_millis" } } })
}

fn signed() -> Value {
    json!({ "exists": { "field": "signed_at" } })
}

fn terminated() -> Value {
    json!({ "terms": { "state": ["terminated"] } })
}

mod fields {
    use super::*;

    fn field() -> RuleField<State> {
        RuleField::new(visible_state(&CX), &[])
    }

    fn value(source: Value) -> Value {
        let mut source = source;
        Computation::with(&CX)
            .rules("visible", visible_state)
            .finish()
            .apply(&mut source)
            .unwrap();
        source["visible"].take()
    }

    #[test]
    fn the_first_arm_that_holds_decides_the_value() {
        let later = "2026-12-01T00:00:00Z";
        let earlier = "2026-11-01T00:00:00Z";
        assert_eq!(
            value(json!({ "state": "terminated", "ends_at": earlier })),
            "draft"
        );
        assert_eq!(
            value(json!({ "signed_at": "2026-01-01", "state": "terminated", "ends_at": later })),
            "closed"
        );
        assert_eq!(
            value(json!({ "signed_at": "2026-01-01", "state": "active", "ends_at": earlier })),
            "closed"
        );
        assert_eq!(
            value(json!({ "signed_at": "2026-01-01", "state": "suspended", "ends_at": later })),
            "suspended"
        );
    }

    /// Fixed values only: draft until signed, closed once ended, else open.
    fn phase() -> RuleField<&'static str> {
        RuleField::new(
            Rules::when(signed_at().missing())
                .then("draft")
                .when(ends_at().lt(CX.now))
                .then("closed")
                .otherwise("open"),
            &[],
        )
    }

    #[test]
    fn a_fixed_value_is_selected_by_its_arm_and_no_earlier_one() {
        assert_eq!(
            phase().eq("draft").to_value(),
            json!({ "bool": { "must_not": [signed()] } })
        );
        assert_eq!(
            phase().eq("closed").to_value(),
            json!({ "bool": { "filter": [signed(), before_noon()] } })
        );
        assert_eq!(
            phase().eq("open").to_value(),
            json!({ "bool": { "filter": [signed(), from_noon()] } })
        );
        assert_eq!(
            phase().eq("archived").to_value(),
            json!({ "match_none": {} })
        );
    }

    #[test]
    fn several_values_are_the_union_of_their_arms() {
        assert_eq!(
            phase().any_of(["draft", "open"]).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "must_not": [signed()] } },
                { "bool": { "filter": [signed(), from_noon()] } },
            ] } })
        );
        assert_eq!(phase().any_of([]).to_value(), json!({ "match_none": {} }));
    }

    #[test]
    fn ne_is_everything_the_value_is_not() {
        assert_eq!(phase().ne("draft").to_value(), signed());
    }

    #[test]
    fn a_stored_outcome_adds_its_own_test_on_the_field() {
        assert_eq!(
            field().eq(State::Suspended).to_value(),
            json!({ "bool": {
                "filter": [signed(), from_noon(), { "terms": { "state": ["suspended"] } }],
                "must_not": [terminated()],
            } })
        );
        // The stored field can hold a value the rules also yield, so that
        // value is selected through both.
        assert_eq!(
            field().eq(State::Draft).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "must_not": [signed()] } },
                { "bool": {
                    "filter": [signed(), from_noon(), { "terms": { "state": ["draft"] } }],
                    "must_not": [terminated()],
                } },
            ] } })
        );
    }

    #[test]
    fn an_ordered_comparison_uses_the_types_own_order() {
        // `Draft < Closed < Scheduled`: below `Scheduled` are both fixed
        // values. The index would order the stored keyword alphabetically, so
        // it is matched against the known values that are below instead.
        let ordered = RuleField::new(
            visible_state(&CX),
            &["active", "suspended", "scheduled", "closed", "draft"],
        );
        assert_eq!(
            ordered.lt(State::Scheduled).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "must_not": [signed()] } },
                { "bool": { "filter": [
                    signed(),
                    { "bool": { "minimum_should_match": 1, "should": [terminated(), before_noon()] } },
                ] } },
                { "bool": {
                    "filter": [signed(), from_noon(), { "terms": { "state": ["closed", "draft"] } }],
                    "must_not": [terminated()],
                } },
            ] } })
        );
    }

    #[test]
    fn an_ordered_comparison_on_a_stored_keyword_needs_the_value_order() {
        let only_stored: Rules<String> = Rules::when(signed_at().missing())
            .then(state())
            .otherwise(state());
        let Err(Error::ReadTimeField(reason)) = refuse_invalid(
            &RuleField::new(only_stored, &[])
                .lt("m".to_owned())
                .to_value(),
        ) else {
            panic!("the comparison must be refused");
        };
        assert!(
            reason.contains("list the values in order with `Rules::order`"),
            "{reason}"
        );
    }

    #[test]
    fn an_ordered_comparison_skips_a_known_value_the_type_cannot_hold() {
        // `archived` is declared by the schema and is no `State`: a document
        // holding it does not decode, so no comparison selects it.
        let ordered = RuleField::new(visible_state(&CX), &["archived", "active", "suspended"]);
        let below = ordered.lte(State::Suspended).to_value().to_string();
        assert!(
            below.contains(r#"{"terms":{"state":["active","suspended","draft","closed"]}}"#),
            "{below}"
        );
        assert!(!below.contains("archived"), "{below}");
    }

    #[test]
    fn an_arm_that_never_holds_yields_nothing() {
        let rules = || {
            Rules::when(state().any_of([""; 0]))
                .then("never")
                .when(state().eq("a"))
                .then("first")
                .otherwise("rest")
        };
        let field = RuleField::new(rules(), &[]);
        assert_eq!(field.eq("never").to_value(), json!({ "match_none": {} }));
        assert_eq!(
            field.eq("first").to_value(),
            json!({ "terms": { "state": ["a"] } })
        );
        assert_eq!(
            field.eq("rest").to_value(),
            json!({ "bool": { "must_not": [{ "terms": { "state": ["a"] } }] } })
        );
        let computation = Computation::with(&()).rules("which", |()| rules()).finish();
        let mut source = json!({ "state": "a" });
        computation.apply(&mut source).unwrap();
        assert_eq!(source["which"], "first");
    }

    fn fee_rules(cx: &Viewing) -> Rules<f64> {
        Rules::when(RuleDate::<Optional>::at("discount_until").gt(cx.now))
            .then(RuleNumber::<kind::Double>::at("discounted_fee"))
            .otherwise(fee())
    }

    #[test]
    fn a_number_picked_from_two_stored_fields_filters_on_whichever_applies() {
        let current = RuleField::new(fee_rules(&CX), &[]);
        let discounted =
            json!({ "range": { "discount_until": { "gt": NOON, "format": "epoch_millis" } } });
        assert_eq!(
            current.lte(600.0).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "filter": [discounted, { "range": { "discounted_fee": { "lte": 600.0 } } }] } },
                { "bool": {
                    "filter": [{ "range": { "fee": { "lte": 600.0 } } }],
                    "must_not": [discounted],
                } },
            ] } })
        );
        assert_eq!(
            current.between(1.0, 2.0).to_value()["bool"]["filter"][0]["bool"]["should"][0]["bool"]
                ["filter"][1],
            json!({ "range": { "discounted_fee": { "gte": 1.0 } } })
        );
    }

    fn optional_fee(cx: &Viewing) -> Rules<Option<f64>> {
        Rules::when(signed_at().missing())
            .then(None)
            .when(RuleDate::<Optional>::at("discount_until").gt(cx.now))
            .then(RuleNumber::<kind::Double, Optional>::at("discounted_fee"))
            .otherwise(fee())
    }

    #[test]
    fn no_value_is_matched_the_same_fixed_or_stored() {
        let current = RuleField::new(optional_fee(&CX), &[]);
        let discounted =
            json!({ "range": { "discount_until": { "gt": NOON, "format": "epoch_millis" } } });
        // `None` is the fixed `None`, and a stored field holding nothing.
        assert_eq!(
            current.eq(None).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "must_not": [signed()] } },
                { "bool": {
                    "filter": [signed(), discounted],
                    "must_not": [{ "exists": { "field": "discounted_fee" } }],
                } },
                { "bool": {
                    "filter": [signed()],
                    "must_not": [discounted, { "exists": { "field": "fee" } }],
                } },
            ] } })
        );
        // No value has no order: neither the fixed `None` nor a missing
        // stored field is "below 600".
        assert_eq!(
            current.lt(Some(600.0)).to_value(),
            json!({ "bool": { "minimum_should_match": 1, "should": [
                { "bool": { "filter": [
                    signed(),
                    discounted,
                    { "range": { "discounted_fee": { "lt": 600.0 } } },
                ] } },
                { "bool": {
                    "filter": [signed(), { "range": { "fee": { "lt": 600.0 } } }],
                    "must_not": [discounted],
                } },
            ] } })
        );
        assert!(refuse_invalid(&current.lt(None).to_value()).is_err());
    }

    #[test]
    fn a_stored_array_is_yielded_whole() {
        let rules: Rules<Vec<String>> = Rules::when(signed_at().missing())
            .then(Vec::new())
            .otherwise(RuleKeyword::<Array>::at("lines.sku"));
        let computation = Computation::with(&()).rules("skus", |()| rules).finish();
        let mut source = json!({
            "signed_at": "2026-01-01",
            "lines": [{ "sku": "x" }, { "sku": ["y", null] }, {}],
        });
        computation.apply(&mut source).unwrap();
        assert_eq!(source["skus"], json!(["x", "y"]));
    }

    #[test]
    fn a_document_missing_a_field_the_schema_requires_gets_no_value() {
        // `ends_at` is declared always present, so "not before noon" is sent
        // as "from noon on". A document without it matches neither, so no
        // filter finds it under any value, and it must not show one.
        let Err(Error::ReadTimeField(reason)) = Computation::with(&CX)
            .rules("visible", visible_state)
            .finish()
            .apply(&mut json!({ "signed_at": "2026-01-01", "state": "active" }))
        else {
            panic!("the document must be refused");
        };
        assert!(
            reason.starts_with("`visible`: no rule decides this document"),
            "{reason}"
        );
    }

    #[test]
    fn a_rule_that_cannot_be_evaluated_fails_the_filter_and_the_value() {
        let broken = |cx: &Viewing| {
            Rules::when(ends_at().lt(cx.now))
                .then("closed")
                .otherwise("open")
        };
        let cx = Viewing { now: "soon" };
        let field = RuleField::new(broken(&cx), &[]);
        assert!(refuse_invalid(&field.eq("open").to_value()).is_err());
        let error = Computation::with(&cx)
            .rules("state", broken)
            .finish()
            .apply(&mut json!({ "ends_at": NOON_TEXT }))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("`state`: `\"soon\"` is not a date"),
            "{error}"
        );
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Contract {}

impl FlussoScope for Contract {
    const PATH: &'static [Segment] = &[];
}

impl FlussoRoot for Contract {
    type Context = ();
    const INDEX: &'static str = "contracts";
    const SCHEMA_HASH: &'static str = "xxxxxx";
}

mod sorting {
    use super::*;

    fn by_state() -> RuleField<State> {
        RuleField::new(
            visible_state(&CX).order([State::Active, State::Draft, State::Closed]),
            &[],
        )
    }

    fn by_mine() -> RuleField<bool> {
        RuleField::new(
            Rules::when(RuleKeyword::<Required>::at("owner_id").eq("u1"))
                .then(true)
                .otherwise(false),
            &[],
        )
    }

    fn mine() -> Value {
        json!({ "terms": { "owner_id": ["u1"] } })
    }

    #[test]
    fn the_score_becomes_the_rank_of_the_first_arm_that_holds() {
        let body = Contract::query().sort(by_state().asc()).body();
        assert_eq!(body["sort"], json!([{ "_score": { "order": "asc" } }]));
        assert_eq!(
            body["query"],
            json!({ "function_score": {
                "query": { "match_all": {} },
                "score_mode": "first",
                "boost_mode": "replace",
                "functions": [
                    { "filter": { "bool": { "must_not": [signed()] } }, "weight": 2 },
                    { "filter": { "bool": { "minimum_should_match": 1, "should": [terminated(), before_noon()] } }, "weight": 3 },
                    { "filter": { "terms": { "state": ["active"] } }, "weight": 1 },
                    { "filter": { "terms": { "state": ["draft"] } }, "weight": 2 },
                    { "filter": { "terms": { "state": ["closed"] } }, "weight": 3 },
                    { "filter": { "match_all": {} }, "weight": 4 },
                ],
            } })
        );
    }

    #[test]
    fn the_order_is_the_schema_variants_then_what_else_the_rules_name() {
        let weights = |field: RuleField<State>| -> Vec<u64> {
            Contract::query().sort(field.asc()).body()["query"]["function_score"]["functions"]
                .as_array()
                .unwrap()
                .iter()
                .map(|function| function["weight"].as_u64().unwrap())
                .collect()
        };
        // The stored enum declares only what it stores. The rules add `draft`
        // and `closed`, which rank after it: active 1, suspended 2, draft 3,
        // closed 4.
        assert_eq!(
            weights(RuleField::new(visible_state(&CX), &["active", "suspended"])),
            [3, 4, 1, 2, 3, 4, 5]
        );
        // A value the schema already declares keeps its declared place.
        assert_eq!(
            weights(RuleField::new(
                visible_state(&CX),
                &["closed", "draft", "active"]
            )),
            [2, 1, 1, 2, 3, 4]
        );
        // With no declared variants, the order the rules name: `draft`, `closed`.
        assert_eq!(
            weights(RuleField::new(visible_state(&CX), &[])),
            [1, 2, 1, 2, 3]
        );
    }

    #[test]
    fn a_rule_sort_turned_around_sorts_the_other_way() {
        let body = Contract::query().sort(by_mine().asc().desc()).body();
        assert_eq!(body["sort"], json!([{ "_score": { "order": "desc" } }]));
        let body = Contract::query().sort(by_mine().desc().asc()).body();
        assert_eq!(body["sort"], json!([{ "_score": { "order": "asc" } }]));
    }

    #[test]
    fn adjacent_rule_sorts_pack_into_one_score() {
        let body = Contract::query()
            .sort(by_mine().desc())
            .sort(by_state().asc())
            .sort(Sort::field::<Contract>("ends_at", SortOrder::Asc))
            .body();
        assert_eq!(
            body["sort"],
            json!([{ "_score": { "order": "desc" } }, { "ends_at": { "order": "asc" } }])
        );
        let outer = &body["query"]["function_score"];
        assert_eq!(outer["boost_mode"], "sum");
        // The second sorts the other way round, so its ranks are reversed: 4 → 1.
        assert_eq!(outer["functions"][0]["weight"], 3);
        assert_eq!(outer["functions"][5]["weight"], 1);
        let inner = &outer["query"]["function_score"];
        assert_eq!(inner["boost_mode"], "replace");
        // `true` ranks 1 of 2 values (+1 for "anything else"), scaled past the second sort.
        assert_eq!(
            inner["functions"],
            json!([
                { "filter": mine(), "weight": 5 },
                { "filter": { "match_all": {} }, "weight": 10 },
            ])
        );
    }

    #[test]
    fn the_rank_wraps_a_nested_projection_so_it_cannot_add_to_the_score() {
        let projection = crate::Nested::<crate::Root, Contract>::at("invoices").project();
        let body = Contract::query()
            .filter_nested(projection)
            .sort(by_mine().asc())
            .body();
        assert!(body["query"]["function_score"]["query"]["bool"]["should"].is_array());
    }

    fn refusal(search: &Search<Contract>) -> String {
        match refuse_invalid(&search.body()) {
            Err(Error::ReadTimeField(reason)) => reason,
            other => panic!("the search must be refused, got {other:?}"),
        }
    }

    #[test]
    fn rule_sorts_separated_by_another_key_are_refused() {
        let search = Contract::query()
            .sort(by_mine().asc())
            .sort(Sort::field::<Contract>("ends_at", SortOrder::Asc))
            .sort(by_state().asc());
        assert!(refusal(&search).contains("separated by another sort key"));
        assert!(refuse_invalid(&search.ids_body()).is_err());
    }

    #[test]
    fn a_rule_sort_beside_relevance_or_min_score_is_refused() {
        let with_score = Contract::query().sort(by_mine().asc()).sort(Sort::score());
        assert!(refusal(&with_score).contains("relevance sort"));
        let with_floor = Contract::query().sort(by_mine().asc()).min_score(0.5);
        assert!(refusal(&with_floor).contains("`min_score`"));
    }

    #[test]
    fn more_combinations_than_a_score_holds_are_refused() {
        let wide = |seed: u32| {
            let order: Vec<u32> = (0..300).map(|value| value + seed).collect();
            RuleField::new(
                Rules::when(state().eq("a"))
                    .then(seed)
                    .otherwise(seed + 1)
                    .order(order),
                &[],
            )
        };
        let search = Contract::query()
            .sort(wide(0).asc())
            .sort(wide(1_000).asc())
            .sort(wide(2_000).asc());
        assert!(refusal(&search).contains("more value combinations"));
        let two = Contract::query()
            .sort(wide(0).asc())
            .sort(wide(1_000).asc());
        assert!(refuse_invalid(&two.body()).is_ok());
    }

    #[test]
    fn one_score_keeps_two_to_the_24th_combinations_apart_and_no_more() {
        // `values` known values rank 1 to `values`, anything else one past
        // them, and the next sort starts above that: `values + 2` steps.
        let ranked = |values: u32, seed: u32| {
            let order: Vec<u32> = (0..values).map(|value| value + seed).collect();
            RuleField::new(
                Rules::when(state().eq("a"))
                    .then(seed)
                    .otherwise(seed + 1)
                    .order(order),
                &[],
            )
        };
        let fits = Contract::query()
            .sort(ranked(4_094, 0).asc())
            .sort(ranked(4_094, 10_000).asc());
        assert!(refuse_invalid(&fits.body()).is_ok());
        let over = Contract::query()
            .sort(ranked(4_094, 0).asc())
            .sort(ranked(4_095, 10_000).asc());
        assert!(refusal(&over).contains("more value combinations"));
    }

    #[test]
    fn a_sort_builder_keeps_two_rule_sorts_apart() {
        let sorts = crate::SortBuilder::new()
            .by(by_mine(), SortOrder::Asc)
            .by(by_state(), SortOrder::Asc)
            .by(by_mine(), SortOrder::Desc)
            .build();
        assert_eq!(sorts.len(), 2);
    }

    #[test]
    fn only_a_search_of_its_own_root_can_run_a_rule_sort() {
        assert!(refuse_invalid(&by_mine().asc().to_value()).is_err());
    }
}

mod computing {
    use super::*;

    #[derive(Clone)]
    struct Rate(f64);

    #[derive(Deserialize)]
    struct Priced {
        fee: f64,
    }

    fn discounted(_: &Rate) -> Rules<bool> {
        Rules::when(RuleNumber::<kind::Double>::at("fee").lt(100))
            .then(true)
            .otherwise(false)
    }

    fn converted(priced: &Priced, rate: &Rate) -> f64 {
        priced.fee * rate.0
    }

    #[derive(Deserialize)]
    struct Converted {
        converted: f64,
        discounted: bool,
    }

    fn label(doc: &Converted, _: &Rate) -> String {
        format!(
            "{:.2}{}",
            doc.converted,
            if doc.discounted { " (discounted)" } else { "" }
        )
    }

    fn computation() -> Computation {
        Computation::with(&Rate(2.0))
            .rules("discounted", discounted)
            .computed("converted", converted)
            .computed("label", label)
            .finish()
    }

    #[test]
    fn computed_fields_read_the_rules_and_the_computed_fields_before_them() {
        let mut source = json!({ "fee": 40.25 });
        computation().apply(&mut source).unwrap();
        assert_eq!(
            source,
            json!({ "fee": 40.25, "discounted": true, "converted": 80.5, "label": "80.50 (discounted)" })
        );
    }

    #[test]
    fn every_hit_of_a_response_is_computed() {
        let mut response = json!({ "hits": { "hits": [
            { "_id": "a", "_source": { "fee": 10.0 } },
            { "_id": "b", "_source": { "fee": 500.0 } },
            { "_id": "c" },
        ] } });
        computation().apply_response(&mut response).unwrap();
        assert_eq!(
            response["hits"]["hits"][0]["_source"]["label"],
            "20.00 (discounted)"
        );
        assert_eq!(response["hits"]["hits"][1]["_source"]["label"], "1000.00");
        assert_eq!(response["hits"]["hits"][2], json!({ "_id": "c" }));
    }

    #[test]
    fn a_computed_input_that_does_not_read_names_its_field() {
        let error = Computation::with(&Rate(2.0))
            .computed("converted", converted)
            .finish()
            .apply(&mut json!({ "fee": "many" }))
            .unwrap_err();
        let text = error.to_string();
        assert!(
            text.starts_with("read-time field: `converted`: its input does not read"),
            "{text}"
        );
    }

    #[test]
    fn a_computed_value_that_does_not_serialize_names_its_field() {
        fn pairs(_: &Value, _: &Rate) -> std::collections::BTreeMap<(u8, u8), u8> {
            std::collections::BTreeMap::from([((1, 2), 3)])
        }
        let error = Computation::with(&Rate(2.0))
            .computed("pairs", pairs)
            .finish()
            .apply(&mut json!({}))
            .unwrap_err();
        let text = error.to_string();
        assert!(
            text.starts_with("read-time field: `pairs`: its value does not serialize"),
            "{text}"
        );
    }

    #[test]
    fn a_rule_that_cannot_be_evaluated_is_known_before_any_document() {
        let broken = |_: &Rate| {
            Rules::when(RuleDate::<Required>::at("ends_at").lt("soon"))
                .then(true)
                .otherwise(false)
        };
        let refused = Computation::with(&Rate(1.0))
            .rules("ends_soon", broken)
            .finish();
        let Err(Error::ReadTimeField(reason)) = refused.check() else {
            panic!("the computation must be refused");
        };
        assert!(reason.starts_with("`ends_soon`: "), "{reason}");
        assert!(computation().check().is_ok());
    }

    #[test]
    fn a_root_without_read_time_fields_computes_nothing() {
        assert!(Computation::none().is_empty());
        assert!(Computation::with(&()).finish().is_empty());
        let rate = Rate(2.0);
        assert!(!computation().is_empty());
        assert!(
            !Computation::with(&rate)
                .rules("discounted", discounted)
                .finish()
                .is_empty()
        );
        assert!(
            !Computation::with(&rate)
                .computed("converted", converted)
                .finish()
                .is_empty()
        );
        let mut source = json!({ "fee": 1 });
        Computation::none().apply(&mut source).unwrap();
        assert_eq!(source, json!({ "fee": 1 }));
    }
}

/// Every read path of a root with read-time fields, against a local server
/// standing in for OpenSearch: the fields are on whatever comes back, and
/// what cannot be evaluated is refused with nothing sent.
mod reading {
    use std::sync::{Arc, Mutex};

    use futures::TryStreamExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpListener, TcpStream};

    use super::*;
    use crate::Client;

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    #[derive(Clone)]
    struct Clock {
        now: &'static str,
    }

    #[derive(Debug, Clone, PartialEq, Deserialize)]
    struct Listed {
        id: String,
        state: String,
        label: String,
    }

    impl FlussoScope for Listed {
        const PATH: &'static [Segment] = &[];
    }

    impl FlussoRoot for Listed {
        type Context = Clock;
        const INDEX: &'static str = "contracts";
        const SCHEMA_HASH: &'static str = "xxxxxx";

        fn computation(cx: &Clock) -> Computation {
            Computation::with(cx)
                .rules("state", state_at)
                .computed("label", label)
                .finish()
        }
    }

    fn state_at(cx: &Clock) -> Rules<String> {
        Rules::when(ends_at().lt(cx.now))
            .then("closed".to_owned())
            .otherwise("open".to_owned())
    }

    #[derive(Deserialize)]
    struct Shown {
        id: String,
        state: String,
    }

    fn label(shown: &Shown, cx: &Clock) -> String {
        format!("{} is {} at {}", shown.id, shown.state, cx.now)
    }

    fn listed(id: &str, state: &str) -> Listed {
        Listed {
            id: id.to_owned(),
            state: state.to_owned(),
            label: format!("{id} is {state} at {NOON_TEXT}"),
        }
    }

    /// What the stored documents become at noon.
    fn at_noon() -> Vec<Listed> {
        vec![listed("a", "closed"), listed("b", "open")]
    }

    const NOON_CLOCK: Clock = Clock { now: NOON_TEXT };
    /// A clock no rule can compare a date with.
    const NO_CLOCK: Clock = Clock { now: "soon" };

    const STORED_A: &str = r#"{"id":"a","ends_at":"2026-11-01T00:00:00Z"}"#;
    const STORED_B: &str = r#"{"id":"b","ends_at":"2026-12-01T00:00:00Z"}"#;

    /// A search response holding both documents, reporting `total` matches.
    fn hits(total: usize) -> String {
        format!(
            r#"{{"hits":{{"total":{{"value":{total},"relation":"eq"}},"hits":[
                {{"_id":"a","_score":1.0,"sort":[1.0,"a"],"_source":{STORED_A}}},
                {{"_id":"b","_score":1.0,"sort":[1.0,"b"],"_source":{STORED_B}}}]}}}}"#
        )
    }

    type Seen = Arc<Mutex<Vec<String>>>;

    /// A server answering each request by its path with the two stored
    /// documents, recording every request line.
    async fn stub() -> std::io::Result<(Client, Seen)> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let base = format!("http://{}", listener.local_addr()?);
        let seen = Seen::default();
        let recorded = Arc::clone(&seen);
        tokio::spawn(async move {
            while let Ok((socket, _)) = listener.accept().await {
                tokio::spawn(answer(socket, Arc::clone(&recorded)));
            }
        });
        let client = Client::connect(&base).map_err(std::io::Error::other)?;
        Ok((client, seen))
    }

    async fn answer(mut socket: TcpStream, seen: Seen) {
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            let text = String::from_utf8_lossy(&request);
            if let Some((head, body)) = text.split_once("\r\n\r\n") {
                let expected = head
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")?
                            .trim()
                            .parse()
                            .ok()
                    })
                    .unwrap_or(0);
                if body.len() >= expected {
                    break;
                }
            }
            match socket.read(&mut chunk).await {
                Ok(read) if read > 0 => request.extend_from_slice(&chunk[..read]),
                _ => break,
            }
        }
        let request = String::from_utf8_lossy(&request).into_owned();
        let line = request.lines().next().unwrap_or_default().to_owned();
        let body = if line.contains("/_msearch") {
            // One response per search (a header line and a body line each),
            // the n-th reporting 10 + n matches.
            let sent = request.split_once("\r\n\r\n").map_or("", |(_, body)| body);
            let searches = sent.lines().filter(|line| !line.is_empty()).count() / 2;
            let responses: Vec<String> = (0..searches).map(|at| hits(10 + at)).collect();
            format!(r#"{{"responses":[{}]}}"#, responses.join(","))
        } else if line.contains("/_search/point_in_time") {
            r#"{"pit_id":"pinned"}"#.to_owned()
        } else if line.contains("/_doc/") {
            format!(r#"{{"found":true,"_source":{STORED_A}}}"#)
        } else if line.contains("/_count") {
            r#"{"count":2}"#.to_owned()
        } else {
            hits(2)
        };
        seen.lock().unwrap().push(line);
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = socket.write_all(response.as_bytes()).await;
    }

    fn sources(page: crate::SearchResponse<Listed>) -> Vec<Listed> {
        page.hits.into_iter().map(|hit| hit.source).collect()
    }

    fn refused<T>(result: crate::Result<T>) -> String {
        match result {
            Err(Error::ReadTimeField(reason)) => reason,
            Err(other) => panic!("refused for another reason: {other}"),
            Ok(_) => panic!("the request must be refused"),
        }
    }

    #[tokio::test]
    async fn every_read_path_decides_the_read_time_fields() -> Result {
        let (client, seen) = stub().await?;
        let search = || Listed::query_in(&NOON_CLOCK).size(5);

        assert_eq!(sources(search().send(&client).await?), at_noon());
        assert_eq!(
            Listed::get_in(&NOON_CLOCK, &client, "a").await?,
            Some(listed("a", "closed"))
        );
        for page in client.msearch_all(&[search(), search()]).await? {
            assert_eq!(sources(page), at_noon());
        }
        let (first, second) = client.msearch((&search(), &search())).await?;
        assert_eq!(sources(first), at_noon());
        assert_eq!(sources(second), at_noon());
        assert_eq!(
            sources(search().cursor(None).send(&client).await?),
            at_noon()
        );
        let walked: Vec<Listed> = search()
            .cursor(None)
            .hits(&client)
            .map_ok(|hit| hit.source)
            .try_collect()
            .await?;
        assert_eq!(walked, at_noon());
        let paged: Vec<crate::SearchResponse<Listed>> =
            search().cursor(None).pages(&client).try_collect().await?;
        assert_eq!(
            paged.into_iter().flat_map(sources).collect::<Vec<_>>(),
            at_noon()
        );

        // A pinned walk opens a point-in-time, reads through it, and is done.
        let pinned = search().cursor(None).point_in_time().send(&client).await?;
        assert_eq!(sources(pinned), at_noon());

        let seen = seen.lock().unwrap();
        assert!(seen.len() >= 9, "{seen:?}");
        assert!(seen[7].contains("/_search/point_in_time"), "{seen:?}");
        Ok(())
    }

    #[tokio::test]
    async fn the_same_documents_read_at_another_moment_show_other_values() -> Result {
        let (client, _seen) = stub().await?;
        let earlier = Clock {
            now: "2026-10-01T00:00:00Z",
        };
        let states: Vec<String> = sources(Listed::query_in(&earlier).send(&client).await?)
            .into_iter()
            .map(|listed| listed.state)
            .collect();
        assert_eq!(states, ["open", "open"]);
        Ok(())
    }

    #[tokio::test]
    async fn counts_ask_once_for_every_value_in_order() -> Result {
        let (client, seen) = stub().await?;
        let state = RuleField::new(
            state_at(&NOON_CLOCK).order(["open", "closed"].map(str::to_owned)),
            &[],
        );
        let counts = state
            .counts(&Listed::query_in(&NOON_CLOCK), &client)
            .await?;
        assert_eq!(counts, [("open".to_owned(), 10), ("closed".to_owned(), 11)]);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "{seen:?}");
        assert!(seen[0].contains("/_msearch"), "{seen:?}");
        Ok(())
    }

    #[tokio::test]
    async fn a_rule_list_that_cannot_be_evaluated_sends_nothing() -> Result {
        let (client, seen) = stub().await?;
        let search = || Listed::query_in(&NO_CLOCK).size(5);

        let reasons = [
            refused(search().send(&client).await),
            refused(search().ids(&client).await),
            refused(search().count(&client).await),
            refused(Listed::get_in(&NO_CLOCK, &client, "a").await),
            refused(client.msearch_all(&[search()]).await),
            refused(client.msearch((&search(), &search())).await),
            refused(search().cursor(None).send(&client).await),
            refused(search().cursor(None).ids(&client).await),
            refused(search().cursor(None).point_in_time().send(&client).await),
            refused(
                search()
                    .cursor(None)
                    .hits(&client)
                    .try_collect::<Vec<_>>()
                    .await,
            ),
            refused(
                search()
                    .cursor(None)
                    .id_stream(&client)
                    .try_collect::<Vec<_>>()
                    .await,
            ),
        ];
        for reason in reasons {
            assert!(
                reason.starts_with("`state`: `\"soon\"` is not a date"),
                "{reason}"
            );
        }
        assert_eq!(*seen.lock().unwrap(), Vec::<String>::new());
        Ok(())
    }

    #[tokio::test]
    async fn a_filter_or_sort_that_cannot_be_evaluated_sends_nothing() -> Result {
        let (client, seen) = stub().await?;
        let state = |cx: &Clock| RuleField::new(state_at(cx), &[]);
        // The root's own fields are sound; the filter and the sort are built
        // from a clock that is not one.
        let filtered = || {
            Listed::query_in(&NOON_CLOCK)
                .size(5)
                .filter(state(&NO_CLOCK).eq("open".to_owned()))
        };
        let sorted = || {
            Listed::query_in(&NOON_CLOCK)
                .size(5)
                .sort(state(&NO_CLOCK).asc())
        };
        let searches: [&dyn Fn() -> Search<Listed>; 2] = [&filtered, &sorted];

        for search in searches {
            let reasons = [
                refused(search().send(&client).await),
                refused(search().ids(&client).await),
                refused(client.msearch_all(&[search()]).await),
                refused(search().cursor(None).send(&client).await),
                refused(search().cursor(None).point_in_time().send(&client).await),
            ];
            for reason in reasons {
                assert!(reason.contains("`\"soon\"` is not a date"), "{reason}");
            }
        }
        assert!(refused(filtered().count(&client).await).contains("`\"soon\"` is not a date"));
        let counted = state(&NO_CLOCK)
            .counts(&Listed::query_in(&NOON_CLOCK), &client)
            .await;
        assert!(refused(counted).contains("`\"soon\"` is not a date"));
        assert_eq!(*seen.lock().unwrap(), Vec::<String>::new());
        Ok(())
    }
}
