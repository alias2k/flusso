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
    Client, Date, Geo, GeoPoint, Keyword, Nested, Number, Query, Search, SearchResponse, SortOrder,
    Text, multi_match,
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
    fn search(client: &Client) -> Search<'_, User> {
        Search::new(client, "users")
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
    let client = Client::connect("http://localhost:9200")?;
    let body = User::search(&client)
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
    let client = Client::connect("http://localhost:9200")?;

    let body = User::search(&client)
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
fn empty_search_matches_all() -> Result {
    let client = Client::connect("http://localhost:9200")?;
    let body = Search::<User>::new(&client, "users").body();
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
    let client = Client::connect("http://localhost:9200")?;
    let body = Search::<User>::new(&client, "places")
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
    let client = Client::connect("http://localhost:9200")?;

    let email: Option<&str> = Some("ada@example.com");
    let min_orders: Option<i64> = None;

    let body = User::search(&client)
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
