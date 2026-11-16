//! Read-time fields end to end: the real derive, the real sink, a live
//! OpenSearch.
//!
//! The point of a rule field is that the value a document shows and the
//! filter, sort and counts on it never disagree. So every check here asks the
//! index and compares with plain Rust: the rules written again as an `if`
//! chain over the seeded data, and each condition decided on the fetched
//! `_source`.
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all -E 'binary(read_time)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    unused_crate_dependencies,
    missing_debug_implementations,
    dead_code
)]

mod common;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Duration, TimeZone, Utc};
use common::{refresh, sink, start_opensearch};
use flusso_query::rules::{Condition, Rules};
use flusso_query::{Client, Error, FlussoRoot, FlussoValue, Search, Sortable};
use kernel::{Envelope, GenericValue, IndexMapping, IndexName, Position};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sink::Sink;
use sink_opensearch::OpensearchSink;

// ---------------------------------------------------------------------------
// The example: what a consumer writes.
// ---------------------------------------------------------------------------

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, FlussoValue,
)]
#[serde(rename_all = "snake_case")]
#[flusso(keyword)]
enum ContractState {
    Expiring,
    Open,
    Scheduled,
    Draft,
    Suspended,
    Closed,
    Terminated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Urgency {
    Overdue,
    ThisWeek,
    ThisMonth,
    Later,
}

/// The request-time values the computed fields read.
#[derive(Debug, Clone)]
struct Viewing {
    now: DateTime<Utc>,
    viewer: String,
    rate: f64,
}

#[derive(Debug, Clone, Deserialize, FlussoRoot)]
#[flusso(
    index = "contracts",
    config = "tests/fixtures/read_time/flusso.toml",
    context = Viewing
)]
struct Contract {
    id: String,
    title: String,
    owner_id: String,
    ends_at: DateTime<Utc>,
    /// Replaces the stored `state` on read.
    #[flusso(rules = visible_state)]
    state: ContractState,
    #[flusso(rules = current_fee)]
    fee: f64,
    #[flusso(rules = is_mine)]
    is_mine: bool,
    #[flusso(rules = urgency)]
    urgency: Urgency,
    #[flusso(computed = days_left)]
    days_left: i64,
    #[flusso(computed = fee_for_viewer)]
    fee_for_viewer: f64,
    #[flusso(computed = label)]
    label: String,
}

fn visible_state(cx: &Viewing) -> Rules<ContractState> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(ContractState::Draft)
        .when(
            doc.state()
                .eq(ContractState::Terminated)
                .or(doc.ends_at().lt(cx.now)),
        )
        .then(ContractState::Closed)
        .when(doc.starts_at().gt(cx.now))
        .then(ContractState::Scheduled)
        .when(
            doc.notice_starts_at()
                .lte(cx.now)
                .and(doc.auto_renew().eq(false)),
        )
        .then(ContractState::Expiring)
        .otherwise(doc.state())
}

/// A number picked from two stored fields. `discounted_fee` can be missing, so
/// the arm that yields it checks it is there.
fn current_fee(cx: &Viewing) -> Rules<f64> {
    let doc = Contract::doc();
    Rules::when(
        doc.discount_until()
            .gt(cx.now)
            .and(doc.discounted_fee().exists()),
    )
    .then(doc.discounted_fee().assume_present())
    .otherwise(doc.monthly_fee())
}

fn is_mine(cx: &Viewing) -> Rules<bool> {
    Rules::when(Contract::doc().owner_id().eq(cx.viewer.as_str()))
        .then(true)
        .otherwise(false)
}

/// The maths happens on the context side; the index compares one field.
fn urgency(cx: &Viewing) -> Rules<Urgency> {
    let ends_at = Contract::doc().ends_at();
    Rules::when(ends_at.lt(cx.now))
        .then(Urgency::Overdue)
        .when(ends_at.lt(cx.now + Duration::days(7)))
        .then(Urgency::ThisWeek)
        .when(ends_at.lt(cx.now + Duration::days(30)))
        .then(Urgency::ThisMonth)
        .otherwise(Urgency::Later)
        .order([
            Urgency::Overdue,
            Urgency::ThisWeek,
            Urgency::ThisMonth,
            Urgency::Later,
        ])
}

#[derive(Deserialize)]
struct Term {
    ends_at: DateTime<Utc>,
}

fn days_left(term: &Term, cx: &Viewing) -> i64 {
    (term.ends_at - cx.now).num_days()
}

/// Reads `fee`, which the rules wrote just before.
#[derive(Deserialize)]
struct Priced {
    fee: f64,
}

fn fee_for_viewer(priced: &Priced, cx: &Viewing) -> f64 {
    (priced.fee * cx.rate * 100.0).round() / 100.0
}

#[derive(Deserialize)]
struct Labelled {
    title: String,
    state: ContractState,
    ends_at: DateTime<Utc>,
}

/// Anything Rust can do: `match`, a helper, string work, calendar maths.
fn label(doc: &Labelled, cx: &Viewing) -> String {
    match doc.state {
        ContractState::Closed | ContractState::Terminated => format!("{} (closed)", doc.title),
        ContractState::Expiring => format!("{}, ends {}", doc.title, weekday(doc.ends_at)),
        ContractState::Draft => format!("{} (unsigned)", doc.title.to_uppercase()),
        _ => format!(
            "{}, {} weeks left",
            doc.title,
            (doc.ends_at - cx.now).num_weeks()
        ),
    }
}

fn weekday(at: DateTime<Utc>) -> String {
    at.format("%A %-d %B").to_string()
}

