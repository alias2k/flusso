//! Real-service e2e for `map` keys (issue #177): the typed query surface works
//! on a key exactly as on a plain field of the map's value kind.
//!
//! Against a live OpenSearch indexed by the **real sink**, which maps every key
//! like a plain field of its `values` type:
//!
//! - **Accent folding** on a `text` key: `cafe` finds "Café".
//! - **Part of a word**, per key and across keys: `excav` finds "Excavator".
//! - **Exact `eq`** on a mixed-case `keyword` key matches only the exact value.
//! - **Numeric precision**: a `double` key whose first value was integral keeps
//!   a later fraction.
//! - **Map sort with fallback** is accent-insensitive: "Èlan" sorts before
//!   "Escavatore".
//!
//! And the cross-key surface (issue #186): every mirrored operator family on
//! every map kind returns exactly the documents an `or` of the same operator on
//! each key returns, `only_preferred` narrows to the preferred keys, and a
//! cross-key query lifts into a `nested` scope. A keyword map with subfields
//! shows why it has no cross-key `eq`: `path.*` also reaches each key's `.text`
//! and `.keyword_lowercase`.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all -E 'binary(map_keys)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

mod common;

use std::collections::BTreeMap;

use common::{bare_sink, refresh, seed_many, sink, start_opensearch};
use flusso_query::{
    AsQuery, Client, DateMap, FlussoRoot, FlussoScope, Fuzziness, KeywordMap, Nested, NoSubfields,
    NumberMap, Query, Root, Segment, SegmentKind, SortBuilder, SortOrder, Sortable, TextMap, kind,
};
use kernel::{
    ContentHash, FieldName, FlussoType, GenericValue, IndexMapping, IndexName, Mapping,
    MappingType, ResolvedField,
};

#[derive(Debug, serde::Deserialize)]
struct Product {}
impl FlussoScope for Product {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for Product {
    const INDEX: &'static str = "products";
    const SCHEMA_HASH: &'static str = "00000177";
}

fn map_field(name: &str, values: &FlussoType) -> ResolvedField {
    ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping: Mapping {
            mapping_type: MappingType::Object,
            extra: BTreeMap::from([("dynamic".to_owned(), GenericValue::Bool(true))]),
            map_values: Some(Box::new(Mapping::of_type(values))),
            decimal: false,
            enum_order: None,
            subfields: true,
            indexed: true,
        },
        nullable: true,
        array: false,
        children: Vec::new(),
    }
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(Product::INDEX).unwrap(),
        hash: ContentHash::new(0x177),
        fields: vec![
            map_field("title", &FlussoType::Text),
            map_field("codes", &FlussoType::Keyword),
            map_field("prices", &FlussoType::Double),
        ],
    }
}

fn strings(pairs: &[(&str, &str)]) -> GenericValue {
    GenericValue::Map(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), GenericValue::String((*value).to_owned())))
            .collect(),
    )
}

fn product(title: &[(&str, &str)], codes: &[(&str, &str)], usd: GenericValue) -> GenericValue {
    GenericValue::Map(BTreeMap::from([
        ("title".to_owned(), strings(title)),
        ("codes".to_owned(), strings(codes)),
        (
            "prices".to_owned(),
            GenericValue::Map(BTreeMap::from([("usd".to_owned(), usd)])),
        ),
    ]))
}

async fn ids(client: &Client, query: impl AsQuery<Root>) -> Vec<String> {
    ids_in::<Product>(client, query).await
}

