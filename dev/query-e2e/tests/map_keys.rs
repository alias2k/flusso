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

use common::{refresh, seed_many, sink, start_opensearch};
use flusso_query::{
    AsQuery, Client, FlussoRoot, FlussoScope, KeywordMap, MultiMatchType, NumberMap, Root, Segment,
    SortBuilder, SortOrder, Sortable, TextMap, kind,
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
    let mut ids: Vec<String> = Product::query()
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
    assert_eq!(
        ids(
            &client,
            title.search("escav").match_type(MultiMatchType::BoolPrefix)
        )
        .await,
        ["1"]
    );

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