// ---------------------------------------------------------------------------
// Data.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Seed {
    id: String,
    title: String,
    owner: String,
    signed: Option<DateTime<Utc>>,
    starts: DateTime<Utc>,
    ends: DateTime<Utc>,
    notice_starts: DateTime<Utc>,
    state: &'static str,
    auto_renew: bool,
    notice_days: i32,
    fee: f64,
    discounted: Option<f64>,
    discount_until: Option<DateTime<Utc>>,
    list_price: Decimal,
    tags: Vec<String>,
    country: Option<&'static str>,
}

fn doc(seed: &Seed) -> GenericValue {
    let at = |at: DateTime<Utc>| GenericValue::TimestampTz(at);
    let text = |text: &str| GenericValue::String(text.to_owned());
    GenericValue::Map(BTreeMap::from([
        ("id".to_owned(), text(&seed.id)),
        ("title".to_owned(), text(&seed.title)),
        ("owner_id".to_owned(), text(&seed.owner)),
        (
            "signed_at".to_owned(),
            seed.signed.map_or(GenericValue::Null, at),
        ),
        ("starts_at".to_owned(), at(seed.starts)),
        ("ends_at".to_owned(), at(seed.ends)),
        ("notice_starts_at".to_owned(), at(seed.notice_starts)),
        ("state".to_owned(), text(seed.state)),
        ("auto_renew".to_owned(), GenericValue::Bool(seed.auto_renew)),
        (
            "notice_days".to_owned(),
            GenericValue::Int(seed.notice_days),
        ),
        ("monthly_fee".to_owned(), GenericValue::Double(seed.fee)),
        (
            "discounted_fee".to_owned(),
            seed.discounted
                .map_or(GenericValue::Null, GenericValue::Double),
        ),
        (
            "discount_until".to_owned(),
            seed.discount_until.map_or(GenericValue::Null, at),
        ),
        (
            "list_price".to_owned(),
            GenericValue::Decimal(seed.list_price),
        ),
        (
            "tags".to_owned(),
            GenericValue::Array(seed.tags.iter().map(|tag| text(tag)).collect()),
        ),
        (
            "billing".to_owned(),
            GenericValue::Map(BTreeMap::from([(
                "country".to_owned(),
                seed.country.map_or(GenericValue::Null, text),
            )])),
        ),
    ]))
}

fn mapping() -> IndexMapping {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/read_time/flusso.toml"
    );
    let config = config::load(std::path::Path::new(path)).unwrap();
    let key = IndexName::try_new("contracts").unwrap();
    config.indexes.get(&key).unwrap().schema.resolve(key)
}

async fn load(sink: &OpensearchSink, mapping: &IndexMapping, seeds: &[Seed]) {
    sink.ensure_index(mapping).await.unwrap();
    for (count, seed) in seeds.iter().enumerate() {
        sink.apply(&Envelope::upsert(
            mapping.index.clone(),
            seed.id.as_str(),
            doc(seed),
            Some(Position(0)),
            Utc::now(),
        ))
        .await
        .unwrap();
        if count % 5_000 == 4_999 {
            sink.flush(false).await.unwrap();
        }
    }
    sink.flush(true).await.unwrap();
    sink.mark_seeded(&mapping.index).await.unwrap();
}

async fn start(seeds: &[Seed]) -> (impl Sized + use<>, String, Client, OpensearchSink) {
    let (container, base) = start_opensearch().await;
    let sink = sink(&base);
    let mapping = mapping();
    assert_eq!(
        format!("{}_{}", mapping.index, mapping.hash),
        Contract::physical_index(),
        "the derive and the test resolve the same schema"
    );
    load(&sink, &mapping, seeds).await;
    refresh(&base, &Contract::physical_index()).await;
    let client = Client::connect(&base).unwrap();
    (container, base, client, sink)
}

/// Twelve readable contracts around `now`.
fn listing(now: DateTime<Utc>) -> Vec<Seed> {
    let day = Duration::days;
    #[allow(clippy::too_many_arguments)]
    let seed = |id: &str,
                title: &str,
                owner: &str,
                signed: bool,
                (starts, ends): (i64, i64),
                state: &'static str,
                auto_renew: bool,
                notice_days: i32,
                fee: f64,
                discount: Option<(f64, i64)>| Seed {
        id: id.to_owned(),
        title: title.to_owned(),
        owner: owner.to_owned(),
        signed: signed.then(|| now + day(starts - 7)),
        starts: now + day(starts),
        ends: now + day(ends),
        notice_starts: now + day(ends - i64::from(notice_days)),
        state,
        auto_renew,
        notice_days,
        fee,
        discounted: discount.map(|(pct, _)| fee * (100.0 - pct) / 100.0),
        discount_until: discount.map(|(_, until)| now + day(until)),
        list_price: Decimal::new((fee * 120.0) as i64, 2),
        tags: vec!["b2b".to_owned(), "annual".to_owned()],
        country: Some("IT"),
    };
    vec![
        seed(
            "c01",
            "Acme hosting",
            "anna",
            true,
            (-300, 65),
            "open",
            false,
            30,
            1200.0,
            None,
        ),
        seed(
            "c02",
            "Borealis support",
            "anna",
            true,
            (-340, 12),
            "open",
            false,
            30,
            800.0,
            Some((20.0, 40)),
        ),
        seed(
            "c03",
            "Cobalt licences",
            "bob",
            true,
            (-100, 9),
            "open",
            true,
            30,
            300.0,
            None,
        ),
        seed(
            "c04",
            "Delta retainer",
            "bob",
            false,
            (5, 370),
            "open",
            false,
            60,
            2500.0,
            Some((10.0, 90)),
        ),
        seed(
            "c05",
            "Echo analytics",
            "anna",
            true,
            (14, 379),
            "open",
            false,
            30,
            990.0,
            Some((50.0, 30)),
        ),
        seed(
            "c06",
            "Foxtrot SLA",
            "cara",
            true,
            (-700, -3),
            "open",
            false,
            30,
            450.0,
            None,
        ),
        seed(
            "c07",
            "Golf consulting",
            "anna",
            true,
            (-200, 160),
            "suspended",
            false,
            30,
            5000.0,
            Some((15.0, -10)),
        ),
        seed(
            "c08",
            "Hotel backup",
            "bob",
            true,
            (-50, 310),
            "terminated",
            false,
            30,
            150.0,
            None,
        ),
        seed(
            "c09",
            "India storage",
            "cara",
            true,
            (-365, 3),
            "open",
            false,
            30,
            700.0,
            None,
        ),
        seed(
            "c10",
            "Juliet audit",
            "anna",
            true,
            (-20, 29),
            "open",
            false,
            30,
            640.0,
            Some((25.0, 200)),
        ),
        seed(
            "c11",
            "Kilo training",
            "cara",
            true,
            (-10, 31),
            "open",
            false,
            14,
            100.0,
            None,
        ),
        seed(
            "c12",
            "Lima escrow",
            "bob",
            true,
            (-400, 200),
            "open",
            false,
            14,
            2000.0,
            Some((100.0, 10)),
        ),
    ]
}

