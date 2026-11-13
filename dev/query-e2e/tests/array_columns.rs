//! Real-service e2e for array columns (issue #276): a field declared
//! `array: true` maps as its element type, so OpenSearch indexes every element,
//! and the typed handle queries it like a plain field of that type.
//!
//! Against a live OpenSearch indexed by the **real sink**:
//!
//! - `eq` / `any_of` on a keyword (ordered enum) array match a document when
//!   any element matches.
//! - A range on an integer array matches when any element is in it.
//! - Sorting picks OpenSearch's default per direction: the smallest element
//!   ascending, the largest descending; an ordered enum array sorts by its
//!   declared rank.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all -E 'binary(array_columns)'
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
    AsQuery, Client, Enum, FlussoRoot, FlussoScope, Number, Root, Segment, Sort, Sortable, kind,
};
use kernel::{
    ContentHash, FieldName, FlussoType, GenericValue, IndexMapping, IndexName, Mapping,
    ResolvedField,
};

#[derive(Debug, serde::Deserialize)]
struct Order {}
impl FlussoScope for Order {
    const PATH: &'static [Segment] = &[];
}
impl FlussoRoot for Order {
    const INDEX: &'static str = "orders";
    const SCHEMA_HASH: &'static str = "00000276";
}

fn array_field(name: &str, ty: &FlussoType, enum_order: Option<Vec<String>>) -> ResolvedField {
    let mut mapping = Mapping::of_type(ty);
    mapping.enum_order = enum_order;
    ResolvedField {
        name: FieldName::try_new(name).unwrap(),
        mapping,
        nullable: true,
        array: true,
        children: Vec::new(),
    }
}

fn mapping() -> IndexMapping {
    IndexMapping {
        index: IndexName::try_new(Order::INDEX).unwrap(),
        hash: ContentHash::new(0x276),
        fields: vec![
            array_field(
                "tags",
                &FlussoType::Enum,
                Some(vec!["urgent".to_owned(), "fragile".to_owned()]),
            ),
            array_field("scores", &FlussoType::Integer, None),
        ],
    }
}

fn order(tags: Option<&[&str]>, scores: &[i32]) -> GenericValue {
    let tags = tags.map_or(GenericValue::Null, |tags| {
        GenericValue::Array(
            tags.iter()
                .map(|tag| GenericValue::String((*tag).to_owned()))
                .collect(),
        )
    });
    GenericValue::Map(BTreeMap::from([
        ("tags".to_owned(), tags),
        (
            "scores".to_owned(),
            GenericValue::Array(scores.iter().copied().map(GenericValue::Int).collect()),
        ),
    ]))
}

async fn ids(client: &Client, query: impl AsQuery<Root>) -> Vec<String> {
    let mut ids: Vec<String> = Order::query()
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

async fn sorted(client: &Client, sort: Sort) -> Vec<String> {
    Order::query()
        .sorts([sort])
        .send(client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn an_array_field_queries_and_sorts_like_its_element() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    seed_many(
        &sink,
        &mapping(),
        &[
            ("1", order(Some(&["fragile", "urgent"]), &[1, 10])),
            ("2", order(Some(&["fragile"]), &[5])),
            ("3", order(None, &[])),
        ],
    )
    .await;
    refresh(&base, &Order::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    let tags = Enum::<Root, _>::at("tags");
    let scores = Number::<kind::Integer, Root>::at("scores");

    assert_eq!(ids(&client, tags.any_of(["urgent"])).await, ["1"]);
    assert_eq!(ids(&client, tags.eq("fragile")).await, ["1", "2"]);
    assert_eq!(ids(&client, tags.exists()).await, ["1", "2"]);
    assert_eq!(ids(&client, scores.gte(8)).await, ["1"]);
    assert_eq!(ids(&client, scores.lte(5)).await, ["1", "2"]);

    // Ascending sorts on the smallest element (1 < 5), descending on the
    // largest (10 > 5): either way document 1 leads, which only holds because
    // each direction reads the other end of the array.
    assert_eq!(
        sorted(&client, scores.asc()).await.get(..2),
        Some(&["1".to_owned(), "2".to_owned()][..])
    );
    assert_eq!(
        sorted(&client, scores.desc()).await.get(..2),
        Some(&["1".to_owned(), "2".to_owned()][..])
    );
    // `urgent` ranks first, so the array holding it leads ascending.
    assert_eq!(
        sorted(&client, tags.asc()).await.get(..2),
        Some(&["1".to_owned(), "2".to_owned()][..])
    );
}
