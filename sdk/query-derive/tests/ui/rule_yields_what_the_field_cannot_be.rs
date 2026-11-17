// What a rule yields must read as the field's type: a stored field that can
// be missing only as `Option<…>`, an array only as `Vec<…>`, and never a
// field of another kind.
use flusso_query::FlussoRoot;
use flusso_query::rules::Rules;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn nullable_into_a_plain_number() -> Rules<f64> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(0.0)
        .otherwise(doc.discounted_fee())
}

fn keyword_into_a_bool() -> Rules<bool> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(false)
        .otherwise(doc.state())
}

fn array_into_one_value() -> Rules<String> {
    let doc = Contract::doc();
    Rules::when(doc.signed_at().missing())
        .then(String::new())
        .otherwise(doc.tags())
}

fn main() {}
