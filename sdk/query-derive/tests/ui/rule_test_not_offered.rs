// A rule handle offers only the tests a rule can make: no prefix or wildcard,
// no subfield, no ordering on a keyword or boolean, no field-against-field.
use flusso_query::FlussoRoot;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn main() {
    let doc = Contract::doc();
    let _ = doc.owner_id().prefix("u");
    let _ = doc.owner_id().keyword_lowercase();
    let _ = doc.state().lt("m");
    let _ = doc.auto_renew().gt(false);
    let _ = doc.starts_at().lt(doc.ends_at());
}
