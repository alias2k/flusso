use flusso_query::FlussoValue;

// Only `keyword` / `text` are nameable kinds. Numeric and date newtypes inherit
// their inner type's kinds, so there is no `number` tag — using one is rejected.
#[derive(serde::Deserialize, FlussoValue)]
#[flusso(number)]
enum Tier {
    Free,
    Pro,
}

fn main() {}