/// Every combination of the instants that sit on a rule's edge: a millisecond
/// either side of `now`, the same millisecond, and well clear of it.
/// Decimals with more digits than a `double` keeps, next to plain ones: the
/// index holds each as the nearest `double`, and a rule compares it as one.
const LIST_PRICES: [&str; 7] = [
    "0.1",
    "0.30000000000000004",
    "100",
    "455.40",
    "455.400000000000034",
    "123456789.123456789",
    "9007199254740993",
];

/// Every combination of the values that sit on a boundary: an instant before,
/// at and after the clock, down to the microsecond the index drops.
fn edges(now: DateTime<Utc>) -> Vec<Seed> {
    let ms = Duration::milliseconds;
    let us = Duration::microseconds;
    let day = Duration::days;
    let ends = [
        day(-1),
        ms(-1),
        us(0),
        us(400),
        ms(1),
        day(3),
        day(20),
        day(200),
    ];
    let starts = [day(-30), us(0), ms(1), day(5)];
    let notices = [day(-1), us(0), us(-100), ms(1)];
    let states = ["open", "suspended", "terminated"];
    let long_tag = "x".repeat(300);
    let mut seeds = Vec::new();
    for signed in [true, false] {
        for state in states {
            for end in ends {
                for start in starts {
                    for notice in notices {
                        let i = seeds.len();
                        let fee = if i % 23 == 0 {
                            455.400_000_000_000_03
                        } else {
                            100.0 + (i % 17) as f64 * 37.3
                        };
                        let (discounted, discount_until) = match i % 5 {
                            0 => (None, None),
                            1 => (Some(fee * 0.8), Some(now + day(-2))),
                            2 => (Some(fee * 0.8), Some(now + day(2))),
                            3 => (Some(fee * 0.7), Some(now)),
                            _ => (Some(fee * 0.7), Some(now + ms(1))),
                        };
                        seeds.push(Seed {
                            id: format!("e{i:04}"),
                            title: format!("Edge {i}"),
                            owner: ["anna", "bob", "cara"][i % 3].to_owned(),
                            signed: signed.then(|| now + day(-40)),
                            starts: now + start,
                            ends: now + end,
                            notice_starts: now + notice,
                            state,
                            auto_renew: i % 2 == 0,
                            notice_days: [0, 14, 30, 60][i % 4],
                            fee,
                            discounted,
                            discount_until,
                            list_price: LIST_PRICES[i % LIST_PRICES.len()].parse().unwrap(),
                            tags: match i % 4 {
                                0 => Vec::new(),
                                1 => vec!["b2b".to_owned()],
                                2 => vec!["b2b".to_owned(), "annual".to_owned()],
                                _ => vec![long_tag.clone(), "vip".to_owned()],
                            },
                            country: [Some("IT"), Some("DE"), None][i % 3],
                        });
                    }
                }
            }
        }
    }
    seeds
}

// ---------------------------------------------------------------------------
// The oracle: the same rules in plain Rust, at the index's millisecond precision.
// ---------------------------------------------------------------------------

fn ms(at: DateTime<Utc>) -> i64 {
    at.timestamp_millis()
}

const STATE_ORDER: [ContractState; 7] = [
    ContractState::Expiring,
    ContractState::Open,
    ContractState::Scheduled,
    ContractState::Draft,
    ContractState::Suspended,
    ContractState::Closed,
    ContractState::Terminated,
];

const URGENCY_ORDER: [Urgency; 4] = [
    Urgency::Overdue,
    Urgency::ThisWeek,
    Urgency::ThisMonth,
    Urgency::Later,
];

fn expected_state(seed: &Seed, now: DateTime<Utc>) -> ContractState {
    let now = ms(now);
    if seed.signed.is_none() {
        ContractState::Draft
    } else if seed.state == "terminated" || ms(seed.ends) < now {
        ContractState::Closed
    } else if ms(seed.starts) > now {
        ContractState::Scheduled
    } else if ms(seed.notice_starts) <= now && !seed.auto_renew {
        ContractState::Expiring
    } else if seed.state == "suspended" {
        ContractState::Suspended
    } else {
        ContractState::Open
    }
}