async fn ids_in<R: FlussoRoot>(client: &Client, query: impl AsQuery<Root>) -> Vec<String> {
    let mut ids: Vec<String> = R::query()
        .filter(query)
        .send(client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn map_keys_query_like_plain_fields() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let mapping = mapping();
    seed_many(
        &sink,
        &mapping,
        &[
            (
                "1",
                product(
                    &[("en", "Café Excavator"), ("it", "Escavatore")],
                    &[("ean", "Foo Bar")],
                    GenericValue::BigInt(3),
                ),
            ),
            (
                "2",
                product(
                    &[("en", "Zebra")],
                    &[("ean", "foo bar")],
                    GenericValue::Double(3.5),
                ),
            ),
            (
                "3",
                product(&[("it", "Èlan")], &[], GenericValue::Double(10.25)),
            ),
        ],
    )
    .await;
    refresh(&base, &Product::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    let title = TextMap::<Root>::at("title");
    let codes = KeywordMap::<Root>::at("codes");
    let prices = NumberMap::<kind::Double, Root>::at("prices");

    assert_eq!(ids(&client, title.key("en").matches("cafe")).await, ["1"]);
    assert_eq!(
        ids(&client, title.key("en").match_bool_prefix("excav")).await,
        ["1"]
    );
    assert_eq!(
        ids(
            &client,
            title.key("en").keyword_lowercase().wildcard("*EXCAV*")
        )
        .await,
        ["1"]
    );
    assert_eq!(ids(&client, title.contains("excav")).await, ["1"]);
    assert_eq!(ids(&client, title.contains("lan")).await, ["3"]);
    assert_eq!(ids(&client, title.contains("ela")).await, ["3"]);
    assert_eq!(ids(&client, title.contains("CAFE")).await, ["1"]);
    assert_eq!(ids(&client, title.contains("")).await, Vec::<String>::new());
    assert_eq!(ids(&client, title.match_bool_prefix("escav")).await, ["1"]);

    assert_eq!(ids(&client, codes.key("ean").eq("Foo Bar")).await, ["1"]);
    assert_eq!(
        ids(&client, codes.key("ean").keyword_lowercase().eq("foo bar")).await,
        ["1", "2"]
    );

    assert_eq!(ids(&client, prices.key("usd").eq(3.5)).await, ["2"]);
    assert_eq!(ids(&client, prices.key("usd").gt(3.2)).await, ["2", "3"]);

    let sorted: Vec<String> = Product::query()
        .sorts(
            SortBuilder::new()
                .by(title.sort_key("it").or("en"), SortOrder::Asc)
                .build(),
        )
        .send(&client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    assert_eq!(sorted, ["3", "1", "2"]);

    let by_key: Vec<String> = Product::query()
        .sorts([title.key("en").desc()])
        .send(&client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    assert_eq!(by_key.get(..2), Some(&["2".to_owned(), "1".to_owned()][..]));
}

#[derive(Debug, serde::Deserialize)]
struct Catalog {}
impl FlussoScope for Catalog {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for Catalog {
    const INDEX: &'static str = "catalog";
    const SCHEMA_HASH: &'static str = "00000186";
}

/// The `lines` nested array of a [`Catalog`] document.
struct Line;
impl FlussoScope for Line {
    const PATH: &'static [Segment] = &[Segment {
        name: "lines",
        kind: SegmentKind::Nested,
    }];
}

#[derive(Debug, serde::Deserialize)]
struct Tagged {}
impl FlussoScope for Tagged {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for Tagged {
    const INDEX: &'static str = "tagged";
    const SCHEMA_HASH: &'static str = "00001860";
}

fn catalog_mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(Catalog::INDEX).unwrap(),
        hash: ContentHash::new(0x186),
        fields: vec![
            map_field("title", &FlussoType::Text),
            map_field("codes", &FlussoType::Keyword),
            map_field("prices", &FlussoType::Double),
            map_field("released", &FlussoType::Timestamp),
            ResolvedField {
                name: FieldName::try_new("lines").unwrap(),
                mapping: Mapping {
                    mapping_type: MappingType::Nested,
                    extra: BTreeMap::new(),
                    map_values: None,
                    decimal: false,
                    enum_order: None,
                },
                nullable: true,
                array: true,
                children: vec![map_field("qty", &FlussoType::Long)],
            },
        ],
    }
}

fn numbers(pairs: &[(&str, f64)]) -> GenericValue {
    GenericValue::Map(
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), GenericValue::Double(*value)))
            .collect(),
    )
}

