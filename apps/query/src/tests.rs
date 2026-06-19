//! Crate tests: the JSON the builder emits, and response decoding. These run as
//! part of the lib (not as separate integration crates) so the lib's own
//! dependencies count as used.
//!
//! The `User`/`Order` handles here are written by hand — exactly the surface the
//! future `#[derive(FlussoDocument)]` will generate, so they double as a worked
//! example of the endgame API from CLIENT.md.

use std::time::Duration;

use serde::Deserialize;
use serde_json::json;

use crate::query::Root;
use crate::{
    AsQuery, Date, FlussoDocument, FlussoMultiDocument, Geo, GeoPoint, Keyword, MsearchBundle,
    Nested, Number, Query, Search, SearchResponse, SortOrder, Text, multi_match,
};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

struct User;

impl User {
    fn email() -> Keyword {
        Keyword::at("email")
    }
    fn full_name() -> Text {
        Text::at("fullName")
    }
    fn order_count() -> Number<i64> {
        Number::at("orderCount")
    }
    fn orders() -> Nested<Root, Order> {
        Nested::at("orders")
    }
    fn query() -> Search<User> {
        Search::new("users", "xxxxxx")
    }
}

/// A nested scope: `orders.*` handles are `Query<Order>` and must be lifted.
struct Order;

impl Order {
    fn status() -> Keyword<Order> {
        Keyword::at("orders.status")
    }
    fn placed_at() -> Date<Order> {
        Date::at("orders.placedAt")
    }
}