fn expected_fee(seed: &Seed, now: DateTime<Utc>) -> f64 {
    match (seed.discount_until, seed.discounted) {
        (Some(until), Some(discounted)) if ms(until) > ms(now) => discounted,
        _ => seed.fee,
    }
}

fn expected_urgency(seed: &Seed, now: DateTime<Utc>) -> Urgency {
    let ends = ms(seed.ends);
    if ends < ms(now) {
        Urgency::Overdue
    } else if ends < ms(now + Duration::days(7)) {
        Urgency::ThisWeek
    } else if ends < ms(now + Duration::days(30)) {
        Urgency::ThisMonth
    } else {
        Urgency::Later
    }
}

fn cx(now: DateTime<Utc>) -> Viewing {
    Viewing {
        now,
        viewer: "anna".to_owned(),
        rate: 0.92,
    }
}

async fn ids(client: &Client, search: Search<Contract>) -> Vec<String> {
    search.size(2_000).ids(client).await.unwrap()
}

fn set(ids: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    ids.into_iter().collect()
}

fn where_seed(seeds: &[Seed], keep: impl Fn(&Seed) -> bool) -> BTreeSet<String> {
    seeds
        .iter()
        .filter(|seed| keep(seed))
        .map(|seed| seed.id.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 1. The example, as a consumer uses it.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_listing_filters_sorts_and_counts_by_the_values_it_shows() {
    let now = Utc.with_ymd_and_hms(2026, 11, 12, 10, 0, 0).unwrap();
    let seeds = listing(now);
    let (_container, _base, client, _sink) = start(&seeds).await;
    let by_id: BTreeMap<&str, &Seed> = seeds.iter().map(|seed| (seed.id.as_str(), seed)).collect();
    let cx = cx(now);

    // The dashboard: by visible state, then soonest to end.
    let page = Contract::query(&cx)
        .sort(Contract::state(&cx).asc())
        .sort(Contract::ends_at().asc())
        .size(50)
        .send(&client)
        .await
        .unwrap();
    assert_eq!(page.hits.len(), seeds.len());
    let states: Vec<ContractState> = page.hits.iter().map(|hit| hit.source.state).collect();
    let mut sorted = states.clone();
    sorted.sort();
    assert_eq!(states, sorted, "the dashboard is in visible-state order");
    for hit in &page.hits {
        let (seed, shown) = (by_id[hit.id.as_str()], &hit.source);
        assert_eq!(shown.state, expected_state(seed, now), "{}", hit.id);
        assert_eq!(shown.fee, expected_fee(seed, now), "{}", hit.id);
        assert_eq!(shown.urgency, expected_urgency(seed, now), "{}", hit.id);
        assert_eq!(shown.is_mine, seed.owner == "anna", "{}", hit.id);
        assert_eq!(shown.days_left, (seed.ends - now).num_days(), "{}", hit.id);
        assert_eq!(
            shown.fee_for_viewer,
            (shown.fee * cx.rate * 100.0).round() / 100.0,
            "{}",
            hit.id
        );
        assert!(
            shown.label.starts_with(&seed.title)
                || shown.label.starts_with(&seed.title.to_uppercase())
        );
    }

    // A tab: expiring, mine.
    let tab = Contract::query(&cx)
        .filter(Contract::state(&cx).eq(ContractState::Expiring))
        .filter(Contract::is_mine(&cx).eq(true))
        .size(50)
        .send(&client)
        .await
        .unwrap();
    assert_eq!(
        set(tab.hits.iter().map(|hit| hit.id.clone())),
        where_seed(&seeds, |seed| {
            expected_state(seed, now) == ContractState::Expiring && seed.owner == "anna"
        })
    );
    assert!(!tab.hits.is_empty());

    // The count beside each tab, in one round trip.
    let counts = Contract::state(&cx)
        .counts(&Contract::query(&cx), &client)
        .await
        .unwrap();
    assert_eq!(
        counts.iter().map(|(state, _)| *state).collect::<Vec<_>>(),
        STATE_ORDER
    );
    for (state, count) in counts {
        let expected = where_seed(&seeds, |seed| expected_state(seed, now) == state).len();
        assert_eq!(count, expected as u64, "{state:?}");
    }

    // A number that is one of two stored fields.
    let cheap = Contract::query(&cx)
        .filter(Contract::fee(&cx).lte(600.0))
        .size(50)
        .send(&client)
        .await
        .unwrap();
    assert_eq!(
        set(cheap.hits.iter().map(|hit| hit.id.clone())),
        where_seed(&seeds, |seed| expected_fee(seed, now) <= 600.0)
    );

    // One contract by id, now and a month on: nothing was written in between.
    let one = Contract::get(&cx, &client, "c02").await.unwrap().unwrap();
    assert_eq!(one.state, ContractState::Expiring);
    let later = Viewing {
        now: now + Duration::days(30),
        ..cx.clone()
    };
    let one = Contract::get(&later, &client, "c02")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(one.state, ContractState::Closed);
    assert!(Contract::get(&cx, &client, "nope").await.unwrap().is_none());

    // Several searches in one `_msearch` are computed too.
    let both = client
        .msearch_all(&[
            Contract::query(&cx).size(50),
            Contract::query(&later).size(50),
        ])
        .await
        .unwrap();
    for (page, now) in both.iter().zip([cx.now, later.now]) {
        for hit in &page.hits {
            assert_eq!(
                hit.source.state,
                expected_state(by_id[hit.id.as_str()], now)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Value, filter, sort and counts agree with each other and with plain Rust.
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn rules_agree_with_the_index() {
    use futures::TryStreamExt;

    let base_now =
        Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap() + Duration::microseconds(123_456);
    let seeds = edges(base_now);
    let (_container, _base, client, _sink) = start(&seeds).await;
    let by_id: BTreeMap<&str, &Seed> = seeds.iter().map(|seed| (seed.id.as_str(), seed)).collect();

    let nows = [
        ("as seeded, with microseconds", base_now),
        (
            "one millisecond later",
            base_now + Duration::milliseconds(1),
        ),
        (
            "one millisecond earlier",
            base_now - Duration::milliseconds(1),
        ),
        (
            "700 microseconds later",
            base_now + Duration::microseconds(700),
        ),
        (
            "a whole second",
            Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap(),
        ),
        ("three days later", base_now + Duration::days(3)),
    ];

    let (mut values, mut filters, mut sorts, mut counted) = (0, 0, 0, 0);
    let mut wrong = Vec::new();
    for (name, now) in nows {
        let cx = cx(now);

        let all = Contract::query(&cx)
            .size(2_000)
            .send(&client)
            .await
            .unwrap();
        assert_eq!(all.hits.len(), seeds.len());
        for hit in &all.hits {
            let seed = by_id[hit.id.as_str()];
            let got = &hit.source;
            values += 4;
            if got.state != expected_state(seed, now) {
                wrong.push(format!(
                    "[{name}] {} state {:?}, plain Rust {:?}",
                    hit.id,
                    got.state,
                    expected_state(seed, now)
                ));
            }
            if got.fee != expected_fee(seed, now) {
                wrong.push(format!(
                    "[{name}] {} fee {:?}, plain Rust {:?}",
                    hit.id,
                    got.fee,
                    expected_fee(seed, now)
                ));
            }
            if got.is_mine != (seed.owner == "anna") {
                wrong.push(format!("[{name}] {} is_mine {}", hit.id, got.is_mine));
            }
            if got.urgency != expected_urgency(seed, now) {
                wrong.push(format!("[{name}] {} urgency {:?}", hit.id, got.urgency));
            }
        }

        let mut check = |what: String, got: Vec<String>, expected: BTreeSet<String>| {
            filters += 1;
            let got = set(got);
            if got != expected {
                let extra: Vec<_> = got.difference(&expected).take(3).collect();
                let missing: Vec<_> = expected.difference(&got).take(3).collect();
                wrong.push(format!(
                    "[{name}] {what}: index has {extra:?} extra, {missing:?} missing"
                ));
            }
        };

        let state = Contract::state(&cx);
        for value in STATE_ORDER {
            check(
                format!("state == {value:?}"),
                ids(&client, Contract::query(&cx).filter(state.eq(value))).await,
                where_seed(&seeds, |seed| expected_state(seed, now) == value),
            );
            check(
                format!("state != {value:?}"),
                ids(&client, Contract::query(&cx).filter(state.ne(value))).await,
                where_seed(&seeds, |seed| expected_state(seed, now) != value),
            );
            // An ordered comparison follows the type's own order, also where
            // the value comes from the stored keyword.
            check(
                format!("state < {value:?}"),
                ids(&client, Contract::query(&cx).filter(state.lt(value))).await,
                where_seed(&seeds, |seed| expected_state(seed, now) < value),
            );
            check(
                format!("state >= {value:?}"),
                ids(&client, Contract::query(&cx).filter(state.gte(value))).await,
                where_seed(&seeds, |seed| expected_state(seed, now) >= value),
            );
        }
        check(
            "state in (Expiring, Closed)".to_owned(),
            ids(
                &client,
                Contract::query(&cx)
                    .filter(state.any_of([ContractState::Expiring, ContractState::Closed])),
            )
            .await,
            where_seed(&seeds, |seed| {
                matches!(
                    expected_state(seed, now),
                    ContractState::Expiring | ContractState::Closed
                )
            }),
        );
        check(
            "state between Open and Draft".to_owned(),
            ids(
                &client,
                Contract::query(&cx)
                    .filter(state.between(ContractState::Open, ContractState::Draft)),
            )
            .await,
            where_seed(&seeds, |seed| {
                (ContractState::Open..=ContractState::Draft).contains(&expected_state(seed, now))
            }),
        );
        for urgency in URGENCY_ORDER {
            check(
                format!("urgency == {urgency:?}"),
                ids(
                    &client,
                    Contract::query(&cx).filter(Contract::urgency(&cx).eq(urgency)),
                )
                .await,
                where_seed(&seeds, |seed| expected_urgency(seed, now) == urgency),
            );
        }
        for mine in [true, false] {
            check(
                format!("is_mine == {mine}"),
                ids(
                    &client,
                    Contract::query(&cx).filter(Contract::is_mine(&cx).eq(mine)),
                )
                .await,
                where_seed(&seeds, |seed| (seed.owner == "anna") == mine),
            );
        }
        let thresholds: BTreeSet<u64> = seeds
            .iter()
            .step_by(37)
            .map(|seed| expected_fee(seed, now).to_bits())
            .collect();
        for bits in thresholds {
            let at = f64::from_bits(bits);
            let fee = Contract::fee(&cx);
            check(
                format!("fee < {at}"),
                ids(&client, Contract::query(&cx).filter(fee.lt(at))).await,
                where_seed(&seeds, |seed| expected_fee(seed, now) < at),
            );
            check(
                format!("fee <= {at}"),
                ids(&client, Contract::query(&cx).filter(fee.lte(at))).await,
                where_seed(&seeds, |seed| expected_fee(seed, now) <= at),
            );
            check(
                format!("fee > {at}"),
                ids(&client, Contract::query(&cx).filter(fee.gt(at))).await,
                where_seed(&seeds, |seed| expected_fee(seed, now) > at),
            );
            check(
                format!("fee >= {at}"),
                ids(&client, Contract::query(&cx).filter(fee.gte(at))).await,
                where_seed(&seeds, |seed| expected_fee(seed, now) >= at),
            );
            check(
                format!("fee == {at}"),
                ids(&client, Contract::query(&cx).filter(fee.eq(at))).await,
                where_seed(&seeds, |seed| expected_fee(seed, now) == at),
            );
        }

        // One rule sort, both ways; then two packed into the one score, the
        // second also against the first's direction.
        let order = |key: &dyn Fn(&Seed) -> (i64, i64)| -> Vec<String> {
            let mut sorted: Vec<&Seed> = seeds.iter().collect();
            sorted.sort_by_key(|seed| (key(seed), seed.id.clone()));
            sorted.iter().map(|seed| seed.id.clone()).collect()
        };
        let state_rank = |seed: &Seed| expected_state(seed, now) as i64;
        let urgency_rank = |seed: &Seed| expected_urgency(seed, now) as i64;
        let urgency = Contract::urgency(&cx);
        let cases: [(&str, Vec<flusso_query::Sort>, Vec<String>); 4] = [
            (
                "state asc",
                vec![state.asc()],
                order(&|seed| (state_rank(seed), 0)),
            ),
            (
                "state desc",
                vec![state.desc()],
                order(&|seed| (-state_rank(seed), 0)),
            ),
            (
                "urgency asc, state asc",
                vec![urgency.asc(), state.asc()],
                order(&|seed| (urgency_rank(seed), state_rank(seed))),
            ),
            (
                "urgency desc, state asc",
                vec![urgency.desc(), state.asc()],
                order(&|seed| (-urgency_rank(seed), state_rank(seed))),
            ),
        ];
        for (what, by, expected) in cases {
            sorts += 1;
            let search = Contract::query(&cx).sorts(by).sort(Contract::id().asc());
            if ids(&client, search.clone()).await != expected {
                wrong.push(format!("[{name}] sort by {what} differs"));
            }
            // The same order page by page: the rank is a sort value a cursor
            // resumes after.
            let walked: Vec<String> = search
                .size(97)
                .cursor(None)
                .id_stream(&client)
                .try_collect()
                .await
                .unwrap();
            if walked != expected {
                wrong.push(format!("[{name}] walk by {what} differs"));
            }
        }

        let counts = state.counts(&Contract::query(&cx), &client).await.unwrap();
        counted += 1;
        for (value, count) in counts {
            let expected = where_seed(&seeds, |seed| expected_state(seed, now) == value).len();
            if count != expected as u64 {
                wrong.push(format!(
                    "[{name}] count of {value:?}: {count}, plain Rust {expected}"
                ));
            }
        }

        let sample = &seeds[seeds.len() / 3];
        let one = Contract::get(&cx, &client, &sample.id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            one.state,
            expected_state(sample, now),
            "get by id computes too"
        );
    }

    println!(
        "{} contracts x {} clocks: {values} values, {filters} filters, {sorts} sorts, \
         {counted} count sets; {} disagreements",
        seeds.len(),
        nows.len(),
        wrong.len()
    );
    assert!(wrong.is_empty(), "{:#?}", &wrong[..wrong.len().min(20)]);
}

// ---------------------------------------------------------------------------
// 3. Random conditions: the index and the Rust side decide alike.
// ---------------------------------------------------------------------------

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as usize
    }

    fn pick<T: Clone>(&mut self, items: &[T]) -> T {
        items[self.next() % items.len()].clone()
    }
}

fn leaf(rng: &mut Rng, now: DateTime<Utc>, fees: &[f64]) -> (String, Condition) {
    let whole = Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap();
    let stamp = |at: DateTime<Utc>| {
        serde_json::to_value(at)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    };
    let dates = [
        stamp(now),
        stamp(now + Duration::milliseconds(1)),
        stamp(now - Duration::milliseconds(1)),
        stamp(now + Duration::microseconds(400)),
        stamp(whole),
        stamp(whole + Duration::seconds(1)),
        stamp(now + Duration::days(3)),
        stamp(now + Duration::days(20)),
        "2026-11-16".to_owned(),
        "2026-11-19".to_owned(),
        "2026-11-16T12:00".to_owned(),
        "2026-11-16T13:00:00+01:00".to_owned(),
        now.timestamp_millis().to_string(),
    ];
    let doc = Contract::doc();
    match rng.next() % 10 {
        0..=3 => {
            let date = rng.pick(&dates);
            let high = rng.pick(&dates);
            let test = rng.next() % 8;
            // Required and nullable date fields take the same tests; the
            // macro keeps the two handle types apart.
            macro_rules! date_test {
                ($name:literal, $field:expr) => {
                    match test {
                        0 => (format!("{} < {date}", $name), $field.lt(date)),
                        1 => (format!("{} <= {date}", $name), $field.lte(date)),
                        2 => (format!("{} > {date}", $name), $field.gt(date)),
                        3 => (format!("{} >= {date}", $name), $field.gte(date)),
                        4 => (format!("{} == {date}", $name), $field.eq(date)),
                        5 => (
                            format!("{} between {date} and {high}", $name),
                            $field.between(date, high),
                        ),
                        6 => (
                            format!("{} in ({date}, {high})", $name),
                            $field.any_of([date, high]),
                        ),
                        _ => (format!("{} exists", $name), $field.exists()),
                    }
                };
            }
            match rng.next() % 5 {
                0 => date_test!("ends_at", doc.ends_at()),
                1 => date_test!("starts_at", doc.starts_at()),
                2 => date_test!("signed_at", doc.signed_at()),
                3 => date_test!("notice_starts_at", doc.notice_starts_at()),
                _ => date_test!("discount_until", doc.discount_until()),
            }
        }
        4 => {
            let state = rng.pick(&["open", "suspended", "terminated", "closed"]);
            match rng.next() % 3 {
                0 => (format!("state == {state}"), doc.state().eq(state)),
                1 => (format!("state != {state}"), doc.state().ne(state)),
                _ => (
                    format!("state in ({state}, open)"),
                    doc.state().any_of([state, "open"]),
                ),
            }
        }
        5 => {
            let fee = rng.pick(fees);
            let test = rng.next() % 7;
            macro_rules! fee_test {
                ($name:literal, $field:expr) => {
                    match test {
                        0 => (format!("{} < {fee}", $name), $field.lt(fee)),
                        1 => (format!("{} <= {fee}", $name), $field.lte(fee)),
                        2 => (format!("{} > {fee}", $name), $field.gt(fee)),
                        3 => (format!("{} >= {fee}", $name), $field.gte(fee)),
                        4 => (format!("{} == {fee}", $name), $field.eq(fee)),
                        5 => (format!("{} != {fee}", $name), $field.ne(fee)),
                        _ => (format!("{} exists", $name), $field.exists()),
                    }
                };
            }
            match rng.next() % 2 {
                0 => fee_test!("monthly_fee", doc.monthly_fee()),
                _ => fee_test!("discounted_fee", doc.discounted_fee()),
            }
        }
        6 => {
            let days = rng.pick(&[0, 14, 30, 45, 60]);
            let field = doc.notice_days();
            match rng.next() % 4 {
                0 => (format!("notice_days < {days}"), field.lt(days)),
                1 => (format!("notice_days >= {days}"), field.gte(days)),
                2 => (format!("notice_days == {days}"), field.eq(days)),
                _ => (
                    format!("notice_days in ({days}, 30)"),
                    field.any_of([days, 30]),
                ),
            }
        }
        7 => {
            let long = "x".repeat(300);
            let tag = rng.pick(&["b2b", "annual", "vip", "none", long.as_str()]);
            match rng.next() % 3 {
                0 => (format!("tags has {tag:.8}"), doc.tags().eq(tag)),
                1 => (
                    format!("tags has any of ({tag:.8}, annual)"),
                    doc.tags().any_of([tag, "annual"]),
                ),
                _ => ("tags exists".to_owned(), doc.tags().exists()),
            }
        }
        8 => {
            // A decimal compares as the index holds it: the nearest double.
            let price: Decimal = rng.pick(&LIST_PRICES).parse().unwrap();
            let field = doc.list_price();
            match rng.next() % 5 {
                0 => (format!("list_price < {price}"), field.lt(price)),
                1 => (format!("list_price <= {price}"), field.lte(price)),
                2 => (format!("list_price > {price}"), field.gt(price)),
                3 => (format!("list_price == {price}"), field.eq(price)),
                _ => (
                    format!("list_price between 0.2 and {price}"),
                    field.between(Decimal::new(2, 1), price),
                ),
            }
        }
        _ => match rng.next() % 4 {
            0 => (
                "billing.country == IT".to_owned(),
                doc.billing().country().eq("IT"),
            ),
            1 => (
                "billing.country missing".to_owned(),
                doc.billing().country().missing(),
            ),
            2 => ("owner_id == anna".to_owned(), doc.owner_id().eq("anna")),
            _ => ("auto_renew == false".to_owned(), doc.auto_renew().eq(false)),
        },
    }
}

fn condition(rng: &mut Rng, now: DateTime<Utc>, fees: &[f64], depth: u32) -> (String, Condition) {
    if depth == 0 || rng.next() % 3 == 0 {
        return leaf(rng, now, fees);
    }
    let (left_name, left) = condition(rng, now, fees, depth - 1);
    match rng.next() % 3 {
        0 => (format!("not ({left_name})"), left.not()),
        1 => {
            let (right_name, right) = condition(rng, now, fees, depth - 1);
            (format!("({left_name}) and ({right_name})"), left.and(right))
        }
        _ => {
            let (right_name, right) = condition(rng, now, fees, depth - 1);
            (format!("({left_name}) or ({right_name})"), left.or(right))
        }
    }
}

/// Every document's `_source`, as the index returns it.
async fn sources(base: &str) -> BTreeMap<String, Value> {
    let response: Value = reqwest::Client::new()
        .post(format!("{base}/{}/_search", Contract::physical_index()))
        .json(&json!({ "size": 5_000, "query": { "match_all": {} } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    response["hits"]["hits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| {
            (
                hit["_id"].as_str().unwrap().to_owned(),
                hit["_source"].clone(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn random_conditions_agree_with_the_index() {
    let now =
        Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap() + Duration::microseconds(123_456);
    let seeds = edges(now);
    let (_container, base, client, _sink) = start(&seeds).await;
    let sources = sources(&base).await;
    assert_eq!(sources.len(), seeds.len());
    let cx = cx(now);
    let mut fees: Vec<f64> = seeds
        .iter()
        .step_by(29)
        .flat_map(|seed| [seed.fee, seed.fee * 0.8])
        .collect();
    fees.push(455.4);

    let mut rng = Rng(42);
    let mut differs = Vec::new();
    let total = 800;
    for _ in 0..total {
        let (name, rule) = condition(&mut rng, now, &fees, 2);
        let in_rust: BTreeSet<String> = sources
            .iter()
            .filter(|(_, source)| rule.holds(source))
            .map(|(id, _)| id.clone())
            .collect();
        let in_index = set(ids(&client, Contract::query(&cx).filter(rule)).await);
        if in_index != in_rust {
            let extra: Vec<_> = in_index.difference(&in_rust).take(2).collect();
            let missing: Vec<_> = in_rust.difference(&in_index).take(2).collect();
            differs.push(format!(
                "{name}: index {} vs Rust {} (index only {extra:?}, Rust only {missing:?})",
                in_index.len(),
                in_rust.len()
            ));
        }
    }
    println!(
        "{total} random conditions over {} contracts: {} differ",
        seeds.len(),
        differs.len()
    );
    assert!(
        differs.is_empty(),
        "{:#?}",
        &differs[..differs.len().min(15)]
    );
}

// ---------------------------------------------------------------------------
// 4. What cannot run is refused, loudly, on the live path.
// ---------------------------------------------------------------------------

/// The one thing a rule can still get wrong at run time: text that is not a date.
fn broken(_cx: &Viewing) -> Rules<bool> {
    Rules::when(Broken::doc().ends_at().lt("next tuesday"))
        .then(true)
        .otherwise(false)
}

#[derive(Debug, Clone, Deserialize, FlussoRoot)]
#[flusso(
    index = "contracts",
    config = "tests/fixtures/read_time/flusso.toml",
    context = Viewing,
    scope_mod = "flusso_broken_query"
)]
struct Broken {
    id: String,
    #[flusso(rules = broken)]
    ends_soon: bool,
}

fn refused<T: std::fmt::Debug>(result: flusso_query::Result<T>) -> String {
    match result {
        Err(Error::ReadTimeField(reason)) => reason,
        other => panic!("expected a refused read-time field, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn what_cannot_run_is_refused() {
    let now = Utc.with_ymd_and_hms(2026, 11, 12, 10, 0, 0).unwrap();
    let (_container, _base, client, _sink) = start(&listing(now)).await;
    let cx = cx(now);
    let state = Contract::state(&cx);

    // A rule comparing a date with text that is not one: the value, a filter,
    // a count and a sort all say so.
    let ends_soon = Broken::ends_soon(&cx);
    let reason = refused(Broken::query(&cx).send(&client).await);
    assert!(
        reason.contains("`ends_soon`") && reason.contains("is not a date"),
        "{reason}"
    );
    refused(
        Broken::query(&cx)
            .filter(ends_soon.eq(true))
            .ids(&client)
            .await,
    );
    refused(
        Broken::query(&cx)
            .filter(ends_soon.eq(true))
            .count(&client)
            .await,
    );
    refused(Broken::query(&cx).sort(ends_soon.asc()).ids(&client).await);
    refused(
        client
            .msearch_all(&[Broken::query(&cx).filter(ends_soon.eq(true))])
            .await,
    );

    // Rule sorts that cannot share the one score.
    let apart = Contract::query(&cx).sorts([
        Contract::urgency(&cx).asc(),
        Contract::ends_at().asc(),
        state.asc(),
    ]);
    assert!(refused(apart.ids(&client).await).contains("separated by another sort key"));
    let floored = Contract::query(&cx).sort(state.asc()).min_score(0.5);
    assert!(refused(floored.ids(&client).await).contains("`min_score`"));

    // A rule sort takes the score over: text relevance no longer orders.
    let ranked = Contract::query(&cx)
        .query(Contract::title().matches("hosting backup storage"))
        .sort(state.asc())
        .size(50)
        .send(&client)
        .await
        .unwrap();
    let states: Vec<ContractState> = ranked.hits.iter().map(|hit| hit.source.state).collect();
    let mut sorted = states.clone();
    sorted.sort();
    assert!(states.len() > 1);
    assert_eq!(states, sorted);

    // A page cursor is bound to its query, and a rule filter carries the
    // clock: the same clock resumes, a moved one is refused. A search that
    // only shows read-time values has no clock in its query and resumes.
    let open = |cx: &Viewing| {
        Contract::query(cx)
            .filter(Contract::state(cx).ne(ContractState::Closed))
            .size(4)
    };
    let first = open(&cx).cursor(None).send(&client).await.unwrap();
    let next = first.next.clone();
    assert!(next.is_some());
    assert_eq!(
        open(&cx)
            .cursor(next.clone())
            .send(&client)
            .await
            .unwrap()
            .hits
            .len(),
        4
    );
    let later = Viewing {
        now: now + Duration::seconds(2),
        ..cx.clone()
    };
    assert!(matches!(
        open(&later).cursor(next).send(&client).await,
        Err(Error::CursorMismatch)
    ));
    let first = Contract::query(&cx)
        .size(4)
        .cursor(None)
        .send(&client)
        .await
        .unwrap();
    let second = Contract::query(&later)
        .size(4)
        .cursor(first.next)
        .send(&client)
        .await
        .unwrap();
    assert_eq!(second.hits.len(), 4);
}
