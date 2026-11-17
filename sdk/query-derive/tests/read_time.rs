//! Read-time fields through the derive: a root with a `context`, `rules` and
//! `computed` fields, and the generated `doc()` rule handles.
#![allow(dead_code, unused_crate_dependencies)]

use flusso_query::rules::{
    Array, Optional, Required, RuleBool, RuleDate, RuleField, RuleKeyword, RuleNumber, Rules,
};
use flusso_query::{AsQuery, Enum, FlussoRoot, FlussoValue, Search, Sortable, kind};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

type Result = std::result::Result<(), Box<dyn std::error::Error>>;

const NOON: i64 = 1_794_830_400_000; // 2026-11-16T12:00:00Z

/// What a request brings that no document holds.
#[derive(Debug, Clone)]
struct Viewing {
    now: &'static str,
    viewer: &'static str,
    rate: f64,
}

const CX: Viewing = Viewing {
    now: "2026-11-16T12:00:00Z",
    viewer: "u1",
    rate: 2.0,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, FlussoValue)]
#[serde(rename_all = "snake_case")]
#[flusso(keyword)]
enum State {
    Draft,
    Closed,
    Active,
    Suspended,
    Terminated,
}

#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "contracts", config = "tests/fixtures/flusso.toml", context = Viewing)]
struct Contract {
    id: String,
    title: String,
    /// Replaces the stored `state` on every fetched document.
    #[flusso(rules = visible_state)]
    state: State,
    #[flusso(rules = is_mine)]
    is_mine: bool,
    /// One of two stored fields, so it can be missing only if both can.
    #[flusso(rules = current_fee)]
    fee: Option<f64>,
    #[flusso(computed = fee_for_viewer)]
    fee_for_viewer: Option<f64>,
    #[flusso(computed = label)]
    #[serde(rename = "displayLabel")]
    label: String,
}

fn visible_state(cx: &Viewing) -> Rules<State> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(State::Draft)
        .when(
            doc.state()
                .eq(State::Terminated)
                .or(doc.ends_at().lt(cx.now)),
        )
        .then(State::Closed)
        .otherwise(doc.state())
}

fn is_mine(cx: &Viewing) -> Rules<bool> {
    Rules::when(Contract::doc().owner_id().eq(cx.viewer))
        .then(true)
        .otherwise(false)
}

fn current_fee(cx: &Viewing) -> Rules<Option<f64>> {
    let doc = Contract::doc();
    Rules::when(doc.discount_until().gt(cx.now))
        .then(doc.discounted_fee())
        .otherwise(doc.monthly_fee())
}

/// Reads `fee`, which the rules wrote just before.
#[derive(Deserialize)]
struct Priced {
    fee: Option<f64>,
}

fn fee_for_viewer(priced: &Priced, cx: &Viewing) -> Option<f64> {
    priced.fee.map(|fee| fee * cx.rate)
}

/// Reads a stored field, a rule field and a computed field.
#[derive(Deserialize)]
struct Labelled {
    title: String,
    state: State,
    fee_for_viewer: Option<f64>,
}

fn label(doc: &Labelled, _: &Viewing) -> String {
    match doc.fee_for_viewer {
        Some(fee) => format!("{} ({:?}, {fee:.2})", doc.title, doc.state),
        None => format!("{} ({:?})", doc.title, doc.state),
    }
}

fn stored() -> Value {
    json!({
        "id": "c1",
        "title": "Hosting",
        "owner_id": "u1",
        "signed_at": "2026-01-10T09:00:00",
        "starts_at": "2026-02-01T00:00:00",
        "ends_at": "2027-02-01T00:00:00",
        "state": "suspended",
        "auto_renew": false,
        "notice_days": 30,
        "monthly_fee": 100.0,
        "discounted_fee": 80.5,
        "discount_until": "2026-12-01T00:00:00",
        "list_price": "120.00",
    })
}

fn read(source: Value) -> std::result::Result<Contract, Box<dyn std::error::Error>> {
    let mut source = source;
    Contract::computation(&CX).apply(&mut source)?;
    Ok(serde_json::from_value(source)?)
}