#[test]
fn filter_nested_wraps_with_inner_hits() -> Result {
    let body = User::query()
        .filter(User::order_count().gte(1))
        .filter_nested(
            User::orders()
                .matching(Order::status().eq("delivered"))
                .sort(Order::placed_at().desc())
                .size(5),
        )
        .body();

    let expected = json!({
        "query": { "bool": {
            "must": [ { "bool": { "filter": [ { "range": { "orderCount": { "gte": 1 } } } ] } } ],
            "should": [ { "nested": {
                "path": "orders",
                "query": { "term": { "orders.status": "delivered" } },
                "inner_hits": {
                    "name": "orders",
                    "size": 5,
                    "sort": [ { "orders.placedAt": { "order": "desc" } } ]
                }
            } } ]
        } }
    });
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn merge_inner_hits_replaces_the_source_array() {
    let mut response = json!({
        "took": 2,
        "hits": { "total": { "value": 1 }, "hits": [ {
            "_id": "1", "_score": 1.0,
            "_source": { "id": 1, "orders": [ { "status": "x" }, { "status": "y" } ] },
            "inner_hits": { "orders": { "hits": { "hits": [
                { "_source": { "status": "delivered" } }
            ] } } }
        } ] }
    });

    crate::search::merge_inner_hits(&mut response, &["orders"]);

    let orders = response
        .pointer("/hits/hits/0/_source/orders")
        .cloned()
        .unwrap_or_default();
    assert_eq!(orders, json!([ { "status": "delivered" } ]));
}

#[test]
fn builds_the_full_search_body() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com"))
        .filter(User::order_count().gte(5))
        .query(User::full_name().matches("ada lovelace"))
        .filter(User::orders().any(Order::status().eq("delivered")))
        .sort(User::order_count().desc())
        .from(0)
        .size(20)
        .body();

    let expected = json!({
        "query": {
            "bool": {
                "must": [
                    { "match": { "fullName": "ada lovelace" } }
                ],
                "filter": [
                    { "term": { "email": "ada@example.com" } },
                    { "range": { "orderCount": { "gte": 5 } } },
                    { "nested": {
                        "path": "orders",
                        "query": { "term": { "orders.status": "delivered" } }
                    } }
                ]
            }
        },
        "sort": [ { "orderCount": { "order": "desc" } } ],
        "from": 0,
        "size": 20
    });

    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn count_body_keeps_the_query_and_drops_the_rest() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com"))
        .filter_nested(User::orders().matching(Order::status().eq("delivered")))
        .sort(User::order_count().desc())
        .from(10)
        .size(20)
        .count_body();

    // Only the matching clauses survive: no sort/from/size (which `_count`
    // rejects), and no inner-hits projection (which never affects matching).
    let expected = json!({
        "query": { "bool": { "filter": [
            { "term": { "email": "ada@example.com" } }
        ] } }
    });
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn ids_body_keeps_paging_and_disables_source() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com"))
        .filter_nested(User::orders().matching(Order::status().eq("delivered")))
        .sort(User::order_count().desc())
        .from(10)
        .size(20)
        .ids_body();

    // Sort and pagination shape the id page, `_source` is off, and the
    // inner-hits projection is dropped (no source to shape).
    let expected = json!({
        "query": { "bool": { "filter": [
            { "term": { "email": "ada@example.com" } }
        ] } },
        "sort": [ { "orderCount": { "order": "desc" } } ],
        "from": 10,
        "size": 20,
        "_source": false
    });
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn empty_count_body_matches_all() -> Result {
    let body = Search::<User>::new("users", "xxxxxx").count_body();
    assert_eq!(body, json!({ "query": { "match_all": {} } }));
    Ok(())
}

#[test]
fn empty_search_matches_all() -> Result {
    let body = Search::<User>::new("users", "xxxxxx").body();
    assert_eq!(body, json!({ "query": { "match_all": {} } }));
    Ok(())
}

#[test]
fn combinators_build_bool_clauses() {
    let and = Keyword::<Root>::at("a")
        .eq("x")
        .and(Keyword::<Root>::at("b").eq("y"));
    assert_eq!(
        and.to_value(),
        json!({ "bool": { "must": [
            { "term": { "a": "x" } },
            { "term": { "b": "y" } }
        ] } })
    );

    let chained = Keyword::<Root>::at("a")
        .eq("x")
        .and(Keyword::<Root>::at("b").eq("y"))
        .and(Keyword::<Root>::at("c").eq("z"));
    assert_eq!(
        chained.to_value(),
        json!({ "bool": { "must": [
            { "term": { "a": "x" } },
            { "term": { "b": "y" } },
            { "term": { "c": "z" } }
        ] } })
    );

    let or = Keyword::<Root>::at("a")
        .eq("x")
        .or(Keyword::<Root>::at("b").eq("y"));
    assert_eq!(
        or.to_value(),
        json!({ "bool": { "should": [
            { "term": { "a": "x" } },
            { "term": { "b": "y" } }
        ] } })
    );

    let negated = Keyword::<Root>::at("a").eq("x").not();
    assert_eq!(
        negated.to_value(),
        json!({ "bool": { "must_not": [ { "term": { "a": "x" } } ] } })
    );
}

#[test]
fn operators_render_expected_clauses() {
    assert_eq!(
        Keyword::<Root>::at("status")
            .in_(["paid", "shipped"])
            .to_value(),
        json!({ "terms": { "status": ["paid", "shipped"] } })
    );

    assert_eq!(
        Keyword::<Root>::at("email").prefix("test-").to_value(),
        json!({ "prefix": { "email": "test-" } })
    );

    assert_eq!(
        Number::<i64, Root>::at("n").between(1, 10).to_value(),
        json!({ "range": { "n": { "gte": 1, "lte": 10 } } })
    );

    assert_eq!(
        Text::<Root>::at("bio").exists().to_value(),
        json!({ "exists": { "field": "bio" } })
    );

    // `all` is "no element fails the predicate".
    let all = User::orders().all(Order::status().eq("paid"));
    assert_eq!(
        all.to_value(),
        json!({ "bool": { "must_not": [
            { "nested": {
                "path": "orders",
                "query": { "bool": { "must_not": [
                    { "term": { "orders.status": "paid" } }
                ] } }
            } }
        ] } })
    );
}

#[test]
fn extended_term_and_text_operators() {
    assert_eq!(
        Keyword::<Root>::at("sku").wildcard("C-?23*").to_value(),
        json!({ "wildcard": { "sku": "C-?23*" } })
    );

    assert_eq!(
        Keyword::<Root>::at("sku").regexp("c-[0-9]+").to_value(),
        json!({ "regexp": { "sku": "c-[0-9]+" } })
    );

    assert_eq!(
        Keyword::<Root>::at("city").fuzzy("bostn").to_value(),
        json!({ "fuzzy": { "city": "bostn" } })
    );

    assert_eq!(
        Text::<Root>::at("bio")
            .match_phrase_prefix("software eng")
            .to_value(),
        json!({ "match_phrase_prefix": { "bio": "software eng" } })
    );

    assert_eq!(
        Text::<Root>::at("bio").matches_fuzzy("enginer").to_value(),
        json!({ "match": { "bio": { "query": "enginer", "fuzziness": "AUTO" } } })
    );
}

#[test]
fn builders_render_shorthand_without_options() {
    // With no options a leaf builder still emits the DSL shorthand.
    assert_eq!(
        Keyword::<Root>::at("status").eq("paid").to_value(),
        json!({ "term": { "status": "paid" } })
    );
    assert_eq!(
        Number::<i64, Root>::at("n").gte(5).to_value(),
        json!({ "range": { "n": { "gte": 5 } } })
    );
}

#[test]
fn universal_boost_and_name_expand_the_clause() {
    assert_eq!(
        Keyword::<Root>::at("status")
            .eq("paid")
            .boost(2.0)
            .name("paid_clause")
            .to_value(),
        json!({ "term": { "status": {
            "value": "paid", "boost": 2.0, "_name": "paid_clause"
        } } })
    );

    // `terms` carries boost beside the field, not inside it.
    assert_eq!(
        Keyword::<Root>::at("status")
            .in_(["paid", "shipped"])
            .boost(1.5)
            .to_value(),
        json!({ "terms": { "status": ["paid", "shipped"], "boost": 1.5 } })
    );

    // `range` merges options into the bounds object.
    assert_eq!(
        Number::<i64, Root>::at("n").gte(5).boost(2.0).to_value(),
        json!({ "range": { "n": { "gte": 5, "boost": 2.0 } } })
    );
}

#[test]
fn string_query_options_render() {
    assert_eq!(
        Keyword::<Root>::at("code")
            .wildcard("*acme*")
            .case_insensitive()
            .boost(3.0)
            .to_value(),
        json!({ "wildcard": { "code": {
            "value": "*acme*", "case_insensitive": true, "boost": 3.0
        } } })
    );

    assert_eq!(
        Keyword::<Root>::at("city")
            .fuzzy("bostn")
            .fuzziness("AUTO")
            .prefix_length(1)
            .to_value(),
        json!({ "fuzzy": { "city": {
            "value": "bostn", "fuzziness": "AUTO", "prefix_length": 1
        } } })
    );

    assert_eq!(
        Text::<Root>::at("bio")
            .matches("ada")
            .fuzziness("AUTO")
            .operator("AND")
            .to_value(),
        json!({ "match": { "bio": {
            "query": "ada", "fuzziness": "AUTO", "operator": "AND"
        } } })
    );

    assert_eq!(
        Text::<Root>::at("title")
            .match_phrase("ada lovelace")
            .slop(2)
            .to_value(),
        json!({ "match_phrase": { "title": { "query": "ada lovelace", "slop": 2 } } })
    );
}

#[test]
fn multi_match_carries_field_weights_and_options() {
    let query = multi_match(
        "acme",
        [
            Text::<Root>::at("name").boosted(3.0),
            Text::<Root>::at("code"),
        ],
    )
    .match_type("best_fields")
    .tie_breaker(0.5)
    .minimum_should_match("1");
    assert_eq!(
        query.to_value(),
        json!({ "multi_match": {
            "query": "acme",
            "fields": ["name^3", "code"],
            "type": "best_fields",
            "tie_breaker": 0.5,
            "minimum_should_match": "1"
        } })
    );
}

#[test]
fn nested_query_options_render() {
    let q = User::orders()
        .any(Order::status().eq("delivered"))
        .score_mode("max")
        .ignore_unmapped(true);
    assert_eq!(
        q.to_value(),
        json!({ "nested": {
            "path": "orders",
            "query": { "term": { "orders.status": "delivered" } },
            "score_mode": "max",
            "ignore_unmapped": true
        } })
    );
}

#[test]
fn geo_distance_options_render() {
    let here = GeoPoint::new(52.37, 4.90);
    assert_eq!(
        Geo::<Root>::at("location")
            .within("10km", here)
            .distance_type("plane")
            .to_value(),
        json!({ "geo_distance": {
            "distance": "10km",
            "location": { "lat": 52.37, "lon": 4.90 },
            "distance_type": "plane"
        } })
    );
}

#[test]
fn sort_builder_options_render() {
    assert_eq!(
        crate::Sort::score().to_value(),
        json!({ "_score": { "order": "desc" } })
    );

    let body = Search::<User>::new("users", "xxxxxx")
        .sort(
            User::order_count()
                .desc()
                .missing_first()
                .mode(crate::SortMode::Max),
        )
        .body();
    assert_eq!(
        body.pointer("/sort/0").cloned().unwrap_or_default(),
        json!({ "orderCount": { "order": "desc", "missing": "_first", "mode": "max" } })
    );
}

#[test]
fn builder_or_composes_into_a_should_bool() {
    // `.or()` on a builder lifts both sides into a should-bool.
    let q = Text::<Root>::at("name")
        .matches("acme")
        .boost(2.0)
        .or(Keyword::<Root>::at("code")
            .wildcard("*acme*")
            .case_insensitive());
    assert_eq!(
        q.to_value(),
        json!({ "bool": { "should": [
            { "match": { "name": { "query": "acme", "boost": 2.0 } } },
            { "wildcard": { "code": { "value": "*acme*", "case_insensitive": true } } }
        ] } })
    );
}

#[test]
fn min_should_match_makes_a_should_group_constraining() -> Result {
    let body = User::query()
        .filter(User::email().eq("ada@example.com"))
        .should(User::full_name().matches("ada"))
        .should(User::full_name().matches("lovelace"))
        .min_should_match(1)
        .body();
    assert_eq!(
        body.pointer("/query/bool/minimum_should_match")
            .cloned()
            .unwrap_or_default(),
        json!(1)
    );
    Ok(())
}

#[test]
fn query_min_should_match_and_boost_on_a_should_group() {
    let q = Keyword::<Root>::at("a")
        .eq("x")
        .or(Keyword::<Root>::at("b").eq("y"))
        .min_should_match(1)
        .boost(2.0);
    assert_eq!(
        q.to_value(),
        json!({ "bool": {
            "should": [ { "term": { "a": "x" } }, { "term": { "b": "y" } } ],
            "minimum_should_match": 1,
            "boost": 2.0
        } })
    );
}

#[test]
fn compound_queries_render() {
    assert_eq!(
        crate::constant_score(Keyword::<Root>::at("status").eq("paid"))
            .boost(1.5)
            .to_value(),
        json!({ "constant_score": {
            "filter": { "term": { "status": "paid" } },
            "boost": 1.5
        } })
    );

    assert_eq!(
        crate::dis_max([
            Text::<Root>::at("title").matches("ada"),
            Text::<Root>::at("body").matches("ada"),
        ])
        .tie_breaker(0.5)
        .to_value(),
        json!({ "dis_max": {
            "queries": [
                { "match": { "title": "ada" } },
                { "match": { "body": "ada" } }
            ],
            "tie_breaker": 0.5
        } })
    );

    assert_eq!(
        crate::boosting(
            Text::<Root>::at("title").matches("ada"),
            Keyword::<Root>::at("status").eq("archived"),
            0.5,
        )
        .to_value(),
        json!({ "boosting": {
            "positive": { "match": { "title": "ada" } },
            "negative": { "term": { "status": "archived" } },
            "negative_boost": 0.5
        } })
    );

    assert_eq!(
        crate::function_score(Text::<Root>::at("title").matches("ada"))
            .weight_when(2.0, Keyword::<Root>::at("status").eq("featured"))
            .boost_mode("sum")
            .to_value(),
        json!({ "function_score": {
            "query": { "match": { "title": "ada" } },
            "functions": [ { "weight": 2.0, "filter": { "term": { "status": "featured" } } } ],
            "boost_mode": "sum"
        } })
    );
}

#[test]
fn multi_match_spans_text_fields() {
    let query = multi_match(
        "ada lovelace",
        [Text::<Root>::at("fullName"), Text::<Root>::at("bio")],
    );
    assert_eq!(
        query.to_value(),
        json!({ "multi_match": {
            "query": "ada lovelace",
            "fields": ["fullName", "bio"]
        } })
    );
}

#[test]
fn geo_queries_render_expected_clauses() {
    let here = GeoPoint::new(52.37, 4.90);

    assert_eq!(
        Geo::<Root>::at("location").within("10km", here).to_value(),
        json!({ "geo_distance": {
            "distance": "10km",
            "location": { "lat": 52.37, "lon": 4.90 }
        } })
    );

    assert_eq!(
        Geo::<Root>::at("location")
            .in_bounding_box(GeoPoint::new(53.0, 4.0), GeoPoint::new(52.0, 5.0))
            .to_value(),
        json!({ "geo_bounding_box": { "location": {
            "top_left": { "lat": 53.0, "lon": 4.0 },
            "bottom_right": { "lat": 52.0, "lon": 5.0 }
        } } })
    );

    assert_eq!(
        Geo::<Root>::at("location")
            .in_polygon([
                GeoPoint::new(0.0, 0.0),
                GeoPoint::new(0.0, 1.0),
                GeoPoint::new(1.0, 1.0),
            ])
            .to_value(),
        json!({ "geo_polygon": { "location": { "points": [
            { "lat": 0.0, "lon": 0.0 },
            { "lat": 0.0, "lon": 1.0 },
            { "lat": 1.0, "lon": 1.0 }
        ] } } })
    );
}

#[test]
fn geo_distance_sort_in_search_body() -> Result {
    let body = Search::<User>::new("places", "xxxxxx")
        .sort(Geo::<Root>::at("location").distance_sort(
            GeoPoint::new(52.37, 4.90),
            SortOrder::Asc,
            "km",
        ))
        .body();

    let expected = json!({
        "query": { "match_all": {} },
        "sort": [ { "_geo_distance": {
            "location": { "lat": 52.37, "lon": 4.90 },
            "order": "asc",
            "unit": "km"
        } } ]
    });
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn optional_filters_apply_only_when_some() -> Result {
    let email: Option<&str> = Some("ada@example.com");
    let min_orders: Option<i64> = None;

    let body = User::query()
        .filter(email.map(|value| User::email().eq(value)))
        .filter(min_orders.map(|value| User::order_count().gte(value)))
        .body();

    // Only the present filter survives; the `None` one is skipped entirely.
    let expected = json!({
        "query": { "bool": { "filter": [
            { "term": { "email": "ada@example.com" } }
        ] } }
    });
    assert_eq!(body, expected);
    Ok(())
}

#[test]
fn absent_clause_is_identity_in_combinators() {
    let none: Option<Query> = None;
    let query = Keyword::at("a").eq("x").and(none);
    // `and(None)` leaves the original leaf untouched — no bool wrapper.
    assert_eq!(query.to_value(), json!({ "term": { "a": "x" } }));
}

#[derive(Debug, Deserialize)]
struct DecodedUser {
    email: String,
    #[serde(rename = "orderCount")]
    order_count: i64,
}

#[derive(Debug, Deserialize)]
struct DecodedOrder {
    status: String,
}

#[test]
fn msearch_ndjson_renders_one_header_and_body_per_slot() -> Result {
    // Handles are document-type-free, so the typed slot is `Search<Decoded…>`.
    let users = Search::<DecodedUser>::new("users", "xxxxxx")
        .filter(User::email().eq("ada@example.com"))
        .size(5);
    let orders = Search::<DecodedOrder>::new("orders", "yyyyyy");

    let ndjson = (&users, &orders).ndjson()?;
    let lines: Vec<serde_json::Value> = ndjson
        .lines()
        .map(serde_json::from_str)
        .collect::<std::result::Result<_, _>>()?;

    let expected = vec![
        json!({ "index": "users_xxxxxx" }),
        json!({
            "query": { "bool": { "filter": [ { "term": { "email": "ada@example.com" } } ] } },
            "size": 5
        }),
        json!({ "index": "orders_yyyyyy" }),
        json!({ "query": { "match_all": {} } }),
    ];
    assert_eq!(lines, expected);
    Ok(())
}

#[test]
fn msearch_decodes_each_slot_with_its_own_type() -> Result {
    let users = Search::<DecodedUser>::new("users", "xxxxxx");
    let orders = Search::<DecodedOrder>::new("orders", "yyyyyy");

    let responses = vec![
        json!({ "took": 1, "hits": { "total": { "value": 7 }, "hits": [
            { "_id": "1", "_score": 1.0,
              "_source": { "email": "ada@example.com", "orderCount": 2 } }
        ] } }),
        json!({ "took": 2, "hits": { "total": { "value": 3 }, "hits": [
            { "_id": "9", "_score": 1.0, "_source": { "status": "open" } }
        ] } }),
    ];

    let (users_page, orders_page) = (&users, &orders).decode(responses)?;
    assert_eq!(users_page.total, 7);
    assert_eq!(
        users_page
            .hits
            .first()
            .ok_or("expected a user hit")?
            .source
            .email,
        "ada@example.com"
    );
    assert_eq!(orders_page.total, 3);
    assert_eq!(
        orders_page
            .hits
            .first()
            .ok_or("expected an order hit")?
            .source
            .status,
        "open"
    );
    Ok(())
}

impl FlussoDocument for DecodedUser {
    const INDEX: &'static str = "users";
    const SCHEMA_HASH: &'static str = "xxxxxx";
}

impl FlussoDocument for DecodedOrder {
    const INDEX: &'static str = "orders";
    const SCHEMA_HASH: &'static str = "yyyyyy";
}

/// A hand-written union over the two decoded types — exactly what the
/// `FlussoMultiDocument` derive will generate.
#[derive(Debug)]
enum StoreItem {
    User(DecodedUser),
    Order(DecodedOrder),
}

impl FlussoMultiDocument for StoreItem {
    const TARGETS: &'static [(&'static str, &'static str)] = &[
        (DecodedUser::INDEX, DecodedUser::SCHEMA_HASH),
        (DecodedOrder::INDEX, DecodedOrder::SCHEMA_HASH),
    ];

    fn decode(physical_index: &str, source: serde_json::Value) -> crate::Result<Self> {
        if physical_index == DecodedUser::physical_index() {
            return Ok(Self::User(serde_json::from_value(source)?));
        }
        if physical_index == DecodedOrder::physical_index() {
            return Ok(Self::Order(serde_json::from_value(source)?));
        }
        Err(crate::Error::UnexpectedIndex {
            index: physical_index.to_owned(),
        })
    }
}

#[test]
fn multi_search_addresses_every_target_index() {
    let search = StoreItem::query()
        .filter(User::email().eq("ada@example.com"))
        .size(20);

    assert_eq!(search.physical_path(), "users_xxxxxx,orders_yyyyyy");
    assert_eq!(
        search.body(),
        json!({
            "query": { "bool": { "filter": [
                { "term": { "email": "ada@example.com" } }
            ] } },
            "size": 20
        })
    );
    assert_eq!(
        search.count_body(),
        json!({ "query": { "bool": { "filter": [
            { "term": { "email": "ada@example.com" } }
        ] } } })
    );
}

#[test]
fn multi_decode_dispatches_hits_by_physical_index() -> Result {
    // A blended, interleaved page: order, user, order — ranked together.
    let response = json!({
        "took": 4,
        "hits": {
            "total": { "value": 3 },
            "max_score": 2.0,
            "hits": [
                { "_index": "orders_yyyyyy", "_id": "9", "_score": 2.0,
                  "_source": { "status": "open" } },
                { "_index": "users_xxxxxx", "_id": "1", "_score": 1.5,
                  "_source": { "email": "ada@example.com", "orderCount": 2 } },
                { "_index": "orders_yyyyyy", "_id": "7", "_score": 1.0,
                  "_source": { "status": "shipped" } }
            ]
        }
    });

    let page: SearchResponse<StoreItem> = crate::multi::decode_response(response)?;
    assert_eq!(page.total, 3);
    assert_eq!(page.max_score, Some(2.0));

    let kinds: Vec<&str> = page
        .hits
        .iter()
        .map(|hit| match &hit.source {
            StoreItem::User(_) => "user",
            StoreItem::Order(_) => "order",
        })
        .collect();
    assert_eq!(kinds, ["order", "user", "order"]);

    let first = page.hits.first().ok_or("expected a hit")?;
    assert_eq!(first.id, "9");
    match &first.source {
        StoreItem::Order(order) => assert_eq!(order.status, "open"),
        StoreItem::User(_) => panic!("expected the top hit to be an order"),
    }

    let second = page.hits.get(1).ok_or("expected a second hit")?;
    match &second.source {
        StoreItem::User(user) => assert_eq!(user.email, "ada@example.com"),
        StoreItem::Order(_) => panic!("expected the second hit to be a user"),
    }
    Ok(())
}

#[test]
fn multi_decode_rejects_a_hit_from_an_unclaimed_index() {
    let response = json!({
        "took": 1,
        "hits": { "total": { "value": 1 }, "hits": [
            { "_index": "ghosts_zzzzzz", "_id": "1", "_score": 1.0, "_source": {} }
        ] }
    });

    match crate::multi::decode_response::<StoreItem>(response) {
        Err(crate::Error::UnexpectedIndex { index }) => {
            assert_eq!(index, "ghosts_zzzzzz");
        }
        other => panic!("expected an unexpected-index error, got {other:?}"),
    }
}

#[test]
fn msearch_surfaces_a_slot_error_with_its_position() {
    let users = Search::<DecodedUser>::new("users", "xxxxxx");
    let orders = Search::<DecodedOrder>::new("orders", "yyyyyy");

    // Slot 0 succeeds; slot 1 carries a per-slot error object.
    let responses = vec![
        json!({ "took": 1, "hits": { "total": { "value": 0 }, "hits": [] } }),
        json!({ "error": { "type": "search_phase_execution_exception" }, "status": 400 }),
    ];

    match (&users, &orders).decode(responses) {
        Err(crate::Error::Msearch { slot, status, .. }) => {
            assert_eq!(slot, 1);
            assert_eq!(status, 400);
        }
        other => panic!("expected a slot error, got {other:?}"),
    }
}

#[test]
fn decodes_a_search_response() -> Result {
    let raw = json!({
        "took": 7,
        "timed_out": false,
        "hits": {
            "total": { "value": 42, "relation": "eq" },
            "max_score": 1.5,
            "hits": [
                {
                    "_index": "users_3f2a1b9c",
                    "_id": "1",
                    "_score": 1.5,
                    "_source": { "email": "ada@example.com", "orderCount": 9 }
                },
                {
                    "_index": "users_3f2a1b9c",
                    "_id": "2",
                    "_score": 0.9,
                    "_source": { "email": "bob@example.com", "orderCount": 3 }
                }
            ]
        }
    });

    let page: SearchResponse<DecodedUser> = SearchResponse::from_value(raw)?;

    assert_eq!(page.total, 42);
    assert_eq!(page.max_score, Some(1.5));
    assert_eq!(page.took, Duration::from_millis(7));
    assert_eq!(page.hits.len(), 2);

    let first = page.hits.first().ok_or("expected a hit")?;
    assert_eq!(first.id, "1");
    assert_eq!(first.score, 1.5);
    assert_eq!(first.source.email, "ada@example.com");
    assert_eq!(first.source.order_count, 9);

    Ok(())
}

#[test]
fn decodes_an_empty_page() -> Result {
    let raw = json!({
        "took": 1,
        "hits": {
            "total": { "value": 0, "relation": "eq" },
            "max_score": null,
            "hits": []
        }
    });

    let page: SearchResponse<DecodedUser> = SearchResponse::from_value(raw)?;
    assert_eq!(page.total, 0);
    assert_eq!(page.max_score, None);
    assert!(page.hits.is_empty());
    Ok(())
}
