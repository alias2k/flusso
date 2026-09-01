use flusso_query::FlussoValue;

// Only an enum declares variants to cover — a newtype inherits its inner
// type's exhaustiveness instead of declaring its own.
#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword, exhaustive)]
struct Sku(String);

fn main() {}