#[test]
fn a_fetched_document_carries_its_read_time_fields() -> Result {
    let contract = read(stored())?;
    assert_eq!(contract.state, State::Suspended);
    assert!(contract.is_mine);
    assert_eq!(contract.fee, Some(80.5));
    assert_eq!(contract.fee_for_viewer, Some(161.0));
    assert_eq!(contract.label, "Hosting (Suspended, 161.00)");
    Ok(())
}

#[test]
fn a_rule_field_replaces_the_stored_field_of_the_same_key() -> Result {
    let mut ended = stored();
    ended["ends_at"] = json!("2026-06-01T00:00:00");
    assert_eq!(read(ended)?.state, State::Closed);

    let mut unsigned = stored();
    unsigned["signed_at"] = Value::Null;
    unsigned["discount_until"] = Value::Null;
    let contract = read(unsigned)?;
    assert_eq!(contract.state, State::Draft);
    assert_eq!(contract.fee, Some(100.0));
    Ok(())
}

#[test]
fn the_entry_points_take_the_context() {
    let search: Search<Contract> = Contract::query(&CX);
    assert_eq!(search.body()["query"], json!({ "match_all": {} }));
    // Generic code reaches the same through the trait.
    fn through_the_trait<R: FlussoRoot>(cx: &R::Context) -> Search<R> {
        R::query_in(cx)
    }
    assert_eq!(
        through_the_trait::<Contract>(&CX).body(),
        Contract::query(&CX).body()
    );
}

#[test]
fn get_takes_the_context_too() -> Result {
    let client = flusso_query::Client::connect("http://127.0.0.1:9200")?;
    // Built, not awaited: nothing listens there.
    let _fetch = Contract::get(&CX, &client, "c1");
    Ok(())
}

#[test]
fn a_rule_field_filters_and_sorts_on_the_value_documents_show() {
    let state: RuleField<State> = Contract::state(&CX);
    let signed = json!({ "exists": { "field": "signed_at" } });
    assert_eq!(
        state.eq(State::Closed).to_value(),
        json!({ "bool": { "minimum_should_match": 1, "should": [
            { "bool": { "filter": [
                signed,
                { "bool": { "minimum_should_match": 1, "should": [
                    { "terms": { "state": ["terminated"] } },
                    { "range": { "ends_at": { "lt": NOON, "format": "epoch_millis" } } },
                ] } },
            ] } },
            { "bool": {
                "filter": [
                    signed,
                    { "range": { "ends_at": { "gte": NOON, "format": "epoch_millis" } } },
                    { "terms": { "state": ["closed"] } },
                ],
                "must_not": [{ "terms": { "state": ["terminated"] } }],
            } },
        ] } })
    );

    // The stored enum declares `active, suspended, terminated`; `draft` and
    // `closed`, which only the rules yield, rank after them.
    let body = Contract::query(&CX).sort(state.asc()).body();
    let weights: Vec<u64> = body["query"]["function_score"]["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|function| function["weight"].as_u64())
        .collect();
    assert_eq!(weights, [4, 5, 1, 2, 3, 4, 5, 6]);
    assert_eq!(body["sort"], json!([{ "_score": { "order": "asc" } }]));
}

#[test]
fn the_stored_field_under_a_read_time_field_keeps_a_handle_of_its_own() {
    // `state` now names the rule field; what the index holds is `stored_state`.
    let stored: Enum = Contract::stored_state();
    assert_eq!(
        stored.eq(State::Suspended).to_value(),
        json!({ "term": { "state": "suspended" } })
    );
    assert_eq!(
        Contract::query(&CX).sort(stored.asc()).body()["sort"],
        json!([{ "state.sort": { "order": "asc" } }])
    );
}

#[test]
fn the_rule_handles_carry_kind_and_presence() {
    let doc = Contract::doc();
    let _: RuleKeyword<Required> = doc.owner_id();
    let _: RuleKeyword<Required> = doc.state();
    let _: RuleDate<Optional> = doc.signed_at();
    let _: RuleDate<Required> = doc.ends_at();
    let _: RuleBool<Required> = doc.auto_renew();
    let _: RuleNumber<kind::Integer, Required> = doc.notice_days();
    let _: RuleNumber<kind::Double, Optional> = doc.discounted_fee();
    let _: RuleNumber<kind::Decimal, Required> = doc.list_price();
    let _: RuleKeyword<Array> = doc.tags();
    // An object's fields are reached through it; a to-one join can be absent,
    // so everything under it can be missing.
    let _: RuleKeyword<Required> = doc.billing().country();
    let _: RuleNumber<kind::Double, Optional> = doc.billing().vat_rate();
    let _: RuleKeyword<Optional> = doc.owner().plan();
}

#[test]
fn a_condition_is_a_filter_like_any_query() {
    let doc = Contract::doc();
    let body = Contract::query(&CX)
        .filter(
            doc.billing()
                .country()
                .eq("PL")
                .and(doc.tags().any_of(["vip"])),
        )
        .filter(Contract::stored_state().eq(State::Active))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"],
        json!([
            { "bool": { "filter": [
                { "terms": { "billing.country": ["PL"] } },
                { "terms": { "tags": ["vip"] } },
            ] } },
            { "term": { "state": "active" } },
        ])
    );
}

