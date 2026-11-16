// A rule compares a field with a value of the field's own kind.
use flusso_query::FlussoRoot;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "contracts", context = Viewing)]
struct Contract {
    id: String,
}

fn main() {
    let doc = Contract::doc();
    let _ = doc.notice_days().lt(1.5);
    let _ = doc.ends_at().lt(5);
    let _ = doc.state().eq(3);
    let _ = doc.auto_renew().eq("yes");
}