fn lines(qty: &[&[(&str, i64)]]) -> GenericValue {
    GenericValue::Array(
        qty.iter()
            .map(|line| {
                let qty = line
                    .iter()
                    .map(|(key, value)| ((*key).to_owned(), GenericValue::BigInt(*value)))
                    .collect();
                GenericValue::Map(BTreeMap::from([("qty".to_owned(), GenericValue::Map(qty))]))
            })
            .collect(),
    )
}

/// The `or` of one per-key query over every key the fixture uses.
fn per_key<Q: AsQuery<Root>>(keys: &[&str], query: impl Fn(&str) -> Q) -> Query<Root> {
    keys.iter()
        .filter_map(|key| query(key).into_query())
        .reduce(|left, right| left.or(right))
        .unwrap()
}

/// Run the cross-key query and its per-key `or`, assert they agree, and
/// return the matching ids.
async fn agree<R: FlussoRoot>(
    client: &Client,
    cross_key: impl AsQuery<Root>,
    or_of_keys: Query<Root>,
) -> Vec<String> {
    let cross = ids_in::<R>(client, cross_key).await;
    assert_eq!(cross, ids_in::<R>(client, or_of_keys).await);
    cross
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn map_cross_key_queries_match_the_per_key_or() {
    let (_container, base) = start_opensearch().await;
    let catalog = catalog_mapping();
    let doc = |title, codes, prices, released, qty| {
        GenericValue::Map(BTreeMap::from([
            ("title".to_owned(), strings(title)),
            ("codes".to_owned(), strings(codes)),
            ("prices".to_owned(), numbers(prices)),
            ("released".to_owned(), strings(released)),
            ("lines".to_owned(), lines(qty)),
        ]))
    };
    seed_many(
        &sink(&base),
        &catalog,
        &[
            (
                "1",
                doc(
                    &[("en", "Café Excavator"), ("it", "Escavatore rosso")],
                    &[("ean", "Foo Bar"), ("sku", "sale")],
                    &[("usd", 3.0)],
                    &[("eu", "2024-01-10T00:00:00Z")],
                    &[&[("a", 1)], &[("b", 2)]],
                ),
            ),
            (
                "2",
                doc(
                    &[("en", "Zebra red")],
                    &[("ean", "foo bar"), ("sku", "big sale now")],
                    &[("usd", 3.5), ("eur", 20.0)],
                    &[("us", "2025-06-01T00:00:00Z")],
                    &[&[("a", 7)]],
                ),
            ),
            (
                "3",
                doc(
                    &[("it", "Èlan")],
                    &[("sku", "Sale")],
                    &[("eur", 10.25)],
                    &[("eu", "2023-03-03T00:00:00Z")],
                    &[],
                ),
            ),
        ],
    )
    .await;

    let tagged = IndexMapping {
        index: IndexName::try_new(Tagged::INDEX).unwrap(),
        hash: ContentHash::new(0x1860),
        fields: vec![map_field("tags", &FlussoType::Keyword)],
    };
    let tag = |pairs| GenericValue::Map(BTreeMap::from([("tags".to_owned(), strings(pairs))]));
    seed_many(
        &bare_sink(&base),
        &tagged,
        &[
            ("1", tag(&[("a", "Sale"), ("b", "x/1")])),
            ("2", tag(&[("a", "big sale now")])),
            ("3", tag(&[("b", "sale")])),
        ],
    )
    .await;
    refresh(&base, &Catalog::physical_index()).await;
    refresh(&base, &Tagged::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    let title = TextMap::<Root>::at("title");
    let t = ["en", "it"];
    let analyzed: [(&str, Vec<&str>); 5] = [
        ("cafe", vec!["1"]),
        ("rosso escavatore", vec!["1"]),
        ("red", vec!["2"]),
        ("ela", vec![]),
        ("excavatr", vec!["1"]),
    ];
    for (value, expected) in &analyzed[..4] {
        assert_eq!(
            agree::<Catalog>(
                &client,
                title.matches(*value),
                per_key(&t, |k| title.key(k).matches(*value))
            )
            .await,
            *expected,
            "matches {value}"
        );
    }
    let (typo, expected) = &analyzed[4];
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.matches_fuzzy(*typo),
            per_key(&t, |k| title.key(k).matches_fuzzy(*typo))
        )
        .await,
        *expected
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.match_phrase("escavatore rosso"),
            per_key(&t, |k| title.key(k).match_phrase("escavatore rosso"))
        )
        .await,
        ["1"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.match_phrase("rosso escavatore"),
            per_key(&t, |k| title.key(k).match_phrase("rosso escavatore"))
        )
        .await,
        Vec::<String>::new()
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.match_phrase_prefix("zebra r"),
            per_key(&t, |k| title.key(k).match_phrase_prefix("zebra r"))
        )
        .await,
        ["2"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.match_bool_prefix("el"),
            per_key(&t, |k| title.key(k).match_bool_prefix("el"))
        )
        .await,
        ["3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.any_of(["Zebra red", "Èlan"]),
            per_key(&t, |k| title.key(k).any_of(["Zebra red", "Èlan"]))
        )
        .await,
        ["2", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            title.keyword_lowercase().eq("zebra RED"),
            per_key(&t, |k| title.key(k).keyword_lowercase().eq("zebra RED"))
        )
        .await,
        ["2"]
    );
    assert_eq!(
        ids_in::<Catalog>(&client, title.matches("zebra")).await,
        ["2"]
    );
    assert_eq!(
        ids_in::<Catalog>(
            &client,
            title.matches("zebra").prefer("it", 1.0).only_preferred()
        )
        .await,
        Vec::<String>::new()
    );
    let ranked: Vec<String> = Catalog::query()
        .query(title.matches_fuzzy("escavator").prefer("it", 10.0))
        .send(&client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    assert_eq!(ranked, ["1"]);

    let codes = KeywordMap::<Root>::at("codes");
    let c = ["ean", "sku"];
    assert_eq!(
        agree::<Catalog>(
            &client,
            codes.keyword_lowercase().eq("SALE"),
            per_key(&c, |k| codes.key(k).keyword_lowercase().eq("SALE"))
        )
        .await,
        ["1", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            codes.keyword_lowercase().prefix("foo b"),
            per_key(&c, |k| codes.key(k).keyword_lowercase().prefix("foo b"))
        )
        .await,
        ["1", "2"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            codes.text().matches("sale"),
            per_key(&c, |k| codes.key(k).text().matches("sale"))
        )
        .await,
        ["1", "2", "3"]
    );
    // Why a keyword map with subfields has no cross-key `eq`: `codes.*` also
    // reaches every key's `.text` and `.keyword_lowercase`.
    assert_eq!(
        ids_in::<Catalog>(
            &client,
            flusso_query::query_string("\"sale\"")
                .fields([flusso_query::Text::<Root, NoSubfields>::leaf("codes.*")])
        )
        .await,
        ["1", "2", "3"]
    );
    assert_eq!(
        ids_in::<Catalog>(&client, per_key(&c, |k| codes.key(k).eq("sale"))).await,
        ["1"]
    );

    let tags = KeywordMap::<Root, NoSubfields>::leaf("tags");
    let g = ["a", "b"];
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.eq("sale"),
            per_key(&g, |k| tags.key(k).eq("sale"))
        )
        .await,
        ["3"]
    );
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.any_of(["Sale", "big sale now"]),
            per_key(&g, |k| tags.key(k).any_of(["Sale", "big sale now"]))
        )
        .await,
        ["1", "2"]
    );
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.prefix("big s"),
            per_key(&g, |k| tags.key(k).prefix("big s"))
        )
        .await,
        ["2"]
    );
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.wildcard("?ale"),
            per_key(&g, |k| tags.key(k).wildcard("?ale"))
        )
        .await,
        ["1", "3"]
    );
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.regexp("x/[0-9]"),
            per_key(&g, |k| tags.key(k).regexp("x/[0-9]"))
        )
        .await,
        ["1"]
    );
    assert_eq!(
        agree::<Tagged>(
            &client,
            tags.fuzzy("Sald").fuzziness(Fuzziness::Edits(1)),
            per_key(&g, |k| tags
                .key(k)
                .fuzzy("Sald")
                .fuzziness(Fuzziness::Edits(1)))
        )
        .await,
        ["1"]
    );
    assert_eq!(
        ids_in::<Tagged>(&client, tags.eq("sale").prefer("a", 2.0).only_preferred()).await,
        Vec::<String>::new()
    );

    let prices = NumberMap::<kind::Double, Root>::at("prices");
    let p = ["usd", "eur"];
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.eq(3.5),
            per_key(&p, |k| prices.key(k).eq(3.5))
        )
        .await,
        ["2"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.any_of([3.0, 10.25]),
            per_key(&p, |k| prices.key(k).any_of([3.0, 10.25]))
        )
        .await,
        ["1", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.gt(3.0),
            per_key(&p, |k| prices.key(k).gt(3.0))
        )
        .await,
        ["2", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.gte(3.0),
            per_key(&p, |k| prices.key(k).gte(3.0))
        )
        .await,
        ["1", "2", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.lt(3.5),
            per_key(&p, |k| prices.key(k).lt(3.5))
        )
        .await,
        ["1"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.lte(3.5),
            per_key(&p, |k| prices.key(k).lte(3.5))
        )
        .await,
        ["1", "2"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.between(-5, 11),
            per_key(&p, |k| prices.key(k).between(-5, 11))
        )
        .await,
        ["1", "2", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            prices.between(15, 25),
            per_key(&p, |k| prices.key(k).between(15, 25))
        )
        .await,
        ["2"]
    );
    assert_eq!(
        ids_in::<Catalog>(&client, prices.gt(15).prefer("usd", 1.0).only_preferred()).await,
        Vec::<String>::new()
    );

    let released = DateMap::<Root>::at("released");
    let r = ["eu", "us"];
    assert_eq!(
        agree::<Catalog>(
            &client,
            released.eq("2024-01-10T00:00:00Z"),
            per_key(&r, |k| released.key(k).eq("2024-01-10T00:00:00Z"))
        )
        .await,
        ["1"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            released.gte("2024-01-01"),
            per_key(&r, |k| released.key(k).gte("2024-01-01"))
        )
        .await,
        ["1", "2"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            released.between("2023-01-01", "2024-12-31"),
            per_key(&r, |k| released.key(k).between("2023-01-01", "2024-12-31"))
        )
        .await,
        ["1", "3"]
    );
    assert_eq!(
        agree::<Catalog>(
            &client,
            released.lt("2024-01-10T00:00:00").time_zone("+01:00"),
            per_key(&r, |k| released
                .key(k)
                .lt("2024-01-10T00:00:00")
                .time_zone("+01:00"))
        )
        .await,
        ["3"]
    );
    assert_eq!(
        ids_in::<Catalog>(&client, released.gte("now-100000d")).await,
        ["1", "2", "3"]
    );

    let qty = NumberMap::<kind::Long, Line>::at("lines.qty");
    let line_items = Nested::<Root, Line>::at("lines");
    assert_eq!(
        agree::<Catalog>(
            &client,
            line_items.any(qty.gt(1)),
            per_key(&["a", "b"], |k| line_items.any(qty.key(k).gt(1)))
        )
        .await,
        ["1", "2"]
    );
    assert_eq!(
        ids_in::<Catalog>(&client, line_items.any(qty.gte(5))).await,
        ["2"]
    );

    let empty = NumberMap::<kind::Double, Root>::at("nothing");
    assert_eq!(
        ids_in::<Catalog>(&client, empty.gt(0)).await,
        Vec::<String>::new()
    );
}
