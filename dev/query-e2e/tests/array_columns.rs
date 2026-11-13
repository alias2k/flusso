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
//! - A `#[derive(FlussoRoot)]` struct reads real hits back: a `Vec<Tag>`, an
//!   `Option<Vec<i32>>` and an `Option<Codes>` array newtype, each with values,
//!   `[]` and `null`, queried and sorted through the derived handles
//!   (`tests/fixtures/arrays/`).
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
    AsQuery, Client, Enum, FlussoRoot, FlussoScope, FlussoValue, Number, Root, Segment, Sort,
    Sortable, kind,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Urgent,
    Fragile,
}

#[derive(Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Codes(Vec<String>);

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "listed", config = "tests/fixtures/arrays/flusso.toml")]
struct Listed {
    id: String,
    tags: Vec<Tag>,
    scores: Option<Vec<i32>>,
    codes: Option<Codes>,
}

/// The fixture's mapping, resolved as the derive resolves it, so the sink
/// writes the physical index `Listed` reads.
fn listed_mapping() -> IndexMapping {
    let config = config::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/arrays/flusso.toml"
    )))
    .unwrap();
    let name = IndexName::try_new(Listed::INDEX).unwrap();
    config.indexes.get(&name).unwrap().schema.resolve(name)
}

fn listed(id: &str, tags: &[&str], scores: Option<&[i32]>, codes: Option<&[&str]>) -> GenericValue {
    let strings = |items: &[&str]| {
        GenericValue::Array(
            items
                .iter()
                .map(|item| GenericValue::String((*item).to_owned()))
                .collect(),
        )
    };
    GenericValue::Map(BTreeMap::from([
        ("id".to_owned(), GenericValue::String(id.to_owned())),
        ("tags".to_owned(), strings(tags)),
        (
            "scores".to_owned(),
            scores.map_or(GenericValue::Null, |scores| {
                GenericValue::Array(scores.iter().copied().map(GenericValue::Int).collect())
            }),
        ),
        (
            "codes".to_owned(),
            codes.map_or(GenericValue::Null, strings),
        ),
    ]))
}

async fn listed_hits(client: &Client, query: impl AsQuery<Root>) -> Vec<Listed> {
    let mut hits: Vec<Listed> = Listed::query()
        .filter(query)
        .send(client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.source)
        .collect();
    hits.sort_by(|a, b| a.id.cmp(&b.id));
    hits
}

fn listed_ids(hits: &[Listed]) -> Vec<&str> {
    hits.iter().map(|hit| hit.id.as_str()).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_derived_root_reads_array_fields_back_and_queries_their_elements() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    seed_many(
        &sink,
        &listed_mapping(),
        &[
            (
                "1",
                listed(
                    "1",
                    &["fragile", "urgent"],
                    Some(&[1, 10]),
                    Some(&["a", "b"]),
                ),
            ),
            ("2", listed("2", &["fragile"], Some(&[]), Some(&[]))),
            ("3", listed("3", &[], None, None)),
        ],
    )
    .await;
    refresh(&base, &Listed::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    let all = listed_hits(&client, Listed::id().exists()).await;
    assert_eq!(listed_ids(&all), ["1", "2", "3"]);
    let [one, two, three] = all.as_slice() else {
        panic!("three documents: {all:?}");
    };
    assert_eq!(one.tags, [Tag::Fragile, Tag::Urgent]);
    assert_eq!(one.scores.as_deref(), Some(&[1, 10][..]));
    assert_eq!(one.codes, Some(Codes(vec!["a".to_owned(), "b".to_owned()])));
    assert_eq!(two.tags, [Tag::Fragile]);
    assert_eq!(two.scores.as_deref(), Some(&[][..]));
    assert_eq!(two.codes, Some(Codes(Vec::new())));
    assert!(three.tags.is_empty());
    assert_eq!(three.scores, None);
    assert_eq!(three.codes, None);

    let ids = |hits: Vec<Listed>| hits.into_iter().map(|hit| hit.id).collect::<Vec<_>>();
    assert_eq!(
        ids(listed_hits(&client, Listed::tags().any_of([Tag::Urgent])).await),
        ["1"]
    );
    assert_eq!(
        ids(listed_hits(&client, Listed::tags().eq(Tag::Fragile)).await),
        ["1", "2"]
    );
    assert_eq!(
        ids(listed_hits(&client, Listed::codes().eq("b")).await),
        ["1"]
    );
    assert_eq!(
        ids(listed_hits(&client, Listed::codes().any_of(["a", "z"])).await),
        ["1"]
    );
    assert_eq!(
        ids(listed_hits(&client, Listed::scores().gte(8)).await),
        ["1"]
    );

    let sorted_by = |sort: Sort| {
        let client = &client;
        async move {
            Listed::query()
                .sorts([sort])
                .send(client)
                .await
                .unwrap()
                .hits
                .into_iter()
                .map(|hit| hit.source.id)
                .collect::<Vec<_>>()
        }
    };
    // `urgent` ranks first, so the array holding it leads ascending, and a
    // document with no element sorts last either way.
    assert_eq!(sorted_by(Listed::tags().asc()).await, ["1", "2", "3"]);
    assert_eq!(
        sorted_by(Listed::scores().desc())
            .await
            .first()
            .map(String::as_str),
        Some("1")
    );
}