/// A root without read-time fields is untouched: no context, the plain entry
/// points, no `doc()`.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(
    index = "contracts",
    config = "tests/fixtures/flusso.toml",
    scope_mod = "plain"
)]
struct PlainContract {
    id: String,
    /// The stored field, so only what the index stores: `State` also holds
    /// the values the rules add, which no stored document has.
    state: String,
}

#[test]
fn a_root_without_a_context_keeps_the_plain_entry_points() {
    fn plain<R: FlussoRoot<Context = ()>>() -> Search<R> {
        R::query()
    }
    assert_eq!(
        plain::<PlainContract>().body(),
        PlainContract::query().body()
    );
    let _: Enum = PlainContract::state();
}

/// A context alone gives the rule handles, for conditions used as filters.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(
    index = "contracts",
    config = "tests/fixtures/flusso.toml",
    scope_mod = "bare",
    context = Viewing
)]
struct BareContract {
    id: String,
}

#[test]
fn a_context_without_read_time_fields_still_gives_rule_handles() {
    let overdue = BareContract::doc().ends_at().lt(CX.now);
    assert_eq!(
        BareContract::query(&CX).filter(overdue).body()["query"]["bool"]["filter"],
        json!([{ "range": { "ends_at": { "lt": NOON, "format": "epoch_millis" } } }])
    );
    let _: Enum = BareContract::state();
}

/// A schema key named like what a root with a context generates for itself
/// (`doc`, `query`, `get`) keeps a handle, as `stored_<name>()`.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(index = "entry_points", config = "tests/fixtures/flusso.toml", context = Viewing)]
struct Thing {
    id: String,
    query: String,
}

#[test]
fn a_schema_key_named_like_an_entry_point_keeps_a_handle() {
    let body = Thing::query(&CX)
        .filter(Thing::stored_query().eq("saved"))
        .filter(Thing::stored_doc().prefix("a"))
        .filter(Thing::doc().query().eq("saved"))
        .body();
    assert_eq!(
        body["query"]["bool"]["filter"],
        json!([
            { "term": { "query": "saved" } },
            { "prefix": { "doc": "a" } },
            { "terms": { "query": ["saved"] } },
        ])
    );
}

/// A root over the fixture holding one field of every kind.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(
    index = "boundary",
    config = "tests/fixtures/flusso.toml",
    scope_mod = "every_kind",
    context = Viewing
)]
struct EveryKind {
    id: String,
}

