use flusso_query::FlussoValue;

// An enum has no inner type to inherit a kind from, and there is no default —
// the derive demands an explicit `#[flusso(keyword)]` or `#[flusso(text)]`.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
enum Tier {
    Free,
    Pro,
}

fn main() {}
