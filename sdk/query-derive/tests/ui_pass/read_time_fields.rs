// A root with a context, a rule field and a computed field, read back.
use flusso_query::rules::Rules;
use flusso_query::{FlussoRoot, FlussoValue};

#[derive(Clone)]
struct Viewing {
    viewer: &'static str,
}

#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(rename_all = "lowercase")]
#[flusso(keyword)]
enum State {
    Draft,
    Active,
    Suspended,
    Terminated,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
    #[flusso(rules = visible_state)]
    state: State,
    #[flusso(rules = is_mine)]
    is_mine: bool,
    #[flusso(computed = label)]
    label: String,
}

fn visible_state(_: &Viewing) -> Rules<State> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(State::Draft)
        .otherwise(doc.state())
}

fn is_mine(cx: &Viewing) -> Rules<bool> {
    Rules::when(Contract::doc().owner_id().eq(cx.viewer))
        .then(true)
        .otherwise(false)
}

#[derive(serde::Deserialize)]
struct Titled {
    title: String,
}

fn label(doc: &Titled, _: &Viewing) -> String {
    doc.title.to_uppercase()
}

fn main() {
    let cx = Viewing { viewer: "u1" };
    let mut source = serde_json::json!({
        "id": "c1",
        "title": "hosting",
        "owner_id": "u1",
        "signed_at": null,
        "state": "active",
    });
    Contract::computation(&cx).apply(&mut source).unwrap();
    let contract: Contract = serde_json::from_value(source).unwrap();
    assert_eq!(contract.id, "c1");
    assert_eq!(contract.state, State::Draft);
    assert!(contract.is_mine);
    assert_eq!(contract.label, "HOSTING");
    let _ = Contract::query(&cx);
    let _ = Contract::state(&cx);
}