/// Every kind a rule can read, with the presence its level and declaration
/// give it. The kinds with no handle are `ui/rule_handle_missing_for_a_kind.rs`.
#[test]
fn every_readable_kind_has_a_rule_handle() {
    let doc = EveryKind::doc();
    let _: RuleKeyword<Required> = doc.kw();
    let _: RuleKeyword<Optional> = doc.kwo();
    let _: RuleKeyword<Array> = doc.kwa();
    let _: RuleKeyword<Array> = doc.kwao();
    let _: RuleKeyword<Required> = doc.uu();
    let _: RuleKeyword<Required> = doc.ev();
    let _: RuleKeyword<Required> = doc.kns();
    let _: RuleBool<Optional> = doc.bo();
    let _: RuleBool<Required> = doc.ex();
    let _: RuleNumber<kind::Short, Required> = doc.sh();
    let _: RuleNumber<kind::Integer, Required> = doc.it();
    let _: RuleNumber<kind::Long, Required> = doc.n();
    let _: RuleNumber<kind::Long, Optional> = doc.nn();
    let _: RuleNumber<kind::Long, Required> = doc.ct();
    let _: RuleNumber<kind::Long, Array> = doc.ids();
    let _: RuleNumber<kind::Double, Required> = doc.db();
    let _: RuleNumber<kind::Double, Optional> = doc.av();
    let _: RuleNumber<kind::Decimal, Required> = doc.dc();
    let _: RuleDate<Required> = doc.dt();
    let _: RuleDate<Required> = doc.ts();
    let _: RuleKeyword<Required> = doc.ob().k();
    let _: RuleNumber<kind::Long, Required> = doc.ob().n();
    // A to-one join can find no row, whatever its fields declare.
    let _: RuleKeyword<Optional> = doc.ho().k();
    let _: RuleNumber<kind::Long, Optional> = doc.ho().n();
    let _: RuleKeyword<Optional> = doc.bt().k();

    assert_eq!(
        doc.ho().n().gte(3).and(doc.uu().exists()).to_value(),
        json!({ "bool": { "filter": [
            { "range": { "ho.n": { "gte": 3 } } },
            { "exists": { "field": "uu" } },
        ] } })
    );
}

/// Two ways a read-time field takes a stored field's name without being a
/// rule field of the same key: a rule field of that Rust name written under
/// another key, and a computed field written over the stored key.
#[derive(Debug, Deserialize, FlussoRoot)]
#[flusso(
    index = "contracts",
    config = "tests/fixtures/flusso.toml",
    scope_mod = "shadowing",
    context = Viewing
)]
struct Shadowing {
    id: String,
    #[flusso(rules = shown_state)]
    #[serde(rename = "shownState")]
    state: State,
    #[flusso(computed = shouted_title)]
    title: String,
}

fn shown_state(_: &Viewing) -> Rules<State> {
    let doc = Shadowing::doc();
    Rules::when(doc.signed_at().missing())
        .then(State::Draft)
        .otherwise(doc.state())
}

#[derive(Deserialize)]
struct Titled {
    title: String,
}

fn shouted_title(titled: &Titled, _: &Viewing) -> String {
    titled.title.to_uppercase()
}

#[test]
fn a_stored_field_keeps_a_handle_whichever_way_its_name_is_taken() -> Result {
    let _: RuleField<State> = Shadowing::state(&CX);
    let in_index: Enum = Shadowing::stored_state();
    assert_eq!(
        in_index.eq(State::Active).to_value(),
        json!({ "term": { "state": "active" } })
    );
    assert_eq!(
        Shadowing::stored_title().matches("hosting").to_value(),
        json!({ "match": { "title": "hosting" } })
    );

    let mut source = stored();
    source["signed_at"] = Value::Null;
    Shadowing::computation(&CX).apply(&mut source)?;
    // The rule field is written beside the stored field it reads, not over it.
    assert_eq!(source["shownState"], "draft");
    assert_eq!(source["state"], "suspended");
    assert_eq!(source["title"], "HOSTING");
    let read: Shadowing = serde_json::from_value(source)?;
    assert_eq!((read.state, read.title.as_str()), (State::Draft, "HOSTING"));
    Ok(())
}
